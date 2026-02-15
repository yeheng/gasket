//! Context builder for constructing LLM prompts

use std::path::PathBuf;

use crate::providers::ChatMessage;
use crate::session::SessionMessage;

/// Context builder for constructing prompts
#[allow(dead_code)]
pub struct ContextBuilder {
    workspace: PathBuf,
    system_prompt: String,
}

impl ContextBuilder {
    /// Create a new context builder
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
        }
    }

    /// Set a custom system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    /// Build the message list for an LLM request
    pub fn build_messages(
        &self,
        history: Vec<SessionMessage>,
        current_message: &str,
        memory: Option<&str>,
        _channel: &str,
        _chat_id: &str,
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // System prompt
        let mut system_content = self.system_prompt.clone();
        if let Some(mem) = memory {
            if !mem.is_empty() {
                system_content.push_str("\n\n## Long-term Memory\n");
                system_content.push_str(mem);
            }
        }
        messages.push(ChatMessage::system(system_content));

        // History
        for msg in history {
            match msg.role.as_str() {
                "user" => messages.push(ChatMessage::user(&msg.content)),
                "assistant" => messages.push(ChatMessage::assistant(&msg.content)),
                _ => {}
            }
        }

        // Current message
        messages.push(ChatMessage::user(current_message));

        messages
    }

    /// Add an assistant message to the history
    pub fn add_assistant_message(
        &self,
        messages: Vec<ChatMessage>,
        content: Option<String>,
        _tool_calls: Vec<serde_json::Value>,
        _reasoning_content: Option<String>,
    ) -> Vec<ChatMessage> {
        let mut result = messages;

        // For now, just add the content as an assistant message
        // Tool calls will be handled separately in the agent loop
        if let Some(c) = content {
            result.push(ChatMessage::assistant(c));
        }

        result
    }

    /// Add a tool result to the messages
    pub fn add_tool_result(
        &self,
        messages: Vec<ChatMessage>,
        tool_id: String,
        tool_name: String,
        result: String,
    ) -> Vec<ChatMessage> {
        let mut result_messages = messages;
        result_messages.push(ChatMessage::tool_result(tool_id, tool_name, result));
        result_messages
    }
}

/// Default system prompt
const DEFAULT_SYSTEM_PROMPT: &str = r#"You are nanobot, a helpful AI assistant.

You have access to tools for reading files, writing files, editing files, listing directories, and executing shell commands.

Be concise and helpful. When using tools, explain what you're doing before and after the tool call.

Working directory: {{workspace}}"#;
