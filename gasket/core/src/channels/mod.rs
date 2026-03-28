//! Channel system
//!
//! This module re-exports types from the `gasket-channels` crate.

pub use gasket_channels::{
    base, log_inbound, middleware, outbound, Channel, ChannelConfigError, ChannelType,
    ChannelsConfig, DingTalkConfig, DiscordConfig, EmailConfig, FeishuConfig, InboundMessage,
    InboundSender, MediaAttachment, OutboundMessage, OutboundSender, OutboundSenderRegistry,
    SessionKey, SessionKeyParseError, SimpleAuthChecker, SimpleRateLimiter, SlackConfig,
    TelegramConfig, WebSocketMessage,
};

#[cfg(any(
    feature = "webhook",
    feature = "dingtalk",
    feature = "feishu",
    feature = "wecom"
))]
pub use gasket_channels::webhook;
