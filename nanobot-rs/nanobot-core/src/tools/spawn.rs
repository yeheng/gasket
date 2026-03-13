//! Spawn tool for subagent execution with synchronous blocking and streaming output
//!
//! This tool spawns a subagent and blocks until completion, streaming events
//! to the WebSocket/channel in real-time. This ensures the main agent waits
//! for results instead of using fire-and-forget semantics.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tracing::{info, instrument, trace, warn};

use super::base::{Tool, ToolError};
use crate::agent::subagent::SubagentManager;
use crate::agent::subagent_tracker::{SubagentEvent, SubagentTracker};
use crate::bus::events::{OutboundMessage, WebSocketMessage};
use crate::config::ModelRegistry;
use crate::providers::ProviderRegistry;

pub struct SpawnTool {
    manager: Option<Arc<SubagentManager>>,
    model_registry: Option<Arc<ModelRegistry>>,
    provider_registry: Option<Arc<ProviderRegistry>>,
}

impl SpawnTool {
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

impl Default for SpawnTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Deserialize)]
struct SpawnArgs {
    task: String,
    model_id: Option<String>,
}

#[async_trait]
impl Tool for SpawnTool {
    fn name(&self) -> &str {
        "spawn"
    }

    fn description(&self) -> &str {
        "Spawn a subagent to execute a task synchronously with real-time streaming output. \
         The main agent blocks until the subagent completes and returns the result. \
         Use this for tasks that need immediate results with progress feedback."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "Task description / prompt to execute"
                },
                "model_id": {
                    "type": "string",
                    "description": "Optional model profile ID to use for this subagent. If not specified, uses the default model."
                }
            },
            "required": ["task"]
        })
    }

    #[instrument(name = "tool.spawn", skip_all)]
    async fn execute(&self, args: Value) -> Result<String, ToolError> {
        let args: SpawnArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let manager = match &self.manager {
            Some(m) => m,
            None => {
                return Err(ToolError::ExecutionError(
                    "Subagent spawning is not available in this mode.".to_string(),
                ))
            }
        };

        if args.task.trim().is_empty() {
            return Err(ToolError::InvalidArguments(
                "Task description cannot be empty".to_string(),
            ));
        }

        // Create tracker for single task
        let mut tracker = SubagentTracker::new();
        let result_tx = tracker.result_sender();
        let event_tx = tracker.event_sender();
        let subagent_id = SubagentTracker::generate_id();
        let task = args.task.clone();

        info!(
            "[Spawn] Starting subagent {} for task: {}",
            subagent_id, task
        );

        // Prepare spawn configuration
        let spawn_result = if let Some(model_id) = &args.model_id {
            // Model switching requested
            let model_registry = self.model_registry.as_ref().ok_or_else(|| {
                ToolError::ExecutionError("Model registry not available".to_string())
            })?;
            let provider_registry = self.provider_registry.as_ref().ok_or_else(|| {
                ToolError::ExecutionError("Provider registry not available".to_string())
            })?;

            let profile = model_registry.get_profile(model_id).ok_or_else(|| {
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

            manager
                .submit_tracked_streaming(
                    subagent_id.clone(),
                    task,
                    result_tx.clone(),
                    event_tx.clone(),
                    Some(provider),
                    Some(agent_config),
                )
                .await
        } else {
            // Use default model
            manager
                .submit_tracked_streaming(
                    subagent_id.clone(),
                    task,
                    result_tx.clone(),
                    event_tx.clone(),
                    None,
                    None,
                )
                .await
        };

        // Check spawn result
        if let Err(e) = spawn_result {
            return Err(ToolError::ExecutionError(format!(
                "Failed to spawn subagent: {}",
                e
            )));
        }

        // Drop original senders - channel will close when all tasks complete
        drop(result_tx);
        drop(event_tx);

        // Take event receiver for streaming
        let mut event_rx = tracker.take_event_receiver();

        // Get outbound channel and session key for WebSocket streaming
        let outbound_tx = manager.outbound_sender();
        let session_key = manager.session_key().cloned();

        // Spawn background task to collect events and forward to WebSocket/channel
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                // Log the event
                match &event {
                    SubagentEvent::Started { id, task } => {
                        info!("[Spawn] Subagent {} started: {}", id, task);
                    }
                    SubagentEvent::Thinking { id, content } => {
                        trace!("[Spawn] Subagent {} thinking: {}", id, content);
                    }
                    SubagentEvent::ToolStart { id, tool_name, .. } => {
                        trace!("[Spawn] Subagent {} tool: {} started", id, tool_name);
                    }
                    SubagentEvent::ToolEnd { id, tool_name, .. } => {
                        trace!("[Spawn] Subagent {} tool: {} done", id, tool_name);
                    }
                    SubagentEvent::Completed { id, result } => {
                        info!(
                            "[Spawn] Subagent {} completed, model={}",
                            id,
                            result.model.as_deref().unwrap_or("unknown")
                        );
                    }
                    SubagentEvent::Error { id, error } => {
                        warn!("[Spawn] Subagent {} error: {}", id, error);
                    }
                }

                // Forward event to WebSocket/channel if session key is available
                if let Some(ref key) = session_key {
                    let ws_msg = match &event {
                        SubagentEvent::Thinking { content, .. } => {
                            Some(WebSocketMessage::thinking(format!("[Spawn] {}", content)))
                        }
                        SubagentEvent::ToolStart {
                            tool_name,
                            arguments,
                            ..
                        } => Some(WebSocketMessage::tool_start(
                            format!("[Spawn] {}", tool_name),
                            arguments.clone(),
                        )),
                        SubagentEvent::ToolEnd {
                            tool_name, output, ..
                        } => Some(WebSocketMessage::tool_end(
                            format!("[Spawn] {}", tool_name),
                            Some(output.clone()),
                        )),
                        SubagentEvent::Error { error, .. } => {
                            Some(WebSocketMessage::text(format!("[Spawn Error] {}", error)))
                        }
                        _ => None, // Started, Completed - don't send to WS
                    };

                    if let Some(msg) = ws_msg {
                        let outbound = OutboundMessage::with_ws_message(
                            key.channel.clone(),
                            &key.chat_id,
                            msg,
                        );
                        if let Err(e) = outbound_tx.try_send(outbound) {
                            warn!("[Spawn] Failed to send event to outbound channel: {}", e);
                        }
                    }
                }
            }
        });

        // Wait for result (blocking)
        info!("[Spawn] Waiting for subagent result...");
        let results = tracker.wait_for_all(1).await;

        if results.is_empty() {
            return Err(ToolError::ExecutionError(
                "Subagent completed but no result was received".to_string(),
            ));
        }

        let result = results.into_iter().next().unwrap();

        // Format output
        let mut output = String::new();

        // Include thinking content if available
        if let Some(ref reasoning) = result.response.reasoning_content {
            if !reasoning.is_empty() {
                output.push_str(&format!("**Thinking:**\n{}\n\n", reasoning));
            }
        }

        output.push_str(&format!(
            "**Model:** {}\n**Task:** {}\n\n**Response:**\n{}",
            result.model.as_deref().unwrap_or("unknown"),
            result.task,
            result.response.content
        ));

        Ok(output)
    }
}
