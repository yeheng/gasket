//! Tantivy-based full-text search for nanobot.
//!
//! This crate provides advanced search capabilities using the Tantivy search engine:
//! - Memory file indexing and search (`~/.nanobot/memory/*.md`)
//! - Session history indexing and search (SQLite `session_messages` table)
//!
//! ## Features
//! - Boolean queries (AND/OR/NOT logic)
//! - Fuzzy matching with typo tolerance
//! - Date range filtering
//! - Role and session filtering
//! - BM25 relevance scoring
//! - Result highlighting
//!
//! ## Usage
//!
//! Add to your `Cargo.toml`:
//! ```toml
//! [dependencies]
//! nanobot-tantivy = { path = "nanobot-tantivy" }
//! ```
//!
//! ## Example
//!
//! ```no_run
//! use nanobot_tantivy::{open_memory_index, SearchQuery};
//!
//! // Open memory index
//! let (reader, writer) = open_memory_index("/path/to/index", "/path/to/memory")?;
//!
//! // Search
//! let query = SearchQuery::text("rust programming").with_limit(10);
//! let results = reader.search(&query)?;
//!
//! for result in results {
//!     println!("{}: {}", result.id, result.score);
//! }
//! # Ok::<(), nanobot_tantivy::TantivyError>(())
//! ```

pub mod search;

// Re-export commonly used types at crate root
pub use search::{
    BooleanQuery, DateRange, FuzzyQuery, HighlightedText, SearchQuery, SearchResult, SortOrder,
};

// Re-export index functions
pub use search::tantivy::{
    open_history_index, open_memory_index, HistoryIndexReader, HistoryIndexWriter,
    IndexUpdateStats, MemoryIndexReader, MemoryIndexWriter, TantivyError,
};
