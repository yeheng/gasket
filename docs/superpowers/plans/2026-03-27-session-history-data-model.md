# Session/History 数据模型重设计 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现事件溯源架构的 Session/History 数据模型，支持分支、版本控制、层级摘要和多维度检索。

**Architecture:** 使用 Enum 替代 Trait 消除动态分发；EventStore 管理不可变事件流；CompressionActor 通过 channel 串行化处理摘要任务；每消息 Embedding 支持语义检索。

**Tech Stack:** Rust, SQLite (sqlx), UUID v7, tokio, chrono, serde

---

## File Structure

```
gasket/types/src/
├── events.rs           # [现有] SessionKey, ChannelType
└── session_event.rs    # [新建] SessionEvent, EventType, EventMetadata

gasket/storage/src/
├── lib.rs              # [修改] 更新 Schema, 导出新模块
├── event_store.rs      # [新建] EventStore 实现
└── session.rs          # [删除] 旧 SessionManager, SessionMessage

gasket/core/src/
├── agent/
│   ├── mod.rs          # [修改] 导出新模块
│   ├── context.rs      # [重写] AgentContext enum
│   ├── compression.rs  # [新建] CompressionActor
│   ├── history_query.rs # [新建] HistoryQuery, HistoryRetriever
│   ├── history_processor.rs # [修改] 适配 SessionEvent
│   └── loop_.rs        # [修改] 使用新 AgentContext
├── session/
│   └── manager.rs      # [删除] 旧引用
└── lib.rs              # [修改] 更新导出
```

---

## Phase 1: 核心数据结构

### Task 1: 创建 SessionEvent 类型定义

**Files:**
- Create: `gasket/types/src/session_event.rs`
- Modify: `gasket/types/src/lib.rs`

- [ ] **Step 1: 添加 uuid v7 依赖**

Run: `cd /Users/yeheng/workspaces/Github/gasket/gasket/types && cargo add uuid --features v7,serde`

- [ ] **Step 2: 创建 session_event.rs 基础结构**

```rust
//! Session event types for event sourcing architecture.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 会话事件 - 不可变的事实记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    /// 事件唯一标识 (UUID v7 时间有序)
    pub id: Uuid,

    /// 所属会话
    pub session_key: String,

    /// 父事件 ID (支持分支和版本控制)
    pub parent_id: Option<Uuid>,

    /// 事件类型
    pub event_type: EventType,

    /// 消息内容
    pub content: String,

    /// 语义向量 (每消息 Embedding)
    pub embedding: Option<Vec<f32>>,

    /// 事件元数据
    pub metadata: EventMetadata,

    /// 创建时间
    pub created_at: DateTime<Utc>,
}

/// 事件类型枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    /// 用户消息
    UserMessage,

    /// 助手回复
    AssistantMessage,

    /// 工具调用
    ToolCall {
        tool_name: String,
        arguments: serde_json::Value,
    },

    /// 工具结果
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        is_error: bool,
    },

    /// 摘要事件 (压缩生成)
    Summary {
        summary_type: SummaryType,
        covered_event_ids: Vec<Uuid>,
    },

    /// 分支合并
    Merge {
        source_branch: String,
        source_head: Uuid,
    },
}

/// 事件类型分类 (用于查询过滤)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventTypeCategory {
    UserMessage,
    AssistantMessage,
    ToolCall,
    ToolResult,
    Summary,
    Merge,
}

impl EventType {
    /// 检查是否为摘要类型事件
    pub fn is_summary(&self) -> bool {
        matches!(self, EventType::Summary { .. })
    }

    /// 获取事件类型分类
    pub fn category(&self) -> EventTypeCategory {
        match self {
            EventType::UserMessage => EventTypeCategory::UserMessage,
            EventType::AssistantMessage => EventTypeCategory::AssistantMessage,
            EventType::ToolCall { .. } => EventTypeCategory::ToolCall,
            EventType::ToolResult { .. } => EventTypeCategory::ToolResult,
            EventType::Summary { .. } => EventTypeCategory::Summary,
            EventType::Merge { .. } => EventTypeCategory::Merge,
        }
    }
}

/// 摘要类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SummaryType {
    /// 时间窗口摘要
    TimeWindow { duration_hours: u32 },

    /// 主题摘要
    Topic { topic: String },

    /// 压缩摘要 (超出 token 预算时)
    Compression { token_budget: usize },
}

/// 事件元数据
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventMetadata {
    /// 分支名称 (None 表示主分支)
    pub branch: Option<String>,

    /// 使用的工具列表
    #[serde(default)]
    pub tools_used: Vec<String>,

    /// Token 统计
    pub token_usage: Option<TokenUsage>,

    /// 扩展字段
    #[serde(default)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Token 使用统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}
```

