//! SQLite-backed memory store with FTS5 full-text search.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use tokio::sync::Mutex;
use tracing::debug;

use super::store::{MemoryEntry, MemoryMetadata, MemoryQuery, MemoryStore};

/// SQLite-backed memory store with FTS5 full-text search.
///
/// Persists memory entries in a SQLite database file. Uses a single
/// `Connection` behind a `tokio::sync::Mutex` for async safety.
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    /// Create a new `SqliteStore` with the default database path
    /// (`~/.nanobot/memory.db`).
    pub fn new() -> anyhow::Result<Self> {
        let path = crate::config::config_dir().join("memory.db");
        Self::with_path(path)
    }

    /// Create a new `SqliteStore` with a custom database path.
    pub fn with_path(path: PathBuf) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        Self::init_db(&conn)?;
        debug!("Opened SqliteStore at {:?}", path);
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn init_db(conn: &Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS memories (
                id          TEXT PRIMARY KEY,
                content     TEXT NOT NULL,
                metadata    TEXT NOT NULL DEFAULT '{}',
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS memory_tags (
                memory_id   TEXT NOT NULL,
                tag         TEXT NOT NULL,
                PRIMARY KEY (memory_id, tag),
                FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_memory_tags_tag ON memory_tags(tag);

            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                id,
                content,
                content='memories',
                content_rowid='rowid'
            );

            -- Triggers to keep FTS5 index in sync
            CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                INSERT INTO memories_fts(rowid, id, content)
                VALUES (new.rowid, new.id, new.content);
            END;

            CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, id, content)
                VALUES ('delete', old.rowid, old.id, old.content);
            END;

            CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, id, content)
                VALUES ('delete', old.rowid, old.id, old.content);
                INSERT INTO memories_fts(rowid, id, content)
                VALUES (new.rowid, new.id, new.content);
            END;

            PRAGMA foreign_keys = ON;
            ",
        )?;
        Ok(())
    }
}

#[async_trait]
impl MemoryStore for SqliteStore {
    async fn save(&self, entry: &MemoryEntry) -> anyhow::Result<()> {
        let conn = self.conn.lock().await;
        let metadata_json = serde_json::to_string(&entry.metadata)?;
        let created = entry.created_at.to_rfc3339();
        let updated = entry.updated_at.to_rfc3339();

        conn.execute(
            "INSERT OR REPLACE INTO memories (id, content, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![entry.id, entry.content, metadata_json, created, updated],
        )?;

        // Sync tags: delete old, insert new
        conn.execute(
            "DELETE FROM memory_tags WHERE memory_id = ?1",
            rusqlite::params![entry.id],
        )?;
        for tag in &entry.metadata.tags {
            conn.execute(
                "INSERT INTO memory_tags (memory_id, tag) VALUES (?1, ?2)",
                rusqlite::params![entry.id, tag],
            )?;
        }

        debug!("Saved memory entry: {}", entry.id);
        Ok(())
    }

