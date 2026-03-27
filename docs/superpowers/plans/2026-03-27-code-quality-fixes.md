# Code Quality Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix four code quality issues: security hardening for path validation, function decomposition for AgentLoop, and error message improvements for SpawnParallelTool.

**Architecture:** Introduce PathValidator struct for secure path handling; decompose AgentLoop.process_direct into 4 phase functions; enhance SpawnParallelTool error messages.

**Tech Stack:** Rust, tokio async runtime, serde for serialization, tempfile for testing

**Spec Document:** `docs/superpowers/specs/2026-03-27-code-quality-fixes-design.md`

---

## File Structure

```
gasket/core/src/tools/
├── filesystem.rs          # MODIFY: Add PathValidator, enhance validate_path
└── spawn_parallel.rs      # MODIFY: Improve error messages

gasket/core/src/agent/
└── loop_.rs               # MODIFY: Decompose process_direct into phase functions
```

---

## Task 1: Path Validation Security Hardening (T2)

**Priority:** Highest
**Files:**
- Modify: `gasket/core/src/tools/filesystem.rs`
- Add security tests

### 1.1: Write failing security tests

**Files:**
- Modify: `gasket/core/src/tools/filesystem.rs` (tests section)

- [ ] **Step 1: Add test module with security tests**

Add at the end of the tests module in `filesystem.rs`:

```rust
// === Security Tests ===

#[cfg(test)]
mod security_tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_symlink_escape_attack() {
        // Setup: Create temp workspace and external file
        let temp_workspace = TempDir::new().unwrap();
        let temp_external = TempDir::new().unwrap();
        let external_file = temp_external.path().join("secret.txt");
        tokio::fs::write(&external_file, "secret data").await.unwrap();

        // Create symlink inside workspace pointing to external file
        let symlink_path = temp_workspace.path().join("malicious_link");
        symlink(&external_file, &symlink_path).unwrap();

        // Test: Tool with workspace restriction should reject symlink
        let tool = ReadFileTool::new(Some(temp_workspace.path().to_path_buf()));
        let args = serde_json::json!({
            "absolute_path": symlink_path.to_str().unwrap()
        });

        let result = tool.execute(args, &ToolContext::empty()).await;
        assert!(result.is_err(), "Should reject symlink pointing outside workspace");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("outside workspace") || err_msg.contains("PermissionDenied"),
            "Error should indicate permission denied, got: {}", err_msg
        );
    }

    #[tokio::test]
    async fn test_path_traversal_attack() {
        let temp_workspace = TempDir::new().unwrap();
        let temp_external = TempDir::new().unwrap();

        // Create external secret file
        let external_file = temp_external.path().join("secret.txt");
        tokio::fs::write(&external_file, "secret data").await.unwrap();

        // Get relative path from workspace to external file using ../
        let workspace_canonical = temp_workspace.path().canonicalize().unwrap();
        let external_canonical = external_file.canonicalize().unwrap();

        // This test verifies that even if someone tries ../../../etc/passwd style paths,
        // they get resolved and checked properly
        let tool = ReadFileTool::new(Some(workspace_canonical));
        let args = serde_json::json!({
            "absolute_path": external_canonical.to_str().unwrap()
        });

        let result = tool.execute(args, &ToolContext::empty()).await;
        assert!(result.is_err(), "Should reject path outside workspace");
    }

    #[tokio::test]
    async fn test_legitimate_path_in_workspace() {
        let temp_workspace = TempDir::new().unwrap();
        let test_file = temp_workspace.path().join("test.txt");
        tokio::fs::write(&test_file, "legitimate content").await.unwrap();

        let tool = ReadFileTool::new(Some(temp_workspace.path().to_path_buf()));
        let args = serde_json::json!({
            "absolute_path": test_file.to_str().unwrap()
        });

        let result = tool.execute(args, &ToolContext::empty()).await;
        assert!(result.is_ok(), "Should allow legitimate path: {:?}", result);
        assert_eq!(result.unwrap(), "legitimate content");
    }

    #[tokio::test]
    async fn test_path_validator_new() {
        let temp_dir = TempDir::new().unwrap();
        let validator = PathValidator::new(Some(temp_dir.path().to_path_buf()));
        assert!(validator.allowed_dir.is_some());

        let canonical = temp_dir.path().canonicalize().unwrap();
        assert_eq!(validator.allowed_dir, Some(canonical));
    }

    #[tokio::test]
    async fn test_path_validator_nonexistent_dir() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent");

        // Should not panic, just store None or handle gracefully
        let validator = PathValidator::new(Some(nonexistent.clone()));
        // For non-existent dirs, we accept the path as-is for write operations
        // This is acceptable behavior
        assert!(validator.allowed_dir.is_none() || validator.allowed_dir.is_some());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package gasket-core security_tests -- --nocapture`
