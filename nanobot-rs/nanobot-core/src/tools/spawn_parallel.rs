//! Parallel spawn tool for concurrent subagent execution with result aggregation

use std::sync::Arc;

use async_trait::async_trait;
use futures::future::join_all;
use serde::Deserialize;
use serde_json::Value;
use tracing::{info, instrument, trace, warn};

use super::base::{Tool, ToolError};
use crate::agent::subagent::SubagentManager;
use crate::agent::subagent_tracker::{SubagentEvent, SubagentTracker};
use crate::bus::events::{OutboundMessage, WebSocketMessage};
use crate::config::ModelRegistry;
use crate::providers::ProviderRegistry;

pub struct SpawnParallelTool {
    manager: Option<Arc<SubagentManager>>,
    model_registry: Option<Arc<ModelRegistry>>,
    provider_registry: Option<Arc<ProviderRegistry>>,
}

impl Default for SpawnParallelTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SpawnParallelTool {
    pub fn new() -> Self {
        Self {
            manager: None,
            model_registry: None,
            provider_registry: None,
        }
    }

    pub fn with_manager(manager: Arc<SubagentManager>) -> Self {
        Self {
            manager: Some(manager),
            model_registry: None,
            provider_registry: None,
        }
    }

    pub fn with_registries(
        manager: Arc<SubagentManager>,
        model_registry: Arc<ModelRegistry>,
        provider_registry: Arc<ProviderRegistry>,
    ) -> Self {
        Self {
            manager: Some(manager),
            model_registry: Some(model_registry),
            provider_registry: Some(provider_registry),
        }
    }
}

#[derive(Deserialize)]
struct TaskSpec {
    task: String,
    model_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TaskInput {
    Simple(Vec<String>),
    WithModels(Vec<TaskSpec>),
    /// Handle LLM passing JSON as a string
    JsonString(String),
}

#[derive(Deserialize)]
struct SpawnParallelArgs {
    tasks: TaskInput,
}

#[async_trait]
impl Tool for SpawnParallelTool {
    fn name(&self) -> &str {
        "spawn_parallel"
    }

