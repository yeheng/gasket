//! VaultStore - Secure storage for sensitive data
//!
//! Provides encrypted storage for secrets using XChaCha20-Poly1305.
//!
//! # Storage Format (v2)
//! ```json
//! {
//!   "version": 2,
//!   "kdf": {
//!     "algorithm": "argon2id",
//!     "salt": "<base64-32-bytes>",
//!     "memory_cost": 65536,
//!     "time_cost": 3,
//!     "parallelism": 4
//!   },
//!   "entries": {
//!     "api_key": {
//!       "key": "api_key",
//!       "encrypted_value": "<base64-ciphertext>",
//!       "nonce": "<base64-24-bytes>",
//!       "description": "Description",
//!       "created_at": "...",
//!       "last_used": null
//!     }
//!   }
//! }
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::crypto::{KdfParams, VaultCrypto};
use super::VaultError;

/// Current vault format version
const VAULT_VERSION: u32 = 2;

// ============================================================================
// Data Structures
// ============================================================================

/// Metadata for a vault entry (excludes the value)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultMetadata {
    pub key: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
}

/// An encrypted vault entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultEntryV2 {
    /// Unique identifier
    pub key: String,
    /// Encrypted value (base64 encoded)
    pub encrypted_value: String,
    /// Nonce for decryption (base64 encoded, 24 bytes)
    pub nonce: String,
    /// Human-readable description
    pub description: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last usage timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<DateTime<Utc>>,
}

impl From<&VaultEntryV2> for VaultMetadata {
    fn from(entry: &VaultEntryV2) -> Self {
        Self {
            key: entry.key.clone(),
            description: entry.description.clone(),
            created_at: entry.created_at,
            last_used: entry.last_used,
        }
    }
}

/// Complete vault file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultFileV2 {
    version: u32,
    kdf: KdfParams,
    pub entries: HashMap<String, VaultEntryV2>,
}

// ============================================================================
// VaultStore
// ============================================================================

/// Vault storage for sensitive data
pub struct VaultStore {
    /// Path to the storage file
    path: PathBuf,
    /// In-memory entries
    entries: HashMap<String, VaultEntryV2>,
    /// Crypto instance (None = locked, Some = unlocked)
    crypto: Option<VaultCrypto>,
    /// KDF parameters (stored for future unlock operations)
    kdf_params: Option<KdfParams>,
}

