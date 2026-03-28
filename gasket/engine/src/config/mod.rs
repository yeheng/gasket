//! Configuration types for gasket-engine

mod tools;

use std::path::PathBuf;

pub use tools::{
    CommandPolicyConfig, ExecToolConfig, ResourceLimitsConfig, SandboxConfig, ToolsConfig,
    WebToolsConfig,
};

/// Get the gasket config directory
pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".gasket")
}
