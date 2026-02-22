//! Telegram channel implementation using teloxide

use std::sync::Arc;

use async_trait::async_trait;
use teloxide::prelude::*;
use teloxide::types::ChatId;
use tracing::{debug, info, instrument};

use super::base::Channel;
use super::middleware::InboundProcessor;
use crate::bus::events::{InboundMessage, OutboundMessage};
use crate::bus::ChannelType;

/// Telegram channel configuration
#[derive(Debug, Clone)]
pub struct TelegramConfig {
    pub token: String,
    pub allow_from: Vec<String>,
}

/// Telegram channel with middleware support.
///
/// Uses `InboundProcessor` to process incoming messages through
/// the middleware stack before publishing to the bus.
pub struct TelegramChannel {
    config: TelegramConfig,
    inbound_processor: Arc<dyn InboundProcessor>,
}

impl TelegramChannel {
    /// Create a new Telegram channel with an inbound processor.
    pub fn new(config: TelegramConfig, inbound_processor: Arc<dyn InboundProcessor>) -> Self {
        Self {
            config,
            inbound_processor,
        }
    }

    /// Start the Telegram bot (blocking)
    #[instrument(name = "channel.telegram.start", skip_all)]
    pub async fn start(self) -> anyhow::Result<()> {
        info!("Starting Telegram bot");

        let bot = Bot::new(&self.config.token);
        let inbound_processor = self.inbound_processor.clone();
        let allow_from = self.config.allow_from.clone();

        // Use Dispatcher for proper handling
        let handler = Update::filter_message().branch(dptree::endpoint(move |msg: Message| {
            let inbound_processor = inbound_processor.clone();
            let allow_from = allow_from.clone();
            async move {
                if let Some(ref user) = msg.from {
                    let user_id = user.id.0;
                    let user_id_str = user_id.to_string();

                    // Check allowlist
                    if !allow_from.is_empty() && !allow_from.contains(&user_id_str) {
                        debug!("Ignoring message from unauthorized user: {}", user_id);
                        return Ok::<_, teloxide::RequestError>(());
                    }

                    if let Some(text) = msg.text() {
                        let chat_id = msg.chat.id.0;

                        debug!("Received message from {}: {}", user_id, text);

                        let inbound = InboundMessage {
                            channel: ChannelType::Telegram,
                            sender_id: user_id_str,
                            chat_id: chat_id.to_string(),
                            content: text.to_string(),
                            media: None,
                            metadata: None,
                            timestamp: chrono::Utc::now(),
                            trace_id: None,
                        };

                        if let Err(e) = inbound_processor.process(inbound).await {
                            debug!("Failed to process inbound message: {}", e);
                        }
                    }
                }
                Ok(())
            }
        }));

        Dispatcher::builder(bot, handler)
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;

        Ok(())
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn name(&self) -> &str {
        "telegram"
    }

    async fn start(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        info!("Stopping Telegram channel");
        Ok(())
    }

    #[instrument(name = "channel.telegram.send", skip_all, fields(chat_id = %msg.chat_id))]
    async fn send(&self, msg: OutboundMessage) -> anyhow::Result<()> {
        let bot = Bot::new(&self.config.token);
        let chat_id: i64 = msg.chat_id.parse()?;
        bot.send_message(ChatId(chat_id), &msg.content).await?;
        Ok(())
    }
}
