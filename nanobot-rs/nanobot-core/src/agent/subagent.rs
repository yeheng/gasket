//! Subagent manager for background task execution

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, instrument, warn};

use crate::agent::executor_core::{AgentExecutor, ExecutionResult};
use crate::agent::loop_::AgentConfig;
use crate::agent::prompt;
use crate::agent::stream::StreamEvent;
use crate::agent::subagent_tracker::{SubagentEvent, SubagentResult};
use crate::bus::events::{OutboundMessage, SessionKey};
use crate::providers::{ChatMessage, LlmProvider};
use crate::tools::ToolRegistry;

use super::loop_::{AgentLoop, AgentResponse};

/// Default timeout for subagent execution (10 minutes)
const SUBAGENT_TIMEOUT_SECS: u64 = 600;

/// Run a subagent with minimal overhead - pure function
pub async fn run_subagent(
    task: &str,
    system_prompt: &str,
    provider: Arc<dyn LlmProvider>,
    tools: Arc<ToolRegistry>,
    config: &AgentConfig,
) -> Result<ExecutionResult, anyhow::Error> {
    let messages = vec![ChatMessage::system(system_prompt), ChatMessage::user(task)];
    let executor = AgentExecutor::new(provider, tools, config);
    executor
        .execute(messages)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))
}

pub struct SubagentManager {
    provider: Arc<dyn LlmProvider>,
    workspace: PathBuf,
    tools: Arc<ToolRegistry>,
    outbound_tx: mpsc::Sender<OutboundMessage>,
    /// Optional session key for WebSocket streaming (set per-request)
    session_key: Option<SessionKey>,
}

impl SubagentManager {
    pub async fn new(
        provider: Arc<dyn LlmProvider>,
        workspace: PathBuf,
        tools: Arc<ToolRegistry>,
        outbound_tx: mpsc::Sender<OutboundMessage>,
    ) -> Self {
        Self {
            provider,
            workspace,
            tools,
            outbound_tx,
            session_key: None,
        }
    }

    /// Create a SubagentManager with a specific session key for WebSocket streaming
    pub fn with_session_key(&self, session_key: SessionKey) -> Self {
        Self {
            provider: self.provider.clone(),
            workspace: self.workspace.clone(),
            tools: self.tools.clone(),
            outbound_tx: self.outbound_tx.clone(),
            session_key: Some(session_key),
        }
    }

    /// Get a clone of the outbound sender for external use
    pub fn outbound_sender(&self) -> mpsc::Sender<OutboundMessage> {
        self.outbound_tx.clone()
    }

    /// Get the current session key (if set)
    pub fn session_key(&self) -> Option<&SessionKey> {
        self.session_key.as_ref()
    }

    /// Check if WebSocket streaming is enabled
    pub fn is_streaming_enabled(&self) -> bool {
        self.session_key.is_some()
    }

    #[instrument(name = "subagent.submit", skip_all)]
    pub fn submit(&self, prompt: &str, channel: &str, chat_id: &str) -> anyhow::Result<()> {
        let provider = self.provider.clone();
        let workspace = self.workspace.clone();
        let tools = self.tools.clone();
        let outbound_tx = self.outbound_tx.clone();
        let prompt = prompt.to_string();

        let channel_enum = match channel {
            "telegram" => crate::bus::ChannelType::Telegram,
            "discord" => crate::bus::ChannelType::Discord,
            "slack" => crate::bus::ChannelType::Slack,
            "email" => crate::bus::ChannelType::Email,
            "dingtalk" => crate::bus::ChannelType::Dingtalk,
            "feishu" => crate::bus::ChannelType::Feishu,
            _ => crate::bus::ChannelType::Cli,
        };
        let chat_id = chat_id.to_string();
        let session_key = SessionKey::new(channel_enum.clone(), &chat_id);

        tokio::spawn(async move {
            info!("Subagent task started: {}", prompt);
            let agent_config = AgentConfig {
                model: provider.default_model().to_string(),
                max_iterations: 10,
                ..Default::default()
            };

            let mut agent =
                match AgentLoop::builder(provider, workspace.clone(), agent_config, tools) {
                    Ok(a) => a,
                    Err(e) => {
                        warn!("Failed to initialise subagent: {}", e);
                        return;
                    }
                };

            // Load minimal system prompt directly (no hook dispatch)
            let system_prompt =
                match prompt::load_system_prompt(&workspace, prompt::BOOTSTRAP_FILES_MINIMAL).await
                {
                    Ok(p) => p,
                    Err(e) => {
                        warn!("Failed to load minimal system prompt: {}", e);
                        return;
                    }
                };
            agent.set_system_prompt(system_prompt);

            let timeout_duration = std::time::Duration::from_secs(SUBAGENT_TIMEOUT_SECS);
            let result = tokio::time::timeout(
                timeout_duration,
                agent.process_direct(&prompt, &session_key),
            )
            .await;

            let content = match result {
                Ok(Ok(response)) => format!("Background task completed:\n{}", response.content),
                Ok(Err(e)) => format!("Background task failed: {}", e),
                Err(_) => format!(
                    "Background task failed: Execution timed out after {:?}",
                    timeout_duration
                ),
            };

            let msg = crate::bus::events::OutboundMessage {
                channel: channel_enum,
                chat_id,
                content,
                metadata: None,
                trace_id: None,
                ws_message: None,
            };

            // Route through the Outbound Actor — no direct HTTP call
            if let Err(e) = outbound_tx.send(msg).await {
                warn!("Failed to send subagent result to outbound channel: {}", e);
            }
        });

        Ok(())
    }

