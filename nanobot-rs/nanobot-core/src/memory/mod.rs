//! Memory storage abstraction
//!
//! Provides a trait-based `MemoryStore` interface with multiple backends:
//! - `FileMemoryStore` — file-based storage
//! - `SqliteStore` — SQLite-backed storage with FTS5 (requires `sqlite` feature)

mod store;

#[cfg(feature = "sqlite")]
mod sqlite;

pub use store::{FileMemoryStore, MemoryEntry, MemoryMetadata, MemoryQuery, MemoryStore};

#[cfg(feature = "sqlite")]
pub use sqlite::SqliteStore;
