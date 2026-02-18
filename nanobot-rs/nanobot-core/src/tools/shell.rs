//! Shell execution tool with security sandboxing

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, warn};

use super::base::simple_schema;
use super::{Tool, ToolError, ToolResult};

/// Commands that are never allowed, regardless of configuration.
const BLOCKED_COMMANDS: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "mkfs",
    "dd if=",
    ":(){:|:&};:",
    "chmod -R 777 /",
    "shutdown",
    "reboot",
    "halt",
    "poweroff",
    "init 0",
    "init 6",
];

/// Path prefixes that should never be accessed.
const BLOCKED_PATHS: &[&str] = &[
    "/etc/shadow",
    "/etc/passwd",
    "/etc/sudoers",
    "/proc/",
    "/sys/",
    "/dev/",
    "/boot/",
];

/// Shell execution tool with security restrictions
pub struct ExecTool {
    working_dir: PathBuf,
    timeout: Duration,
    restrict_to_workspace: bool,
}

impl ExecTool {
    /// Create a new exec tool
    pub fn new(
        working_dir: impl Into<PathBuf>,
        timeout: Duration,
        restrict_to_workspace: bool,
    ) -> Self {
        Self {
            working_dir: working_dir.into(),
            timeout,
            restrict_to_workspace,
        }
    }

    /// Check if a command is blocked by security policy
    fn is_command_blocked(&self, command: &str) -> Option<String> {
        let normalized = command.trim().to_lowercase();

        // Check against blocked command patterns
        for blocked in BLOCKED_COMMANDS {
            if normalized.contains(blocked) {
                return Some(format!("Command blocked: contains dangerous pattern '{}'", blocked));
            }
        }

        // Check for access to sensitive paths
        for path in BLOCKED_PATHS {
            if normalized.contains(path) {
                return Some(format!("Command blocked: accesses restricted path '{}'", path));
            }
        }

        // If restricted to workspace, verify no path escaping
        if self.restrict_to_workspace {
            // Block cd to absolute paths outside workspace
            if normalized.contains("cd /") && !normalized.contains(&format!("cd {}", self.working_dir.display()).to_lowercase()) {
                return Some("Command blocked: cannot navigate outside workspace".to_string());
            }
            // Block common path-traversal patterns
            if normalized.contains("../") {
                return Some("Command blocked: path traversal ('..') not allowed in restricted mode".to_string());
            }
        }

        None
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
        "Execute a shell command in the workspace directory. Dangerous commands (rm -rf /, access to /etc/shadow, etc.) are blocked."
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

        // Security check: block dangerous commands
        if let Some(reason) = self.is_command_blocked(&args.command) {
            warn!("Blocked command execution: {} ({})", args.command, reason);
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
    fn test_blocked_rm_rf() {
        let tool = ExecTool::default();
        assert!(tool.is_command_blocked("rm -rf /").is_some());
        assert!(tool.is_command_blocked("rm -rf /*").is_some());
        assert!(tool.is_command_blocked("sudo rm -rf /").is_some());
    }

    #[test]
    fn test_blocked_sensitive_paths() {
        let tool = ExecTool::default();
        assert!(tool.is_command_blocked("cat /etc/shadow").is_some());
        assert!(tool.is_command_blocked("cat /etc/passwd").is_some());
    }

    #[test]
    fn test_allowed_commands() {
        let tool = ExecTool::default();
        assert!(tool.is_command_blocked("ls -la").is_none());
        assert!(tool.is_command_blocked("echo hello").is_none());
        assert!(tool.is_command_blocked("cargo build").is_none());
        assert!(tool.is_command_blocked("git status").is_none());
    }

    #[test]
    fn test_workspace_restriction_blocks_traversal() {
        let tool = ExecTool::new("/home/user/project", Duration::from_secs(60), true);
        assert!(tool.is_command_blocked("cat ../../etc/hosts").is_some());
    }

    #[test]
    fn test_workspace_restriction_allows_normal_commands() {
        let tool = ExecTool::new("/home/user/project", Duration::from_secs(60), true);
        assert!(tool.is_command_blocked("ls -la").is_none());
        assert!(tool.is_command_blocked("cargo test").is_none());
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
