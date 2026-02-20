//! Tool system

mod base;
mod cron;
mod filesystem;
mod message;
pub mod middleware;
mod registry;
mod shell;
mod spawn;
mod web;

pub use base::{simple_schema, ExecutionContext, Tool, ToolError, ToolMetadata, ToolResult};
pub use cron::CronTool;
pub use filesystem::{EditFileTool, ListDirTool, ReadFileTool, WriteFileTool};
pub use message::MessageTool;
pub use middleware::{
    ToolInvocation, ToolLoggingMiddleware, ToolMetricsMiddleware, ToolPermissionMiddleware,
    ToolTimeoutMiddleware,
};
pub use registry::ToolRegistry;
pub use shell::ExecTool;
pub use spawn::SpawnTool;
pub use web::{WebFetchTool, WebSearchTool};
