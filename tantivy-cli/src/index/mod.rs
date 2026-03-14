//! Index management module.

mod document;
mod manager;
mod schema;
mod search;

pub use document::{Document, DocumentOperations};
pub use manager::IndexManager;
pub use schema::{FieldDef, FieldType, IndexConfig, IndexSchema};
pub use search::{SearchQuery, SearchResult, SortOrder};
