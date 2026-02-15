//! Message events

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Inbound message from a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    /// Source channel (telegram, discord, etc.)
    pub channel: String,

    /// Sender ID
    pub sender_id: String,

    /// Chat ID (for routing responses)
    pub chat_id: String,

    /// Message content
    pub content: String,

    /// Media attachments (if any)
    #[serde(default)]
    pub media: Option<Vec<MediaAttachment>>,

    /// Additional metadata
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,

    /// Timestamp
    #[serde(default = "Utc::now")]
    pub timestamp: DateTime<Utc>,
}

impl InboundMessage {
    /// Get the session key for this message
    pub fn session_key(&self) -> String {
        format!("{}:{}", self.channel, self.chat_id)
    }
}

/// Outbound message to a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    /// Target channel
    pub channel: String,

    /// Target chat ID
    pub chat_id: String,

    /// Message content
    pub content: String,

    /// Additional metadata
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Media attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAttachment {
    /// Media type (image, audio, video, etc.)
    pub media_type: String,

    /// URL or base64 data
    pub data: String,

    /// Optional caption
    #[serde(default)]
    pub caption: Option<String>,
}
