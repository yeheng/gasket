//! Full-text search types and Tantivy index implementations.
//!
//! Provides high-performance search capabilities for:
//! - Memory files (`~/.nanobot/memory/*.md`)
//! - Session history (SQLite `session_messages` table)

mod query;
mod result;
pub mod tantivy;

pub use query::{BooleanQuery, DateRange, FuzzyQuery, SearchQuery, SortOrder};
pub use result::{HighlightedText, SearchResult};
