//! Memory store trait and implementations

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;

// ──────────────────────────────────────────────
//  Core types
// ──────────────────────────────────────────────

/// Metadata attached to a memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetadata {
    /// Provenance of the memory (e.g. "user", "agent", "system").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// Categorical tags for filtering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Extensible key-value pairs.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extra: serde_json::Value,
}

impl Default for MemoryMetadata {
    fn default() -> Self {
        Self {
            source: None,
            tags: Vec::new(),
            extra: serde_json::Value::Null,
        }
    }
}

/// A single memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique identifier.
    pub id: String,

    /// The stored content.
    pub content: String,

    /// Structured metadata.
    pub metadata: MemoryMetadata,

    /// When the entry was first created.
    pub created_at: DateTime<Utc>,

    /// When the entry was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Composable query for searching memories.
#[derive(Debug, Clone, Default)]
pub struct MemoryQuery {
    /// Full-text / semantic search query.
    pub text: Option<String>,

    /// Filter by tags (AND semantics — entry must have all listed tags).
    pub tags: Vec<String>,

    /// Filter by source.
    pub source: Option<String>,

    /// Maximum number of results.
    pub limit: Option<usize>,

    /// Number of results to skip (pagination).
    pub offset: Option<usize>,
}

// ──────────────────────────────────────────────
//  MemoryStore trait
// ──────────────────────────────────────────────

/// Abstract storage interface for structured memories.
///
/// Implementations must be safe to share across async tasks.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// Save a memory entry. If an entry with the same id exists, it is replaced.
    async fn save(&self, entry: &MemoryEntry) -> anyhow::Result<()>;

    /// Retrieve a memory entry by id.
    async fn get(&self, id: &str) -> anyhow::Result<Option<MemoryEntry>>;

    /// Delete a memory entry by id. Returns `true` if the entry existed.
    async fn delete(&self, id: &str) -> anyhow::Result<bool>;

    /// Search memories matching a query.
    async fn search(&self, query: &MemoryQuery) -> anyhow::Result<Vec<MemoryEntry>>;
}

// ──────────────────────────────────────────────
//  FileMemoryStore — file-based implementation
// ──────────────────────────────────────────────

/// File-based memory store that persists data under a workspace directory.
///
/// Each memory entry is stored as a JSON file named `{id}.json` inside the
/// `memory/` subdirectory of the workspace.
///
/// Also provides direct key-value helpers (`read_raw`, `write_raw`, `append_raw`)
/// for backward compatibility with the agent memory wrapper.
pub struct FileMemoryStore {
    memory_dir: PathBuf,
    /// Simple global lock for all file operations
    lock: tokio::sync::Mutex<()>,
}

impl FileMemoryStore {
    /// Create a new file-based memory store.
    pub fn new(workspace: PathBuf) -> Self {
        let memory_dir = workspace.join("memory");
        let _ = std::fs::create_dir_all(&memory_dir);
        Self {
            memory_dir,
            lock: tokio::sync::Mutex::new(()),
        }
    }

    /// Return the workspace directory (parent of the `memory/` dir).
    pub fn workspace(&self) -> &Path {
        self.memory_dir
            .parent()
            .expect("memory_dir always has a parent")
    }

    fn entry_path(&self, id: &str) -> PathBuf {
        let safe_id = id.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        self.memory_dir.join(format!("{}.json", safe_id))
    }

    fn raw_path(&self, key: &str) -> PathBuf {
        let safe_key = key.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        self.memory_dir.join(safe_key)
    }

    // ── Raw key-value helpers (used by agent::memory wrapper) ──

