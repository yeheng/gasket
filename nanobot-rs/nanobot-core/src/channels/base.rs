//! Channel base types

use async_trait::async_trait;

use crate::bus::events::OutboundMessage;
use crate::trail::TrailContext;

/// Channel trait for implementing chat channel integrations.
///
/// Provides a unified lifecycle: `init` → `start` → `stop` → `graceful_shutdown`.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Get the channel name
    fn name(&self) -> &str;

    /// Start the channel (begin receiving messages)
    async fn start(&mut self) -> anyhow::Result<()>;

    /// Stop the channel
    async fn stop(&mut self) -> anyhow::Result<()>;

    /// Send a message through this channel
    async fn send(&self, msg: OutboundMessage) -> anyhow::Result<()>;

    /// Graceful shutdown with optional timeout.
    ///
    /// Default implementation delegates to `stop()`.
    async fn graceful_shutdown(&mut self) -> anyhow::Result<()> {
        self.stop().await
    }
}

/// Context attached to inbound/outbound messages for observability.
#[derive(Debug, Clone)]
pub struct MessageContext {
    /// Trail context for tracing
    pub trail_ctx: TrailContext,

    /// Arbitrary metadata
    pub metadata: std::collections::HashMap<String, String>,
}

impl MessageContext {
    /// Create a new message context with trail information.
    pub fn new(trail_ctx: TrailContext) -> Self {
        Self {
            trail_ctx,
            metadata: std::collections::HashMap::new(),
        }
    }
}

impl Default for MessageContext {
    fn default() -> Self {
        Self {
            trail_ctx: TrailContext::default(),
            metadata: std::collections::HashMap::new(),
        }
    }
}