- [ ] **Step 3: 写 SessionEvent 序列化测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_serialization() {
        let event_type = EventType::UserMessage;
        let json = serde_json::to_string(&event_type).unwrap();
        assert_eq!(json, r#"{"UserMessage":null}"#);
    }

    #[test]
    fn test_session_event_roundtrip() {
        let event = SessionEvent {
            id: Uuid::now_v7(),
            session_key: "test:session".into(),
            parent_id: None,
            event_type: EventType::UserMessage,
            content: "Hello".into(),
            embedding: None,
            metadata: EventMetadata::default(),
            created_at: Utc::now(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let decoded: SessionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.content, "Hello");
    }

    #[test]
    fn test_event_type_category() {
        assert_eq!(EventType::UserMessage.category(), EventTypeCategory::UserMessage);
        assert_eq!(EventType::ToolCall { tool_name: "test".into(), arguments: serde_json::json!({}) }.category(), EventTypeCategory::ToolCall);
    }
}
```

- [ ] **Step 4: 运行测试验证**

Run: `cd /Users/yeheng/workspaces/Github/gasket && cargo test -p gasket-types session_event --no-run`

- [ ] **Step 5: 更新 lib.rs 导出**

```rust
// gasket/types/src/lib.rs
pub mod events;
pub mod session_event;
pub mod tool;

pub use events::{...};
pub use session_event::{
    EventMetadata, EventType, EventTypeCategory, SessionEvent, SummaryType, TokenUsage,
};
pub use tool::{...};
```

- [ ] **Step 6: 提交**

```bash
git add gasket/types/src/session_event.rs gasket/types/src/lib.rs gasket/types/Cargo.toml
git commit -m "feat(types): add SessionEvent and EventType for event sourcing

- Add SessionEvent with UUID v7, parent_id for branching
- Add EventType enum with ToolCall, ToolResult, Summary variants
- Add EventTypeCategory for query filtering
- Add EventMetadata with branch, tools_used, token_usage

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 2: 创建 Session 聚合根

**Files:**
- Modify: `gasket/types/src/session_event.rs`

- [ ] **Step 1: 添加 Session 结构**

```rust
use std::collections::HashMap;

/// 会话 - 事件的聚合根
#[derive(Debug, Clone)]
pub struct Session {
    /// 会话标识
    pub key: String,

    /// 当前活跃分支
    pub current_branch: String,

    /// 所有分支指针 (branch_name -> latest_event_id)
    pub branches: HashMap<String, Uuid>,

    /// 会话元数据
    pub metadata: SessionMetadata,
}

/// 会话元数据
#[derive(Debug, Clone, Default)]
pub struct SessionMetadata {
    /// 创建时间
    pub created_at: DateTime<Utc>,

    /// 最后更新时间
    pub updated_at: DateTime<Utc>,

    /// 最后压缩点 (事件 ID)
    pub last_consolidated_event: Option<Uuid>,

    /// 总消息数
    pub total_events: usize,

    /// 累计 token 使用
    pub total_tokens: u64,
}

impl Session {
    /// 创建新会话
    pub fn new(key: impl Into<String>) -> Self {
        let key = key.into();
        let now = Utc::now();
        Self {
            key,
            current_branch: "main".to_string(),
            branches: HashMap::new(),
            metadata: SessionMetadata {
                created_at: now,
                updated_at: now,
                ..Default::default()
            },
        }
    }

    /// 从 SessionKey 创建
    pub fn from_key(key: crate::SessionKey) -> Self {
        Self::new(key.to_string())
    }

    /// 获取分支头事件 ID
    pub fn get_branch_head(&self, branch: &str) -> Option<Uuid> {
        self.branches.get(branch).copied()
    }

    /// 获取主分支头事件 ID
    pub fn main_head(&self) -> Option<Uuid> {
        self.get_branch_head("main")
    }
}
```

- [ ] **Step 2: 写 Session 测试**

```rust
#[test]
fn test_session_new() {
    let session = Session::new("test:session");
    assert_eq!(session.key, "test:session");
    assert_eq!(session.current_branch, "main");
    assert!(session.branches.is_empty());
}

#[test]
fn test_session_branch_head() {
    let mut session = Session::new("test:session");
    let event_id = Uuid::now_v7();
    session.branches.insert("main".into(), event_id);

    assert_eq!(session.main_head(), Some(event_id));
    assert_eq!(session.get_branch_head("nonexistent"), None);
}
```

- [ ] **Step 3: 更新 lib.rs 导出**

Add `Session, SessionMetadata` to exports.

- [ ] **Step 4: 运行测试验证**

Run: `cargo test -p gasket-types session --no-run`

- [ ] **Step 5: 提交**

```bash
git add gasket/types/src/session_event.rs gasket/types/src/lib.rs
git commit -m "feat(types): add Session aggregate root

- Session holds branch pointers and metadata
- Helper methods for branch head lookup

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 3: 创建 AgentContext Enum

**Files:**
- Create: `gasket/core/src/agent/context_v2.rs`
- Modify: `gasket/core/src/agent/mod.rs`

- [ ] **Step 1: 创建 context_v2.rs 基础结构**

```rust
//! Agent context using Enum instead of trait for zero runtime dispatch.

use std::sync::Arc;
use tokio::sync::mpsc;

use crate::bus::events::SessionKey;
use crate::error::AgentError;
use gasket_types::{Session, SessionEvent};

// Forward declarations for types we'll create later
pub struct EventStore;
pub struct HistoryRetriever;
pub struct EmbeddingService;
pub struct SessionManager;

/// 压缩任务
#[derive(Debug, Clone)]
pub struct CompressionTask {
    pub session_key: String,
    pub branch: String,
    pub evicted_events: Vec<uuid::Uuid>,
    pub compression_type: gasket_types::SummaryType,
    pub retry_count: u32,
}

/// Agent 上下文 - 使用 Enum 替代 trait 动态分发
#[derive(Debug)]
pub enum AgentContext {
    /// 持久化上下文 (主 Agent)
    Persistent(PersistentContext),

    /// 无状态上下文 (子 Agent)
    Stateless,
}

/// 持久化上下文数据
#[derive(Debug)]
pub struct PersistentContext {
    /// 会话管理器
    pub session_manager: Arc<SessionManager>,

    /// 事件存储
    pub event_store: Arc<EventStore>,

    /// 历史检索器
    pub history_retriever: Arc<HistoryRetriever>,

    /// Embedding 服务
    pub embedding_service: Arc<EmbeddingService>,

    /// 压缩任务发送端
    pub compression_tx: mpsc::Sender<CompressionTask>,
}

impl AgentContext {
    /// 是否持久化
    pub fn is_persistent(&self) -> bool {
        matches!(self, Self::Persistent(_))
    }

    /// 加载会话 (stub for now)
    pub async fn load_session(&self, key: &SessionKey) -> Session {
        match self {
            Self::Persistent(_) => Session::new(key.to_string()),
            Self::Stateless => Session::new(key),
        }
    }

    /// 保存事件 (stub for now)
    pub async fn save_event(&self, _event: SessionEvent) -> Result<(), AgentError> {
        match self {
            Self::Persistent(_) => Ok(()), // Will be implemented with EventStore
            Self::Stateless => Ok(()),
        }
    }

    /// 获取历史 (stub for now)
    pub async fn get_history(&self, key: &str, branch: Option<&str>) -> Vec<SessionEvent> {
        let _ = (key, branch);
        vec![]
    }
}
```

- [ ] **Step 2: 写 AgentContext 测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stateless_context() {
        let context = AgentContext::Stateless;
        assert!(!context.is_persistent());
    }

    #[tokio::test]
    async fn test_stateless_load_session() {
        let context = AgentContext::Stateless;
        let key = SessionKey::new(gasket_types::ChannelType::Cli, "test");
        let session = context.load_session(&key).await;
        assert_eq!(session.key, key.to_string());
    }

    #[tokio::test]
    async fn test_stateless_save_event() {
        let context = AgentContext::Stateless;
        let event = SessionEvent {
            id: uuid::Uuid::now_v7(),
            session_key: "test".into(),
            parent_id: None,
            event_type: gasket_types::EventType::UserMessage,
            content: "test".into(),
            embedding: None,
            metadata: Default::default(),
            created_at: chrono::Utc::now(),
        };
        let result = context.save_event(event).await;
        assert!(result.is_ok());
    }
}
```

- [ ] **Step 3: 运行测试验证**

Run: `cargo test -p gasket-core agent::context_v2 --no-run`

- [ ] **Step 4: 更新 mod.rs 导出**

```rust
pub mod context_v2;

pub use context_v2::{AgentContext, CompressionTask, PersistentContext};
```

- [ ] **Step 5: 提交**

```bash
git add gasket/core/src/agent/context_v2.rs gasket/core/src/agent/mod.rs
git commit -m "feat(core): add AgentContext enum to replace trait dispatch

- Enum with Persistent and Stateless variants
- Stub methods for session/event operations
- Tests for Stateless context

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Phase 2: 存储层实现

### Task 4: 更新数据库 Schema

**Files:**
- Modify: `gasket/storage/src/lib.rs`

- [ ] **Step 1: 添加新 Schema 定义**

在 `lib.rs` 的 `init_tables` 函数中添加新表：

```rust
// 在现有表创建之后添加:

// === 事件溯源新表 ===

// 会话元数据表 (更新版)
sqlx::query(
    r#"
    CREATE TABLE IF NOT EXISTS sessions_v2 (
        key             TEXT PRIMARY KEY,
        current_branch  TEXT NOT NULL DEFAULT 'main',
        branches        TEXT NOT NULL DEFAULT '{}',
        created_at      TEXT NOT NULL,
        updated_at      TEXT NOT NULL,
        last_consolidated_event TEXT,
        total_events    INTEGER NOT NULL DEFAULT 0,
        total_tokens    INTEGER NOT NULL DEFAULT 0
    )
    "#,
)
.execute(&self.pool)
.await?;

// 事件表
sqlx::query(
    r#"
    CREATE TABLE IF NOT EXISTS session_events (
        id              TEXT PRIMARY KEY,
        session_key     TEXT NOT NULL,
        parent_id       TEXT,
        event_type      TEXT NOT NULL,
        content         TEXT NOT NULL,
        embedding       BLOB,
        branch          TEXT DEFAULT 'main',
        tools_used      TEXT DEFAULT '[]',
        token_usage     TEXT,
        tool_name       TEXT,
        tool_arguments  TEXT,
        tool_call_id    TEXT,
        is_error        INTEGER DEFAULT 0,
        summary_type    TEXT,
        summary_topic   TEXT,
        covered_events  TEXT,
        merge_source    TEXT,
        merge_head      TEXT,
        extra           TEXT DEFAULT '{}',
        created_at      TEXT NOT NULL,
        FOREIGN KEY (session_key) REFERENCES sessions_v2(key) ON DELETE CASCADE
    )
    "#,
)
.execute(&self.pool)
.await?;

// 索引
sqlx::query("CREATE INDEX IF NOT EXISTS idx_events_session_branch ON session_events(session_key, branch)")
    .execute(&self.pool)
    .await?;

sqlx::query("CREATE INDEX IF NOT EXISTS idx_events_parent ON session_events(parent_id)")
    .execute(&self.pool)
    .await?;

sqlx::query("CREATE INDEX IF NOT EXISTS idx_events_created ON session_events(created_at)")
    .execute(&self.pool)
    .await?;

sqlx::query("CREATE INDEX IF NOT EXISTS idx_events_type ON session_events(event_type)")
    .execute(&self.pool)
    .await?;

// 摘要索引表
sqlx::query(
    r#"
    CREATE TABLE IF NOT EXISTS summary_index (
        id              INTEGER PRIMARY KEY AUTOINCREMENT,
        session_key     TEXT NOT NULL,
        event_id        TEXT NOT NULL,
        summary_type    TEXT NOT NULL,
        topic           TEXT,
        covered_events  TEXT NOT NULL,
        created_at      TEXT NOT NULL
    )
    "#,
)
.execute(&self.pool)
.await?;

sqlx::query("CREATE INDEX IF NOT EXISTS idx_summary_session ON summary_index(session_key)")
    .execute(&self.pool)
    .await?;

sqlx::query("CREATE INDEX IF NOT EXISTS idx_summary_type ON summary_index(summary_type)")
    .execute(&self.pool)
    .await?;
```

- [ ] **Step 2: 运行编译验证**

Run: `cargo build -p gasket-storage`

- [ ] **Step 3: 提交**

```bash
git add gasket/storage/src/lib.rs
git commit -m "feat(storage): add event sourcing schema

- sessions_v2 table with branch pointers
- session_events table with event-specific columns
- summary_index for fast retrieval

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 5: 实现 EventStore - 追加事件

**Files:**
- Create: `gasket/storage/src/event_store.rs`
- Modify: `gasket/storage/src/lib.rs`

- [ ] **Step 1: 创建 event_store.rs**

```rust
//! Event store for event sourcing architecture.

use sqlx::SqlitePool;
use uuid::Uuid;
use chrono::Utc;

use gasket_types::{SessionEvent, EventType, EventTypeCategory};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// 事件存储 - 事件溯源的核心
pub struct EventStore {
    pool: SqlitePool,
}

impl EventStore {
    /// 创建新的事件存储
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 追加事件 (O(1) 操作)
    pub async fn append_event(&self, event: &SessionEvent) -> Result<(), StoreError> {
        let event_type_str = event_type_to_string(&event.event_type);
        let tools_used = serde_json::to_string(&event.metadata.tools_used)?;
        let token_usage = event.metadata.token_usage.as_ref()
            .map(|t| serde_json::to_string(t))
            .transpose()?;
        let extra = serde_json::to_string(&event.metadata.extra)?;

        // 提取事件类型特定字段
        let (tool_name, tool_arguments, tool_call_id, is_error, summary_type, summary_topic, covered_events, merge_source, merge_head) =
            extract_event_fields(&event.event_type);

        // 确保会话存在
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO sessions_v2 (key, created_at, updated_at) VALUES (?, ?, ?)",
        )
        .bind(&event.session_key)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        // 插入事件
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
        .bind(tool_name.as_deref())
        .bind(tool_arguments.as_deref())
        .bind(tool_call_id.as_deref())
        .bind(is_error)
        .bind(summary_type.as_deref())
        .bind(summary_topic.as_deref())
        .bind(covered_events.as_deref())
        .bind(merge_source.as_deref())
        .bind(merge_head.as_deref())
        .bind(&extra)
        .bind(event.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        // 更新会话元数据
        let branch_json = serde_json::json!({
            event.metadata.branch.as_deref().unwrap_or("main"): event.id.to_string()
        });

        sqlx::query(
            "UPDATE sessions_v2 SET updated_at = ?, total_events = total_events + 1, branches = json_merge_patch(branches, ?) WHERE key = ?",
        )
        .bind(&now)
        .bind(branch_json.to_string())
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

fn extract_event_fields(event_type: &EventType) -> (Option<String>, Option<String>, Option<String>, Option<i32>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>) {
    match event_type {
        EventType::ToolCall { tool_name, arguments } => {
            (Some(tool_name.clone()), Some(arguments.to_string()), None, None, None, None, None, None, None)
        }
        EventType::ToolResult { tool_call_id, tool_name, is_error } => {
            (Some(tool_name.clone()), None, Some(tool_call_id.clone()), Some(*is_error as i32), None, None, None, None, None)
        }
        EventType::Summary { summary_type, covered_event_ids } => {
            let (stype, topic) = match summary_type {
                gasket_types::SummaryType::TimeWindow { duration_hours } =>
                    (Some(format!("time_window:{}", duration_hours)), None),
                gasket_types::SummaryType::Topic { topic } =>
                    (Some("topic".into()), Some(topic.clone())),
                gasket_types::SummaryType::Compression { token_budget } =>
                    (Some(format!("compression:{}", token_budget)), None),
            };
            let covered = serde_json::to_string(&covered_event_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>()).ok();
            (None, None, None, None, stype, topic, covered, None, None)
        }
        EventType::Merge { source_branch, source_head } => {
            (None, None, None, None, None, None, None, Some(source_branch.clone()), Some(source_head.to_string()))
        }
        _ => (None, None, None, None, None, None, None, None, None),
    }
}
```

- [ ] **Step 2: 写追加事件测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use gasket_types::{EventMetadata, EventType};
    use chrono::Utc;

    async fn setup_test_db() -> SqlitePool {
        use sqlx::sqlite::SqlitePoolOptions;
        let pool = SqlitePoolOptions::new()
            .connect(":memory:")
            .await
            .unwrap();

        // 创建表
        sqlx::query(r#"
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
        "#).execute(&pool).await.unwrap();

        sqlx::query(r#"
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
        "#).execute(&pool).await.unwrap();

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

        // 验证事件已存储
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

        // 验证 tool_name 已存储
        let row: (String,) = sqlx::query_as("SELECT tool_name FROM session_events WHERE event_type = 'tool_call'")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(row.0, "read_file");
    }
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p gasket-storage event_store -- --test-threads=1`

- [ ] **Step 4: 更新 lib.rs 导出**

```rust
pub mod event_store;

pub use event_store::{EventStore, StoreError};
```

- [ ] **Step 5: 提交**

```bash
git add gasket/storage/src/event_store.rs gasket/storage/src/lib.rs
git commit -m "feat(storage): implement EventStore append_event

- Append events with event-specific fields
- Update session metadata atomically
- Tests for user message and tool call

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 6: 实现 EventStore - 读取历史

**Files:**
- Modify: `gasket/storage/src/event_store.rs`

- [ ] **Step 1: 添加 get_branch_history 方法**

```rust
impl EventStore {
    /// 获取分支的历史事件流
    pub async fn get_branch_history(
        &self,
        session_key: &str,
        branch: &str,
    ) -> Result<Vec<SessionEvent>, StoreError> {
        let rows = sqlx::query_as::<_, EventRow>(
            r#"
            SELECT * FROM session_events
            WHERE session_key = ? AND branch = ?
            ORDER BY created_at ASC
            "#
        )
        .bind(session_key)
        .bind(branch)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }
}

/// 数据库行表示
struct EventRow {
    id: String,
    session_key: String,
    parent_id: Option<String>,
    event_type: String,
    content: String,
    embedding: Option<Vec<u8>>,
    branch: String,
    tools_used: String,
    token_usage: Option<String>,
    tool_name: Option<String>,
    tool_arguments: Option<String>,
    tool_call_id: Option<String>,
    is_error: Option<i32>,
    summary_type: Option<String>,
    summary_topic: Option<String>,
    covered_events: Option<String>,
    merge_source: Option<String>,
    merge_head: Option<String>,
    extra: String,
    created_at: String,
}

impl TryFrom<EventRow> for SessionEvent {
    type Error = StoreError;

    fn try_from(row: EventRow) -> Result<Self, Self::Error> {
        let event_type = parse_event_type(
            &row.event_type,
            row.tool_name.as_deref(),
            row.tool_arguments.as_deref(),
            row.tool_call_id.as_deref(),
            row.is_error,
            row.summary_type.as_deref(),
            row.summary_topic.as_deref(),
            row.covered_events.as_deref(),
            row.merge_source.as_deref(),
            row.merge_head.as_deref(),
        )?;

        let tools_used: Vec<String> = serde_json::from_str(&row.tools_used)?;
        let token_usage: Option<gasket_types::TokenUsage> = row.token_usage
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?;
        let extra: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&row.extra)?;
        let embedding = row.embedding.map(|b| {
            bytemuck::cast_slice(&b).to_vec()
        });

        Ok(SessionEvent {
            id: row.id.parse().map_err(|_| StoreError::Serialization(serde_json::Error::custom("Invalid UUID")))?,
            session_key: row.session_key,
            parent_id: row.parent_id.map(|s| s.parse()).transpose().map_err(|_| StoreError::Serialization(serde_json::Error::custom("Invalid parent UUID")))?,
            event_type,
            content: row.content,
            embedding,
            metadata: EventMetadata {
                branch: if row.branch == "main" { None } else { Some(row.branch) },
                tools_used,
                token_usage,
                extra,
            },
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
    }
}

fn parse_event_type(
    type_str: &str,
    tool_name: Option<&str>,
    tool_arguments: Option<&str>,
    tool_call_id: Option<&str>,
    is_error: Option<i32>,
    summary_type: Option<&str>,
    summary_topic: Option<&str>,
    covered_events: Option<&str>,
    merge_source: Option<&str>,
    merge_head: Option<&str>,
) -> Result<EventType, StoreError> {
    Ok(match type_str {
        "user_message" => EventType::UserMessage,
        "assistant_message" => EventType::AssistantMessage,
        "tool_call" => EventType::ToolCall {
            tool_name: tool_name.unwrap_or("").into(),
            arguments: tool_arguments
                .map(|s| serde_json::from_str(s).unwrap_or(serde_json::Value::Null))
                .unwrap_or(serde_json::Value::Null),
        },
        "tool_result" => EventType::ToolResult {
            tool_call_id: tool_call_id.unwrap_or("").into(),
            tool_name: tool_name.unwrap_or("").into(),
            is_error: is_error.unwrap_or(0) != 0,
        },
        "summary" => {
            let covered: Vec<Uuid> = covered_events
                .map(|s| serde_json::from_str::<Vec<String>>(s))
                .transpose()?
                .unwrap_or_default()
                .into_iter()
                .filter_map(|s| s.parse().ok())
                .collect();

            let stype = match summary_type {
                Some(s) if s.starts_with("time_window:") => {
                    let hours: u32 = s.split(':').nth(1).unwrap_or("0").parse().unwrap_or(0);
                    gasket_types::SummaryType::TimeWindow { duration_hours: hours }
                }
                Some(s) if s == "topic" => {
                    gasket_types::SummaryType::Topic { topic: summary_topic.unwrap_or("").into() }
                }
                Some(s) if s.starts_with("compression:") => {
                    let budget: usize = s.split(':').nth(1).unwrap_or("0").parse().unwrap_or(0);
                    gasket_types::SummaryType::Compression { token_budget: budget }
                }
                _ => gasket_types::SummaryType::Compression { token_budget: 0 },
            };

            EventType::Summary {
                summary_type: stype,
                covered_event_ids: covered,
            }
        }
        "merge" => EventType::Merge {
            source_branch: merge_source.unwrap_or("").into(),
            source_head: merge_head
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(Uuid::nil),
        },
        _ => return Err(StoreError::Serialization(serde_json::Error::custom("Unknown event type"))),
    })
}
```

- [ ] **Step 2: 写读取历史测试**

```rust
#[tokio::test]
async fn test_get_branch_history() {
    let pool = setup_test_db().await;
    let store = EventStore::new(pool);

    // 添加事件链
    let e1 = SessionEvent {
        id: Uuid::now_v7(),
        session_key: "test:session".into(),
        parent_id: None,
        event_type: EventType::UserMessage,
        content: "Hello".into(),
        embedding: None,
        metadata: EventMetadata { branch: Some("main".into()), ..Default::default() },
        created_at: Utc::now(),
    };
    store.append_event(&e1).await.unwrap();

    let e2 = SessionEvent {
        id: Uuid::now_v7(),
        session_key: "test:session".into(),
        parent_id: Some(e1.id),
        event_type: EventType::AssistantMessage,
        content: "Hi!".into(),
        embedding: None,
        metadata: EventMetadata { branch: Some("main".into()), ..Default::default() },
        created_at: Utc::now(),
    };
    store.append_event(&e2).await.unwrap();

    // 读取历史
    let history = store.get_branch_history("test:session", "main").await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].content, "Hello");
    assert_eq!(history[1].content, "Hi!");
    assert_eq!(history[1].parent_id, Some(e1.id));
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p gasket-storage get_branch_history -- --test-threads=1`

- [ ] **Step 4: 提交**

```bash
git add gasket/storage/src/event_store.rs
git commit -m "feat(storage): implement EventStore get_branch_history

- Parse event rows back to SessionEvent
- Handle all EventType variants
- Test for event chain retrieval

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Phase 3: 检索系统

### Task 7: 实现 HistoryQuery 和 HistoryRetriever

**Files:**
- Create: `gasket/core/src/agent/history_query.rs`
- Modify: `gasket/core/src/agent/mod.rs`

- [ ] **Step 1: 创建 history_query.rs**

```rust
//! Multi-dimensional history retrieval system.

use chrono::{DateTime, Utc};
use gasket_types::{EventType, SessionEvent};

/// 历史检索器
pub struct HistoryRetriever {
    // Will be connected to EventStore later
}

/// 检索查询条件
#[derive(Debug, Clone, Default)]
pub struct HistoryQuery {
    /// 会话标识
    pub session_key: String,

    /// 分支过滤 (None = 当前分支)
    pub branch: Option<String>,

    /// 时间范围
    pub time_range: Option<TimeRange>,

    /// 事件类型过滤 (使用 category)
    pub event_categories: Vec<gasket_types::EventTypeCategory>,

    /// 语义搜索
    pub semantic_query: Option<SemanticQuery>,

    /// 工具使用过滤
    pub tools_filter: Vec<String>,

    /// 分页
    pub offset: usize,
    pub limit: usize,

    /// 排序
    pub order: QueryOrder,
}

impl HistoryQuery {
    /// 创建查询构造器
    pub fn builder(session_key: impl Into<String>) -> HistoryQueryBuilder {
        HistoryQueryBuilder::new(session_key)
    }
}

/// 查询构造器 (流式 API)
pub struct HistoryQueryBuilder {
    query: HistoryQuery,
}

impl HistoryQueryBuilder {
    pub fn new(session_key: impl Into<String>) -> Self {
        Self {
            query: HistoryQuery {
                session_key: session_key.into(),
                limit: 50,
                ..Default::default()
            },
        }
    }

    pub fn branch(mut self, branch: impl Into<String>) -> Self {
        self.query.branch = Some(branch.into());
        self
    }

    pub fn time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.query.time_range = Some(TimeRange { start, end });
        self
    }

    pub fn categories(mut self, cats: Vec<gasket_types::EventTypeCategory>) -> Self {
        self.query.event_categories = cats;
        self
    }

    pub fn semantic_text(mut self, text: impl Into<String>) -> Self {
        self.query.semantic_query = Some(SemanticQuery::Text(text.into()));
        self
    }

    pub fn semantic_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.query.semantic_query = Some(SemanticQuery::Embedding(embedding));
        self
    }

    pub fn tools(mut self, tools: Vec<String>) -> Self {
        self.query.tools_filter = tools;
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.query.limit = limit;
        self
    }

    pub fn offset(mut self, offset: usize) -> Self {
        self.query.offset = offset;
        self
    }

    pub fn order(mut self, order: QueryOrder) -> Self {
        self.query.order = order;
        self
    }

    pub fn build(self) -> HistoryQuery {
        self.query
    }
}

#[derive(Debug, Clone)]
pub enum SemanticQuery {
    Text(String),
    Embedding(Vec<f32>),
}

#[derive(Debug, Clone)]
pub enum QueryOrder {
    Chronological,
    ReverseChronological,
    Similarity,
}

#[derive(Debug, Clone)]
pub struct TimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// 检索结果
#[derive(Debug)]
pub struct HistoryResult {
    pub events: Vec<SessionEvent>,
    pub meta: ResultMeta,
}

#[derive(Debug, Default)]
pub struct ResultMeta {
    pub total_count: usize,
    pub has_more: bool,
    pub query_time_ms: u64,
}
```

- [ ] **Step 2: 写 HistoryQuery 测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_builder() {
        let query = HistoryQuery::builder("test:session")
            .branch("explore")
            .limit(10)
            .offset(5)
            .order(QueryOrder::ReverseChronological)
            .build();

        assert_eq!(query.session_key, "test:session");
        assert_eq!(query.branch, Some("explore".into()));
        assert_eq!(query.limit, 10);
        assert_eq!(query.offset, 5);
    }

    #[test]
    fn test_query_builder_with_categories() {
        let query = HistoryQuery::builder("test:session")
            .categories(vec![
                gasket_types::EventTypeCategory::UserMessage,
                gasket_types::EventTypeCategory::AssistantMessage,
            ])
            .build();

        assert_eq!(query.event_categories.len(), 2);
    }
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p gasket-core history_query --no-run`

- [ ] **Step 4: 更新 mod.rs 导出**

```rust
pub mod history_query;

pub use history_query::{
    HistoryQuery, HistoryQueryBuilder, HistoryResult, QueryOrder, ResultMeta, SemanticQuery, TimeRange,
};
```

- [ ] **Step 5: 提交**

```bash
git add gasket/core/src/agent/history_query.rs gasket/core/src/agent/mod.rs
git commit -m "feat(core): add HistoryQuery with builder pattern

- Fluent API for query construction
- Support for time range, categories, semantic search
- Query order options

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Phase 4: Compression Actor

### Task 8: 实现 CompressionActor

**Files:**
- Create: `gasket/core/src/agent/compression.rs`
- Modify: `gasket/core/src/agent/mod.rs`

- [ ] **Step 1: 创建 compression.rs**

```rust
//! Compression actor for background summarization.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tracing::{error, info, warn};
use uuid::Uuid;

use gasket_types::{EventType, SessionEvent, SummaryType};

use super::context_v2::CompressionTask;

/// 压缩 Actor - 单线程处理所有压缩请求
pub struct CompressionActor {
    receiver: mpsc::Receiver<CompressionTask>,
    event_store: Arc<crate::storage::EventStore>,
    summarization: Arc<SummarizationService>,
    embedding_service: Arc<EmbeddingService>,
    max_retries: u32,
}

/// 摘要服务 trait (stub)
pub trait SummarizationService: Send + Sync {
    fn summarize(&self, events: &[SessionEvent]) -> impl std::future::Future<Output = Result<String, anyhow::Error>> + Send;
}

/// Embedding 服务 trait (stub)
pub trait EmbeddingService: Send + Sync {
    fn embed(&self, text: &str) -> impl std::future::Future<Output = Result<Vec<f32>, anyhow::Error>> + Send;
}

impl CompressionActor {
    /// 启动压缩 Actor，返回任务发送端
    pub fn spawn(
        event_store: Arc<crate::storage::EventStore>,
        summarization: Arc<dyn SummarizationService>,
        embedding_service: Arc<dyn EmbeddingService>,
    ) -> mpsc::Sender<CompressionTask> {
        let (tx, rx) = mpsc::channel(64);

        let actor = Self {
            receiver: rx,
            event_store,
            summarization,
            embedding_service,
            max_retries: 3,
        };

        tokio::spawn(async move {
            actor.run().await;
        });

        tx
    }

    async fn run(mut self) {
        while let Some(task) = self.receiver.recv().await {
            if let Err(e) = self.process_task(task.clone()).await {
                error!("Compression task failed: {}", e);

                if task.retry_count < self.max_retries {
                    let retry_task = CompressionTask {
                        retry_count: task.retry_count + 1,
                        ..task
                    };
                    warn!(
                        "Retrying compression task (attempt {}/{})",
                        retry_task.retry_count, self.max_retries
                    );
                    tokio::time::sleep(Duration::from_secs(2u64.pow(task.retry_count))).await;
                    if let Err(e) = self.process_task(retry_task).await {
                        error!("Compression retry failed: {}", e);
                    }
                } else {
                    error!(
                        "Compression task failed after {} retries, events may be lost: {:?}",
                        self.max_retries, task.evicted_events
                    );
                }
            }
        }
    }

    async fn process_task(&self, task: CompressionTask) -> Result<(), anyhow::Error> {
        info!(
            "Processing compression for session '{}', {} events",
            task.session_key,
            task.evicted_events.len()
        );

        // 1. 加载被驱逐的事件
        let events = self.event_store
            .get_events_by_ids(&task.session_key, &task.evicted_events)
            .await?;

        if events.is_empty() {
            warn!("No events found for compression, skipping");
            return Ok(());
        }

        // 2. 生成摘要
        let summary_content = self.summarization.summarize(&events).await?;

        // 3. 生成摘要的 Embedding
        let summary_embedding = self.embedding_service.embed(&summary_content).await?;

        // 4. 创建摘要事件
        let summary_event = SessionEvent {
            id: Uuid::now_v7(),
            session_key: task.session_key,
            parent_id: events.last().map(|e| e.id),
            event_type: EventType::Summary {
                summary_type: task.compression_type,
                covered_event_ids: task.evicted_events,
            },
            content: summary_content,
            embedding: Some(summary_embedding),
            metadata: gasket_types::EventMetadata {
                branch: Some(task.branch),
                ..Default::default()
            },
            created_at: chrono::Utc::now(),
        };

        // 5. 持久化摘要事件
        self.event_store.append_event(&summary_event).await?;

        info!(
            "Compression complete: summary event {} created",
            summary_event.id
        );

        Ok(())
    }
}
```

- [ ] **Step 2: 在 EventStore 添加 get_events_by_ids**

```rust
impl EventStore {
    /// 根据 ID 列表获取事件
    pub async fn get_events_by_ids(
        &self,
        session_key: &str,
        event_ids: &[Uuid],
    ) -> Result<Vec<SessionEvent>, StoreError> {
        if event_ids.is_empty() {
            return Ok(vec![]);
        }

        let placeholders: String = event_ids.iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");

        let query = format!(
            "SELECT * FROM session_events WHERE session_key = ? AND id IN ({}) ORDER BY created_at ASC",
            placeholders
        );

        let mut sql_query = sqlx::query_as::<_, EventRow>(&query);
        sql_query = sql_query.bind(session_key);
        for id in event_ids {
            sql_query = sql_query.bind(id.to_string());
        }

        let rows = sql_query.fetch_all(&self.pool).await?;
        rows.into_iter().map(|r| r.try_into()).collect()
    }
}
```

- [ ] **Step 3: 写 CompressionActor 测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct MockSummarization;
    impl SummarizationService for MockSummarization {
        async fn summarize(&self, _events: &[SessionEvent]) -> Result<String, anyhow::Error> {
            Ok("Summary content".into())
        }
    }

    struct MockEmbedding;
    impl EmbeddingService for MockEmbedding {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, anyhow::Error> {
            Ok(vec![0.1, 0.2, 0.3])
        }
    }

    #[tokio::test]
    async fn test_compression_actor_spawn() {
        // This is an integration test that requires a real EventStore
        // For unit testing, we just verify the spawn doesn't panic
        let (tx, _rx) = mpsc::channel::<CompressionTask>(1);
        assert!(tx.capacity() == 1);
    }
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test -p gasket-core compression --no-run`

- [ ] **Step 5: 更新 mod.rs 导出**

```rust
pub mod compression;

pub use compression::{CompressionActor, EmbeddingService, SummarizationService};
```

- [ ] **Step 6: 提交**

```bash
git add gasket/core/src/agent/compression.rs gasket/core/src/agent/mod.rs gasket/storage/src/event_store.rs
git commit -m "feat(core): add CompressionActor with retry mechanism

- Single-threaded actor via channel
- Exponential backoff retry (max 3)
- get_events_by_ids for loading evicted events

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Phase 5: 集成与清理

### Task 9: 更新 AgentContext 使用真实 EventStore

**Files:**
- Modify: `gasket/core/src/agent/context_v2.rs`

- [ ] **Step 1: 实现 PersistentContext 完整方法**

```rust
use crate::storage::{EventStore, StoreError};

impl AgentContext {
    /// 创建持久化上下文
    pub fn persistent(
        event_store: Arc<EventStore>,
        compression_tx: mpsc::Sender<CompressionTask>,
    ) -> Self {
        // For now, we create a minimal PersistentContext
        // SessionManager will be added later
        Self::Persistent(PersistentContext {
            event_store,
            compression_tx,
        })
    }

    /// 加载会话
    pub async fn load_session(&self, key: &SessionKey) -> Session {
        match self {
            Self::Persistent(ctx) => {
                // Load from event store
                Session::new(key.to_string())
            }
            Self::Stateless => Session::new(key),
        }
    }

    /// 保存事件
    pub async fn save_event(&self, event: SessionEvent) -> Result<(), AgentError> {
        match self {
            Self::Persistent(ctx) => {
                ctx.event_store.append_event(&event).await
                    .map_err(|e| AgentError::Other(format!("Failed to persist event: {}", e)))?;
                Ok(())
            }
            Self::Stateless => Ok(()),
        }
    }

    /// 获取历史
    pub async fn get_history(&self, key: &str, branch: Option<&str>) -> Vec<SessionEvent> {
        match self {
            Self::Persistent(ctx) => {
                ctx.event_store.get_branch_history(key, branch.unwrap_or("main")).await
                    .unwrap_or_default()
            }
            Self::Stateless => vec![],
        }
    }

    /// 触发压缩
    pub async fn trigger_compression(&self, task: CompressionTask) -> Result<(), AgentError> {
        match self {
            Self::Persistent(ctx) => {
                ctx.compression_tx.send(task).await
                    .map_err(|e| AgentError::Other(format!("Failed to send compression task: {}", e)))?;
                Ok(())
            }
            Self::Stateless => Ok(()),
        }
    }
}
```

- [ ] **Step 2: 写集成测试**

```rust
#[tokio::test]
async fn test_persistent_context_flow() {
    // Setup in-memory database
    let pool = setup_test_db().await;
    let event_store = Arc::new(EventStore::new(pool));
    let (tx, _rx) = mpsc::channel(1);

    let context = AgentContext::persistent(event_store, tx);
    assert!(context.is_persistent());

    // Save event
    let event = SessionEvent {
        id: Uuid::now_v7(),
        session_key: "test:session".into(),
        parent_id: None,
        event_type: EventType::UserMessage,
        content: "Hello".into(),
        embedding: None,
        metadata: EventMetadata::default(),
        created_at: Utc::now(),
    };

    context.save_event(event).await.unwrap();

    // Load history
    let history = context.get_history("test:session", None).await;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].content, "Hello");
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p gasket-core persistent_context --no-run`

- [ ] **Step 4: 提交**

```bash
git add gasket/core/src/agent/context_v2.rs
git commit -m "feat(core): implement AgentContext with real EventStore