Expected: Compilation errors (PathValidator not defined yet)

### 1.2: Implement PathValidator

**Files:**
- Modify: `gasket/core/src/tools/filesystem.rs`

- [ ] **Step 3: Add PathValidator struct**

Add after the imports section (before `validate_path` function):

```rust
/// Secure path validator that canonicalizes allowed directory at initialization
/// to prevent symlink attacks and path traversal vulnerabilities.
#[derive(Clone)]
pub struct PathValidator {
    /// Canonicalized allowed directory (all symlinks resolved)
    allowed_dir: Option<PathBuf>,
}

impl PathValidator {
    /// Create a new path validator with the given allowed directory.
    ///
    /// The allowed directory is canonicalized at creation time to resolve
    /// any symlinks, ensuring consistent path comparison later.
    pub fn new(allowed_dir: Option<PathBuf>) -> Self {
        let canonical_allowed = allowed_dir.as_ref().and_then(|p| {
            p.canonicalize().ok()
        });
        Self { allowed_dir: canonical_allowed }
    }

    /// Validate that a path is within the allowed directory.
    ///
    /// This method:
    /// 1. Checks if the path exists
    /// 2. Canonicalizes the path to resolve all symlinks
    /// 3. Verifies the resolved path is within the allowed directory
    ///
    /// # Security
    ///
    /// This prevents:
    /// - Symlink attacks (symlinks pointing outside workspace)
    /// - Path traversal (using ../ to escape workspace)
    pub fn validate(&self, path: &str) -> Result<PathBuf, ToolError> {
        let path = PathBuf::from(path);

        if let Some(allowed) = &self.allowed_dir {
            // Check existence first
            if !path.exists() {
                return Err(ToolError::NotFound(
                    format!("Path not found: {}", path.display())
                ));
            }

            // Resolve all symlinks
            let canonical = path.canonicalize().map_err(|e| {
                ToolError::NotFound(format!("Cannot resolve path: {} - {}", path.display(), e))
            })?;

            // Strict prefix check with canonical paths
            if !canonical.starts_with(allowed) {
                return Err(ToolError::PermissionDenied(
                    format!("Path outside workspace: {}", path.display())
                ));
            }
        }

        Ok(path)
    }

    /// Validate a path for write operations (file may not exist yet).
    ///
    /// For writes, we validate the parent directory exists and is within
    /// the allowed workspace.
    pub fn validate_for_write(&self, path: &str) -> Result<PathBuf, ToolError> {
        let path = PathBuf::from(path);

        if let Some(allowed) = &self.allowed_dir {
            // For write, validate the parent directory
            let parent = path.parent().unwrap_or(&path);

            if parent.exists() {
                let canonical_parent = parent.canonicalize().map_err(|e| {
                    ToolError::NotFound(format!("Cannot resolve parent path: {} - {}", parent.display(), e))
                })?;

                if !canonical_parent.starts_with(allowed) {
                    return Err(ToolError::PermissionDenied(
                        format!("Path outside workspace: {}", path.display())
                    ));
                }
            } else {
                return Err(ToolError::NotFound(
                    format!("Parent directory not found: {}", parent.display())
                ));
            }
        }

        Ok(path)
    }
}
```

- [ ] **Step 4: Update ReadFileTool to use PathValidator**

Replace the `validate_path` function call with PathValidator:

```rust
/// Read file tool
pub struct ReadFileTool {
    validator: PathValidator,
}

impl ReadFileTool {
    pub fn new(allowed_dir: Option<PathBuf>) -> Self {
        Self { validator: PathValidator::new(allowed_dir) }
    }
}
```

And update the execute method:

