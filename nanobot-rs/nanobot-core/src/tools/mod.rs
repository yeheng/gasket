//! Tool system

mod base;
pub mod command_policy;
mod cron;
mod filesystem;
mod memory_search;
mod message;
mod registry;
pub mod resource_limits;
pub mod sandbox;
mod shell;
mod spawn;
mod web_fetch;
mod web_search;

// Tantivy-based tools (only available when tantivy feature is enabled)
#[cfg(feature = "tantivy")]
mod history_tantivy_index;
#[cfg(feature = "tantivy")]
mod history_tantivy_search;
#[cfg(feature = "tantivy")]
mod memory_tantivy_index;

pub use base::{simple_schema, Tool, ToolError, ToolMetadata, ToolResult};
pub use cron::CronTool;
pub use filesystem::{EditFileTool, ListDirTool, ReadFileTool, WriteFileTool};
pub use memory_search::MemorySearchTool;
pub use message::MessageTool;
pub use registry::ToolRegistry;
pub use shell::ExecTool;
pub use spawn::SpawnTool;
pub use web_fetch::WebFetchTool;
pub use web_search::WebSearchTool;

// Re-export Tantivy tools when feature is enabled
#[cfg(feature = "tantivy")]
pub use history_tantivy_index::HistoryTantivyIndexTool;
#[cfg(feature = "tantivy")]
pub use history_tantivy_search::HistoryTantivySearchTool;
#[cfg(feature = "tantivy")]
pub use memory_tantivy_index::MemoryTantivyIndexTool;