impl VaultStore {
    /// Default storage path: ~/.nanobot/vault/secrets.json
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .expect("Could not find home directory")
            .join(".nanobot")
            .join("vault")
            .join("secrets.json")
    }

    /// Create a new VaultStore with default path
    pub fn new() -> Result<Self, VaultError> {
        Self::with_path(Self::default_path())
    }

    /// Create a VaultStore with a custom path
    pub fn with_path(path: PathBuf) -> Result<Self, VaultError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut store = Self {
            path,
            entries: HashMap::new(),
            crypto: None,
            kdf_params: None,
        };

        store.load()?;
        Ok(store)
    }

    /// Create an in-memory VaultStore (for testing)
    pub fn new_in_memory() -> Self {
        Self {
            path: PathBuf::from(":memory:"),
            entries: HashMap::new(),
            crypto: None,
            kdf_params: None,
        }
    }

    /// Check if the vault is locked
    pub fn is_locked(&self) -> bool {
        self.crypto.is_none()
    }

    /// Unlock the vault with a password
    pub fn unlock(&mut self, password: &str) -> Result<(), VaultError> {
        match &self.kdf_params {
            Some(params) => {
                let crypto = VaultCrypto::derive_key(password, params)?;
                self.crypto = Some(crypto);
                debug!("[Vault] Vault unlocked successfully");
                Ok(())
            }
            None => {
                // New vault - initialize with new KDF params
                let params = KdfParams::default();
                let crypto = VaultCrypto::derive_key(password, &params)?;
                self.crypto = Some(crypto);
                self.persist_header(&params)?;
                self.kdf_params = Some(params);
                debug!("[Vault] Initialized new encrypted vault");
                Ok(())
            }
        }
    }

    /// Get a sensitive value by key
    ///
    /// This is a read-only operation that does NOT update `last_used`.
    /// If you need to track usage, use the `show` CLI command instead.
    /// This design avoids requiring `&mut self` which would force
    /// all concurrent readers to serialize on a global lock.
    ///
    /// # Error Logging
    ///
    /// Returns `None` and logs appropriate warnings for different failure cases:
    /// - Vault locked: debug log (expected when vault hasn't been unlocked)
    /// - Key not found: no log (caller handles this via missing_keys)
    /// - Decryption failed: warning log (indicates data corruption or wrong password)
    pub fn get(&self, key: &str) -> Option<String> {
        debug!("[Vault] Getting value for key: {}", key);

        if self.is_locked() {
            debug!("[Vault] Vault is locked, cannot get key: {}", key);
            return None;
        }

        let crypto = self.crypto.as_ref()?;
        let entry = match self.entries.get(key) {
            Some(e) => e,
            None => {
                // Key not found - caller will handle via missing_keys
                return None;
            }
        };

        debug!("[Vault] Found entry for key: {}", key);

        match crypto.decrypt_from_base64(&entry.encrypted_value, &entry.nonce) {
            Ok(decrypted) => {
                // NOTE: We intentionally do NOT update last_used here.
                // Updating would require &mut self, forcing Arc<Mutex<>> which
                // creates a global serialization bottleneck for all concurrent sessions.
                // Usage tracking is a "nice to have" that doesn't justify this cost.
                Some(decrypted)
            }
            Err(e) => {
                // Decryption failed - this is a serious issue
                warn!(
                    "[Vault] Decryption failed for key '{}': {}. This may indicate data corruption or wrong password.",
                    key, e
                );
                None
            }
        }
    }

    /// Set a sensitive value
    pub fn set(
        &mut self,
        key: &str,
        value: &str,
        description: Option<&str>,
    ) -> Result<(), VaultError> {
        Self::validate_key(key)?;

        if self.is_locked() {
            return Err(VaultError::Locked);
        }

        let crypto = self.crypto.as_ref().ok_or(VaultError::Locked)?;
        let (encrypted_value, nonce) = crypto.encrypt_to_base64(value)?;

        let created_at = self
            .entries
            .get(key)
            .map(|e| e.created_at)
            .unwrap_or_else(Utc::now);

        let entry = VaultEntryV2 {
            key: key.to_string(),
            encrypted_value,
            nonce,
            description: description.map(|s| s.to_string()),
            created_at,
            last_used: None,
        };

        self.entries.insert(key.to_string(), entry);
        self.persist()?;

        debug!("[Vault] Set entry: {}", key);
        Ok(())
    }

    /// List all keys with metadata
    pub fn list_keys(&self) -> Vec<VaultMetadata> {
        self.entries.values().map(VaultMetadata::from).collect()
    }

    /// Delete an entry by key
    pub fn delete(&mut self, key: &str) -> Result<bool, VaultError> {
        let deleted = self.entries.remove(key).is_some();

        if deleted {
            self.persist()?;
            debug!("[Vault] Deleted entry: {}", key);
        }

        Ok(deleted)
    }

    /// Check if a key exists
    pub fn exists(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the vault is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get the vault format version
    pub fn version(&self) -> u32 {
        if self.kdf_params.is_some() {
            2
        } else {
            0
        }
    }

    // ========================================================================
    // Private methods
    // ========================================================================

    fn load(&mut self) -> Result<(), VaultError> {
        if self.path.to_str() == Some(":memory:") || !self.path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&self.path)?;
        if content.trim().is_empty() {
            return Ok(());
        }

        let file: VaultFileV2 =
            serde_json::from_str(&content).map_err(|e| VaultError::Io(std::io::Error::other(e)))?;

        if file.version != VAULT_VERSION {
            warn!(
                "[Vault] Unsupported version: {}, expected {}",
                file.version, VAULT_VERSION
            );
            return Err(VaultError::Io(std::io::Error::other(format!(
                "Unsupported vault version: {}",
                file.version
            ))));
        }

        let entry_count = file.entries.len();
        self.entries = file.entries;
        self.kdf_params = Some(file.kdf);
        debug!("[Vault] Loaded {} encrypted entries", entry_count);
        Ok(())
    }

    fn persist(&self) -> Result<(), VaultError> {
        if self.path.to_str() == Some(":memory:") {
            return Ok(());
        }

        let kdf_params = self
            .kdf_params
            .clone()
            .ok_or_else(|| VaultError::Migration("KDF params not set".to_string()))?;

        let file = VaultFileV2 {
            version: VAULT_VERSION,
            kdf: kdf_params,
            entries: self.entries.clone(),
        };

        let content = serde_json::to_string_pretty(&file)?;
        std::fs::write(&self.path, content)?;
        Ok(())
    }

    fn persist_header(&self, kdf_params: &KdfParams) -> Result<(), VaultError> {
        if self.path.to_str() == Some(":memory:") {
            return Ok(());
        }

        let file = VaultFileV2 {
            version: VAULT_VERSION,
            kdf: kdf_params.clone(),
            entries: HashMap::new(),
        };

        let content = serde_json::to_string_pretty(&file)?;
        std::fs::write(&self.path, content)?;
        Ok(())
    }

    // NOTE: update_last_used has been intentionally removed.
    // It was only used to update a "last_used" timestamp on get(),
    // but this required &mut self which created lock contention.
    // The feature is not worth the performance cost.
    // If usage tracking is needed, consider:
    // - Using async background tasks to batch update
    // - Only tracking when explicitly listing keys
    // - Using a separate analytics store that doesn't block reads

    fn validate_key(key: &str) -> Result<(), VaultError> {
        if key.is_empty() {
            return Err(VaultError::InvalidKey("Key cannot be empty".to_string()));
        }
        if !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(VaultError::InvalidKey(
                "Key must contain only alphanumeric characters and underscores".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unlock_and_encrypt_decrypt() {
        let mut store = VaultStore::new_in_memory();
        assert!(store.is_locked());
        store.unlock("test-password").unwrap();
        assert!(!store.is_locked());
        store
            .set("api_key", "secret123", Some("Test API key"))
            .unwrap();
        assert_eq!(store.get("api_key"), Some("secret123".to_string()));
    }

    #[test]
    fn test_locked_vault_rejects_set() {
        let mut store = VaultStore::new_in_memory();
        let result = store.set("key", "value", None);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), VaultError::Locked));
    }

    #[test]
    fn test_locked_vault_returns_none_on_get() {
        let store = VaultStore::new_in_memory();
        assert!(store.get("key").is_none());
    }

    #[test]
    fn test_delete() {
        let mut store = VaultStore::new_in_memory();
        store.unlock("password").unwrap();
        store.set("key", "value", None).unwrap();
        assert!(store.delete("key").unwrap());
        assert!(!store.exists("key"));
    }

    #[test]
    fn test_list_keys() {
        let mut store = VaultStore::new_in_memory();
        store.unlock("password").unwrap();
        store.set("key1", "secret1", Some("Description 1")).unwrap();
        store.set("key2", "secret2", None).unwrap();
        let keys = store.list_keys();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn test_invalid_key() {
        let mut store = VaultStore::new_in_memory();
        store.unlock("password").unwrap();
        assert!(store.set("", "value", None).is_err());
        assert!(store.set("key-with-dash", "value", None).is_err());
        assert!(store.set("api_key_v2", "value", None).is_ok());
    }

    #[test]
    fn test_update_preserves_created_at() {
        let mut store = VaultStore::new_in_memory();
        store.unlock("password").unwrap();
        store.set("key", "value1", None).unwrap();

        let original = store
            .list_keys()
            .into_iter()
            .find(|m| m.key == "key")
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.set("key", "value2", Some("Updated")).unwrap();

        let updated = store
            .list_keys()
            .into_iter()
            .find(|m| m.key == "key")
            .unwrap();
        assert_eq!(original.created_at, updated.created_at);
        assert_eq!(updated.description, Some("Updated".to_string()));
    }

    #[test]
    fn test_version() {
        let store = VaultStore::new_in_memory();
        assert_eq!(store.version(), 0);

        let mut store = VaultStore::new_in_memory();
        store.unlock("password").unwrap();
        assert_eq!(store.version(), 2);
    }

    #[test]
    fn test_last_used_not_updated_on_get() {
        // NOTE: last_used is NO LONGER updated on get() for concurrency reasons.
        // Updating last_used would require &mut self, which forces Arc<Mutex<>>.
        // This test verifies that get() is read-only (no last_used update).
        let mut store = VaultStore::new_in_memory();
        store.unlock("password").unwrap();
        store.set("key", "value", None).unwrap();

        let before = store
            .list_keys()
            .into_iter()
            .find(|m| m.key == "key")
            .unwrap();
        assert!(before.last_used.is_none());

        // get() is now read-only, so last_used should NOT change
        let _ = store.get("key");

        let after = store
            .list_keys()
            .into_iter()
            .find(|m| m.key == "key")
            .unwrap();
        // last_used should still be None because get() doesn't update it anymore
        assert!(after.last_used.is_none());
    }

    #[test]
    fn test_multiple_entries() {
        let mut store = VaultStore::new_in_memory();
        store.unlock("password").unwrap();
        store.set("key1", "value1", None).unwrap();
        store.set("key2", "value2", None).unwrap();
        store.set("key3", "value3", None).unwrap();

        assert_eq!(store.len(), 3);
        assert_eq!(store.get("key1"), Some("value1".to_string()));
        assert_eq!(store.get("key2"), Some("value2".to_string()));
        assert_eq!(store.get("key3"), Some("value3".to_string()));

        store.delete("key2").unwrap();
        assert_eq!(store.len(), 2);
        assert!(store.get("key2").is_none());
    }

    #[test]
    fn test_unicode_value() {
        let mut store = VaultStore::new_in_memory();
        store.unlock("password").unwrap();
        let value = "密码 🔐 パスワード";
        store.set("key", value, None).unwrap();
        assert_eq!(store.get("key"), Some(value.to_string()));
    }

    #[test]
    fn test_long_value() {
        let mut store = VaultStore::new_in_memory();
        store.unlock("password").unwrap();
        let long_value = "x".repeat(10000);
        store.set("key", &long_value, None).unwrap();
        assert_eq!(store.get("key"), Some(long_value));
    }
}
