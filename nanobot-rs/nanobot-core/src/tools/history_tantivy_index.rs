//! Tantivy-powered history index management tool.
//!
//! Provides index management operations for session history:
//! - `rebuild`: Clear and rebuild the entire index from SQLite database
//! - `update`: Incremental update - only sync new/deleted messages

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sqlx::Row;
use tokio::sync::Mutex;
use tracing::{debug, info};

use super::{simple_schema, Tool, ToolError, ToolResult};
use crate::memory::SqliteStore;
use crate::search::{open_history_index, HistoryIndexWriter, IndexUpdateStats};

/// Tool that manages the history Tantivy index.
pub struct HistoryTantivyIndexTool {
    writer: Arc<Mutex<HistoryIndexWriter>>,
    db: SqliteStore,
}

impl HistoryTantivyIndexTool {
    /// Create a new history tantivy index tool from a shared writer.
    pub fn new(writer: Arc<Mutex<HistoryIndexWriter>>, db: SqliteStore) -> Self {
        Self { writer, db }
    }

    /// Create with default paths.
    pub async fn with_defaults() -> Result<Self, ToolError> {
        let config_dir = crate::config::config_dir();
        let index_path = config_dir.join("tantivy-index").join("history");

        let (_reader, writer) = open_history_index(&index_path).map_err(|e| {
            ToolError::ExecutionError(format!("Failed to open history index: {}", e))
        })?;

        let db = SqliteStore::new()
            .await
            .map_err(|e| ToolError::ExecutionError(format!("Failed to open database: {}", e)))?;

        Ok(Self {
            writer: Arc::new(Mutex::new(writer)),
            db,
        })
    }
}

#[derive(Debug, Deserialize)]
struct IndexArgs {
    /// Action to perform: "rebuild" or "update"
    #[serde(default = "default_action")]
    action: String,
}

fn default_action() -> String {
    "update".to_string()
}

#[async_trait]
impl Tool for HistoryTantivyIndexTool {
    fn name(&self) -> &str {
        "history_tantivy_index"
    }

    fn description(&self) -> &str {
        "Manage the Tantivy full-text index for conversation history. \
         Use 'rebuild' to completely rebuild the index from the database (slow but thorough), \
         or 'update' for incremental sync (fast, only processes changes)."
    }

    fn parameters(&self) -> Value {
        simple_schema(&[(
            "action",
            "string",
            false,
            "Action: 'rebuild' (full rebuild) or 'update' (incremental sync, default)",
        )])
    }

    async fn execute(&self, args: Value) -> ToolResult {
        let parsed: IndexArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArguments(format!("Invalid arguments: {}", e)))?;

        info!("history_tantivy_index: executing {}", parsed.action);

        match parsed.action.as_str() {
            "rebuild" => self.execute_rebuild().await,
            "update" => self.execute_update().await,
            _ => Err(ToolError::InvalidArguments(format!(
                "Unknown action: {}. Use 'rebuild' or 'update'.",
                parsed.action
            ))),
        }
    }
}

impl HistoryTantivyIndexTool {
    async fn execute_rebuild(&self) -> ToolResult {
        let mut w = self.writer.lock().await;

        // Clear existing index
        w.clear().map_err(|e| {
            ToolError::ExecutionError(format!("Failed to clear history index: {}", e))
        })?;

        // Query all messages with their IDs
        let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
            "SELECT id, session_key, role, content, timestamp, tools_used FROM session_messages ORDER BY id ASC",
        )
        .fetch_all(&self.db.pool)
        .await
        .map_err(|e| ToolError::ExecutionError(format!("Failed to query messages: {}", e)))?;

