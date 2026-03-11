//! MCP (Model Context Protocol) implementation.

mod handler;
mod tools;
mod transport;
mod types;

pub use handler::McpHandler;
pub use tools::ToolRegistry;
pub use transport::StdioTransport;
pub use types::*;
