//! Configuration management
//!
//! Compatible with Python nanobot config format (`~/.nanobot/config.yaml`)
//!
//! ## Module Structure
//!
//! - [`schema`] - Root configuration and re-exports
//! - [`loader`] - Configuration loading from files
//! - [`resolver`] - Vault placeholder resolution
//! - [`provider`] - LLM provider configuration (OpenAI, Anthropic, etc.)
//! - [`agent`] - Agent default settings
//! - [`channel`] - Messaging channels (Telegram, Discord, Slack, etc.)
//! - [`tools`] - Tool configuration (Web, MCP, Exec)

mod agent;
mod channel;
mod loader;
mod provider;
mod resolver;
mod schema;
mod tools;

pub use loader::{config_dir, config_path, load_config, ConfigLoader};
pub use provider::ProviderConfig;
pub use resolver::{resolve_string_placeholders, VaultPlaceholderResolve, VAULT_PASSWORD_ENV};
pub use schema::*;