        let mut count = 0;
        for row in rows {
            let id: i64 = row.get("id");
            let session_key: String = row.get("session_key");
            let role: String = row.get("role");
            let content: String = row.get("content");
            let timestamp_str: String = row.get("timestamp");
            let tools_json: Option<String> = row.get("tools_used");

            let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            let tools: Option<Vec<String>> =
                tools_json.and_then(|json| serde_json::from_str(&json).ok());

            // Use database ID as the index ID
            let doc_id = format!("{}:{}", session_key, id);

            w.index_document(
                &doc_id,
                &content,
                &role,
                &session_key,
                timestamp,
                tools.as_deref(),
            )
            .map_err(|e| ToolError::ExecutionError(format!("Failed to index document: {}", e)))?;
            count += 1;

            if count % 100 == 0 {
                info!("Rebuilding history index: {} documents", count);
            }
        }

        w.commit().map_err(|e| {
            ToolError::ExecutionError(format!("Failed to commit history index: {}", e))
        })?;

        Ok(format!(
            "History index rebuilt successfully. {} messages indexed.",
            count
        ))
    }

    async fn execute_update(&self) -> ToolResult {
        let mut w = self.writer.lock().await;
        let mut stats = IndexUpdateStats::default();

        // Get all indexed document IDs
        let indexed_docs = w
            .get_indexed_documents()
            .map_err(|e| ToolError::ExecutionError(format!("Failed to get indexed docs: {}", e)))?;

        // Query all messages from database
        let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
            "SELECT id, session_key, role, content, timestamp, tools_used FROM session_messages ORDER BY id ASC",
        )
        .fetch_all(&self.db.pool)
        .await
        .map_err(|e| ToolError::ExecutionError(format!("Failed to query messages: {}", e)))?;

        /// Helper struct to hold message data during sync.
        struct MessageData {
            doc_id: String,
            session_key: String,
            role: String,
            content: String,
            timestamp: DateTime<Utc>,
            tools: Option<Vec<String>>,
        }

        // Build a set of database message IDs
        let mut db_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut new_messages: Vec<MessageData> = Vec::new();

        for row in rows {
            let id: i64 = row.get("id");
            let session_key: String = row.get("session_key");
            let role: String = row.get("role");
            let content: String = row.get("content");
            let timestamp_str: String = row.get("timestamp");
            let tools_json: Option<String> = row.get("tools_used");

            let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            let tools: Option<Vec<String>> =
                tools_json.and_then(|json| serde_json::from_str(&json).ok());

            let doc_id = format!("{}:{}", session_key, id);
            db_ids.insert(doc_id.clone());

            // Check if this message is new (not in index)
            if !indexed_docs.contains(&doc_id) {
                new_messages.push(MessageData {
                    doc_id,
                    session_key,
                    role,
                    content,
                    timestamp,
                    tools,
                });
            }
        }

        // Remove documents that no longer exist in database
        for id in &indexed_docs {
            if !db_ids.contains(id) {
                w.delete_document(id).map_err(|e| {
                    ToolError::ExecutionError(format!("Failed to delete document: {}", e))
                })?;
                stats.removed += 1;
                debug!("Removed deleted message from history index: {}", id);
            }
        }

        // Add new messages
        for msg in new_messages {
            w.index_document(
                &msg.doc_id,
                &msg.content,
                &msg.role,
                &msg.session_key,
                msg.timestamp,
                msg.tools.as_deref(),
            )
            .map_err(|e| ToolError::ExecutionError(format!("Failed to index document: {}", e)))?;
            stats.added += 1;
            debug!("Added new message to history index: {}", msg.doc_id);
        }

        if stats.added > 0 || stats.removed > 0 {
            w.commit().map_err(|e| {
                ToolError::ExecutionError(format!("Failed to commit history index: {}", e))
            })?;
        }

        info!(
            "History index incremental update: {} added, {} removed",
            stats.added, stats.removed
        );

        Ok(format!(
            "History index updated. Added: {}, Removed: {}",
            stats.added, stats.removed
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_args_parsing() {
        let args = serde_json::json!({
            "action": "rebuild"
        });

        let parsed: IndexArgs = serde_json::from_value(args).unwrap();
        assert_eq!(parsed.action, "rebuild");
    }

    #[test]
    fn test_index_args_default() {
        let args = serde_json::json!({});
        let parsed: IndexArgs = serde_json::from_value(args).unwrap();
        assert_eq!(parsed.action, "update");
    }
}
