//! Vault: Sensitive data isolation module
//!
//! Provides secure storage and placeholder scanning for sensitive data.
//!
//! # Design Principles
//!
//! 1. **Data Structure Isolation**: VaultStore is completely separated from memory/history
//! 2. **Runtime Injection**: Secrets are only injected at the last moment before sending to LLM
//! 3. **Zero Trust**: Sensitive data never persists to LLM-accessible storage
//!
//! # Usage
//!
//! ```ignore
//! use gasket_vault::VaultStore;
//! use std::sync::Arc;
//!
//! // Create store
//! let store = Arc::new(VaultStore::new()?);
//! store.set("api_key", "sk-12345", Some("OpenAI API key"))?;
//!
//! // Get value
//! let value = store.get("api_key")?;
//! ```
//!
//! # Placeholder Format
//!
//! Use `{{vault:key_name}}` in your messages:
//!
//! ```text
//! "Connect to database with {{vault:db_password}}"
//! "API key: {{vault:openai_api_key}}"
//! "AWS credentials: {{vault:aws_access_key}} {{vault:aws_secret_key}}"
//! ```

mod crypto;
mod error;
mod redaction;
mod scanner;
mod store;

pub use crypto::{EncryptedData, KdfParams, VaultCrypto};
pub use error::VaultError;
pub use redaction::{contains_secrets, redact_message_secrets, redact_secrets};
pub use scanner::{
    contains_placeholders, extract_keys, replace_placeholders, scan_placeholders, Placeholder,
};
pub use store::{AtomicTimestamp, VaultEntryV2, VaultFileV2, VaultMetadata, VaultStore};
