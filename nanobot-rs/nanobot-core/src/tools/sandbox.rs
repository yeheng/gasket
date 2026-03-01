//! Sandbox execution providers for shell commands.
//!
//! - `BwrapSandbox`: Uses bubblewrap for namespace-isolated execution (Linux only)
//! - `FallbackExecutor`: Direct `bash -c` with ulimit prefix (all platforms)

use std::path::{Path, PathBuf};
use std::process::Command;

use tracing::{debug, info, warn};

use super::resource_limits::ResourceLimits;
use crate::config::SandboxConfig;

/// Bubblewrap-based sandbox (Linux only).
///
/// Mount layout:
/// - `/`         → bind-ro from host
/// - `workspace` → bind-rw from configured workspace
/// - `/tmp`      → tmpfs (size-limited)
/// - `/dev`      → minimal devtmpfs
/// - `/proc`     → new proc namespace
pub struct BwrapSandbox {
    bwrap_path: PathBuf,
    workspace: PathBuf,
    tmp_size_mb: u32,
}

impl BwrapSandbox {
    /// Detect bwrap binary and create a sandbox.
    /// Returns `None` if bwrap is not available.
    pub fn detect(workspace: &Path, config: &SandboxConfig) -> Option<Self> {
        let bwrap_path = which_bwrap()?;
        info!("bwrap detected at {:?}", bwrap_path);
        Some(Self {
            bwrap_path,
            workspace: workspace.to_path_buf(),
            tmp_size_mb: config.tmp_size_mb,
        })
    }

    fn build_command(&self, cmd: &str, _working_dir: &Path, limits: &ResourceLimits) -> Command {
        let mut command = Command::new(&self.bwrap_path);

        // Namespace isolation
        command.arg("--unshare-pid").arg("--unshare-ipc");

        // Filesystem mounts
        command
            // Read-only root
            .arg("--ro-bind")
            .arg("/")
            .arg("/")
            // Read-write workspace
            .arg("--bind")
            .arg(&self.workspace)
            .arg(&self.workspace)
            // Tmpfs for /tmp
            .arg("--tmpfs")
            .arg("/tmp")
            // Minimal /dev
            .arg("--dev")
            .arg("/dev")
            // New /proc
            .arg("--proc")
            .arg("/proc");

        // Tmpfs size limit
        command
            .arg("--size")
            .arg(format!("{}", u64::from(self.tmp_size_mb) * 1024 * 1024));

        // Resource limits
        for arg in limits.to_bwrap_args() {
            command.arg(arg);
        }

        // Working directory inside sandbox
        command.arg("--chdir").arg(&self.workspace);

        // The actual command
        command.arg("bash").arg("-c").arg(cmd);

        debug!("bwrap command: {:?}", command);
        command
    }
}

/// Fallback executor — direct `bash -c` with ulimit-based resource limits.
///
/// Used when bwrap is unavailable or sandbox is disabled.
pub struct FallbackExecutor;

impl FallbackExecutor {
    fn build_command(&self, cmd: &str, working_dir: &Path, limits: &ResourceLimits) -> Command {
        let prefixed_cmd = format!("{}{}", limits.to_ulimit_prefix(), cmd);

        let mut command = Command::new("bash");
        command.arg("-c").arg(prefixed_cmd).current_dir(working_dir);

        command
    }
}

/// Sandbox executor — statically dispatched enum replacing the old
/// `Box<dyn SandboxProvider>` dynamic dispatch.
///
/// Only two variants exist and are known at compile time, so an enum
/// eliminates the unnecessary heap allocation and vtable indirection.
pub enum SandboxExecutor {
    Bwrap(BwrapSandbox),
    Fallback(FallbackExecutor),
}

impl SandboxExecutor {
    /// Build a `Command` that will execute `cmd` in the given `working_dir`
    /// with the specified `limits`.
    pub fn build_command(&self, cmd: &str, working_dir: &Path, limits: &ResourceLimits) -> Command {
        match self {
            Self::Bwrap(s) => s.build_command(cmd, working_dir, limits),
            Self::Fallback(s) => s.build_command(cmd, working_dir, limits),
        }
    }

    /// Human-readable name for logging.
    pub fn name(&self) -> &str {
        match self {
            Self::Bwrap(_) => "bwrap",
            Self::Fallback(_) => "fallback",
        }
    }
}

/// Detect bwrap binary on the system.
fn which_bwrap() -> Option<PathBuf> {
    // Try common locations
    let candidates = ["/usr/bin/bwrap", "/usr/local/bin/bwrap"];
    for path in &candidates {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    // Try `which` command
    let output = Command::new("which").arg("bwrap").output().ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }

    warn!("bwrap not found on system — sandbox unavailable, falling back to ulimit-based limits");
    None
}

/// Create the appropriate sandbox executor based on configuration.
pub fn create_provider(workspace: &Path, config: &SandboxConfig) -> SandboxExecutor {
    if !config.enabled {
        debug!("Sandbox disabled by config");
        return SandboxExecutor::Fallback(FallbackExecutor);
    }

    // Only bwrap backend is supported
    if config.backend != "bwrap" {
        warn!(
            "Unknown sandbox backend '{}', falling back to unsandboxed",
            config.backend
        );
        return SandboxExecutor::Fallback(FallbackExecutor);
    }

    // macOS: bwrap is Linux-only
    if cfg!(target_os = "macos") {
        warn!("bwrap sandbox is Linux-only. macOS falls back to ulimit-based resource limits.");
        return SandboxExecutor::Fallback(FallbackExecutor);
    }

    match BwrapSandbox::detect(workspace, config) {
        Some(sandbox) => {
            info!("Sandbox enabled: bwrap at {:?}", sandbox.bwrap_path);
            SandboxExecutor::Bwrap(sandbox)
        }
        None => {
            warn!("bwrap not available — running without sandbox (ulimit-based limits only)");
            SandboxExecutor::Fallback(FallbackExecutor)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_executor_builds_command() {
        let executor = FallbackExecutor;
        let limits = ResourceLimits::default();
        let cmd = executor.build_command("echo hello", Path::new("/tmp"), &limits);

        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(cmd.get_program(), "bash");
        assert_eq!(args[0], "-c");
        // Second arg should contain ulimit prefix + actual command
        let full_cmd = args[1].to_string_lossy();
        assert!(full_cmd.contains("ulimit"));
        assert!(full_cmd.contains("echo hello"));
    }

    #[test]
    fn test_create_provider_disabled() {
        let config = SandboxConfig::default(); // enabled: false
        let provider = create_provider(Path::new("/tmp"), &config);
        assert_eq!(provider.name(), "fallback");
    }

    #[test]
    fn test_create_provider_unknown_backend() {
        let config = SandboxConfig {
            enabled: true,
            backend: "docker".to_string(),
            tmp_size_mb: 64,
        };
        let provider = create_provider(Path::new("/tmp"), &config);
        assert_eq!(provider.name(), "fallback");
    }
}
