//! Agent module: core processing engine

pub mod context;
pub mod executor;
pub mod loop_;
pub mod memory;
pub mod subagent;
pub mod task_store;

#[cfg(feature = "sqlite")]
pub mod task_store_sqlite;

pub use context::ContextBuilder;
pub use executor::ToolExecutor;
pub use loop_::{AgentConfig, AgentLoop};
pub use memory::MemoryStore;
pub use subagent::{SubagentConfig, SubagentManager, SubagentTask, TaskNotification, TaskPriority, TaskStatus};
