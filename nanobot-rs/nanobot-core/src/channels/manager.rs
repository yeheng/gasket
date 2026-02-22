//! Channel manager for coordinating multiple chat channels

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::base::Channel;
use super::middleware::{log_inbound, log_outbound, SimpleAuthChecker, SimpleRateLimiter};
use crate::bus::events::{ChannelType, InboundMessage, OutboundMessage};
use crate::bus::MessageBus;

/// Manager for coordinating multiple channels.
///
/// Owns the `MessageBus` and drives the outbound message routing loop.
/// Uses simple, direct method calls instead of over-engineered middleware stacks.
pub struct ChannelManager {
    channels: Arc<RwLock<HashMap<ChannelType, Box<dyn Channel>>>>,
    bus: Arc<MessageBus>,
    /// Optional rate limiter for inbound messages
    rate_limiter: Option<SimpleRateLimiter>,
    /// Optional auth checker for inbound messages
    auth_checker: Option<SimpleAuthChecker>,
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
        self.rate_limiter = Some(SimpleRateLimiter::new(max_messages, window));
        self
    }

    /// Create a new channel manager with auth checking
    pub fn with_auth(mut self, allowed_senders: Vec<String>) -> Self {
        self.auth_checker = Some(SimpleAuthChecker::new(allowed_senders));
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
        // We need write access to call start(&mut self) on each channel
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
        // Log the message
        log_inbound(&msg);

        // Check auth if configured
        if let Some(ref auth) = self.auth_checker {
            if !auth.check_and_log(&msg) {
                return Ok(()); // Silently drop unauthorized messages
            }
        }

        // Check rate limit if configured
        if let Some(ref rl) = self.rate_limiter {
            if !rl.check_and_log(&msg) {
                return Ok(()); // Silently drop rate-limited messages
            }
        }

        // Publish to bus
        self.bus.publish_inbound(msg).await;
        Ok(())
    }

    /// Send a message through a specific channel.
    pub async fn send(&self, msg: OutboundMessage) -> Result<()> {
        let channels = self.channels.read().await;
        if let Some(channel) = channels.get(&msg.channel) {
            // Log outbound message
            log_outbound(&msg.channel.to_string(), &msg.chat_id, msg.content.len());

            channel.send(msg).await?;
        } else {
            warn!(
                "No channel registered for type {:?}, dropping outbound message to {}",
                msg.channel, msg.chat_id
            );
        }
        Ok(())
    }

    /// Get a reference to the inner bus
    pub fn bus(&self) -> &Arc<MessageBus> {
        &self.bus
    }

    /// Get a cloneable sender for inbound messages.
    /// Channels can use this to send messages directly to the bus.
    pub fn inbound_sender(&self) -> Sender<InboundMessage> {
        self.bus.inbound_sender()
    }

    /// Spawn the outbound routing loop.
    ///
    /// Consumes `outbound_rx` and routes each message to the matching channel.
    /// Returns a `JoinHandle` so the caller can track the task.
    pub fn spawn_outbound_router(
        self: &Arc<Self>,
        mut outbound_rx: tokio::sync::mpsc::Receiver<OutboundMessage>,
    ) -> tokio::task::JoinHandle<()> {
        let mgr = self.clone();
        tokio::spawn(async move {
            while let Some(msg) = outbound_rx.recv().await {
                if let Err(e) = mgr.send(msg).await {
                    warn!("Outbound routing error: {}", e);
                }
            }
            info!("Outbound router exited");
        })
    }
}
