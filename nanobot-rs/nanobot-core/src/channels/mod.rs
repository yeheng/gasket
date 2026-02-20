//! Channel integrations

pub mod base;
pub mod manager;
pub mod middleware;

#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "discord")]
pub mod discord;

#[cfg(feature = "slack")]
pub mod slack;

#[cfg(feature = "email")]
pub mod email;

#[cfg(feature = "feishu")]
pub mod feishu;

pub use base::{Channel, MessageContext};
pub use manager::ChannelManager;
pub use middleware::{
    ChannelAuthMiddleware, ChannelError, ChannelLoggingMiddleware,
    ChannelOutboundLoggingMiddleware, ChannelRateLimitMiddleware,
};
