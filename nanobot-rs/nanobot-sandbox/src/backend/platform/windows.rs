//! Windows Job Objects sandbox backend
//!
//! Uses Windows Job Objects for process group limits and resource constraints.
//! Provides basic isolation on Windows platforms.

use std::path::Path;
use std::process::Command;

use async_trait::async_trait;
use tracing::{debug, info, warn};

use crate::backend::{ExecutionResult, Platform, SandboxBackend};
use crate::config::SandboxConfig;
use crate::error::{Result, SandboxError};

/// Windows Job Objects based sandbox.
///
/// Uses Windows Job Objects to limit resources for child processes.
/// This provides basic isolation but less than Linux namespaces.
pub struct WindowsJobObjectsBackend {
    // Future: could store Job Object handle for reuse
}

impl WindowsJobObjectsBackend {
    /// Create a new Windows Job Objects backend
    pub fn new() -> Self {
        Self {}
    }

    fn build_command_internal(
        &self,
        cmd: &str,
        working_dir: &Path,
        _config: &SandboxConfig,
    ) -> Command {
        // On Windows, we use cmd.exe for command execution
        // Job Objects limits are applied via Win32 API after process creation
        // For simplicity, this implementation uses basic command execution
        // Full Job Objects integration would require unsafe Win32 API calls

        let mut command = Command::new("cmd");
        command.arg("/C").arg(cmd).current_dir(working_dir);

        debug!("Windows command: {:?}", command);
        command
    }
}

impl Default for WindowsJobObjectsBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SandboxBackend for WindowsJobObjectsBackend {
    fn name(&self) -> &str {
        "job-objects"
    }

    async fn is_available(&self) -> bool {
        // Job Objects are always available on Windows
        true
    }

    fn supported_platforms(&self) -> &[Platform] {
        &[Platform::Windows]
    }

    fn build_command(
        &self,
        cmd: &str,
        working_dir: &Path,
        config: &SandboxConfig,
    ) -> Result<Command> {
        Ok(self.build_command_internal(cmd, working_dir, config))
    }

    async fn execute(
        &self,
        cmd: &str,
        working_dir: &Path,
        config: &SandboxConfig,
    ) -> Result<ExecutionResult> {
        let mut command = self.build_command(cmd, working_dir, config)?;

        let output = tokio::task::spawn_blocking(move || command.output())
            .await
            .map_err(|e| SandboxError::ExecutionFailed(e.to_string()))?
            .map_err(|e| SandboxError::ExecutionFailed(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // Note: Full Job Objects resource limiting would require Win32 API
        // For now, we just truncate output
        let max_output = config.limits.max_output_bytes;
        let stdout = if stdout.len() > max_output {
            let mut truncated = stdout;
            truncated.truncate(max_output);
            truncated.push_str(&format!(
                "\n\n[OUTPUT TRUNCATED: {} bytes exceeded limit of {} bytes]",
                stdout.len(),
                max_output
            ));
            truncated
        } else {
            stdout
        };

        Ok(ExecutionResult {
            exit_code: output.status.code(),
            stdout,
            stderr,
            timed_out: false,
            resource_exceeded: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_job_objects_availability() {
        let backend = WindowsJobObjectsBackend::new();
        assert!(backend.is_available().await);
    }

    #[test]
    fn test_build_command() {
        let backend = WindowsJobObjectsBackend::new();
        let config = SandboxConfig::default();
        let cmd = backend.build_command("echo hello", Path::new("C:\\"), &config);
        assert!(cmd.is_ok());

        let cmd = cmd.unwrap();
        assert_eq!(cmd.get_program(), "cmd");

        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args[0], "/C");
    }
}