    fn description(&self) -> &str {
        "Execute multiple tasks in parallel using subagents with optional per-task model selection. Returns aggregated responses from all subagents. Useful for parallel research, data gathering, or independent computations with different models."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tasks": {
                    "description": "List of tasks to execute in parallel. Can be simple strings or objects with task and model_id",
                    "oneOf": [
                        {
                            "type": "array",
                            "items": {
                                "type": "string"
                            },
                            "minItems": 1,
                            "maxItems": 10
                        },
                        {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "task": {
                                        "type": "string",
                                        "description": "Task description"
                                    },
                                    "model_id": {
                                        "type": "string",
                                        "description": "Optional model profile ID for this specific task"
                                    }
                                },
                                "required": ["task"]
                            },
                            "minItems": 1,
                            "maxItems": 10
                        }
                    ]
                }
            },
            "required": ["tasks"]
        })
    }

    #[instrument(name = "tool.spawn_parallel", skip_all)]
    async fn execute(&self, args: Value) -> Result<String, ToolError> {
        let args: SpawnParallelArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let manager = match &self.manager {
            Some(m) => m,
            None => {
                return Err(ToolError::ExecutionError(
                    "Parallel task spawning is not available in this mode.".to_string(),
                ))
            }
        };

        // Normalize tasks to TaskSpec format
        let task_specs: Vec<TaskSpec> = match args.tasks {
            TaskInput::Simple(tasks) => tasks
                .into_iter()
                .map(|task| TaskSpec {
                    task,
                    model_id: None,
                })
                .collect(),
            TaskInput::WithModels(specs) => specs,
            TaskInput::JsonString(json_str) => {
                // Try to parse the JSON string
                // First try as Vec<TaskSpec> (with models)
                if let Ok(specs) = serde_json::from_str::<Vec<TaskSpec>>(&json_str) {
                    specs
                } else if let Ok(tasks) = serde_json::from_str::<Vec<String>>(&json_str) {
                    // Try as Vec<String> (simple)
                    tasks
                        .into_iter()
                        .map(|task| TaskSpec {
                            task,
                            model_id: None,
                        })
                        .collect()
                } else {
                    return Err(ToolError::InvalidArguments(
                        "Failed to parse tasks JSON string. Expected array of strings or objects with 'task' field.".to_string()
                    ));
                }
            }
        };

        if task_specs.is_empty() {
            return Err(ToolError::InvalidArguments(
                "At least one task is required".to_string(),
            ));
        }

        if task_specs.len() > 10 {
            return Err(ToolError::InvalidArguments(
                "Maximum 10 parallel tasks allowed".to_string(),
            ));
        }

        let tracker = SubagentTracker::new();
        let result_tx = tracker.result_sender();
        let event_tx = tracker.event_sender();
        let task_count = task_specs.len();

        info!(
            "Preparing {} parallel subagent tasks with streaming support",
            task_count
        );

        // Prepare spawn configurations for all tasks first (sequential but fast)
        // Use Box<dyn Future> to unify different async block types
        let mut spawn_futures: Vec<
            std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send>>,
        > = Vec::with_capacity(task_count);

        // Clone senders for each task BEFORE the loop to avoid shadowing issues
        let result_senders: Vec<_> = (0..task_count).map(|_| result_tx.clone()).collect();
        let event_senders: Vec<_> = (0..task_count).map(|_| event_tx.clone()).collect();

        // Drop the original senders after cloning - the clones will keep the channel alive
        drop(result_tx);
        drop(event_tx);

        for (idx, spec) in task_specs.into_iter().enumerate() {
            let subagent_id = SubagentTracker::generate_id();
            let task = spec.task;
            let result_tx = result_senders[idx].clone();
            let event_tx_clone = event_senders[idx].clone();

            if let Some(model_id) = spec.model_id {
                // Model switching requested
                let model_registry = self.model_registry.as_ref().ok_or_else(|| {
                    ToolError::ExecutionError("Model registry not available".to_string())
                })?;
                let provider_registry = self.provider_registry.as_ref().ok_or_else(|| {
                    ToolError::ExecutionError("Provider registry not available".to_string())
                })?;

                let profile = model_registry.get_profile(&model_id).ok_or_else(|| {
                    ToolError::ExecutionError(format!("Model profile not found: {}", model_id))
                })?;

                let provider = provider_registry
                    .get_or_create(&profile.provider)
                    .map_err(|e| {
                        ToolError::ExecutionError(format!("Failed to create provider: {}", e))
                    })?;

                let agent_config = crate::agent::loop_::AgentConfig {
                    model: profile.model.clone(),
                    temperature: profile.temperature.unwrap_or(0.7),
                    thinking_enabled: profile.thinking_enabled.unwrap_or(false),
                    max_tokens: profile.max_tokens.unwrap_or(4096),
                    ..Default::default()
                };

                // Create boxed future for this spawn with streaming
                let manager = manager.clone();
                spawn_futures.push(Box::pin(async move {
                    manager
                        .submit_tracked_streaming(
                            subagent_id,
                            task,
                            result_tx,
                            event_tx_clone,
                            Some(provider),
                            Some(agent_config),
                        )
                        .await
                }));
            } else {
                // Use default model - also use streaming
                let manager = manager.clone();
                spawn_futures.push(Box::pin(async move {
                    manager
                        .submit_tracked_streaming(
                            subagent_id,
                            task,
                            result_tx,
                            event_tx_clone,
                            None,
                            None,
                        )
                        .await
                }));
            }
        }

        // Spawn all subagents in parallel - this is the key change!
        info!(
            "Spawning {} subagents in parallel with streaming",
            spawn_futures.len()
        );

        // Get event receiver before spawning background task
        let event_rx = tracker.event_receiver();

        // Get outbound channel and session key from manager for WebSocket streaming
        let outbound_tx = manager.outbound_sender();
        let session_key = manager.session_key().cloned();

        // Spawn a background task to collect events and forward to WebSocket/channel
        tokio::spawn(async move {
            let mut rx = event_rx.lock().await;
            while let Some(event) = rx.recv().await {
                // Log the event
                match &event {
                    SubagentEvent::Started { id, task } => {
                        info!("[Subagent {}] Started: {}", id, task);
                    }
                    SubagentEvent::Thinking { id, content } => {
                        trace!("[Subagent {}] Thinking: {}", id, content);
                    }
                    SubagentEvent::ToolStart { id, tool_name, .. } => {
                        trace!("[Subagent {}] Tool: {} started", id, tool_name);
                    }
                    SubagentEvent::ToolEnd { id, tool_name, .. } => {
                        trace!("[Subagent {}] Tool: {} done", id, tool_name);
                    }
                    SubagentEvent::Completed { id, result } => {
                        info!(
                            "[Subagent {}] Completed, model={}",
                            id,
                            result.model.as_deref().unwrap_or("unknown")
                        );
                    }
                    SubagentEvent::Error { id, error } => {
                        warn!("[Subagent {}] Error: {}", id, error);
                    }
                }

                // Forward event to WebSocket/channel if session key is available
                if let Some(ref key) = session_key {
                    let ws_msg = match &event {
                        SubagentEvent::Thinking { content, .. } => Some(
                            WebSocketMessage::thinking(format!("[Subagent] {}", content)),
                        ),
                        SubagentEvent::ToolStart {
                            tool_name,
                            arguments,
                            ..
                        } => Some(WebSocketMessage::tool_start(
                            format!("[Subagent] {}", tool_name),
                            arguments.clone(),
                        )),
                        SubagentEvent::ToolEnd {
                            tool_name, output, ..
                        } => Some(WebSocketMessage::tool_end(
                            format!("[Subagent] {}", tool_name),
                            Some(output.clone()),
                        )),
                        SubagentEvent::Error { error, .. } => Some(WebSocketMessage::text(
                            format!("[Subagent Error] {}", error),
                        )),
                        _ => None, // Started, Completed - don't send to WS
                    };

                    if let Some(msg) = ws_msg {
                        let outbound = OutboundMessage::with_ws_message(
                            key.channel.clone(),
                            &key.chat_id,
                            msg,
                        );
                        // Use try_send to avoid blocking the event loop
                        if let Err(e) = outbound_tx.try_send(outbound) {
                            warn!("Failed to send subagent event to outbound channel: {}", e);
                        }
                    }
                }
            }
        });

        // Wait for spawn results
        let spawn_results = join_all(spawn_futures).await;

        info!(
            "All {} subagent spawn requests submitted (results pending)",
            spawn_results.len()
        );

        // Check for spawn failures
        let mut spawn_failures = 0;
        for (idx, result) in spawn_results.into_iter().enumerate() {
            if let Err(e) = result {
                warn!("Task {} failed to spawn: {}", idx + 1, e);
                spawn_failures += 1;
            }
        }

        if spawn_failures > 0 {
            warn!(
                "{} subagent(s) failed to spawn, expecting {} results",
                spawn_failures,
                task_count - spawn_failures
            );
        }

        // Wait for all results
        info!("Waiting for {} subagent results...", task_count);
        let results = tracker.wait_for_all(task_count).await;

        if results.len() < task_count {
            warn!(
                "Only received {}/{} subagent results. Missing results may be due to: \
                 1) Subagent task crashed before sending result, \
                 2) Channel closed unexpectedly, \
                 3) Timeout waiting for results",
                results.len(),
                task_count
            );
        } else {
            info!("All {} subagents completed successfully", results.len());
        }

        // Aggregate results
        let mut output = format!("Completed {} parallel tasks:\n\n", task_count);
        for (idx, result) in results.iter().enumerate() {
            // Include thinking content if available
            if let Some(ref reasoning) = result.response.reasoning_content {
                if !reasoning.is_empty() {
                    output.push_str(&format!("**Thinking:**\n{}\n\n", reasoning));
                }
            }

            output.push_str(&format!(
                "## Task {} (ID: {})\n**Model:** {}\n**Prompt:** {}\n**Response:**\n{}\n\n",
                idx + 1,
                &result.id,
                result.model.as_deref().unwrap_or("unknown"),
                &result.task,
                result.response.content
            ));
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = SpawnParallelTool::new();
        assert_eq!(tool.name(), "spawn_parallel");
    }

    #[test]
    fn test_tool_description() {
        let tool = SpawnParallelTool::new();
        assert!(tool.description().contains("parallel"));
        assert!(tool.description().contains("subagents"));
    }

    #[test]
    fn test_parameters_schema() {
        let tool = SpawnParallelTool::new();
        let params = tool.parameters();

        assert_eq!(params["type"], "object");
        assert!(params["properties"]["tasks"].is_object());
        assert_eq!(params["required"][0], "tasks");
    }

    #[tokio::test]
    async fn test_empty_tasks_validation() {
        let tool = SpawnParallelTool::new();
        let args = serde_json::json!({
            "tasks": []
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_too_many_tasks_validation() {
        let tool = SpawnParallelTool::new();
        let tasks: Vec<String> = (0..15).map(|i| format!("Task {}", i)).collect();
        let args = serde_json::json!({
            "tasks": tasks
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_no_manager_error() {
        let tool = SpawnParallelTool::new();
        let args = serde_json::json!({
            "tasks": ["Task 1"]
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not available"));
    }

    #[test]
    fn test_json_string_parsing_with_models() {
        // Simulate LLM passing tasks as a JSON string
        let json_str = r#"[{"task": "Task 1", "model_id": "gpt-4"}, {"task": "Task 2"}]"#;
        let args = serde_json::json!({
            "tasks": json_str
        });
        let parsed: SpawnParallelArgs = serde_json::from_value(args).unwrap();
        match parsed.tasks {
            TaskInput::JsonString(s) => {
                assert_eq!(s, json_str);
            }
            _ => panic!("Expected JsonString variant"),
        }
    }
}
