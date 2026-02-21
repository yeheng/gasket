//! Channel middleware infrastructure
//!
//! Provides middleware types, error handling, and built-in middlewares
//! for channel operations.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use tracing::{debug, warn};

use crate::bus::events::{InboundMessage, OutboundMessage};
use crate::trail::{Handler, Middleware, MiddlewareStack, Next};

/// Type alias for inbound channel middleware (message reception).
pub type ChannelInboundMiddleware = dyn Middleware<InboundMessage, InboundMessage>;

/// Type alias for outbound channel middleware (message sending).
pub type ChannelOutboundMiddleware = dyn Middleware<OutboundMessage, ()>;

// ── InboundProcessor Trait ─────────────────────────────────

/// Trait for processing inbound messages through middleware.
///
/// Channels use this to process incoming messages instead of publishing
/// directly to the bus, ensuring middleware is applied.
#[async_trait]
pub trait InboundProcessor: Send + Sync {
    /// Process an inbound message through the middleware stack.
    async fn process(&self, msg: InboundMessage) -> anyhow::Result<()>;
}

/// A no-op inbound processor that just drops messages.
/// Used for testing or when no processing is needed.
pub struct NoopInboundProcessor;

#[async_trait]
impl InboundProcessor for NoopInboundProcessor {
    async fn process(&self, _msg: InboundMessage) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Inbound processor that publishes directly to a bus without middleware.
/// This is a bridge for backward compatibility.
pub struct BusInboundProcessor {
    bus: crate::bus::MessageBus,
}

impl BusInboundProcessor {
    /// Create a new bus inbound processor.
    pub fn new(bus: crate::bus::MessageBus) -> Self {
        Self { bus }
    }
}

#[async_trait]
impl InboundProcessor for BusInboundProcessor {
    async fn process(&self, msg: InboundMessage) -> anyhow::Result<()> {
        self.bus.publish_inbound(msg).await;
        Ok(())
    }
}

// ── ChannelError ──────────────────────────────────────────

/// Structured error type for channel operations.
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    /// The channel is not connected or not started.
    #[error("Channel '{channel}' is not connected")]
    NotConnected { channel: String },

    /// Authentication with the channel service failed.
    #[error("Auth error for channel '{channel}': {message}")]
    AuthError { channel: String, message: String },

    /// The message could not be delivered.
    #[error("Delivery failed for channel '{channel}': {message}")]
    DeliveryFailed { channel: String, message: String },

    /// Rate limited by the channel service.
    #[error("Rate limited by channel '{channel}'")]
    RateLimited { channel: String },

    /// The message format is invalid for this channel.
    #[error("Invalid message format: {0}")]
    InvalidFormat(String),

    /// Other/unknown errors.
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl ChannelError {
    /// Get the channel name associated with this error (if any).
    pub fn channel(&self) -> Option<&str> {
        match self {
            Self::NotConnected { channel }
            | Self::AuthError { channel, .. }
            | Self::DeliveryFailed { channel, .. }
            | Self::RateLimited { channel } => Some(channel),
            _ => None,
        }
    }

    /// Whether this error is likely transient and retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::DeliveryFailed { .. }
        )
    }
}

// ── ChannelLoggingMiddleware ──────────────────────────────

/// Logs inbound messages at debug level.
pub struct ChannelLoggingMiddleware;

#[async_trait]
impl Middleware<InboundMessage, InboundMessage> for ChannelLoggingMiddleware {
    async fn handle(
        &self,
        request: InboundMessage,
        next: Next<'_, InboundMessage, InboundMessage>,
    ) -> anyhow::Result<InboundMessage> {
        debug!(
            channel = %request.channel,
            sender = %request.sender_id,
            chat_id = %request.chat_id,
            content_len = request.content.len(),
            "Inbound message"
        );

        let start = Instant::now();
        let result = next.run(request).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => debug!(elapsed_ms = elapsed.as_millis() as u64, "Inbound processed"),
            Err(e) => warn!(elapsed_ms = elapsed.as_millis() as u64, error = %e, "Inbound error"),
        }

        result
    }

    fn name(&self) -> &str {
        "ChannelLoggingMiddleware"
    }
}

/// Logs outbound messages at debug level.
pub struct ChannelOutboundLoggingMiddleware;

