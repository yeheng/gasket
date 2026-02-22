//! Memory store for long-term context
//!
//! This module wraps the `MemoryStore` trait to provide the high-level
//! `read_long_term`, `write_long_term`, `read_history`, and `append_history`
//! API used by the agent loop.
//!
//! When the `sqlite` feature is enabled, the default backend is `SqliteStore`.
//! Otherwise it falls back to `FileMemoryStore`.

use std::path::PathBuf;

use anyhow::Result;
use tracing::instrument;

use crate::memory::{
    FileMemoryStore, MemoryEntry, MemoryQuery,
    MemoryStore as MemoryStoreTrait,
};

/// Memory store for long-term context.
///
/// Uses `SqliteStore` by default (when the `sqlite` feature is enabled),
/// falling back to `FileMemoryStore` otherwise. Also keeps a `FileMemoryStore`
/// for legacy raw-file operations (MEMORY.md, HISTORY.md).
pub struct MemoryStore {
    store: Box<dyn MemoryStoreTrait>,
    file_store: FileMemoryStore,
}

impl MemoryStore {
    /// Create a new memory store backed by the workspace directory.
    ///
    /// With the `sqlite` feature enabled, structured memory operations go
    /// through `SqliteStore` (persisted at `~/.nanobot/memory.db`).
    /// Legacy file operations always use `FileMemoryStore`.
    pub fn new(workspace: PathBuf) -> Self {
        let file_store = FileMemoryStore::new(workspace);

        #[cfg(feature = "sqlite")]
        let store: Box<dyn MemoryStoreTrait> = match crate::memory::SqliteStore::new() {
            Ok(s) => Box::new(s),
            Err(e) => {
                tracing::warn!("Failed to open SqliteStore, falling back to FileMemoryStore: {}", e);
                Box::new(FileMemoryStore::new(file_store.workspace().to_path_buf()))
            }
        };

        #[cfg(not(feature = "sqlite"))]
        let store: Box<dyn MemoryStoreTrait> = Box::new(
            FileMemoryStore::new(file_store.workspace().to_path_buf()),
        );

        Self { store, file_store }
    }

    // ── Structured memory API (delegates to trait store) ──

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

    // ── Legacy file-based API (MEMORY.md / HISTORY.md) ──

    /// Read long-term memory (`MEMORY.md`).
    #[instrument(name = "memory.read_long_term", skip_all)]
    pub async fn read_long_term(&self) -> Result<String> {
        Ok(self
            .file_store
            .read_raw("MEMORY.md")
            .await?
            .unwrap_or_default())
    }

    /// Write long-term memory (`MEMORY.md`).
    #[instrument(name = "memory.write_long_term", skip_all)]
    pub async fn write_long_term(&self, content: &str) -> Result<()> {
        self.file_store.write_raw("MEMORY.md", content).await
    }

    /// Read history (`HISTORY.md`).
    #[instrument(name = "memory.read_history", skip_all)]
    pub async fn read_history(&self) -> Result<String> {
        Ok(self
            .file_store
            .read_raw("HISTORY.md")
            .await?
            .unwrap_or_default())
    }

    /// Append to history (`HISTORY.md`).
    #[instrument(name = "memory.append_history", skip_all)]
    pub async fn append_history(&self, entry: &str) -> Result<()> {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M");
        let content = format!("\n[{}] {}\n", timestamp, entry);
        self.file_store.append_raw("HISTORY.md", &content).await
    }
}