    /// Submit a prompt and **synchronously wait** for the agent response.
    ///
    /// Unlike `submit()` (fire-and-forget), this method blocks the caller
    /// until the subagent finishes. It is designed for governance-layer
    /// agents (e.g. review gates) where the pipeline must wait for a
    /// decision before proceeding.
    ///
    /// An optional `system_prompt` can be provided to inject a role-specific
    /// SOUL.md — if `None`, the default minimal bootstrap prompt is used.
    #[instrument(name = "subagent.submit_and_wait", skip_all)]
    pub async fn submit_and_wait(
        &self,
        prompt_text: &str,
        system_prompt: Option<&str>,
        channel: &str,
        chat_id: &str,
    ) -> anyhow::Result<AgentResponse> {
        info!("Subagent (sync) started: {}", &prompt_text);

        let agent_config = AgentConfig {
            model: self.provider.default_model().to_string(),
            max_iterations: 10,
            ..Default::default()
        };

        let mut agent = AgentLoop::builder(
            self.provider.clone(),
            self.workspace.clone(),
            agent_config,
            self.tools.clone(),
        )?;

        let sys = match system_prompt {
            Some(s) => s.to_string(),
            None => {
                prompt::load_system_prompt(&self.workspace, prompt::BOOTSTRAP_FILES_MINIMAL).await?
            }
        };
        agent.set_system_prompt(sys);

        let channel_enum = crate::bus::ChannelType::new(channel);
        let session_key = SessionKey::new(channel_enum, chat_id);

        tokio::time::timeout(
            std::time::Duration::from_secs(SUBAGENT_TIMEOUT_SECS),
            agent.process_direct(prompt_text, &session_key),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Subagent timed out after {SUBAGENT_TIMEOUT_SECS}s"))?
        .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Submit a prompt with a **specific model** and wait for the response.
    ///
    /// This method allows switching to a different provider/model for the
    /// subagent execution. Used by the `switch_model` tool.
    ///
    /// # Arguments
    /// * `prompt_text` - The task description for the subagent
    /// * `system_prompt` - Optional custom system prompt (uses minimal bootstrap if None)
    /// * `provider` - The LLM provider to use for this execution
    /// * `agent_config` - Agent configuration including model, temperature, etc.
    #[instrument(name = "subagent.submit_and_wait_with_model", skip_all)]
    pub async fn submit_and_wait_with_model(
        &self,
        prompt_text: &str,
        system_prompt: Option<&str>,
        provider: Arc<dyn LlmProvider>,
        agent_config: AgentConfig,
        channel: &str,
        chat_id: &str,
    ) -> anyhow::Result<AgentResponse> {
        info!(
            "Subagent (model switch) started with model '{}': {}",
            agent_config.model, &prompt_text
        );

        let mut agent = AgentLoop::builder(
            provider,
            self.workspace.clone(),
            agent_config,
            self.tools.clone(),
        )?;

        let sys = match system_prompt {
            Some(s) => s.to_string(),
            None => {
                prompt::load_system_prompt(&self.workspace, prompt::BOOTSTRAP_FILES_MINIMAL).await?
            }
        };
        agent.set_system_prompt(sys);

        let channel_enum = crate::bus::ChannelType::new(channel);
        let session_key = SessionKey::new(channel_enum, chat_id);

        tokio::time::timeout(
            std::time::Duration::from_secs(SUBAGENT_TIMEOUT_SECS),
            agent.process_direct(prompt_text, &session_key),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Model switch task timed out after {SUBAGENT_TIMEOUT_SECS}s"))?
        .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Submit a prompt with a **specific model** and stream events to a callback.
    ///
    /// This method allows switching to a different provider/model for the
    /// subagent execution with streaming support. Used by the `switch_model` tool
    /// to send real-time updates to WebSocket clients.
    ///
    /// # Arguments
    /// * `prompt_text` - The task description for the subagent
    /// * `system_prompt` - Optional custom system prompt (uses minimal bootstrap if None)
    /// * `provider` - The LLM provider to use for this execution
    /// * `agent_config` - Agent configuration including model, temperature, etc.
    /// * `stream_callback` - Callback function for streaming events
    #[instrument(name = "subagent.submit_and_wait_with_model_streaming", skip_all)]
    pub async fn submit_and_wait_with_model_streaming<F>(
        &self,
        prompt_text: &str,
        system_prompt: Option<&str>,
        provider: Arc<dyn LlmProvider>,
        agent_config: AgentConfig,
        mut stream_callback: F,
    ) -> anyhow::Result<AgentResponse>
    where
        F: FnMut(StreamEvent) + Send + 'static,
    {
        info!(
            "Subagent (model switch streaming) started with model '{}': {}",
            agent_config.model, prompt_text
        );

        let mut agent = AgentLoop::builder(
            provider,
            self.workspace.clone(),
            agent_config,
            self.tools.clone(),
        )?;

        let sys = match system_prompt {
            Some(s) => s.to_string(),
            None => {
                prompt::load_system_prompt(&self.workspace, prompt::BOOTSTRAP_FILES_MINIMAL).await?
            }
        };
        agent.set_system_prompt(sys);

        let session_key = SessionKey::new(crate::bus::ChannelType::Cli, "model_switch_streaming");

        tokio::time::timeout(
            std::time::Duration::from_secs(SUBAGENT_TIMEOUT_SECS),
            agent.process_direct_streaming(prompt_text, &session_key, move |event| {
                stream_callback(event);
            }),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Model switch task timed out after {SUBAGENT_TIMEOUT_SECS}s"))?
        .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Send a WebSocket message to the outbound channel.
    ///
    /// This is a helper method for tools to send real-time updates.
    pub async fn send_ws_message(&self, msg: OutboundMessage) {
        if let Err(e) = self.outbound_tx.send(msg).await {
            warn!("Failed to send WebSocket message: {}", e);
        }
    }

    /// Try to send a WebSocket message without waiting.
    ///
    /// Uses try_send to avoid blocking if the channel is full.
    pub fn try_send_ws_message(&self, msg: OutboundMessage) -> bool {
        self.outbound_tx.try_send(msg).is_ok()
    }

    /// Submit a task with tracking support for parallel coordination.
    ///
    /// Returns a unique subagent ID that can be used to retrieve results later.
    /// The result will be sent to the provided result_tx channel when complete.
    #[instrument(name = "subagent.submit_tracked", skip_all)]
    pub async fn submit_tracked(
        &self,
        subagent_id: String,
        task: String,
        result_tx: mpsc::Sender<SubagentResult>,
    ) -> anyhow::Result<String> {
        self.submit_tracked_impl(subagent_id, task, result_tx, None, None, None)
            .await
    }

    /// Submit a task with custom model and provider configuration.
    ///
    /// Allows specifying a different model/provider for this specific subagent execution.
    #[instrument(name = "subagent.submit_tracked_with_config", skip_all)]
    pub async fn submit_tracked_with_config(
        &self,
        subagent_id: String,
        task: String,
        result_tx: mpsc::Sender<SubagentResult>,
        provider: Option<Arc<dyn LlmProvider>>,
        agent_config: Option<AgentConfig>,
    ) -> anyhow::Result<String> {
        self.submit_tracked_impl(subagent_id, task, result_tx, provider, agent_config, None)
            .await
    }

    /// Submit a task with streaming events support.
    ///
    /// This method sends real-time events (thinking, tool calls) through the event_tx channel.
    #[instrument(name = "subagent.submit_tracked_streaming", skip_all)]
    pub async fn submit_tracked_streaming(
        &self,
        subagent_id: String,
        task: String,
        result_tx: mpsc::Sender<SubagentResult>,
        event_tx: mpsc::Sender<SubagentEvent>,
        provider: Option<Arc<dyn LlmProvider>>,
        agent_config: Option<AgentConfig>,
    ) -> anyhow::Result<String> {
        self.submit_tracked_impl(
            subagent_id,
            task,
            result_tx,
            provider,
            agent_config,
            Some(event_tx),
        )
        .await
    }

    /// Internal implementation with optional streaming support.
    #[instrument(name = "subagent.submit_tracked_impl", skip_all)]
    async fn submit_tracked_impl(
        &self,
        subagent_id: String,
        task: String,
        result_tx: mpsc::Sender<SubagentResult>,
        provider: Option<Arc<dyn LlmProvider>>,
        agent_config: Option<AgentConfig>,
        event_tx: Option<mpsc::Sender<SubagentEvent>>,
    ) -> anyhow::Result<String> {
        let provider = provider.unwrap_or_else(|| self.provider.clone());
        let workspace = self.workspace.clone();
        let tools = self.tools.clone();
        let task_clone = task.clone();
        let id_clone = subagent_id.clone();

        let agent_config = agent_config.unwrap_or_else(|| AgentConfig {
            model: provider.default_model().to_string(),
            max_iterations: 10,
            ..Default::default()
        });

        tokio::spawn(async move {
            info!(
                "[Subagent {}] Task started with model '{}': {}",
                &subagent_id, &agent_config.model, &task
            );

            // Send started event
            if let Some(ref tx) = event_tx {
                let _ = tx.try_send(SubagentEvent::Started {
                    id: subagent_id.clone(),
                    task: task_clone.clone(),
                });
            }

            // Helper to send error result
            let model_name = agent_config.model.clone();
            let send_error = |error_msg: &str, model: &str| -> AgentResponse {
                AgentResponse {
                    content: format!("Error: {}", error_msg),
                    reasoning_content: None,
                    tools_used: vec![],
                    model: Some(model.to_string()),
                }
            };

            // Helper to send error event
            let send_error_event = |tx: &mpsc::Sender<SubagentEvent>, id: &str, error: &str| {
                let _ = tx.try_send(SubagentEvent::Error {
                    id: id.to_string(),
                    error: error.to_string(),
                });
            };

            let mut agent =
                match AgentLoop::builder(provider, workspace.clone(), agent_config, tools) {
                    Ok(a) => a,
                    Err(e) => {
                        warn!("[Subagent {}] Failed to initialize: {}", subagent_id, e);
                        if let Some(ref tx) = event_tx {
                            send_error_event(
                                tx,
                                &subagent_id,
                                &format!("Agent initialization failed: {}", e),
                            );
                        }
                        let _ = result_tx
                            .send(SubagentResult {
                                id: subagent_id.clone(),
                                task: task_clone.clone(),
                                response: send_error(
                                    &format!("Agent initialization failed: {}", e),
                                    &model_name,
                                ),
                                model: Some(model_name.clone()),
                            })
                            .await;
                        return;
                    }
                };

            let system_prompt =
                match prompt::load_system_prompt(&workspace, prompt::BOOTSTRAP_FILES_MINIMAL).await
                {
                    Ok(p) => p,
                    Err(e) => {
                        warn!(
                            "[Subagent {}] Failed to load system prompt: {}",
                            subagent_id, e
                        );
                        if let Some(ref tx) = event_tx {
                            send_error_event(
                                tx,
                                &subagent_id,
                                &format!("System prompt load failed: {}", e),
                            );
                        }
                        let _ = result_tx
                            .send(SubagentResult {
                                id: subagent_id.clone(),
                                task: task_clone.clone(),
                                response: send_error(
                                    &format!("System prompt load failed: {}", e),
                                    &model_name,
                                ),
                                model: Some(model_name.clone()),
                            })
                            .await;
                        return;
                    }
                };
            agent.set_system_prompt(system_prompt);

            let session_key = SessionKey::new(crate::bus::ChannelType::Cli, &subagent_id);
            let timeout_duration = std::time::Duration::from_secs(SUBAGENT_TIMEOUT_SECS);

            // Use streaming if event_tx is provided
            let response = if let Some(ref tx) = event_tx {
                let tx_clone = tx.clone();
                let id_clone = subagent_id.clone();

                let result = tokio::time::timeout(
                    timeout_duration,
                    agent.process_direct_streaming(&task, &session_key, move |event| match event {
                        StreamEvent::Reasoning(content) => {
                            let _ = tx_clone.try_send(SubagentEvent::Thinking {
                                id: id_clone.clone(),
                                content,
                            });
                        }
                        StreamEvent::ToolStart { name, arguments } => {
                            let _ = tx_clone.try_send(SubagentEvent::ToolStart {
                                id: id_clone.clone(),
                                tool_name: name,
                                arguments,
                            });
                        }
                        StreamEvent::ToolEnd { name, output } => {
                            let _ = tx_clone.try_send(SubagentEvent::ToolEnd {
                                id: id_clone.clone(),
                                tool_name: name,
                                output,
                            });
                        }
                        _ => {}
                    }),
                )
                .await;

                match result {
                    Ok(Ok(resp)) => resp,
                    Ok(Err(e)) => {
                        warn!("[Subagent {}] Execution failed: {}", subagent_id, e);
                        send_error_event(tx, &subagent_id, &format!("Execution failed: {}", e));
                        let _ = result_tx
                            .send(SubagentResult {
                                id: subagent_id.clone(),
                                task: task_clone.clone(),
                                response: send_error(
                                    &format!("Execution failed: {}", e),
                                    &model_name,
                                ),
                                model: Some(model_name.clone()),
                            })
                            .await;
                        return;
                    }
                    Err(_) => {
                        warn!(
                            "[Subagent {}] Timed out after {:?}",
                            subagent_id, timeout_duration
                        );
                        send_error_event(
                            tx,
                            &subagent_id,
                            &format!("Timed out after {:?}", timeout_duration),
                        );
                        let _ = result_tx
                            .send(SubagentResult {
                                id: subagent_id.clone(),
                                task: task_clone.clone(),
                                response: send_error(
                                    &format!("Timed out after {:?}", timeout_duration),
                                    &model_name,
                                ),
                                model: Some(model_name.clone()),
                            })
                            .await;
                        return;
                    }
                }
            } else {
                // Non-streaming path
                let result = tokio::time::timeout(
                    timeout_duration,
                    agent.process_direct(&task, &session_key),
                )
                .await;

                match result {
                    Ok(Ok(resp)) => resp,
                    Ok(Err(e)) => {
                        warn!("[Subagent {}] Execution failed: {}", subagent_id, e);
                        let _ = result_tx
                            .send(SubagentResult {
                                id: subagent_id.clone(),
                                task: task_clone.clone(),
                                response: send_error(
                                    &format!("Execution failed: {}", e),
                                    &model_name,
                                ),
                                model: Some(model_name.clone()),
                            })
                            .await;
                        return;
                    }
                    Err(_) => {
                        warn!(
                            "[Subagent {}] Timed out after {:?}",
                            subagent_id, timeout_duration
                        );
                        let _ = result_tx
                            .send(SubagentResult {
                                id: subagent_id.clone(),
                                task: task_clone.clone(),
                                response: send_error(
                                    &format!("Timed out after {:?}", timeout_duration),
                                    &model_name,
                                ),
                                model: Some(model_name.clone()),
                            })
                            .await;
                        return;
                    }
                }
            };

            let subagent_result = SubagentResult {
                id: subagent_id.clone(),
                task: task_clone,
                response: response.clone(),
                model: Some(model_name),
            };

            // Send completed event
            if let Some(ref tx) = event_tx {
                let _ = tx.try_send(SubagentEvent::Completed {
                    id: subagent_id.clone(),
                    result: subagent_result.clone(),
                });
            }

            if let Err(e) = result_tx.send(subagent_result).await {
                warn!("[Subagent {}] Failed to send result: {}", subagent_id, e);
            }
        });

        Ok(id_clone)
    }
}