#[async_trait]
impl Middleware<OutboundMessage, ()> for ChannelOutboundLoggingMiddleware {
    async fn handle(
        &self,
        request: OutboundMessage,
        next: Next<'_, OutboundMessage, ()>,
    ) -> anyhow::Result<()> {
        debug!(
            channel = %request.channel,
            chat_id = %request.chat_id,
            content_len = request.content.len(),
            "Outbound message"
        );

        let start = Instant::now();
        let result = next.run(request).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(()) => debug!(elapsed_ms = elapsed.as_millis() as u64, "Outbound sent"),
            Err(e) => warn!(elapsed_ms = elapsed.as_millis() as u64, error = %e, "Outbound error"),
        }

        result
    }

    fn name(&self) -> &str {
        "ChannelOutboundLoggingMiddleware"
    }
}

// ── ChannelAuthMiddleware ─────────────────────────────────

/// Filters inbound messages by sender allowlist.
///
/// If the allowlist is empty, all messages pass through.
pub struct ChannelAuthMiddleware {
    allowed_senders: HashSet<String>,
}

impl ChannelAuthMiddleware {
    /// Create an auth middleware with an allowlist of sender IDs.
    pub fn new(allowed_senders: impl IntoIterator<Item = String>) -> Self {
        Self {
            allowed_senders: allowed_senders.into_iter().collect(),
        }
    }
}

#[async_trait]
impl Middleware<InboundMessage, InboundMessage> for ChannelAuthMiddleware {
    async fn handle(
        &self,
        request: InboundMessage,
        next: Next<'_, InboundMessage, InboundMessage>,
    ) -> anyhow::Result<InboundMessage> {
        if !self.allowed_senders.is_empty()
            && !self.allowed_senders.contains(&request.sender_id)
        {
            warn!(
                sender = %request.sender_id,
                channel = %request.channel,
                "Message rejected: sender not in allowlist"
            );
            anyhow::bail!(
                "Sender '{}' not authorized for channel '{}'",
                request.sender_id,
                request.channel
            );
        }
        next.run(request).await
    }

    fn name(&self) -> &str {
        "ChannelAuthMiddleware"
    }
}

// ── ChannelRateLimitMiddleware ────────────────────────────

/// Token-bucket rate limiter for inbound messages per sender.
///
/// Allows at most `max_messages` per sender within any rolling `window` period.
pub struct ChannelRateLimitMiddleware {
    max_messages: u32,
    window: std::time::Duration,
    /// sender_id -> deque of timestamps
    timestamps: Arc<Mutex<std::collections::HashMap<String, VecDeque<Instant>>>>,
}

impl ChannelRateLimitMiddleware {
    /// Create a rate limiter allowing `max_messages` per sender per `window`.
    pub fn new(max_messages: u32, window: std::time::Duration) -> Self {
        Self {
            max_messages,
            window,
            timestamps: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }
}

#[async_trait]
impl Middleware<InboundMessage, InboundMessage> for ChannelRateLimitMiddleware {
    async fn handle(
        &self,
        request: InboundMessage,
        next: Next<'_, InboundMessage, InboundMessage>,
    ) -> anyhow::Result<InboundMessage> {
        let allowed = {
            let mut map = self.timestamps.lock().unwrap();
            let ts = map.entry(request.sender_id.clone()).or_default();
            let now = Instant::now();

            // Evict expired entries
            while let Some(&front) = ts.front() {
                if now.duration_since(front) > self.window {
                    ts.pop_front();
                } else {
                    break;
                }
            }

            if ts.len() < self.max_messages as usize {
                ts.push_back(now);
                true
            } else {
                false
            }
        };

        if !allowed {
            warn!(
                sender = %request.sender_id,
                channel = %request.channel,
                "Rate limit exceeded"
            );
            anyhow::bail!(
                "Rate limit exceeded for sender '{}' on channel '{}'",
                request.sender_id,
                request.channel
            );
        }

        next.run(request).await
    }

