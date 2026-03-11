//! Index rebuild operations.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::index::{FieldDef, IndexManager};
use crate::Result;

/// Result of rebuild operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebuildResult {
    /// Index name.
    pub index_name: String,
    /// Documents reindexed.
    pub docs_reindexed: u64,
    /// Whether schema was changed.
    pub schema_changed: bool,
    /// Timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Rebuild an index with optional new schema.
///
/// This operation:
/// 1. Reads all documents from the existing index
/// 2. Creates a new index with the new schema (if provided) or the existing schema
/// 3. Reindexes all documents
/// 4. Replaces the old index with the new one
pub fn rebuild_index(
    manager: &mut IndexManager,
    index_name: &str,
    new_fields: Option<Vec<FieldDef>>,
    batch_size: usize,
) -> Result<RebuildResult> {
    info!("Rebuilding index: {}", index_name);

    // Get existing schema
    let old_schema = manager
        .get_schema(index_name)?
        .ok_or_else(|| crate::Error::IndexNotFound(index_name.to_string()))?;

    // Use new schema or keep existing
    let schema_changed = new_fields.is_some();
    let fields = new_fields.unwrap_or_else(|| old_schema.fields.clone());

    // Get all documents from existing index
    let docs = manager.list_documents(index_name, usize::MAX, 0)?;

    let docs_count = docs.len() as u64;
    info!("Found {} documents to reindex", docs_count);

    // Drop the old index
    manager.drop_index(index_name)?;

    // Create new index with the schema
    manager.create_index(index_name, fields, None)?;

    // Reindex documents in batches
    let mut indexed = 0u64;
    for chunk in docs.chunks(batch_size.max(1)) {
        for doc in chunk {
            manager.add_document(index_name, doc.clone())?;
            indexed += 1;
        }
        // Commit each batch
        manager.commit(index_name)?;
    }

    info!("Reindexed {} documents", indexed);

    Ok(RebuildResult {
        index_name: index_name.to_string(),
        docs_reindexed: indexed,
        schema_changed,
        timestamp: Utc::now(),
    })
}
