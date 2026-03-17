//! Message events — re-exports from gasket-channels
//!
//! All event types (ChannelType, SessionKey, InboundMessage, OutboundMessage, etc.)
//! are canonically defined in `gasket-channels::events` and re-exported here
//! to maintain backward compatibility for `crate::bus::events::*` imports.

pub use gasket_channels::events::*;
