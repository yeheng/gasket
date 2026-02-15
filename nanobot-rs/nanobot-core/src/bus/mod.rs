//! Message bus for inter-component communication

pub mod events;
pub mod queue;

pub use events::{InboundMessage, OutboundMessage};
pub use queue::MessageBus;
