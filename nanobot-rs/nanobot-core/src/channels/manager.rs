//! Channel manager for coordinating multiple chat channels

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::base::Channel;
use crate::bus::events::{ChannelType, InboundMessage, OutboundMessage};
use crate::bus::MessageBus;
use crate::trail::{Handler, Middleware, MiddlewareStack};

/// Manager for coordinating multiple channels.
///
/// Owns the `MessageBus` and drives the outbound message routing loop.
/// Supports inbound and outbound middleware stacks.
pub struct ChannelManager {
    channels: Arc<RwLock<HashMap<ChannelType, Box<dyn Channel>>>>,
    bus: Arc<MessageBus>,
    inbound_middleware: MiddlewareStack<InboundMessage, InboundMessage>,
    outbound_middleware: MiddlewareStack<OutboundMessage, ()>,
}

impl ChannelManager {
    /// Create a new channel manager
    pub fn new(bus: Arc<MessageBus>) -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            bus,
            inbound_middleware: MiddlewareStack::new(),
            outbound_middleware: MiddlewareStack::new(),
        }
    }

    /// Add an inbound middleware (applied to messages received from channels).
    pub fn add_inbound_middleware(
        &mut self,
        mw: Arc<dyn Middleware<InboundMessage, InboundMessage> + Send + Sync>,
    ) {
        self.inbound_middleware.push(mw);
    }

    /// Add an outbound middleware (applied to messages sent to channels).
    pub fn add_outbound_middleware(
        &mut self,
        mw: Arc<dyn Middleware<OutboundMessage, ()> + Send + Sync>,
    ) {
        self.outbound_middleware.push(mw);
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

    /// Process an inbound message through the middleware stack, then publish to the bus.
    pub async fn process_inbound(&self, msg: InboundMessage) -> Result<()> {
        let handler = InboundPassthroughHandler;
        let processed = self.inbound_middleware.execute(msg, &handler).await?;
        self.bus.publish_inbound(processed).await;
        Ok(())
    }

    /// Send a message through a specific channel (with outbound middleware).
    pub async fn send(&self, msg: OutboundMessage) -> Result<()> {
        let channels = self.channels.read().await;
        if let Some(channel) = channels.get(&msg.channel) {
            let handler = OutboundChannelHandler {
                channel: channel.as_ref(),
            };
            self.outbound_middleware
                .execute(msg, &handler)
                .await?;
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

// ── Internal handlers ───────────────────────────────────

/// Handler that passes inbound messages through unchanged.
struct InboundPassthroughHandler;

#[async_trait::async_trait]
impl Handler<InboundMessage, InboundMessage> for InboundPassthroughHandler {
    async fn handle(&self, request: InboundMessage) -> Result<InboundMessage> {
        Ok(request)
    }
}

/// Handler that sends outbound messages to the actual channel.
struct OutboundChannelHandler<'a> {
    channel: &'a dyn Channel,
}

#[async_trait::async_trait]
impl<'a> Handler<OutboundMessage, ()> for OutboundChannelHandler<'a> {
    async fn handle(&self, request: OutboundMessage) -> Result<()> {
        self.channel.send(request).await
    }
}
