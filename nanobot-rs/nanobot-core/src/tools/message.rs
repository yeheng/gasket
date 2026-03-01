//! Message tool for sending messages to specific channels

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tracing::instrument;

use super::{Tool, ToolError, ToolResult};
use crate::bus::events::ChannelType;
use crate::bus::events::OutboundMessage;

/// Message tool for sending messages to specific channels
pub struct MessageTool {
    config: std::sync::Arc<crate::config::ChannelsConfig>,
}

impl MessageTool {
    /// Create a new message tool
    pub fn new(config: std::sync::Arc<crate::config::ChannelsConfig>) -> Self {
        Self { config }
    }
}

#[derive(Debug, Deserialize)]
struct MessageParams {
    /// Target channel (e.g., "telegram", "discord", "slack")
    channel: ChannelType,

    /// Target chat ID
    chat_id: String,

    /// Message content
    content: String,
}

#[async_trait]
impl Tool for MessageTool {
    fn name(&self) -> &str {
        "send_message"
    }

    fn description(&self) -> &str {
        "Send a message to a specific channel and chat. Use this to proactively reach out to users or send notifications."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "channel": {
                    "type": "string",
                    "description": "Target channel (e.g., 'telegram', 'discord', 'slack', 'email')",
                    "enum": ["telegram", "discord", "slack", "email", "dingtalk", "feishu", "cli"]
                },
                "chat_id": {
                    "type": "string",
                    "description": "Target chat ID (e.g., '123456' for Telegram, 'general' for Slack channel)"
                },
                "content": {
                    "type": "string",
                    "description": "Message content to send (supports Markdown formatting)"
                }
            },
            "required": ["channel", "chat_id", "content"]
        })
    }

    #[instrument(name = "tool.send_message", skip_all)]
    async fn execute(&self, params: Value) -> ToolResult {
        let params: MessageParams = serde_json::from_value(params)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        // Create outbound message
        let channel_name = params.channel.to_string();
        let message = OutboundMessage {
            channel: params.channel,
            chat_id: params.chat_id.clone(),
            content: params.content.clone(),
            metadata: Default::default(),
            trace_id: None,
        };

        crate::channels::send_outbound(&self.config, message)
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;

        Ok(format!(
            "Message sent successfully to {}:{}",
            channel_name, params.chat_id
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_message_tool_creation() {
        let config = std::sync::Arc::new(crate::config::ChannelsConfig::default());
        let tool = MessageTool::new(config);

        assert_eq!(tool.name(), "send_message");
        assert!(tool.description().contains("Send a message"));
    }

    #[tokio::test]
    async fn test_message_tool_parameters() {
        let config = std::sync::Arc::new(crate::config::ChannelsConfig::default());
        let tool = MessageTool::new(config);

        let params = tool.parameters();
        assert!(params["properties"]["channel"].is_object());
        assert!(params["properties"]["chat_id"].is_object());
        assert!(params["properties"]["content"].is_object());
    }

    #[tokio::test]
    async fn test_invalid_parameters() {
        let config = std::sync::Arc::new(crate::config::ChannelsConfig::default());
        let tool = MessageTool::new(config);

        let params = serde_json::json!({
            "channel": "telegram"
            // Missing chat_id and content
        });

        let result = tool.execute(params).await;
        assert!(result.is_err());
    }
}
