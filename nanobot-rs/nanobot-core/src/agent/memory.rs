//! Memory store for long-term context
//!
//! This module wraps `SqliteStore` to provide the high-level
//! `read_long_term` and `write_long_term` API used by the agent loop.

use anyhow::Result;
use tracing::instrument;

use crate::memory::{MemoryEntry, MemoryQuery, MemoryStore as MemoryStoreTrait, SqliteStore};

/// Memory store for long-term context.
///
/// Backed by `SqliteStore` for all operations:
/// - Structured memories (save/get/delete/search)
pub struct MemoryStore {
    store: SqliteStore,
}

impl MemoryStore {
    /// Create a new memory store.
    ///
    /// Opens the default `SqliteStore` at `~/.nanobot/nanobot.db`.
    pub async fn new() -> Self {
        let store = SqliteStore::new()
            .await
            .expect("Failed to open SqliteStore");

        Self { store }
    }

    /// Create a memory store with a specific `SqliteStore` instance.
    pub fn with_store(store: SqliteStore) -> Self {
        Self { store }
    }

    /// Get a reference to the underlying `SqliteStore`.
    pub fn sqlite_store(&self) -> &SqliteStore {
        &self.store
    }

    // ── Structured memory API ──

    /// Save a structured memory entry.
    #[instrument(name = "memory.save", skip_all, fields(id = %entry.id))]
    pub async fn save(&self, entry: &MemoryEntry) -> Result<()> {
        self.store.save(entry).await
    }

    /// Retrieve a structured memory entry by id.
    #[instrument(name = "memory.get", skip_all)]
    pub async fn get(&self, id: &str) -> Result<Option<MemoryEntry>> {
        self.store.get(id).await
    }

    /// Delete a structured memory entry by id.
    #[instrument(name = "memory.delete", skip_all)]
    pub async fn delete(&self, id: &str) -> Result<bool> {
        self.store.delete(id).await
    }

    /// Search structured memories.
    #[instrument(name = "memory.search", skip_all)]
    pub async fn search(&self, query: &MemoryQuery) -> Result<Vec<MemoryEntry>> {
        self.store.search(query).await
    }
}