    /// Read a raw value by key (file content as string).
    pub async fn read_raw(&self, key: &str) -> anyhow::Result<Option<String>> {
        let path = self.raw_path(key);
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => Ok(Some(content)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Write a raw value by key.
    pub async fn write_raw(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let _guard = self.lock.lock().await;
        let path = self.raw_path(key);
        let tmp_path = path.with_extension("tmp");
        tokio::fs::write(&tmp_path, value).await?;
        tokio::fs::rename(&tmp_path, &path).await?;
        debug!("Wrote memory key: {}", key);
        Ok(())
    }

    /// Delete a raw key.
    pub async fn delete_raw(&self, key: &str) -> anyhow::Result<bool> {
        let _guard = self.lock.lock().await;
        let path = self.raw_path(key);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Append to a raw key.
    pub async fn append_raw(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let _guard = self.lock.lock().await;
        let path = self.raw_path(key);
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        file.write_all(value.as_bytes()).await?;
        debug!("Appended to memory key: {}", key);
        Ok(())
    }
}

#[async_trait]
impl MemoryStore for FileMemoryStore {
    async fn save(&self, entry: &MemoryEntry) -> anyhow::Result<()> {
        let _guard = self.lock.lock().await;
        let path = self.entry_path(&entry.id);
        let json = serde_json::to_string_pretty(entry)?;
        let tmp_path = path.with_extension("tmp");
        tokio::fs::write(&tmp_path, &json).await?;
        tokio::fs::rename(&tmp_path, &path).await?;
        debug!("Saved memory entry: {}", entry.id);
        Ok(())
    }

    async fn get(&self, id: &str) -> anyhow::Result<Option<MemoryEntry>> {
        let path = self.entry_path(id);
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                let entry: MemoryEntry = serde_json::from_str(&content)?;
                Ok(Some(entry))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn delete(&self, id: &str) -> anyhow::Result<bool> {
        let _guard = self.lock.lock().await;
        let path = self.entry_path(id);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn search(&self, query: &MemoryQuery) -> anyhow::Result<Vec<MemoryEntry>> {
        let mut entries = Vec::new();

        let mut dir = match tokio::fs::read_dir(&self.memory_dir).await {
            Ok(dir) => dir,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(entries),
            Err(e) => return Err(e.into()),
        };

        let mut skipped = 0usize;

        while let Some(dir_entry) = dir.next_entry().await? {
            let filename = dir_entry.file_name().to_string_lossy().to_string();
            // Only consider .json entry files
            if !filename.ends_with(".json") {
                continue;
            }

            let content = match tokio::fs::read_to_string(dir_entry.path()).await {
                Ok(c) => c,
                Err(_) => continue,
            };
            let entry: MemoryEntry = match serde_json::from_str(&content) {
                Ok(e) => e,
                Err(_) => continue,
            };

            // Filter by text (simple substring match for file backend)
            if let Some(text) = &query.text {
                if !entry.content.contains(text.as_str()) {
                    continue;
                }
            }

            // Filter by tags (AND semantics)
            if !query.tags.is_empty()
                && !query.tags.iter().all(|t| entry.metadata.tags.contains(t))
            {
                continue;
            }

            // Filter by source
            if let Some(source) = &query.source {
                if entry.metadata.source.as_deref() != Some(source.as_str()) {
                    continue;
                }
            }

            // Offset
            if let Some(offset) = query.offset {
                if skipped < offset {
                    skipped += 1;
                    continue;
                }
            }

            entries.push(entry);

            // Limit
            if let Some(limit) = query.limit {
                if entries.len() >= limit {
                    break;
                }
            }
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // ── FileMemoryStore: new trait tests ──

    #[tokio::test]
    async fn test_file_store_save_and_get() {
        let dir = std::env::temp_dir().join(format!("nanobot_test_{}", uuid::Uuid::new_v4()));
        let store = FileMemoryStore::new(dir.clone());

        let entry = make_entry("e1", "hello world");
        store.save(&entry).await.unwrap();

        let got = store.get("e1").await.unwrap().unwrap();
        assert_eq!(got.id, "e1");
        assert_eq!(got.content, "hello world");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_file_store_save_overwrites() {
        let dir = std::env::temp_dir().join(format!("nanobot_test_{}", uuid::Uuid::new_v4()));
        let store = FileMemoryStore::new(dir.clone());

        store.save(&make_entry("e1", "v1")).await.unwrap();
        store.save(&make_entry("e1", "v2")).await.unwrap();

        let got = store.get("e1").await.unwrap().unwrap();
        assert_eq!(got.content, "v2");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_file_store_get_nonexistent() {
        let dir = std::env::temp_dir().join(format!("nanobot_test_{}", uuid::Uuid::new_v4()));
        let store = FileMemoryStore::new(dir.clone());

        assert!(store.get("nope").await.unwrap().is_none());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_file_store_delete() {
        let dir = std::env::temp_dir().join(format!("nanobot_test_{}", uuid::Uuid::new_v4()));
        let store = FileMemoryStore::new(dir.clone());

        store.save(&make_entry("e1", "data")).await.unwrap();
        assert!(store.delete("e1").await.unwrap());
        assert!(!store.delete("e1").await.unwrap());
        assert!(store.get("e1").await.unwrap().is_none());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_file_store_search_text() {
        let dir = std::env::temp_dir().join(format!("nanobot_test_{}", uuid::Uuid::new_v4()));
        let store = FileMemoryStore::new(dir.clone());

        store.save(&make_entry("e1", "rust is great")).await.unwrap();
        store
            .save(&make_entry("e2", "python is also great"))
            .await
            .unwrap();
        store.save(&make_entry("e3", "hello world")).await.unwrap();

        let results = store
            .search(&MemoryQuery {
                text: Some("great".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(results.len(), 2);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_file_store_search_tags() {
        let dir = std::env::temp_dir().join(format!("nanobot_test_{}", uuid::Uuid::new_v4()));
        let store = FileMemoryStore::new(dir.clone());

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

        let results = store
            .search(&MemoryQuery {
                tags: vec!["rust".to_string(), "lang".to_string()],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(results.len(), 1);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_file_store_search_source() {
        let dir = std::env::temp_dir().join(format!("nanobot_test_{}", uuid::Uuid::new_v4()));
        let store = FileMemoryStore::new(dir.clone());

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

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_file_store_search_limit_offset() {
        let dir = std::env::temp_dir().join(format!("nanobot_test_{}", uuid::Uuid::new_v4()));
        let store = FileMemoryStore::new(dir.clone());

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

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_file_store_search_empty_returns_all() {
        let dir = std::env::temp_dir().join(format!("nanobot_test_{}", uuid::Uuid::new_v4()));
        let store = FileMemoryStore::new(dir.clone());

        store.save(&make_entry("e1", "a")).await.unwrap();
        store.save(&make_entry("e2", "b")).await.unwrap();

        let results = store
            .search(&MemoryQuery::default())
            .await
            .unwrap();
        assert_eq!(results.len(), 2);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_file_store_metadata_extra_preserved() {
        let dir = std::env::temp_dir().join(format!("nanobot_test_{}", uuid::Uuid::new_v4()));
        let store = FileMemoryStore::new(dir.clone());

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

        let _ = std::fs::remove_dir_all(dir);
    }

    // ── Raw key-value tests (backward compat) ──

    #[tokio::test]
    async fn test_file_store_raw_read_write() {
        let dir = std::env::temp_dir().join(format!("nanobot_test_{}", uuid::Uuid::new_v4()));
        let store = FileMemoryStore::new(dir.clone());

        store.write_raw("MEMORY.md", "# Memory").await.unwrap();
        assert_eq!(
            store.read_raw("MEMORY.md").await.unwrap(),
            Some("# Memory".to_string())
        );

        assert!(store.delete_raw("MEMORY.md").await.unwrap());
        assert_eq!(store.read_raw("MEMORY.md").await.unwrap(), None);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_file_store_raw_append() {
        let dir = std::env::temp_dir().join(format!("nanobot_test_{}", uuid::Uuid::new_v4()));
        let store = FileMemoryStore::new(dir.clone());

        store.append_raw("HISTORY.md", "line1\n").await.unwrap();
        store.append_raw("HISTORY.md", "line2\n").await.unwrap();
        let content = store.read_raw("HISTORY.md").await.unwrap().unwrap();
        assert!(content.contains("line1"));
        assert!(content.contains("line2"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_file_store_concurrent_writes() {
        let dir =
            std::env::temp_dir().join(format!("nanobot_test_concurrent_{}", uuid::Uuid::new_v4()));
        let store = std::sync::Arc::new(FileMemoryStore::new(dir.clone()));

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

        let _ = std::fs::remove_dir_all(dir);
    }
}
