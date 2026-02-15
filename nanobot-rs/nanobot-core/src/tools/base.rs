//! Base trait for tools

use async_trait::async_trait;
use serde_json::Value;

/// Result type for tool execution
pub type ToolResult = Result<String, ToolError>;

/// Error type for tool execution
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

/// Tool trait for implementing agent tools
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the tool name
    fn name(&self) -> &str;

    /// Get the tool description
    fn description(&self) -> &str;

    /// Get the JSON schema for parameters
    fn parameters(&self) -> Value;

    /// Execute the tool with given arguments
    async fn execute(&self, args: Value) -> ToolResult;
}

/// Helper to create a simple JSON schema for tool parameters
pub fn simple_schema(properties: &[(&str, &str, bool)]) -> Value {
    let mut props = serde_json::Map::new();
    let mut required = Vec::new();

    for (name, type_desc, is_required) in properties {
        props.insert(
            name.to_string(),
            serde_json::json!({
                "type": type_desc,
                "description": name
            }),
        );
        if *is_required {
            required.push(name.to_string());
        }
    }

    serde_json::json!({
        "type": "object",
        "properties": props,
        "required": required
    })
}
