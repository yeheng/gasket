//! Prompt loading utilities.
//!
//! Provides functions to load workspace bootstrap files and skills context
//! for injection into the system prompt. These are called directly by
//! `AgentLoop` during initialization — no dynamic hook dispatch needed.

use std::path::Path;

use tokio::fs;
use tracing::{debug, info, warn};

use crate::agent::history_processor::count_tokens;
use crate::agent::skill_loader;

/// Bootstrap files loaded into the system prompt for the full (main agent) profile
pub const BOOTSTRAP_FILES_FULL: &[&str] = &["AGENTS.md", "SOUL.md", "USER.md", "TOOLS.md"];

/// Bootstrap files loaded for the minimal (subagent) profile — only core identity
pub const BOOTSTRAP_FILES_MINIMAL: &[&str] = &["SOUL.md"];

/// Maximum tokens allowed per single bootstrap file before emitting a warning
const BOOTSTRAP_TOKEN_WARN_THRESHOLD: usize = 2000;

/// Load the system prompt from workspace bootstrap files.
///
/// Reads the specified files from the workspace directory and concatenates them.
/// Returns an identity header plus any loaded bootstrap file contents.
/// If no files are found, returns only the identity header.
///
/// # Errors
/// Returns an error if a bootstrap file **exists** but cannot be read.
pub async fn load_system_prompt(
    workspace: &Path,
    files: &[&str],
) -> Result<String, std::io::Error> {
    let mut parts = Vec::new();

    // Identity header
    parts.push(format!(
        "You are TinyDog 🐈, a personal AI assistant.\nYour working directory: {}.YOU can ONLY READ and WRITE under working directory.",
        workspace.display()
    ));

    // Load bootstrap files
    let mut loaded_any = false;
    let mut total_tokens: usize = 0;
    for filename in files {
        let file_path = workspace.join(filename);
        if file_path.exists() {
            let content = fs::read_to_string(&file_path).await?;
            if !content.trim().is_empty() {
                let tokens = count_tokens(content.trim());
                if tokens > BOOTSTRAP_TOKEN_WARN_THRESHOLD {
                    warn!(
                        "Bootstrap file {} has {} tokens (threshold {}). Consider trimming it.",
                        filename, tokens, BOOTSTRAP_TOKEN_WARN_THRESHOLD
                    );
                }
                total_tokens += tokens;
                debug!("Loaded bootstrap file: {} ({} tokens)", filename, tokens);
                parts.push(format!("## {}\n\n{}", filename, content.trim()));
                loaded_any = true;
            }
        }
    }

    info!(
        "System prompt: {} bootstrap files loaded ({} found), ~{} tokens total",
        files.len(),
        loaded_any,
        total_tokens
    );

    Ok(parts.join("\n\n"))
}

/// Load the skills context from the workspace.
///
/// Scans for skill definitions and returns a formatted string for prompt injection,
/// or `None` if no skills are found.
pub async fn load_skills_context(workspace: &Path) -> Option<String> {
    let ctx = skill_loader::load_skills(workspace).await?;
    if ctx.is_empty() {
        None
    } else {
        Some(format!("# Skills\n\n{}", ctx))
    }
}
