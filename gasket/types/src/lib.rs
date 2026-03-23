//! Shared types and events for gasket.
//!
//! This crate provides the core data types used across all gasket components:
//! - Message types (InboundMessage, OutboundMessage)
//! - Channel identifiers (ChannelType, SessionKey)
//! - WebSocket streaming messages
//!
//! By keeping these types in a separate crate, we avoid circular dependencies
//! between `gasket-core` and `gasket-channels`.

pub mod events;

pub use events::{
    ChannelType, InboundMessage, MediaAttachment, OutboundMessage, SessionKey,
    SessionKeyParseError, WebSocketMessage,
};