    fn name(&self) -> &str {
        "ChannelRateLimitMiddleware"
    }
}

// ── MiddlewareInboundProcessor ─────────────────────────────

/// Inbound processor that applies middleware before publishing.
pub struct MiddlewareInboundProcessor {
    middleware: MiddlewareStack<InboundMessage, InboundMessage>,
    bus: crate::bus::MessageBus,
}

impl MiddlewareInboundProcessor {
    /// Create a new processor with middleware and bus.
    pub fn new(
        middleware: MiddlewareStack<InboundMessage, InboundMessage>,
        bus: crate::bus::MessageBus,
    ) -> Self {
        Self { middleware, bus }
    }
}

#[async_trait]
impl InboundProcessor for MiddlewareInboundProcessor {
    async fn process(&self, msg: InboundMessage) -> anyhow::Result<()> {
        let handler = InboundPassthroughHandler;
        let processed = self.middleware.execute(msg, &handler).await?;
        self.bus.publish_inbound(processed).await;
        Ok(())
    }
}

/// Passthrough handler for inbound messages.
struct InboundPassthroughHandler;

#[async_trait::async_trait]
impl Handler<InboundMessage, InboundMessage> for InboundPassthroughHandler {
    async fn handle(&self, request: InboundMessage) -> anyhow::Result<InboundMessage> {
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::events::ChannelType;
    use crate::trail::{Handler, MiddlewareStack};
    use chrono::Utc;

    fn make_inbound(sender: &str) -> InboundMessage {
        InboundMessage {
            channel: ChannelType::Cli,
            sender_id: sender.to_string(),
            chat_id: "chat1".to_string(),
            content: "hello".to_string(),
            media: None,
            metadata: None,
            timestamp: Utc::now(),
            trace_id: None,
        }
    }

    struct PassthroughHandler;

    #[async_trait]
    impl Handler<InboundMessage, InboundMessage> for PassthroughHandler {
        async fn handle(&self, request: InboundMessage) -> anyhow::Result<InboundMessage> {
            Ok(request)
        }
    }

    #[test]
    fn test_channel_error_retryable() {
        let err = ChannelError::RateLimited {
            channel: "telegram".to_string(),
        };
        assert!(err.is_retryable());
        assert_eq!(err.channel(), Some("telegram"));

        let err = ChannelError::NotConnected {
            channel: "discord".to_string(),
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_channel_error_display() {
        let err = ChannelError::AuthError {
            channel: "slack".to_string(),
            message: "invalid token".to_string(),
        };
        assert!(err.to_string().contains("slack"));
        assert!(err.to_string().contains("invalid token"));
    }

    #[tokio::test]
    async fn test_logging_middleware() {
        let mut stack = MiddlewareStack::new();
        stack.push(Arc::new(ChannelLoggingMiddleware));

        let handler = PassthroughHandler;
        let result = stack.execute(make_inbound("user1"), &handler).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().sender_id, "user1");
    }

    #[tokio::test]
    async fn test_auth_middleware_allow() {
        let auth = ChannelAuthMiddleware::new(vec!["user1".to_string(), "user2".to_string()]);
        let mut stack = MiddlewareStack::new();
        stack.push(Arc::new(auth));

        let handler = PassthroughHandler;
        let result = stack.execute(make_inbound("user1"), &handler).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_auth_middleware_reject() {
        let auth = ChannelAuthMiddleware::new(vec!["user1".to_string()]);
        let mut stack = MiddlewareStack::new();
        stack.push(Arc::new(auth));

        let handler = PassthroughHandler;
        let result = stack.execute(make_inbound("unknown"), &handler).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not authorized"));
    }

    #[tokio::test]
    async fn test_auth_middleware_empty_allows_all() {
        let auth = ChannelAuthMiddleware::new(Vec::<String>::new());
        let mut stack = MiddlewareStack::new();
        stack.push(Arc::new(auth));

        let handler = PassthroughHandler;
        let result = stack.execute(make_inbound("anyone"), &handler).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limit_middleware() {
        let rl = ChannelRateLimitMiddleware::new(2, std::time::Duration::from_secs(60));
        let rl = Arc::new(rl);
        let mut stack = MiddlewareStack::new();
        stack.push(rl);

        let handler = PassthroughHandler;

        // First two should pass
        let r1 = stack.execute(make_inbound("user1"), &handler).await;
        let r2 = stack.execute(make_inbound("user1"), &handler).await;
        assert!(r1.is_ok());
        assert!(r2.is_ok());

        // Third should be rate limited
        let r3 = stack.execute(make_inbound("user1"), &handler).await;
        assert!(r3.is_err());
        assert!(r3.unwrap_err().to_string().contains("Rate limit"));

        // Different sender should still pass
        let r4 = stack.execute(make_inbound("user2"), &handler).await;
        assert!(r4.is_ok());
    }
}