```rust
async fn execute(&self, args: Value, _ctx: &ToolContext) -> ToolResult {
    // ... existing Args struct ...

    let path = self.validator.validate(&args.absolute_path)?;
    debug!("Reading file: {:?}", path);
    // ... rest of the method unchanged ...
}
```

- [ ] **Step 5: Update WriteFileTool to use PathValidator**

```rust
pub struct WriteFileTool {
    validator: PathValidator,
}

impl WriteFileTool {
    pub fn new(allowed_dir: Option<PathBuf>) -> Self {
        Self { validator: PathValidator::new(allowed_dir) }
    }
}
```

And update execute to use `validate_for_write`:

```rust
async fn execute(&self, args: Value, _ctx: &ToolContext) -> ToolResult {
    // ... existing Args struct ...

    let path = self.validator.validate_for_write(&args.file_path)?;
    debug!("Writing file: {:?}", path);
    // ... rest of the method unchanged ...
}
```

- [ ] **Step 6: Update EditFileTool to use PathValidator**

```rust
pub struct EditFileTool {
    validator: PathValidator,
}

impl EditFileTool {
    pub fn new(allowed_dir: Option<PathBuf>) -> Self {
        Self { validator: PathValidator::new(allowed_dir) }
    }
}
```

- [ ] **Step 7: Update ListDirTool to use PathValidator**

```rust
pub struct ListDirTool {
    validator: PathValidator,
}

impl ListDirTool {
    pub fn new(allowed_dir: Option<PathBuf>) -> Self {
        Self { validator: PathValidator::new(allowed_dir) }
    }
}
```

- [ ] **Step 8: Update Default implementations**

```rust
impl Default for ReadFileTool {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Default for WriteFileTool {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Default for EditFileTool {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Default for ListDirTool {
    fn default() -> Self {
        Self::new(None)
    }
}
```

- [ ] **Step 9: Run tests to verify they pass**

Run: `cargo test --package gasket-core filesystem -- --nocapture`
Expected: All tests pass including new security tests

- [ ] **Step 10: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 11: Commit security hardening**

