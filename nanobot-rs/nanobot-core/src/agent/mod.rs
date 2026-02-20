//! Agent module: core processing engine

pub mod context;
pub mod executor;
pub mod loop_;
pub mod memory;
pub mod subagent;

pub use context::ContextBuilder;
pub use executor::ToolExecutor;
pub use loop_::{AgentConfig, AgentDependencies, AgentLoop};
pub use memory::MemoryStore;
pub use subagent::{SubagentConfig, SubagentManager, SubagentTask, TaskNotification, TaskPriority, TaskStatus};
