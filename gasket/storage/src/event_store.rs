//! Event store for event sourcing architecture.

use chrono::Utc;
use gasket_types::{EventType, SessionEvent};
use serde_json::Value;
use sqlx::SqlitePool;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Event store - core of event sourcing architecture.
pub struct EventStore {
    pool: SqlitePool,
}

impl EventStore {
    /// Create a new event store.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Append an event (O(1) operation).
    pub async fn append_event(&self, event: &SessionEvent) -> Result<(), StoreError> {
        let event_type_str = event_type_to_string(&event.event_type);
        let tools_used = serde_json::to_string(&event.metadata.tools_used)?;
        let token_usage = event
            .metadata
            .token_usage
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let extra = serde_json::to_string(&event.metadata.extra)?;

        // Extract event type specific fields
        let fields = extract_event_fields(&event.event_type);

        // Ensure session exists
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO sessions_v2 (key, created_at, updated_at) VALUES (?, ?, ?)",
        )
        .bind(&event.session_key)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        // Insert event
        sqlx::query(
            r#"
            INSERT INTO session_events
            (id, session_key, parent_id, event_type, content, embedding, branch,
             tools_used, token_usage, tool_name, tool_arguments, tool_call_id, is_error,
             summary_type, summary_topic, covered_events, merge_source, merge_head, extra, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(event.id.to_string())
        .bind(&event.session_key)
        .bind(event.parent_id.map(|id| id.to_string()))
        .bind(&event_type_str)
        .bind(&event.content)
        .bind(event.embedding.as_ref().map(|e| bytemuck::cast_slice(e) as &[u8]))
        .bind(event.metadata.branch.as_deref().unwrap_or("main"))
        .bind(&tools_used)
        .bind(token_usage.as_deref())
        .bind(fields.tool_name.as_deref())
        .bind(fields.tool_arguments.as_deref())
        .bind(fields.tool_call_id.as_deref())
        .bind(fields.is_error)
        .bind(fields.summary_type.as_deref())
        .bind(fields.summary_topic.as_deref())
        .bind(fields.covered_events.as_deref())
        .bind(fields.merge_source.as_deref())
        .bind(fields.merge_head.as_deref())
        .bind(&extra)
        .bind(event.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        // Update session metadata - read current branches, merge, and update
        let branch_name = event.metadata.branch.as_deref().unwrap_or("main");
        let current_branches: Option<String> =
            sqlx::query_scalar("SELECT branches FROM sessions_v2 WHERE key = ?")
                .bind(&event.session_key)
                .fetch_one(&self.pool)
                .await?;

        let mut branches: Value = current_branches
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::json!({}));

        if let Some(obj) = branches.as_object_mut() {
            obj.insert(branch_name.to_string(), Value::String(event.id.to_string()));
        }

        let branches_str = serde_json::to_string(&branches)?;

        sqlx::query(
            "UPDATE sessions_v2 SET updated_at = ?, total_events = total_events + 1, branches = ? WHERE key = ?",
        )
        .bind(&now)
        .bind(&branches_str)
        .bind(&event.session_key)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

fn event_type_to_string(event_type: &EventType) -> String {
    match event_type {
        EventType::UserMessage => "user_message".into(),
        EventType::AssistantMessage => "assistant_message".into(),
        EventType::ToolCall { .. } => "tool_call".into(),
        EventType::ToolResult { .. } => "tool_result".into(),
        EventType::Summary { .. } => "summary".into(),
        EventType::Merge { .. } => "merge".into(),
    }
}

/// Extracted event-specific fields for database storage.
#[derive(Default)]
struct EventFields {
    tool_name: Option<String>,
    tool_arguments: Option<String>,
    tool_call_id: Option<String>,
    is_error: Option<i32>,
    summary_type: Option<String>,
    summary_topic: Option<String>,
    covered_events: Option<String>,
    merge_source: Option<String>,
    merge_head: Option<String>,
}

fn extract_event_fields(event_type: &EventType) -> EventFields {
    match event_type {
        EventType::ToolCall {
            tool_name,
            arguments,
        } => EventFields {
            tool_name: Some(tool_name.clone()),
            tool_arguments: Some(arguments.to_string()),
            ..Default::default()
        },
        EventType::ToolResult {
            tool_call_id,
            tool_name,
            is_error,
        } => EventFields {
            tool_name: Some(tool_name.clone()),
            tool_call_id: Some(tool_call_id.clone()),
            is_error: Some(*is_error as i32),
            ..Default::default()
        },
        EventType::Summary {
            summary_type,
            covered_event_ids,
        } => {
            let (stype, topic) = match summary_type {
                gasket_types::SummaryType::TimeWindow { duration_hours } => {
                    (Some(format!("time_window:{}", duration_hours)), None)
                }
                gasket_types::SummaryType::Topic { topic } => {
                    (Some("topic".into()), Some(topic.clone()))
                }
                gasket_types::SummaryType::Compression { token_budget } => {
                    (Some(format!("compression:{}", token_budget)), None)
                }
            };
            let covered = serde_json::to_string(
                &covered_event_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>(),
            )
            .ok();
            EventFields {
                summary_type: stype,
                summary_topic: topic,
                covered_events: covered,
                ..Default::default()
            }
        }
        EventType::Merge {
            source_branch,
            source_head,
        } => EventFields {
            merge_source: Some(source_branch.clone()),
            merge_head: Some(source_head.to_string()),
            ..Default::default()
        },
        _ => EventFields::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use gasket_types::{EventMetadata, EventType};
    use sqlx::sqlite::SqlitePoolOptions;
    use uuid::Uuid;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new().connect(":memory:").await.unwrap();

        // Create tables
        sqlx::query(
            r#"
            CREATE TABLE sessions_v2 (
                key TEXT PRIMARY KEY,
                current_branch TEXT NOT NULL DEFAULT 'main',
                branches TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_consolidated_event TEXT,
                total_events INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            CREATE TABLE session_events (
                id TEXT PRIMARY KEY,
                session_key TEXT NOT NULL,
                parent_id TEXT,
                event_type TEXT NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB,
                branch TEXT DEFAULT 'main',
                tools_used TEXT DEFAULT '[]',
                token_usage TEXT,
                tool_name TEXT,
                tool_arguments TEXT,
                tool_call_id TEXT,
                is_error INTEGER DEFAULT 0,
                summary_type TEXT,
                summary_topic TEXT,
                covered_events TEXT,
                merge_source TEXT,
                merge_head TEXT,
                extra TEXT DEFAULT '{}',
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_append_user_message() {
        let pool = setup_test_db().await;
        let store = EventStore::new(pool);

        let event = SessionEvent {
            id: Uuid::now_v7(),
            session_key: "test:session".into(),
            parent_id: None,
            event_type: EventType::UserMessage,
            content: "Hello, world!".into(),
            embedding: None,
            metadata: EventMetadata::default(),
            created_at: Utc::now(),
        };

        store.append_event(&event).await.unwrap();

        // Verify event was stored
        let count: (i32,) = sqlx::query_as("SELECT COUNT(*) FROM session_events")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn test_append_tool_call() {
        let pool = setup_test_db().await;
        let store = EventStore::new(pool);

        let event = SessionEvent {
            id: Uuid::now_v7(),
            session_key: "test:session".into(),
            parent_id: None,
            event_type: EventType::ToolCall {
                tool_name: "read_file".into(),
                arguments: serde_json::json!({"path": "/test.txt"}),
            },
            content: "".into(),
            embedding: None,
            metadata: EventMetadata::default(),
            created_at: Utc::now(),
        };

        store.append_event(&event).await.unwrap();

        // Verify tool_name was stored
        let row: (String,) =
            sqlx::query_as("SELECT tool_name FROM session_events WHERE event_type = 'tool_call'")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert_eq!(row.0, "read_file");
    }

    #[tokio::test]
    async fn test_append_tool_result() {
        let pool = setup_test_db().await;
        let store = EventStore::new(pool);

        let event = SessionEvent {
            id: Uuid::now_v7(),
            session_key: "test:session".into(),
            parent_id: None,
            event_type: EventType::ToolResult {
                tool_call_id: "call_123".into(),
                tool_name: "read_file".into(),
                is_error: false,
            },
            content: "file contents".into(),
            embedding: None,
            metadata: EventMetadata::default(),
            created_at: Utc::now(),
        };

        store.append_event(&event).await.unwrap();

        // Verify tool_result fields were stored
        let row: (String, String, i32) = sqlx::query_as(
            "SELECT tool_name, tool_call_id, is_error FROM session_events WHERE event_type = 'tool_result'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(row.0, "read_file");
        assert_eq!(row.1, "call_123");
        assert_eq!(row.2, 0);
    }

    #[tokio::test]
    async fn test_append_summary_event() {
        let pool = setup_test_db().await;
        let store = EventStore::new(pool);

        let covered_ids = vec![Uuid::now_v7(), Uuid::now_v7()];
        let event = SessionEvent {
            id: Uuid::now_v7(),
            session_key: "test:session".into(),
            parent_id: None,
            event_type: EventType::Summary {
                summary_type: gasket_types::SummaryType::Topic {
                    topic: "discussion about API".into(),
                },
                covered_event_ids: covered_ids.clone(),
            },
            content: "Summary of the discussion...".into(),
            embedding: None,
            metadata: EventMetadata::default(),
            created_at: Utc::now(),
        };

        store.append_event(&event).await.unwrap();

        // Verify summary fields were stored
        let row: (String, Option<String>) = sqlx::query_as(
            "SELECT summary_type, summary_topic FROM session_events WHERE event_type = 'summary'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(row.0, "topic");
        assert_eq!(row.1, Some("discussion about API".to_string()));
    }

    #[tokio::test]
    async fn test_session_auto_created() {
        let pool = setup_test_db().await;
        let store = EventStore::new(pool);

        let event = SessionEvent {
            id: Uuid::now_v7(),
            session_key: "auto:session".into(),
            parent_id: None,
            event_type: EventType::UserMessage,
            content: "Test".into(),
            embedding: None,
            metadata: EventMetadata::default(),
            created_at: Utc::now(),
        };

        store.append_event(&event).await.unwrap();

        // Verify session was auto-created
        let count: (i32,) = sqlx::query_as("SELECT COUNT(*) FROM sessions_v2")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(count.0, 1);

        // Verify total_events was incremented
        let total_events: (i32,) =
            sqlx::query_as("SELECT total_events FROM sessions_v2 WHERE key = 'auto:session'")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert_eq!(total_events.0, 1);
    }

    #[tokio::test]
    async fn test_branch_tracking() {
        let pool = setup_test_db().await;
        let store = EventStore::new(pool);

        let event = SessionEvent {
            id: Uuid::now_v7(),
            session_key: "test:session".into(),
            parent_id: None,
            event_type: EventType::UserMessage,
            content: "Test".into(),
            embedding: None,
            metadata: EventMetadata {
                branch: Some("feature".into()),
                ..Default::default()
            },
            created_at: Utc::now(),
        };

        store.append_event(&event).await.unwrap();

        // Verify branch is tracked in event
        let branch: (String,) =
            sqlx::query_as("SELECT branch FROM session_events WHERE session_key = 'test:session'")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert_eq!(branch.0, "feature");

        // Verify branches JSON was updated
        let branches: (String,) =
            sqlx::query_as("SELECT branches FROM sessions_v2 WHERE key = 'test:session'")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert!(branches.0.contains("feature"));
    }
}
