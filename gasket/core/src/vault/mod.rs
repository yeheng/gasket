//! Vault system
//!
//! This module re-exports types from the `gasket-vault` and `gasket-engine` vault modules.

pub use gasket_engine::{InjectionReport, VaultInjector};
pub use gasket_vault::{
    contains_placeholders, contains_secrets, extract_keys, redact_message_secrets, redact_secrets,
    replace_placeholders, scan_placeholders, AtomicTimestamp, EncryptedData, KdfParams,
    Placeholder, VaultCrypto, VaultEntryV2, VaultError, VaultFileV2, VaultMetadata, VaultStore,
};
