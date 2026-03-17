//! Messaging channel abstractions and implementations for gasket.
//!
//! This crate provides:
//! - Core channel types (`events`, `config`, `base`, `middleware`, `outbound`)
//! - Feature-gated channel implementations (Telegram, Discord, Slack, etc.)
//! - WeCom crypto utilities (feature-gated)

// Core types (always compiled)
pub mod base;
pub mod config;
pub mod error;
pub mod events;
pub mod middleware;
pub mod outbound;

// WeCom crypto (feature-gated)
#[cfg(feature = "wecom")]
pub mod crypto_wecom;

// Channel implementations (feature-gated)
#[cfg(feature = "dingtalk")]
pub mod dingtalk;
#[cfg(feature = "discord")]
pub mod discord;
#[cfg(feature = "email")]
pub mod email;
#[cfg(feature = "feishu")]
pub mod feishu;
#[cfg(feature = "slack")]
pub mod slack;
#[cfg(feature = "telegram")]
pub mod telegram;
#[cfg(feature = "webhook")]
pub mod websocket;
#[cfg(feature = "wecom")]
pub mod wecom;

// Convenience re-exports
pub use base::Channel;
pub use config::{
    ChannelsConfig, DingTalkConfig, DiscordConfig, EmailConfig, FeishuConfig, SlackConfig,
    TelegramConfig,
};
pub use error::ChannelConfigError;
pub use events::{
    ChannelType, InboundMessage, MediaAttachment, OutboundMessage, SessionKey,
    SessionKeyParseError, WebSocketMessage,
};
pub use middleware::{
    log_inbound, ChannelError, InboundSender, SimpleAuthChecker, SimpleRateLimiter,
};
pub use outbound::{OutboundSender, OutboundSenderRegistry};
