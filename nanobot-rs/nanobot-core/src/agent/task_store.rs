//! Task persistence abstraction for SubagentManager.
//!
//! Defines the `TaskStore` trait and provides a JSON-file fallback implementation.
//! When the `sqlite` feature is enabled, `SqliteTaskStore` is preferred.

use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use tracing::{debug, info, warn};

use super::subagent::SubagentTask;

/// Abstraction over task persistence backends.
///
/// The in-memory `HashMap` in `SubagentManager` is the authoritative read
/// source; this trait only governs durable storage.
#[async_trait]
pub trait TaskStore: Send + Sync {
    /// Load all persisted tasks (called once at startup).
    async fn load_all(&self) -> anyhow::Result<HashMap<String, SubagentTask>>;

    /// Persist a single task (insert or update).
    /// For JSON backends this is a no-op — use `save_all` instead.
    async fn save_task(&self, task: &SubagentTask) -> anyhow::Result<()>;

    /// Persist all tasks at once.
    /// For SQLite backends this is a no-op — use `save_task` instead.
    async fn save_all(&self, tasks: &[SubagentTask]) -> anyhow::Result<()>;

    /// Remove tasks by IDs.
    async fn remove_tasks(&self, ids: &[String]) -> anyhow::Result<()>;
}

// ── JSON file backend ──────────────────────────────────────

/// JSON-file-based task persistence (the original behavior).
///
/// Always writes the complete task list via `save_all`.
/// `save_task` is a no-op because JSON must serialize the full collection.
pub struct JsonTaskStore {
    path: PathBuf,
}

impl JsonTaskStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[async_trait]
impl TaskStore for JsonTaskStore {
    async fn load_all(&self) -> anyhow::Result<HashMap<String, SubagentTask>> {
        if !self.path.exists() {
            return Ok(HashMap::new());
        }

        match std::fs::read_to_string(&self.path) {
            Ok(content) => match serde_json::from_str::<Vec<SubagentTask>>(&content) {
                Ok(tasks) => {
                    let map: HashMap<String, SubagentTask> =
                        tasks.into_iter().map(|t| (t.id.clone(), t)).collect();
                    info!("Loaded {} persisted tasks from {:?}", map.len(), self.path);
                    Ok(map)
                }
                Err(e) => {
                    warn!("Failed to parse tasks file: {}, starting fresh", e);
                    Ok(HashMap::new())
                }
            },
            Err(e) => {
                warn!("Failed to read tasks file: {}, starting fresh", e);
                Ok(HashMap::new())
            }
        }
    }

    async fn save_task(&self, _task: &SubagentTask) -> anyhow::Result<()> {
        // No-op: JSON must write the full collection via save_all.
        Ok(())
    }

    async fn save_all(&self, tasks: &[SubagentTask]) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }

        let content = serde_json::to_string_pretty(tasks)?;
        tokio::fs::write(&self.path, content).await?;
        debug!("Flushed {} tasks to {:?}", tasks.len(), self.path);
        Ok(())
    }

    async fn remove_tasks(&self, _ids: &[String]) -> anyhow::Result<()> {
        // No-op: caller will follow up with save_all.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::subagent::{SubagentTask, TaskStatus};

    #[tokio::test]
    async fn test_json_store_load_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        let store = JsonTaskStore::new(path);

        let tasks = store.load_all().await.unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn test_json_store_save_all_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        let store = JsonTaskStore::new(path);

        let t1 = SubagentTask::new("prompt1", "ch", "chat", "sess");
        let t2 = SubagentTask::new("prompt2", "ch", "chat", "sess");
        store.save_all(&[t1.clone(), t2.clone()]).await.unwrap();

        let loaded = store.load_all().await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[&t1.id].prompt, "prompt1");
        assert_eq!(loaded[&t2.id].prompt, "prompt2");
    }

    #[tokio::test]
    async fn test_json_store_round_trip_all_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        let store = JsonTaskStore::new(path);

        let mut task = SubagentTask::new("test prompt", "telegram", "chat123", "session:key")
            .with_priority(crate::agent::subagent::TaskPriority::High)
            .with_timeout(600)
            .with_metadata("key1", "value1");
        task.status = TaskStatus::Completed;
        task.started_at = Some(chrono::Utc::now());
        task.completed_at = Some(chrono::Utc::now());
        task.result = Some("done".to_string());
        task.progress = 100;

        store.save_all(&[task.clone()]).await.unwrap();

        let loaded = store.load_all().await.unwrap();
        let loaded_task = &loaded[&task.id];
        assert_eq!(loaded_task.prompt, task.prompt);
        assert_eq!(loaded_task.channel, task.channel);
        assert_eq!(loaded_task.status, task.status);
        assert_eq!(loaded_task.priority, task.priority);
        assert_eq!(loaded_task.timeout_secs, task.timeout_secs);
        assert_eq!(loaded_task.result, task.result);
        assert_eq!(loaded_task.progress, task.progress);
        assert_eq!(loaded_task.metadata.get("key1").unwrap(), "value1");
    }
}