    async fn get(&self, id: &str) -> anyhow::Result<Option<MemoryEntry>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, content, metadata, created_at, updated_at FROM memories WHERE id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![id])?;

        if let Some(row) = rows.next()? {
            let entry = row_to_entry(row)?;
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    async fn delete(&self, id: &str) -> anyhow::Result<bool> {
        let conn = self.conn.lock().await;
        let changed = conn.execute(
            "DELETE FROM memories WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(changed > 0)
    }

    async fn search(&self, query: &MemoryQuery) -> anyhow::Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().await;
        search_impl(&conn, query)
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> anyhow::Result<MemoryEntry> {
    let id: String = row.get(0)?;
    let content: String = row.get(1)?;
    let metadata_json: String = row.get(2)?;
    let created_str: String = row.get(3)?;
    let updated_str: String = row.get(4)?;

    let metadata: MemoryMetadata = serde_json::from_str(&metadata_json)?;
    let created_at = DateTime::parse_from_rfc3339(&created_str)?.with_timezone(&Utc);
    let updated_at = DateTime::parse_from_rfc3339(&updated_str)?.with_timezone(&Utc);

    Ok(MemoryEntry {
        id,
        content,
        metadata,
        created_at,
        updated_at,
    })
}

/// Build and execute the search query dynamically.
fn search_impl(conn: &Connection, query: &MemoryQuery) -> anyhow::Result<Vec<MemoryEntry>> {
    let mut sql = String::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1u32;

    if query.text.is_some() {
        sql.push_str(
            "SELECT m.id, m.content, m.metadata, m.created_at, m.updated_at \
             FROM memories m \
             JOIN memories_fts f ON m.id = f.id \
             WHERE f.content MATCH ?",
        );
        sql.push_str(&param_idx.to_string());
        params.push(Box::new(query.text.clone().unwrap()));
        param_idx += 1;
    } else {
        sql.push_str(
            "SELECT m.id, m.content, m.metadata, m.created_at, m.updated_at \
             FROM memories m WHERE 1=1",
        );
    }

    // Filter by source
    if let Some(source) = &query.source {
        sql.push_str(&format!(
            " AND json_extract(m.metadata, '$.source') = ?{}",
            param_idx
        ));
        params.push(Box::new(source.clone()));
        param_idx += 1;
    }

    // Filter by tags (AND semantics: entry must have ALL tags)
    for tag in &query.tags {
        sql.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM memory_tags t WHERE t.memory_id = m.id AND t.tag = ?{})",
            param_idx
        ));
        params.push(Box::new(tag.clone()));
        param_idx += 1;
    }

    // Order by updated_at descending for deterministic results
    sql.push_str(" ORDER BY m.updated_at DESC");

    // Limit / offset
    if let Some(limit) = query.limit {
        sql.push_str(&format!(" LIMIT ?{}", param_idx));
        params.push(Box::new(limit as i64));
        param_idx += 1;
    }
    if let Some(offset) = query.offset {
        if query.limit.is_none() {
            sql.push_str(&format!(" LIMIT -1 OFFSET ?{}", param_idx));
        } else {
            sql.push_str(&format!(" OFFSET ?{}", param_idx));
        }
        params.push(Box::new(offset as i64));
    }

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut rows = stmt.query(param_refs.as_slice())?;

    let mut entries = Vec::new();
    while let Some(row) = rows.next()? {
        entries.push(row_to_entry(row)?);
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> SqliteStore {
        let path =
            std::env::temp_dir().join(format!("nanobot_sqlite_test_{}.db", uuid::Uuid::new_v4()));
        SqliteStore::with_path(path).unwrap()
    }

    fn make_entry(id: &str, content: &str) -> MemoryEntry {
        let now = Utc::now();
        MemoryEntry {
            id: id.to_string(),
            content: content.to_string(),
            metadata: MemoryMetadata::default(),
            created_at: now,
            updated_at: now,
        }
    }

    fn make_entry_with_meta(
        id: &str,
        content: &str,
        source: Option<&str>,
        tags: &[&str],
    ) -> MemoryEntry {
        let now = Utc::now();
        MemoryEntry {
            id: id.to_string(),
            content: content.to_string(),
            metadata: MemoryMetadata {
                source: source.map(|s| s.to_string()),
                tags: tags.iter().map(|t| t.to_string()).collect(),
                extra: serde_json::Value::Null,
            },
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn test_sqlite_save_and_get() {
        let store = temp_store();
        let entry = make_entry("e1", "hello world");
        store.save(&entry).await.unwrap();

        let got = store.get("e1").await.unwrap().unwrap();
        assert_eq!(got.id, "e1");
        assert_eq!(got.content, "hello world");
    }

    #[tokio::test]
    async fn test_sqlite_save_overwrites() {
        let store = temp_store();
        store.save(&make_entry("e1", "v1")).await.unwrap();
        store.save(&make_entry("e1", "v2")).await.unwrap();

        let got = store.get("e1").await.unwrap().unwrap();
        assert_eq!(got.content, "v2");
    }

    #[tokio::test]
    async fn test_sqlite_get_nonexistent() {
        let store = temp_store();
        assert!(store.get("nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_sqlite_delete() {
        let store = temp_store();
        store.save(&make_entry("e1", "data")).await.unwrap();
        assert!(store.delete("e1").await.unwrap());
        assert!(!store.delete("e1").await.unwrap());
        assert!(store.get("e1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_sqlite_fts5_search() {
        let store = temp_store();
        store
            .save(&make_entry("e1", "rust is a systems programming language"))
            .await
            .unwrap();
        store
            .save(&make_entry("e2", "python is great for data science"))
            .await
            .unwrap();
        store
            .save(&make_entry("e3", "rust and python are both popular"))
            .await
            .unwrap();

        let results = store
            .search(&MemoryQuery {
                text: Some("rust".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(results.len(), 2);

        let ids: Vec<&str> = results.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"e1"));
        assert!(ids.contains(&"e3"));
    }

    #[tokio::test]
    async fn test_sqlite_search_by_tags() {
        let store = temp_store();
        store
            .save(&make_entry_with_meta("e1", "a", None, &["rust", "lang"]))
            .await
            .unwrap();
        store
            .save(&make_entry_with_meta("e2", "b", None, &["rust"]))
            .await
            .unwrap();
        store
            .save(&make_entry_with_meta("e3", "c", None, &["python"]))
            .await
            .unwrap();

        let results = store
            .search(&MemoryQuery {
                tags: vec!["rust".to_string()],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(results.len(), 2);

        // AND semantics
        let results = store
            .search(&MemoryQuery {
                tags: vec!["rust".to_string(), "lang".to_string()],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "e1");
    }

    #[tokio::test]
    async fn test_sqlite_search_by_source() {
        let store = temp_store();
        store
            .save(&make_entry_with_meta("e1", "a", Some("user"), &[]))
            .await
            .unwrap();
        store
            .save(&make_entry_with_meta("e2", "b", Some("agent"), &[]))
            .await
            .unwrap();

        let results = store
            .search(&MemoryQuery {
                source: Some("user".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "e1");
    }

    #[tokio::test]
    async fn test_sqlite_search_limit_offset() {
        let store = temp_store();
        for i in 0..5 {
            store
                .save(&make_entry(&format!("e{}", i), &format!("content {}", i)))
                .await
                .unwrap();
        }

        let results = store
            .search(&MemoryQuery {
                limit: Some(2),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(results.len(), 2);

        let all = store
            .search(&MemoryQuery::default())
            .await
            .unwrap();
        assert_eq!(all.len(), 5);
    }

    #[tokio::test]
    async fn test_sqlite_search_empty_returns_all() {
        let store = temp_store();
        store.save(&make_entry("e1", "a")).await.unwrap();
        store.save(&make_entry("e2", "b")).await.unwrap();

        let results = store.search(&MemoryQuery::default()).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_sqlite_metadata_extra_preserved() {
        let store = temp_store();
        let now = Utc::now();
        let entry = MemoryEntry {
            id: "e1".to_string(),
            content: "test".to_string(),
            metadata: MemoryMetadata {
                source: Some("user".to_string()),
                tags: vec!["a".to_string()],
                extra: serde_json::json!({"key": "value", "num": 42}),
            },
            created_at: now,
            updated_at: now,
        };

        store.save(&entry).await.unwrap();
        let got = store.get("e1").await.unwrap().unwrap();
        assert_eq!(got.metadata.extra["key"], "value");
        assert_eq!(got.metadata.extra["num"], 42);
        assert_eq!(got.metadata.source.as_deref(), Some("user"));
        assert_eq!(got.metadata.tags, vec!["a".to_string()]);
    }

    #[tokio::test]
    async fn test_sqlite_persistence() {
        let path =
            std::env::temp_dir().join(format!("nanobot_sqlite_persist_{}.db", uuid::Uuid::new_v4()));

        // Write with first store instance
        {
            let store = SqliteStore::with_path(path.clone()).unwrap();
            store.save(&make_entry("e1", "persisted")).await.unwrap();
        }

        // Read with second store instance
        {
            let store = SqliteStore::with_path(path.clone()).unwrap();
            let got = store.get("e1").await.unwrap().unwrap();
            assert_eq!(got.content, "persisted");
        }

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn test_sqlite_concurrent_access() {
        let store = Arc::new(temp_store());

        let mut handles = vec![];
        for i in 0..10 {
            let store = store.clone();
            let handle = tokio::spawn(async move {
                let entry = make_entry(&format!("e{}", i), &format!("content {}", i));
                store.save(&entry).await.unwrap();
                let got = store.get(&format!("e{}", i)).await.unwrap();
                assert!(got.is_some());
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        let all = store.search(&MemoryQuery::default()).await.unwrap();
        assert_eq!(all.len(), 10);
    }
}