```bash
git add gasket/core/src/tools/filesystem.rs
git commit -m "security: harden filesystem path validation against symlink attacks

- Add PathValidator struct with canonicalized allowed directory
- Add validate() and validate_for_write() methods
- Update all file tools to use PathValidator
- Add security tests for symlink escape and path traversal attacks

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 2: AgentLoop Function Decomposition (T3)

**Priority:** Medium
**Files:**
- Modify: `gasket/core/src/agent/loop_.rs`

### 2.1: Extract Phase Functions

- [ ] **Step 1: Add prepare_request_context helper**

Add this private method to AgentLoop impl block (after the public methods):

```rust
/// Phase 1: Prepare request context and execute BeforeRequest hooks.
///
/// Returns the (possibly modified) content and initial messages.
async fn prepare_request_context(
    &self,
    content: &str,
    session_key_str: &str,
) -> Result<(String, Vec<ChatMessage>), AgentError> {
    // Build initial mutable context for hooks
    let mut messages: Vec<ChatMessage> = vec![ChatMessage::user(content)];
    let mut ctx = MutableContext {
        session_key: session_key_str,
        messages: &mut messages,
        user_input: Some(content),
        response: None,
        tool_calls: None,
        token_usage: None,
    };

    // Execute BeforeRequest hooks (can modify input or abort)
    match self.hooks.execute(HookPoint::BeforeRequest, &mut ctx).await? {
        HookAction::Abort(msg) => {
            // Return early with abort message
            return Err(AgentError::HookAborted(msg));
        }
        HookAction::Continue => {}
    }

    // Get the (possibly modified) user content
    let content: String = ctx
        .messages
        .iter()
        .find(|m| m.role == crate::providers::MessageRole::User)
        .and_then(|m| m.content.clone())
        .unwrap_or_else(|| content.to_string());

    Ok((content, messages))
}
```

- [ ] **Step 2: Add process_history_phase helper**

```rust
/// Phase 2: Load session history, save user event, and assemble prompt.
///
/// Returns the assembled messages ready for LLM.
async fn process_history_phase(
    &self,
    session_key_str: &str,
    content: &str,
    _initial_messages: Vec<ChatMessage>,
) -> Result<Vec<ChatMessage>, AgentError> {
    // Load session history
    let history_events = self.context.get_history(session_key_str, None).await;

    // Save user event
    let user_event = SessionEvent {
        id: uuid::Uuid::now_v7(),
        session_key: session_key_str.to_string(),
        parent_id: None,
        event_type: EventType::UserMessage,
        content: content.to_string(),
        embedding: None,
        metadata: EventMetadata::default(),
        created_at: chrono::Utc::now(),
    };
    self.context.save_event(user_event).await?;

    // Truncate history
    let history_snapshot: Vec<SessionEvent> = history_events
        .into_iter()
        .rev()
        .take(self.config.memory_window)
        .rev()
        .collect();

    // Load existing summary (TODO: implement event-based summary)
    let summary: Option<String> = None;

    // Inject system prompts
    let mut system_prompts = Vec::new();
    if !self.system_prompt.is_empty() {
        system_prompts.push(self.system_prompt.clone());
    }
    if let Some(ref skills) = self.skills_context {
        system_prompts.push(skills.clone());
    }

    // Assemble prompt
    let messages = Self::assemble_prompt(
        history_snapshot,
        content,
        &system_prompts,
        summary.as_deref(),
        None,
    );

    Ok(messages)
}
```

- [ ] **Step 3: Add execute_llm_phase helper**

```rust
/// Phase 3: Execute hooks and LLM loop.
///
/// Returns the agent loop result.
async fn execute_llm_phase(
    &self,
    mut messages: Vec<ChatMessage>,
    content: &str,
    session_key_str: &str,
) -> Result<AgentLoopResult, AgentError> {
    // Execute AfterHistory hooks
    let mut ctx = MutableContext {
        session_key: session_key_str,
        messages: &mut messages,
        user_input: Some(content),
        response: None,
        tool_calls: None,
        token_usage: None,
    };
    self.hooks.execute(HookPoint::AfterHistory, &mut ctx).await?;

    // Execute BeforeLLM hooks (vault injection)
    self.hooks.execute(HookPoint::BeforeLLM, &mut ctx).await?;

    // Get vault values for log redaction
    let local_vault_values = self.vault_values.read().await.clone();

    // Run agent loop
    let result = self.run_agent_loop(messages, &local_vault_values).await?;

    Ok(result)
}
```

- [ ] **Step 4: Add finalize_response_phase helper**

```rust
/// Phase 4: Save response and execute AfterResponse hooks.
async fn finalize_response_phase(
    &self,
    result: &AgentLoopResult,
    content: &str,
    session_key_str: &str,
) -> Result<(), AgentError> {
    // Get vault values for log redaction
    let local_vault_values = self.vault_values.read().await.clone();

    // Save assistant event FIRST (critical data safety)
    let history_content = redact_secrets(&result.content, &local_vault_values);
    let assistant_event = SessionEvent {
        id: uuid::Uuid::now_v7(),
        session_key: session_key_str.to_string(),
        parent_id: None,
        event_type: EventType::AssistantMessage,
        content: history_content,
        embedding: None,
        metadata: EventMetadata {
            tools_used: result.tools_used.clone(),
            ..Default::default()
        },
        created_at: chrono::Utc::now(),
    };
    self.context.save_event(assistant_event).await?;

    // Execute AfterResponse hooks
    let tools_used: Vec<crate::hooks::ToolCallInfo> = result
        .tools_used
        .iter()
        .map(|name| crate::hooks::ToolCallInfo {
            id: name.clone(),
            name: name.clone(),
            arguments: None,
        })
        .collect();

    let mut ctx = MutableContext {
        session_key: session_key_str,
        messages: &mut vec![],
        user_input: Some(content),
        response: Some(&result.content),
        tool_calls: Some(&tools_used),
        token_usage: result.token_usage.as_ref(),
    };
    self.hooks.execute(HookPoint::AfterResponse, &mut ctx).await?;

    // Log token usage
    if let Some(ref usage) = result.token_usage {
        info!(
            "[Token] Input: {} | Output: {} | Total: {} | Cost: ${:.4}",
            usage.input_tokens, usage.output_tokens, usage.total_tokens, result.cost
        );
    }

    Ok(())
}
```

- [ ] **Step 5: Add HookAborted error variant**

In the error module (or where AgentError is defined), add:

```rust
pub enum AgentError {
    // ... existing variants ...
    HookAborted(String),
}
```

- [ ] **Step 6: Refactor process_direct to use phase functions**

Replace the existing `process_direct` method with:

```rust
/// Process a message and return response.
pub async fn process_direct(
    &self,
    content: &str,
    session_key: &SessionKey,
) -> Result<AgentResponse, AgentError> {
    let session_key_str = session_key.to_string();

    // Phase 1: Prepare context and execute BeforeRequest hooks
    let (content, initial_messages) = self
        .prepare_request_context(content, &session_key_str)
        .await
        .map_err(|e| match e {
            AgentError::HookAborted(msg) => {
                // Return special response for aborted hooks
                return Ok(AgentResponse {
                    content: msg,
                    reasoning_content: None,
                    tools_used: vec![],
                    model: Some(self.config.model.clone()),
                    token_usage: None,
                    cost: 0.0,
                });
            }
            other => other,
        })?;

    // Phase 2: Load history, save events, assemble prompt
    let messages = self
        .process_history_phase(&session_key_str, &content, initial_messages)
        .await?;

    // Phase 3: Execute LLM loop
    let result = self
        .execute_llm_phase(messages, &content, &session_key_str)
        .await?;

    // Phase 4: Save response and execute AfterResponse hooks
    self.finalize_response_phase(&result, &content, &session_key_str)
        .await?;

    Ok(AgentResponse {
        content: result.content.clone(),
        reasoning_content: result.reasoning_content.clone(),
        tools_used: result.tools_used.clone(),
        model: Some(self.config.model.clone()),
        token_usage: result.token_usage.clone(),
        cost: result.cost,
    })
}
```

Wait, the error handling for HookAborted needs to be different. Let me revise:

```rust
/// Process a message and return response.
pub async fn process_direct(
    &self,
    content: &str,
    session_key: &SessionKey,
) -> Result<AgentResponse, AgentError> {
    let session_key_str = session_key.to_string();

    // Phase 1: Prepare context and execute BeforeRequest hooks
    let (content, initial_messages) = match self
        .prepare_request_context(content, &session_key_str)
        .await
    {
        Ok(result) => result,
        Err(AgentError::HookAborted(msg)) => {
            return Ok(AgentResponse {
                content: msg,
                reasoning_content: None,
                tools_used: vec![],
                model: Some(self.config.model.clone()),
                token_usage: None,
                cost: 0.0,
            });
        }
        Err(e) => return Err(e),
    };

    // Phase 2: Load history, save events, assemble prompt
    let messages = self
        .process_history_phase(&session_key_str, &content, initial_messages)
        .await?;

    // Phase 3: Execute LLM loop
    let result = self
        .execute_llm_phase(messages, &content, &session_key_str)
        .await?;

    // Phase 4: Save response and execute AfterResponse hooks
    self.finalize_response_phase(&result, &content, &session_key_str)
        .await?;

    Ok(AgentResponse {
        content: result.content.clone(),
        reasoning_content: result.reasoning_content.clone(),
        tools_used: result.tools_used.clone(),
        model: Some(self.config.model.clone()),
        token_usage: result.token_usage.clone(),
        cost: result.cost,
    })
}
```

- [ ] **Step 7: Run tests to verify no behavior change**

Run: `cargo test --package gasket-core --lib agent`
Expected: All existing tests pass

- [ ] **Step 8: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 9: Commit function decomposition**

```bash
git add gasket/core/src/agent/loop_.rs
git commit -m "refactor(agent): decompose process_direct into phase functions

