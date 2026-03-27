# Code Quality Fixes Design

**Date:** 2026-03-27
**Status:** Approved
**Scope:** T1-T4 code quality improvements from task.md

## Overview

This document outlines the design for fixing four code quality issues identified in the codebase review:

1. **T1:** SpawnParallelTool parsing logic optimization
2. **T2:** Filesystem path validation security hardening
3. **T3:** AgentLoop process_direct function refactoring
4. **T4:** Vault scanner performance (keeping current implementation)

## T1: SpawnParallelTool Parsing Logic

### Current State
- Uses `serde(untagged)` to handle different input formats (simple strings, objects with model_id, JSON strings)
- Logic is relatively clean with proper error handling
- Nesting depth is within acceptable limits (≤3 levels)

### Improvements
- Enhance error messages for better debugging
- Add more edge case tests for JSON string parsing

### Files
- `gasket/core/src/tools/spawn_parallel.rs`

### Acceptance Criteria
- Error messages are clear and actionable
- Test coverage includes edge cases

## T2: Filesystem Path Validation Security

### Problem Analysis

Current `validate_path` function has potential vulnerabilities:

1. **Symlink Attack:** If `allowed_dir` is a symlink or path contains symlinks, canonicalization might not match correctly
2. **Path Traversal:** Need to ensure resolved path stays within allowed directory

### Solution: Complete Security Hardening

```rust
pub struct PathValidator {
    /// Canonicalized allowed directory (resolved at initialization)
    allowed_dir: Option<PathBuf>,
}

impl PathValidator {
    pub fn new(allowed_dir: Option<PathBuf>) -> Self {
        let canonical_allowed = allowed_dir.as_ref().and_then(|p| {
            p.canonicalize().ok().or_else(|| {
                // If directory doesn't exist yet, use parent's canonical path
                p.parent().and_then(|parent| parent.canonicalize().ok())
                    .map(|canonical| canonical.join(p.file_name().unwrap_or_default()))
            })
        });
        Self { allowed_dir: canonical_allowed }
    }

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
}
```

### Security Test Cases

```rust
#[cfg(test)]
mod security_tests {
    use super::*;

    #[tokio::test]
    async fn test_symlink_escape_attack() {
        // Create symlink pointing outside workspace
        // Verify it's rejected
    }

    #[tokio::test]
    async fn test_path_traversal_attack() {
        // Test paths with ../ sequences
        // Verify they're properly resolved and rejected if outside
    }

    #[tokio::test]
    async fn test_nested_symlink_attack() {
        // Test symlinks within symlinks
        // Verify all are resolved correctly
    }
}
```

### Files
- `gasket/core/src/tools/filesystem.rs`

### Acceptance Criteria
- All symlink attack tests pass
- Path traversal attempts are blocked
- Legitimate paths within workspace still work

## T3: AgentLoop Function Refactoring

### Problem Analysis
`process_direct` function is ~170 lines with 13 distinct steps. While well-commented, it can be decomposed for better readability and testability.

### Solution: Function Decomposition

```rust
impl AgentLoop {
    /// Main entry point - now clean and readable
    pub async fn process_direct(
        &self,
        content: &str,
        session_key: &SessionKey,
    ) -> Result<AgentResponse, AgentError> {
        let session_key_str = session_key.to_string();

        // Phase 1: Prepare context and execute BeforeRequest hooks
        let (content, initial_messages) = self
            .prepare_request_context(content, &session_key_str)
            .await?;

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

        Ok(result)
    }

    // --- Private helper methods (each < 50 lines) ---

    async fn prepare_request_context(
        &self,
        content: &str,
        session_key_str: &str,
    ) -> Result<(String, Vec<ChatMessage>), AgentError> {
        // Steps 1-2: Build context and BeforeRequest hooks
    }

    async fn process_history_phase(
        &self,
        session_key_str: &str,
        content: &str,
        mut messages: Vec<ChatMessage>,
    ) -> Result<Vec<ChatMessage>, AgentError> {
        // Steps 3-8: Load session, save events, truncate history, assemble prompt
    }

    async fn execute_llm_phase(
        &self,
        messages: Vec<ChatMessage>,
        content: &str,
        session_key_str: &str,
    ) -> Result<AgentLoopResult, AgentError> {
        // Steps 9-11: AfterHistory hooks, BeforeLLM hooks, run agent loop
    }

    async fn finalize_response_phase(
        &self,
        result: &AgentLoopResult,
        content: &str,
        session_key_str: &str,
    ) -> Result<(), AgentError> {
        // Steps 12-13: Save assistant event, AfterResponse hooks
    }
}
```

### Benefits
1. **Readability:** Each function has a single responsibility
2. **Testability:** Can unit test each phase independently
3. **Maintainability:** Easier to modify individual phases

### Files
- `gasket/core/src/agent/loop_.rs`

### Acceptance Criteria
- Main function body ≤ 20 lines
- Each helper function ≤ 50 lines
- All existing tests pass
- No behavior changes

## T4: Vault Scanner Performance

### Decision: Keep Current Implementation

### Rationale
- Current byte-slice scanning is already optimal for single pattern matching (`{{vault:`)
- Aho-Corasick algorithm is beneficial for multi-pattern matching, which we don't need
- Current implementation:
  - O(n) time complexity
  - Zero allocations for pattern matching
  - No regex engine overhead

### No Changes Required

## Implementation Order

1. **T2** - Security fixes (highest priority)
2. **T3** - Function refactoring (medium priority)
3. **T1** - Error message improvements (low priority)
4. **T4** - No changes

## Testing Strategy

### Unit Tests
- Each refactored function gets its own test module
- Security tests for path validation
- Edge case tests for parsing

### Integration Tests
- Verify end-to-end behavior unchanged
- Test with real filesystem operations

## Rollback Plan

If issues arise:
1. Git revert to previous commit
2. Each task is independent - can rollback individually

## Success Metrics

- [ ] All existing tests pass
- [ ] New security tests pass
- [ ] Code coverage maintained or improved
- [ ] No behavior changes in production
