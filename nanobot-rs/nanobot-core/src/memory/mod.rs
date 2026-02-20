//! Memory storage abstraction
//!
//! Provides a trait-based `MemoryStore` interface with multiple backends:
//! - `FileMemoryStore` — file-based storage (migrated from `agent/memory.rs`)
//! - `InMemoryStore` — in-memory storage (for testing)

pub mod middleware;
mod store;

pub use middleware::{
    MemoryLoggingMiddleware, MemoryMetrics, MemoryMetricsMiddleware, MemoryMiddleware,
    MemoryOperation, MemoryResponse,
};
pub use store::{FileMemoryStore, InMemoryStore, MemoryEntry, MemoryQuery, MemoryStore};
