//! Pure agent executor - the minimal LLM loop
//!
//! This is the core execution engine that handles:
//! - LLM request/response cycle
//! - Tool call detection and execution
//! - Iteration control
//!
//! It does NOT handle:
//! - Session persistence
//! - History management
//! - Hooks
//! - Vault injection
//! - Token tracking (returns raw usage data)

use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::agent::executor::ToolExecutor;
use crate::agent::loop_::AgentConfig;
use crate::agent::request::RequestHandler;
use crate::agent::stream::{self, StreamEvent};
use crate::error::AgentError;
use crate::providers::{ChatMessage, ChatResponse, LlmProvider};
use crate::token_tracker::TokenUsage;
use crate::tools::ToolRegistry;

/// Pure agent execution result
#[derive(Debug)]
pub struct ExecutionResult {
    pub content: String,
    pub reasoning_content: Option<String>,
    pub tools_used: Vec<String>,
    pub token_usage: Option<TokenUsage>,
}

/// Pure agent executor - minimal LLM loop
pub struct AgentExecutor<'a> {
    provider: Arc<dyn LlmProvider>,
    tools: Arc<ToolRegistry>,
    config: &'a AgentConfig,
}

impl<'a> AgentExecutor<'a> {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        tools: Arc<ToolRegistry>,
        config: &'a AgentConfig,
    ) -> Self {
        Self {
            provider,
            tools,
            config,
        }
    }

    /// Execute agent loop - pure function
    pub async fn execute(
        &self,
        mut messages: Vec<ChatMessage>,
    ) -> Result<ExecutionResult, AgentError> {
        let mut tools_used = Vec::new();
        let executor = ToolExecutor::new(&self.tools, self.config.max_tool_result_chars);
        let request_handler = RequestHandler::new(&self.provider, &self.tools, self.config);
        let mut total_usage: Option<TokenUsage> = None;

        for iteration in 1..=self.config.max_iterations {
            debug!("[Executor] iteration {}", iteration);

            let request = request_handler.build_chat_request(&messages);
            let stream_result = request_handler.send_with_retry(request).await?;
            let response = stream::collect_stream_response(stream_result).await?;

            // Accumulate token usage
            if let Some(usage) = response.token_usage() {
                total_usage = Some(match total_usage {
                    Some(mut acc) => {
                        acc.input_tokens += usage.input_tokens;
                        acc.output_tokens += usage.output_tokens;
                        acc.total_tokens += usage.total_tokens;
                        acc
                    }
                    None => usage.clone(),
                });
            }

            if !response.has_tool_calls() {
                let content = response.content.unwrap_or_else(|| {
                    "I've completed processing but have no response to give.".to_string()
                });
                return Ok(ExecutionResult {
                    content,
                    reasoning_content: response.reasoning_content,
                    tools_used,
                    token_usage: total_usage,
                });
            }

            // Execute tool calls
            self.handle_tool_calls(&response, &executor, &mut messages, &mut tools_used)
                .await;
        }

        Ok(ExecutionResult {
            content: "Maximum iterations reached.".to_string(),
            reasoning_content: None,
            tools_used,
            token_usage: total_usage,
        })
    }

    /// Execute with streaming - sends events to provided channel
    pub async fn execute_stream(
        &self,
        mut messages: Vec<ChatMessage>,
        event_tx: mpsc::Sender<StreamEvent>,
    ) -> Result<ExecutionResult, AgentError> {
        let mut tools_used = Vec::new();
        let executor = ToolExecutor::new(&self.tools, self.config.max_tool_result_chars);
        let request_handler = RequestHandler::new(&self.provider, &self.tools, self.config);
        let mut total_usage: Option<TokenUsage> = None;

        for iteration in 1..=self.config.max_iterations {
            debug!("[Executor] iteration {}", iteration);

            let request = request_handler.build_chat_request(&messages);
            let stream_result = request_handler.send_with_retry(request).await?;

            let (mut event_stream, response_future) = stream::stream_events(stream_result);

            // Forward events
            while let Some(event) = event_stream.next().await {
                let _ = event_tx.send(event).await;
            }

            let response = response_future.await?;

            if let Some(usage) = response.token_usage() {
                total_usage = Some(match total_usage {
                    Some(mut acc) => {
                        acc.input_tokens += usage.input_tokens;
                        acc.output_tokens += usage.output_tokens;
                        acc.total_tokens += usage.total_tokens;
                        acc
                    }
                    None => usage.clone(),
                });
            }

            if !response.has_tool_calls() {
                let content = response.content.unwrap_or_else(|| {
                    "I've completed processing but have no response to give.".to_string()
                });
                return Ok(ExecutionResult {
                    content,
                    reasoning_content: response.reasoning_content,
                    tools_used,
                    token_usage: total_usage,
                });
            }

            self.handle_tool_calls(&response, &executor, &mut messages, &mut tools_used)
                .await;
        }

        Ok(ExecutionResult {
            content: "Maximum iterations reached.".to_string(),
            reasoning_content: None,
            tools_used,
            token_usage: total_usage,
        })
    }

    async fn handle_tool_calls(
        &self,
        response: &ChatResponse,
        executor: &ToolExecutor<'_>,
        messages: &mut Vec<ChatMessage>,
        tools_used: &mut Vec<String>,
    ) {
        messages.push(ChatMessage::assistant_with_tools(
            response.content.clone(),
            response.tool_calls.clone(),
        ));

        for tool_call in &response.tool_calls {
            info!("[Executor] Executing tool: {}", tool_call.function.name);
            tools_used.push(tool_call.function.name.clone());

            let result = executor.execute_one(tool_call).await;
            messages.push(ChatMessage::tool_result(
                &tool_call.id,
                &tool_call.function.name,
                &result.output,
            ));
        }
    }
}
