//! Channel configuration — re-exports from gasket-channels
//!
//! All channel configuration types (ChannelsConfig, TelegramConfig, etc.)
//! are canonically defined in `gasket-channels::config` and re-exported here
//! to maintain backward compatibility for `crate::config::channel::*` imports.

pub use gasket_channels::config::*;
