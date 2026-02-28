//! Task persistence utilities for SubagentManager.
//!
//! Provides JSON migration support for legacy `tasks.json` files.
//! The actual task storage is handled by `SqliteTaskStore`.

use std::collections::HashMap;
use std::path::PathBuf;

use tokio::fs;
use tracing::{info, warn};

use super::subagent::SubagentTask;

/// Read tasks from a legacy JSON file.
///
/// Used during migration from JSON to SQLite.
pub async fn load_from_json(path: &PathBuf) -> anyhow::Result<HashMap<String, SubagentTask>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let content = fs::read_to_string(path).await?;
    match serde_json::from_str::<Vec<SubagentTask>>(&content) {
        Ok(tasks) => {
            let map: HashMap<String, SubagentTask> =
                tasks.into_iter().map(|t| (t.id.clone(), t)).collect();
            info!("Loaded {} tasks from legacy JSON {:?}", map.len(), path);
            Ok(map)
        }
        Err(e) => {
            warn!("Failed to parse tasks file: {}, starting fresh", e);
            Ok(HashMap::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_from_json_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tasks = rt.block_on(load_from_json(&path)).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_load_from_json_with_tasks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");

        // Write a valid JSON file
        let json = r#"[{
            "id": "test-1",
            "prompt": "test prompt",
            "channel": "cli",
            "chat_id": "chat1",
            "session_key": "session:key",
            "status": "Pending",
            "priority": "Normal",
            "created_at": "2024-01-01T00:00:00Z",
            "started_at": null,
            "completed_at": null,
            "result": null,
            "error": null,
            "timeout_secs": 300,
            "progress": 0,
            "metadata": {}
        }]"#;
        std::fs::write(&path, json).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let tasks = rt.block_on(load_from_json(&path)).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks["test-1"].prompt, "test prompt");
    }

    #[test]
    fn test_load_from_json_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");

        // Write invalid JSON
        std::fs::write(&path, "not valid json").unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let tasks = rt.block_on(load_from_json(&path)).unwrap();
        assert!(tasks.is_empty());
    }
}
