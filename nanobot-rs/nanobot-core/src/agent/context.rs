//! Context builder for constructing LLM prompts

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::fs;
use tracing::{debug, info, warn};

use crate::providers::ChatMessage;
use crate::session::SessionMessage;

use super::history_processor::{count_tokens, HistoryConfig};
use super::summarization::SUMMARY_PREFIX;

/// Bootstrap files loaded into the system prompt for the full (main agent) profile
const BOOTSTRAP_FILES_FULL: &[&str] = &["AGENTS.md", "SOUL.md", "USER.md", "TOOLS.md"];

/// Bootstrap files loaded for the minimal (subagent) profile — only core identity
const BOOTSTRAP_FILES_MINIMAL: &[&str] = &["SOUL.md"];

/// Maximum tokens allowed per single bootstrap file before emitting a warning
const BOOTSTRAP_TOKEN_WARN_THRESHOLD: usize = 2000;

/// Context builder for constructing prompts.
///
/// A **pure, synchronous** data assembler.  It knows how to turn a system
/// prompt, skills context, history messages, and an optional summary into the
/// `Vec<ChatMessage>` expected by LLM providers.
///
/// Side-effects (LLM summarization, DB persistence) are handled externally
/// by the `AgentLoop` through the `ContextCompressionHook` trait.
///
/// This struct is designed to be created once at startup and shared across
/// multiple agent loops via `Arc`.
#[derive(Clone)]
pub struct ContextBuilder {
    _workspace: PathBuf,
    system_prompt: Arc<String>,
    skills_context: Option<Arc<String>>,
    /// History processing configuration (pub so AgentLoop can use it)
    pub history_config: HistoryConfig,
}

impl ContextBuilder {
    /// Create a new context builder.
    ///
    /// Loads bootstrap files (AGENTS.md, SOUL.md, USER.md, TOOLS.md) from the
    /// workspace directory. Falls back to a minimal default prompt if none exist.
    ///
    /// # Errors
    ///
    /// Returns an error if a bootstrap file **exists** but cannot be read
    /// (permission denied, I/O error, etc.). A missing file is not an error.
    pub async fn new(workspace: PathBuf) -> Result<Self, std::io::Error> {
        let system_prompt = Self::build_system_prompt(&workspace, BOOTSTRAP_FILES_FULL).await?;
        let history_config = HistoryConfig::default();

        Ok(Self {
            _workspace: workspace,
            system_prompt: Arc::new(system_prompt),
            skills_context: None,
            history_config,
        })
    }

    /// Create a minimal context builder for subagents.
    ///
    /// Only loads SOUL.md (core identity) and skips skills context to save tokens.
    /// Subagents execute focused background tasks and don't need the full prompt.
    pub async fn new_minimal(workspace: PathBuf) -> Result<Self, std::io::Error> {
        let system_prompt = Self::build_system_prompt(&workspace, BOOTSTRAP_FILES_MINIMAL).await?;
        let history_config = HistoryConfig {
            max_messages: 20,
            token_budget: 4000,
            recent_keep: 5,
        };

        Ok(Self {
            _workspace: workspace,
            system_prompt: Arc::new(system_prompt),
            skills_context: None,
            history_config,
        })
    }

    /// Derive a minimal context builder from an existing (full) instance.
    ///
    /// Rebuilds the system prompt with only SOUL.md and drops skills context.
    /// This is the recommended way to create subagent contexts after startup.
    pub async fn to_minimal(&self) -> Result<Self, std::io::Error> {
        let system_prompt =
            Self::build_system_prompt(&self._workspace, BOOTSTRAP_FILES_MINIMAL).await?;

        Ok(Self {
            _workspace: self._workspace.clone(),
            system_prompt: Arc::new(system_prompt),
            skills_context: None,
            history_config: HistoryConfig {
                max_messages: 20,
                token_budget: 4000,
                recent_keep: 5,
            },
        })
    }

    /// Create a context builder with custom history configuration
    pub fn with_history_config(mut self, config: HistoryConfig) -> Self {
        self.history_config = config;
        self
    }

    /// Create a context builder with "smart" history processing
    /// (token budget management)
    pub fn with_smart_history(mut self, token_budget: usize) -> Self {
        self.history_config.token_budget = token_budget;
        self
    }