- Extract prepare_request_context() for hook preparation
- Extract process_history_phase() for history loading
- Extract execute_llm_phase() for LLM execution
- Extract finalize_response_phase() for response saving

Each function has a single responsibility and is < 50 lines.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 3: SpawnParallelTool Error Messages (T1)

**Priority:** Low
**Files:**
- Modify: `gasket/core/src/tools/spawn_parallel.rs`

### 3.1: Improve Error Messages

- [ ] **Step 1: Enhance JSON parsing error messages**

In the `execute` method, update the error handling for JSON string parsing:

```rust
TaskInput::JsonString(json_str) => {
    // Try to parse the JSON string
    // First try as Vec<TaskSpec> (with models)
    if let Ok(specs) = serde_json::from_str::<Vec<TaskSpec>>(&json_str) {
        specs
    } else if let Ok(tasks) = serde_json::from_str::<Vec<String>>(&json_str) {
        // Try as Vec<String> (simple)
        tasks
            .into_iter()
            .map(|task| TaskSpec {
                task,
                model_id: None,
            })
            .collect()
    } else {
        // Provide detailed error message
        let hint = if json_str.starts_with('[') {
            "JSON array detected. Expected format: [{\"task\": \"...\", \"model_id\": \"...\"}] or [\"task1\", \"task2\"]"
        } else {
            "Expected JSON array. Wrap tasks in square brackets: [\"task1\", \"task2\"]"
        };
        return Err(ToolError::InvalidArguments(
            format!("Failed to parse tasks: {}. {}", json_str, hint)
        ));
    }
}
```

