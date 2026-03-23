//! Message events — re-exports from gasket-types
//!
//! All event types (ChannelType, SessionKey, InboundMessage, OutboundMessage, etc.)
//! are canonically defined in `gasket-types::events` and re-exported here
//! to maintain backward compatibility for `crate::events::*` imports.

pub use gasket_types::events::*;
