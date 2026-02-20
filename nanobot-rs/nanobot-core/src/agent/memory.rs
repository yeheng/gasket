//! Memory store for long-term context
//!
//! This module wraps the generic `memory::FileMemoryStore` to provide the
//! high-level `read_long_term`, `write_long_term`, `read_history`, and
//! `append_history` API used by the agent loop.

use std::path::PathBuf;

use anyhow::Result;

use crate::memory::{FileMemoryStore, MemoryStore as MemoryStoreTrait};

/// Memory store for long-term context.
///
/// Delegates to [`FileMemoryStore`] internally.
pub struct MemoryStore {
    store: FileMemoryStore,
}

impl MemoryStore {
    /// Create a new memory store backed by the workspace directory.
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            store: FileMemoryStore::new(workspace),
        }
    }

    /// Read long-term memory (`MEMORY.md`).
    pub fn read_long_term(&self) -> Result<String> {
        run_blocking(async {
            Ok(self
                .store
                .read("MEMORY.md")
                .await?
                .unwrap_or_default())
        })
    }

    /// Write long-term memory (`MEMORY.md`).
    pub fn write_long_term(&self, content: &str) -> Result<()> {
        run_blocking(async { self.store.write("MEMORY.md", content).await })
    }

    /// Read history (`HISTORY.md`).
    pub fn read_history(&self) -> Result<String> {
        run_blocking(async {
            Ok(self
                .store
                .read("HISTORY.md")
                .await?
                .unwrap_or_default())
        })
    }

    /// Append to history (`HISTORY.md`).
    pub fn append_history(&self, entry: &str) -> Result<()> {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M");
        let content = format!("\n[{}] {}\n", timestamp, entry);
        run_blocking(async { self.store.append("HISTORY.md", &content).await })
    }

    /// Get a reference to the underlying `FileMemoryStore`.
    pub fn inner(&self) -> &FileMemoryStore {
        &self.store
    }
}

/// Run an async future synchronously, safe to call from within a tokio runtime.
///
/// Uses `block_in_place` on multi-threaded runtimes, and spawns a dedicated
/// thread on single-threaded runtimes (e.g. `#[tokio::test]`).
fn run_blocking<F, T>(fut: F) -> T
where
    F: std::future::Future<Output = T> + Send,
    T: Send + 'static,
{
    let handle = tokio::runtime::Handle::current();
    match handle.runtime_flavor() {
        tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(|| handle.block_on(fut))
        }
        _ => {
            // Current-thread runtime: spawn a blocking thread that creates
            // its own runtime to avoid the nested block_on panic.
            std::thread::scope(|s| {
                s.spawn(|| {
                    let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
                    rt.block_on(fut)
                })
                .join()
                .expect("Blocking thread panicked")
            })
        }
    }
}