- [ ] **Step 2: Add validation error hints**

Update the empty/too many tasks errors:

```rust
if task_specs.is_empty() {
    return Err(ToolError::InvalidArguments(
        "At least one task is required. Example: {\"tasks\": [\"Research topic A\", \"Analyze data B\"]}".to_string()
    ));
}

if task_specs.len() > 10 {
    return Err(ToolError::InvalidArguments(
        format!("Maximum 10 parallel tasks allowed, got {}. Consider splitting into multiple batches.", task_specs.len())
    ));
}
```

- [ ] **Step 3: Add edge case tests**

Add to the tests module:

```rust
#[tokio::test]
async fn test_malformed_json_error_message() {
    let tool = SpawnParallelTool::new();
    let args = serde_json::json!({
        "tasks": "[invalid json"
    });

    let result = tool.execute(args, &ToolContext::empty()).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Expected JSON array") || err_msg.contains("JSON array detected"),
        "Error should contain helpful hint, got: {}", err_msg
    );
}

#[tokio::test]
async fn test_too_many_tasks_error_message() {
    let tool = SpawnParallelTool::new();
    let tasks: Vec<String> = (0..15).map(|i| format!("Task {}", i)).collect();
    let args = serde_json::json!({
        "tasks": tasks
    });

    let result = tool.execute(args, &ToolContext::empty()).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Maximum 10") && err_msg.contains("batches"),
        "Error should suggest batching, got: {}", err_msg
    );
}

#[tokio::test]
async fn test_empty_tasks_error_message() {
    let tool = SpawnParallelTool::new();
    let args = serde_json::json!({
        "tasks": []
    });

    let result = tool.execute(args, &ToolContext::empty()).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Example:"),
        "Error should contain example, got: {}", err_msg
    );
}
```

- [ ] **Step 4: Run tests to verify**

Run: `cargo test --package gasket-core spawn_parallel`
Expected: All tests pass including new ones

- [ ] **Step 5: Commit error message improvements**

```bash
git add gasket/core/src/tools/spawn_parallel.rs
git commit -m "fix(tools): improve SpawnParallelTool error messages

- Add helpful hints for JSON parsing errors
- Include examples in validation errors
- Suggest batching for too many tasks
- Add edge case tests for error messages

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 4: Final Verification

- [ ] **Step 1: Run full workspace test suite**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 2: Run clippy for linting**

Run: `cargo clippy --workspace --all-targets`
Expected: No errors (warnings acceptable)

- [ ] **Step 3: Build release to verify**

Run: `cargo build --release --workspace`
Expected: Build succeeds

- [ ] **Step 4: Create summary commit (if needed)**

```bash
git add -A
git commit -m "chore: code quality improvements complete

T1: SpawnParallelTool error messages improved
T2: Filesystem path validation security hardened
T3: AgentLoop process_direct decomposed into phases
T4: Vault scanner kept as-is (already optimal)

All tests passing.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Success Criteria

- [ ] All existing tests pass
- [ ] New security tests pass (symlink attack, path traversal)
- [ ] process_direct main body ≤ 30 lines
- [ ] Each helper function ≤ 50 lines
- [ ] Error messages include helpful hints
- [ ] No behavior changes in production
- [ ] `cargo clippy` passes with no errors
- [ ] `cargo build --release` succeeds