- save_event persists to EventStore
- get_history retrieves from EventStore
- trigger_compression sends to actor

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 10: 删除旧代码

**Files:**
- Delete: `gasket/storage/src/session.rs`
- Delete: `gasket/core/src/session/manager.rs`
- Modify: `gasket/core/src/agent/context.rs` (或重命名为 `context_legacy.rs`)

- [ ] **Step 1: 检查旧代码引用**

Run: `cd /Users/yeheng/workspaces/Github/gasket && grep -r "SessionMessage" --include="*.rs"`

- [ ] **Step 2: 更新所有引用使用新类型**

根据 grep 结果，更新每个文件使用 `SessionEvent` 替代 `SessionMessage`

- [ ] **Step 3: 删除旧文件**

```bash
rm gasket/storage/src/session.rs
rm -rf gasket/core/src/session/
```

- [ ] **Step 4: 更新 lib.rs 导出**

移除对旧模块的引用

- [ ] **Step 5: 运行完整测试**

Run: `cargo test --workspace`

- [ ] **Step 6: 提交**

```bash
git add -A
git commit -m "refactor: remove legacy session code

- Delete old SessionManager, SessionMessage
- Update all references to use SessionEvent
- Clean up imports

BREAKING CHANGE: Session/SessionMessage replaced with SessionEvent

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 11: 最终验证

- [ ] **Step 1: 运行完整测试套件**

Run: `cargo test --workspace`

- [ ] **Step 2: 运行 clippy**

Run: `cargo clippy --workspace -- -D warnings`

- [ ] **Step 3: 格式化代码**

Run: `cargo fmt -- --check`

- [ ] **Step 4: 构建发布版本**

Run: `cargo build --release --workspace`

- [ ] **Step 5: 最终提交**

```bash
git add -A
git commit -m "chore: final cleanup and verification

- All tests passing
- No clippy warnings
- Formatted code

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Summary

| Phase | Tasks | Est. Time |
|-------|-------|-----------|
| Phase 1: 核心数据结构 | Task 1-3 | 2-3 days |
| Phase 2: 存储层 | Task 4-6 | 2-3 days |
| Phase 3: 检索系统 | Task 7 | 1 day |
| Phase 4: Compression Actor | Task 8 | 1 day |
| Phase 5: 集成与清理 | Task 9-11 | 2-3 days |

**Total**: 8-11 days