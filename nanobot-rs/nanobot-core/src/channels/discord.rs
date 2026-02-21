//! Discord channel implementation using serenity

use std::sync::Arc;

use async_trait::async_trait;
use serenity::all::{GatewayIntents, Message as DiscordMessage};
use serenity::prelude::*;
use tracing::{debug, info};

use super::base::Channel;
use super::middleware::InboundProcessor;
use crate::bus::events::{InboundMessage, OutboundMessage};
use crate::bus::ChannelType;
use crate::trail::TrailContext;

/// Discord channel configuration
#[derive(Debug, Clone)]
pub struct DiscordConfig {
    pub token: String,
    pub allow_from: Vec<String>,
}

/// Discord channel with middleware support.
///
/// Uses `InboundProcessor` to process incoming messages through
/// the middleware stack before publishing to the bus.
pub struct DiscordChannel {
    config: DiscordConfig,
    inbound_processor: Arc<dyn InboundProcessor>,
    trail_ctx: TrailContext,
}

impl DiscordChannel {
    /// Create a new Discord channel with an inbound processor.
    pub fn new(config: DiscordConfig, inbound_processor: Arc<dyn InboundProcessor>) -> Self {
        Self {
            config,
            inbound_processor,
            trail_ctx: TrailContext::default(),
        }
    }

    /// Set the trail context for this channel.
    pub fn with_trail_context(mut self, ctx: TrailContext) -> Self {
        self.trail_ctx = ctx;
        self
    }

    /// Start the Discord bot
    pub async fn start_bot(&self) -> anyhow::Result<()> {
        info!("Starting Discord bot");

        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;

        let token = self.config.token.clone();
        let inbound_processor = self.inbound_processor.clone();
        let allow_from = self.config.allow_from.clone();

        let handler = DiscordHandler {
            inbound_processor,
            allow_from,
        };

        let mut client = Client::builder(&token, intents)
            .event_handler(handler)
            .await?;

        client.start().await?;

        Ok(())
    }
}

#[async_trait]
impl Channel for DiscordChannel {
    fn name(&self) -> &str {
        "discord"
    }

    async fn start(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        info!("Stopping Discord channel");
        Ok(())
    }

    async fn send(&self, _msg: OutboundMessage) -> anyhow::Result<()> {
        // Note: Sending requires the client instance, handled differently
        Ok(())
    }
}

/// Discord event handler
struct DiscordHandler {
    inbound_processor: Arc<dyn InboundProcessor>,
    allow_from: Vec<String>,
}

#[serenity::async_trait]
impl EventHandler for DiscordHandler {
    async fn message(&self, _ctx: Context, msg: DiscordMessage) {
        // Ignore bot messages
        if msg.author.bot {
            return;
        }

        let user_id = msg.author.id.to_string();

        // Check allowlist
        if !self.allow_from.is_empty() && !self.allow_from.contains(&user_id) {
            debug!("Ignoring message from unauthorized user: {}", user_id);
            return;
        }

        debug!("Received message from {}: {}", user_id, msg.content);

        // Get current tracing context
        let ctx = crate::trail::TrailContext::current();

        let inbound = InboundMessage {
            channel: ChannelType::Discord,
            sender_id: user_id,
            chat_id: msg.channel_id.to_string(),
            content: msg.content.clone(),
            media: None,
            metadata: None,
            timestamp: chrono::Utc::now(),
            trace_id: ctx.trace_id(),
        };

        if let Err(e) = self.inbound_processor.process(inbound).await {
            debug!("Failed to process inbound message: {}", e);
        }
    }

    async fn ready(&self, _ctx: Context, ready: serenity::model::gateway::Ready) {
        info!("Discord bot ready: {}", ready.user.name);
    }
}
