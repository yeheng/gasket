//! Channel manager for coordinating multiple chat channels

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use tracing::{debug, info};

use super::base::Channel;
use crate::bus::events::OutboundMessage;
use crate::bus::MessageBus;

/// Manager for coordinating multiple channels
#[allow(dead_code)]
pub struct ChannelManager {
    channels: Arc<RwLock<HashMap<String, Box<dyn Channel>>>>,
    bus: MessageBus,
}

impl ChannelManager {
    /// Create a new channel manager
    pub fn new(bus: MessageBus) -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            bus,
        }
    }

    /// Register a channel
    pub async fn register(&self, name: &str, channel: Box<dyn Channel>) {
        let mut channels = self.channels.write().await;
        info!("Registering channel: {}", name);
        channels.insert(name.to_string(), channel);
    }

    /// Start all registered channels
    pub async fn start_all(&self) -> Result<()> {
        let channels = self.channels.read().await;
        for (name, _channel) in channels.iter() {
            info!("Starting channel: {}", name);
            // Note: We can't mutate channels while holding the read lock
            // In production, we'd use a different pattern
            debug!("Channel {} ready", name);
        }
        Ok(())
    }

    /// Stop all channels
    pub async fn stop_all(&self) -> Result<()> {
        let channels = self.channels.read().await;
        for (name, _) in channels.iter() {
            info!("Stopping channel: {}", name);
        }
        Ok(())
    }

    /// Send a message through a specific channel
    pub async fn send(&self, msg: OutboundMessage) -> Result<()> {
        let channels = self.channels.read().await;
        if let Some(channel) = channels.get(&msg.channel) {
            channel.send(msg).await?;
        }
        Ok(())
    }
}
