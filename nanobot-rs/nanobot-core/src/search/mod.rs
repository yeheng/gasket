//! Full-text search types.
//!
//! When the `tantivy` feature is enabled, this module re-exports types from
//! `nanobot-tantivy`. Otherwise, it provides local definitions for basic types.

#[cfg(feature = "tantivy")]
pub use nanobot_tantivy::{
    open_history_index, open_memory_index, BooleanQuery, DateRange, FuzzyQuery, HighlightedText,
    HistoryIndexReader, HistoryIndexWriter, IndexUpdateStats, MemoryIndexReader, MemoryIndexWriter,
    SearchQuery, SearchResult, SortOrder, TantivyError,
};

// When tantivy feature is disabled, we still need basic types for memory_search
#[cfg(not(feature = "tantivy"))]
mod query;
#[cfg(not(feature = "tantivy"))]
mod result;

#[cfg(not(feature = "tantivy"))]
pub use query::{BooleanQuery, DateRange, FuzzyQuery, SearchQuery, SortOrder};
#[cfg(not(feature = "tantivy"))]
pub use result::{HighlightedText, SearchResult};
