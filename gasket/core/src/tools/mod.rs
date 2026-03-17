//! Tool system
//!
//! Provides various tools for the agent to interact with the environment:
//! - `exec`: Shell command execution with sandbox support (via nanobot-sandbox)
//! - `filesystem`: File read/write/edit operations
//! - `web_fetch`: Web content fetching
//! - `web_search`: Web search
//! - `memory_search`: Memory search
//! - `history_search`: Conversation history search
//! - `message`: Send messages to users
//! - `cron`: Scheduled tasks
//! - `spawn`: Spawn sub-agents
//! - `spawn_parallel`: Parallel sub-agent spawning

mod base;
#[cfg(feature = "tool-cron")]
mod cron;
mod filesystem;
mod history_search;
mod memory_search;
mod message;
mod registry;
mod shell;
#[cfg(feature = "tool-spawn")]
mod spawn;
#[cfg(feature = "tool-spawn")]
mod spawn_parallel;
#[cfg(feature = "tool-web-fetch")]
mod web_fetch;
#[cfg(feature = "tool-web-search")]
mod web_search;

pub use base::{simple_schema, Tool, ToolError, ToolMetadata, ToolResult};
#[cfg(feature = "tool-cron")]
pub use cron::CronTool;
pub use filesystem::{EditFileTool, ListDirTool, ReadFileTool, WriteFileTool};
pub use history_search::HistorySearchTool;
pub use memory_search::MemorySearchTool;
pub use message::MessageTool;
pub use registry::ToolRegistry;
pub use shell::ExecTool;
#[cfg(feature = "tool-spawn")]
pub use spawn::SpawnTool;
#[cfg(feature = "tool-spawn")]
pub use spawn_parallel::SpawnParallelTool;
#[cfg(feature = "tool-web-fetch")]
pub use web_fetch::WebFetchTool;
#[cfg(feature = "tool-web-search")]
pub use web_search::WebSearchTool;

// Re-export sandbox types from nanobot-sandbox for backward compatibility
pub use gasket_sandbox::ProcessManager;
pub use gasket_sandbox::SandboxConfig;