    /// Build system prompt from workspace bootstrap files.
    ///
    /// `files` controls which bootstrap files are loaded — pass
    /// `BOOTSTRAP_FILES_FULL` for the main agent or `BOOTSTRAP_FILES_MINIMAL`
    /// for subagents.
    ///
    /// Files that don't exist are silently skipped. Files that exist but fail
    /// to read cause an immediate error — silent degradation on core config is
    /// dangerous.
    ///
    /// A warning is logged for any file exceeding `BOOTSTRAP_TOKEN_WARN_THRESHOLD`.
    async fn build_system_prompt(
        workspace: &Path,
        files: &[&str],
    ) -> Result<String, std::io::Error> {
        let mut parts = Vec::new();

        // Identity header
        parts.push(format!(
            "你叫阿乐 🐈, 夜痕的专业私人助理.\n\nWorking directory: {}",
            workspace.display()
        ));

        // Load bootstrap files
        let mut loaded_any = false;
        let mut total_tokens: usize = 0;
        for filename in files {
            let file_path = workspace.join(filename);
            if file_path.exists() {
                // File exists — a read failure here is a hard error.
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

        if !loaded_any {
            // Fallback: use minimal default instructions
            parts.push(DEFAULT_INSTRUCTIONS.to_string());
        }

        info!(
            "System prompt: {} bootstrap files, ~{} tokens total",
            files.len(),
            total_tokens
        );

        Ok(parts.join("\n\n"))
    }

    /// Set a custom system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Arc::new(prompt.into());
        self
    }

    /// Set skills context summary
    pub fn with_skills_context(mut self, context: Option<String>) -> Self {
        self.skills_context = context.map(Arc::new);
        self
    }

    /// Get a cloneable reference to the context builder.
    /// Useful for sharing with subagents.
    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }

    /// Build the message list for an LLM request.
    ///
    /// This is a **pure, synchronous** function.  It only assembles data that
    /// has already been computed by the caller:
    ///
    /// 1. System prompt + memory + skills context
    /// 2. Optional summary (from the compression hook)
    /// 3. Processed history messages (already truncated by `process_history`)
    /// 4. The current user message
    pub fn build_messages(
        &self,
        processed_messages: Vec<SessionMessage>,
        current_message: &str,
        memory: Option<&str>,
        summary: Option<&str>,
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // System prompt
        let mut system_content = (*self.system_prompt).clone();
        if let Some(mem) = memory {
            if !mem.is_empty() {
                system_content.push_str("\n\n## Long-term Memory\n");
                system_content.push_str(mem);
            }
        }
        if let Some(skills) = &self.skills_context {
            if !skills.is_empty() {
                system_content.push_str("\n\n# Skills\n\n");
                system_content.push_str(skills);
            }
        }
        messages.push(ChatMessage::system(system_content));

        // Inject summary as assistant message (if exists)
        if let Some(summary_text) = summary {
            if !summary_text.is_empty() {
                messages.push(ChatMessage::assistant(format!(
                    "{}{}",
                    SUMMARY_PREFIX, summary_text
                )));
            }
        }

        // Add processed history messages
        let history_count = processed_messages.len();
        for msg in processed_messages {
            match msg.role.as_str() {
                "user" => messages.push(ChatMessage::user(&msg.content)),
                "assistant" => messages.push(ChatMessage::assistant(&msg.content)),
                _ => {}
            }
        }

        // Current message
        messages.push(ChatMessage::user(current_message));

        debug!(
            "Built messages: {} history msgs, summary: {}",
            history_count,
            summary.is_some()
        );

        messages
    }

    /// Add an assistant message to the history
    pub fn add_assistant_message(
        &self,
        messages: &mut Vec<ChatMessage>,
        content: Option<String>,
        tool_calls: Vec<crate::providers::ToolCall>,
    ) {
        if tool_calls.is_empty() {
            // No tool calls - simple assistant message
            if let Some(c) = content {
                messages.push(ChatMessage::assistant(c));
            }
        } else {
            // Has tool calls - must include them in the message
            messages.push(ChatMessage::assistant_with_tools(content, tool_calls));
        }
    }

    /// Add a tool result to the messages
    pub fn add_tool_result(
        &self,
        messages: &mut Vec<ChatMessage>,
        tool_id: String,
        tool_name: String,
        result: String,
    ) {
        messages.push(ChatMessage::tool_result(tool_id, tool_name, result));
    }
}

/// Fallback instructions when no bootstrap files exist
const DEFAULT_INSTRUCTIONS: &str = r#"You have access to tools for reading files, writing files, editing files, listing directories, and executing shell commands.

Be concise and helpful. When using tools, explain what you're doing before and after the tool call."#;
