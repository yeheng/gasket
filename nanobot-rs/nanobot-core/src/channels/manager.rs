//! Channel manager for coordinating multiple chat channels
//!
//! NOTE: The gateway currently bypasses `ChannelManager` entirely.
//! - Inbound: channels push to `InboundSender::raw_sender()` directly.
//! - Outbound: `send_outbound()` in `channels/mod.rs` handles stateless routing.
//!
//! This module is retained for potential future use (e.g., managed channel lifecycle).
//! The previous `send()` and `spawn_outbound_router()` outbound methods have been
//! removed to eliminate the duplicate routing path that `send_outbound()` already covers.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::base::Channel;
use super::middleware::{InboundSender, SimpleAuthChecker, SimpleRateLimiter};
use crate::bus::events::{ChannelType, InboundMessage};
use crate::bus::MessageBus;

/// Manager for coordinating multiple channels.
///
/// Handles channel lifecycle (register / start / stop) and inbound processing
/// with optional auth + rate-limit middleware.
///
/// **Outbound routing** is handled by [`super::send_outbound`], not this struct.
pub struct ChannelManager {
    channels: Arc<RwLock<HashMap<ChannelType, Box<dyn Channel>>>>,
    bus: Arc<MessageBus>,
    /// Optional rate limiter for inbound messages (shared with InboundSenders)
    rate_limiter: Option<Arc<SimpleRateLimiter>>,
    /// Optional auth checker for inbound messages (shared with InboundSenders)
    auth_checker: Option<Arc<SimpleAuthChecker>>,
}

impl ChannelManager {
    /// Create a new channel manager
    pub fn new(bus: Arc<MessageBus>) -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            bus,
            rate_limiter: None,
            auth_checker: None,
        }
    }

    /// Create a new channel manager with rate limiting
    pub fn with_rate_limit(mut self, max_messages: u32, window: std::time::Duration) -> Self {
        self.rate_limiter = Some(Arc::new(SimpleRateLimiter::new(max_messages, window)));
        self
    }

    /// Create a new channel manager with auth checking
    pub fn with_auth(mut self, allowed_senders: Vec<String>) -> Self {
        self.auth_checker = Some(Arc::new(SimpleAuthChecker::new(allowed_senders)));
        self
    }

    /// Register a channel
    pub async fn register(&self, channel_type: ChannelType, channel: Box<dyn Channel>) {
        let mut channels = self.channels.write().await;
        info!("Registering channel: {}", channel_type);
        channels.insert(channel_type, channel);
    }

    /// Start all registered channels
    pub async fn start_all(&self) -> Result<()> {
        let mut channels = self.channels.write().await;
        for (channel_type, channel) in channels.iter_mut() {
            info!("Starting channel: {}", channel_type);
            if let Err(e) = channel.start().await {
                warn!("Failed to start channel {}: {}", channel_type, e);
            }
        }
        Ok(())
    }

    /// Stop all channels
    pub async fn stop_all(&self) -> Result<()> {
        let mut channels = self.channels.write().await;
        for (channel_type, channel) in channels.iter_mut() {
            info!("Stopping channel: {}", channel_type);
            if let Err(e) = channel.stop().await {
                warn!("Failed to stop channel {}: {}", channel_type, e);
            }
        }
        Ok(())
    }

    /// Process an inbound message through simple checks, then publish to the bus.
    pub async fn process_inbound(&self, msg: InboundMessage) -> Result<()> {
        use super::middleware::log_inbound;
        log_inbound(&msg);

        if let Some(ref auth) = self.auth_checker {
            if !auth.check_and_log(&msg) {
                return Ok(());
            }
        }

        if let Some(ref rl) = self.rate_limiter {
            if !rl.check_and_log(&msg) {
                return Ok(());
            }
        }

        self.bus.publish_inbound(msg).await;
        Ok(())
    }

    /// Get a reference to the inner bus
    pub fn bus(&self) -> &Arc<MessageBus> {
        &self.bus
    }

    /// Get a cloneable sender for inbound messages.
    ///
    /// The returned `InboundSender` wraps the raw bus sender with the same
    /// auth and rate-limit middleware that `process_inbound` applies.
    pub fn inbound_sender(&self) -> InboundSender {
        let mut sender = InboundSender::new(self.bus.inbound_sender());
        if let Some(ref rl) = self.rate_limiter {
            sender = sender.with_rate_limiter(Arc::clone(rl));
        }
        if let Some(ref ac) = self.auth_checker {
            sender = sender.with_auth_checker(Arc::clone(ac));
        }
        sender
    }
}
