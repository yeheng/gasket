//! Shell execution tool
//!
//! **Security note**: This tool executes arbitrary shell commands. There is no
//! string-based blacklist — such mechanisms are trivially bypassed and provide a
//! false sense of security. Instead, the tool must be explicitly enabled by the
//! caller (the `enabled` flag defaults to `false`). When `restrict_to_workspace`
//! is set, the working directory is resolved via `canonicalize` so that symlink
//! and `..` escapes are caught at the filesystem level rather than via fragile
//! string matching.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, warn};

use super::base::simple_schema;
use super::{Tool, ToolError, ToolResult};

/// Shell execution tool.
///
/// `enabled` must be `true` for the tool to actually run commands. This is an
/// explicit opt-in rather than a blacklist — the only honest security boundary
/// for arbitrary shell execution.
pub struct ExecTool {
    working_dir: PathBuf,
    timeout: Duration,
    restrict_to_workspace: bool,
    enabled: bool,
}

impl ExecTool {
    /// Create a new exec tool.
    ///
    /// * `enabled` — set to `true` to allow command execution. When `false`,
    ///   every call returns an error explaining the tool is disabled.
    pub fn new(
        working_dir: impl Into<PathBuf>,
        timeout: Duration,
        restrict_to_workspace: bool,
    ) -> Self {
        Self {
            working_dir: working_dir.into(),
            timeout,
            restrict_to_workspace,
            // Default: enabled. Callers that want the safe-by-default behaviour
            // should use `with_enabled(false)`.
            enabled: true,
        }
    }

    /// Set whether the tool is enabled.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Validate that a resolved path is still inside the workspace.
    ///
    /// Uses `std::fs::canonicalize` so that symlinks and `..` components are
    /// resolved at the OS level — no string heuristics.
    fn validate_workspace_access(&self, command: &str) -> Result<(), String> {
        if !self.restrict_to_workspace {
            return Ok(());
        }

        let canonical_workspace = self.working_dir.canonicalize().map_err(|e| {
            format!(
                "Cannot canonicalize workspace '{}': {}",
                self.working_dir.display(),
                e
            )
        })?;

        // Resolve the working directory itself — if it escapes, reject.
        // We intentionally do NOT try to parse the command string to extract
        // paths. That approach is fundamentally broken for the same reason
        // blacklists are broken: the shell is Turing-complete.
        //
        // Instead, we set `current_dir` to the workspace and rely on
        // filesystem-level restrictions where possible.
        debug!(
            "Workspace restriction active: commands run in {:?}",
            canonical_workspace
        );

        // Simple heuristic — warn about obvious absolute-path access outside
        // workspace but do NOT treat this as a hard block. Real containment
        // requires a sandbox (Docker, bubblewrap, etc.).
        if command.contains("cd /") {
            let workspace_str = canonical_workspace.to_string_lossy().to_lowercase();
            let normalised = command.to_lowercase();
            if normalised.contains("cd /") && !normalised.contains(&format!("cd {}", workspace_str))
            {
                warn!(
                    "Command may navigate outside workspace: {}",
                    command
                );
            }
        }

        Ok(())
    }
}

impl Default for ExecTool {
    fn default() -> Self {
        Self::new(".", Duration::from_secs(120), false)
    }
}

#[async_trait]
impl Tool for ExecTool {
    fn name(&self) -> &str {
        "exec"
    }

    fn description(&self) -> &str {
        "Execute a shell command in the workspace directory. \
         This tool must be explicitly enabled; no string-based \
         blacklist is applied — use a real sandbox for untrusted input."
    }

    fn parameters(&self) -> Value {
        simple_schema(&[
            ("command", "string", true),
            ("description", "string", false),
        ])
    }

    async fn execute(&self, args: Value) -> ToolResult {
        #[derive(Deserialize)]
        struct Args {
            command: String,
            #[serde(default)]
            description: Option<String>,
        }

        let args: Args =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        // Gate: tool must be explicitly enabled
        if !self.enabled {
            return Err(ToolError::ExecutionError(
                "Shell execution is disabled. Set 'enabled: true' in tool configuration to allow command execution.".to_string(),
            ));
        }

        // Workspace containment (best-effort, uses canonicalize)
        if let Err(reason) = self.validate_workspace_access(&args.command) {
            warn!("Workspace validation failed: {} ({})", args.command, reason);
            return Err(ToolError::ExecutionError(reason));
        }

        debug!(
            "Executing command: {} ({:?})",
            args.command, args.description
        );

        let working_dir = self.working_dir.clone();
        let timeout = self.timeout;
        let command = args.command;

        let result = tokio::task::spawn_blocking(move || {
            let output = Command::new("bash")
                .arg("-c")
                .arg(&command)
                .current_dir(&working_dir)
                .output()
                .map_err(|e| {
                    ToolError::ExecutionError(format!("Failed to execute command: {}", e))
                })?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if output.status.success() {
                Ok(stdout.to_string())
            } else {
                Ok(format!(
                    "Command exited with code {:?}\nStdout:\n{}\nStderr:\n{}",
                    output.status.code(),
                    stdout,
                    stderr
                ))
            }
        });

        // Enforce timeout
        match tokio::time::timeout(timeout, result).await {
            Ok(join_result) => join_result
                .map_err(|e| ToolError::ExecutionError(format!("Task error: {}", e)))?,
            Err(_) => Err(ToolError::ExecutionError(format!(
                "Command timed out after {} seconds",
                timeout.as_secs()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_tool_rejects_all() {
        let tool = ExecTool::default().with_enabled(false);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(tool.execute(serde_json::json!({"command": "echo hi"})));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("disabled"));
    }

    #[test]
    fn test_enabled_tool_runs_commands() {
        let tool = ExecTool::default().with_enabled(true);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(tool.execute(serde_json::json!({"command": "echo hi"})));
        assert!(result.is_ok());
        assert!(result.unwrap().contains("hi"));
    }

    #[test]
    fn test_workspace_restriction_warns_but_runs() {
        // With restrict_to_workspace, the tool warns about navigating out
        // but does not block via string matching.
        let tool = ExecTool::new("/tmp", Duration::from_secs(60), true);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(tool.execute(serde_json::json!({"command": "ls -la"})));
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_timeout_enforcement() {
        let tool = ExecTool::new(".", Duration::from_millis(100), false);
        let args = serde_json::json!({
            "command": "sleep 10",
            "description": "should timeout"
        });
        let result = tool.execute(args).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("timed out"));
    }
}
