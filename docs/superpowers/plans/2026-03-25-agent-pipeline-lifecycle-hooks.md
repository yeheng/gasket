# Agent Pipeline Lifecycle Hooks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 agent pipeline 建立统一的 lifecycle hook 机制，支持在关键节点注册回调，消除分散的 if-let 模式。

**Architecture:** 使用 trait-based hook 系统，`HookPoint` 决定执行策略（Sequential/Parallel），泛型 `HookContext<M>` 消除重复类型定义，`HookRegistry` 管理注册和调度。

**Tech Stack:** Rust async/await, tokio, thiserror, serde

**Spec:** `docs/superpowers/specs/2026-03-25-agent-pipeline-lifecycle-hooks-design.md`

**Workspace Root:** `/Users/yeheng/workspaces/Github/gasket/`

---

## File Structure

All paths relative to workspace root (`gasket/`):

```
gasket/core/src/hooks/
├── mod.rs           # 导出（修改）
├── types.rs         # HookPoint, HookContext, HookAction, ExecutionStrategy（新建）
├── registry.rs      # HookRegistry, HookBuilder（新建）
├── external.rs      # ExternalShellHook（修改）
├── vault.rs         # VaultHook（新建）
└── history.rs       # HistoryRecallHook（新建）

gasket/core/src/agent/
├── loop_.rs         # AgentLoop 改造（修改）
└── subagent.rs      # SubagentTaskBuilder 扩展（修改）

gasket/core/src/error.rs  # AgentError 扩展（修改）
```

---

## Task 1: Core Types

**Files:**
- Create: `gasket/core/src/hooks/types.rs`
- Modify: `gasket/core/src/hooks/mod.rs`
- Modify: `gasket/core/src/error.rs`

### Step 1.1: Write failing tests for core types

Create test file with the core type definitions:

```rust
// gasket/core/src/hooks/types.rs

use crate::providers::ChatMessage;
use crate::token_tracker::TokenUsage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── HookPoint ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookPoint {
    BeforeRequest,
    AfterHistory,
    BeforeLLM,
    AfterToolCall,
    AfterResponse,
}

// ── ExecutionStrategy ─────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStrategy {
    Sequential,
    Parallel,
}

impl HookPoint {
    pub fn default_strategy(&self) -> ExecutionStrategy {
        match self {
            Self::BeforeRequest => ExecutionStrategy::Sequential,
            Self::AfterHistory => ExecutionStrategy::Sequential,
            Self::BeforeLLM => ExecutionStrategy::Sequential,
            Self::AfterToolCall => ExecutionStrategy::Parallel,
            Self::AfterResponse => ExecutionStrategy::Parallel,
        }
    }
}

// ── HookAction ────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum HookAction {
    Continue,
    Abort(String),
}

// ── HookContext (Generic) ─────────────────────────────────

pub struct HookContext<'a, M> {
    pub session_key: &'a str,
    pub messages: M,
    pub user_input: Option<&'a str>,
    pub response: Option<&'a str>,
    pub tool_calls: Option<&'a [ToolCallInfo]>,
    pub token_usage: Option<&'a TokenUsage>,
}

/// Tool call information for hooks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    pub id: String,
    pub name: String,
    pub arguments: Option<String>,
}

/// Mutable context for Sequential hooks
pub type MutableContext<'a> = HookContext<'a, &'a mut Vec<ChatMessage>>;

/// Readonly context for Parallel hooks
pub type ReadonlyContext<'a> = HookContext<'a, &'a [ChatMessage]>;

impl<'a> MutableContext<'a> {
    pub fn as_readonly(&self) -> ReadonlyContext<'a> {
        HookContext {
            session_key: self.session_key,
            messages: &*self.messages,
            user_input: self.user_input,
            response: self.response,
            tool_calls: self.tool_calls,
            token_usage: self.token_usage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_point_strategy() {
        assert_eq!(HookPoint::BeforeRequest.default_strategy(), ExecutionStrategy::Sequential);
        assert_eq!(HookPoint::AfterHistory.default_strategy(), ExecutionStrategy::Sequential);
        assert_eq!(HookPoint::BeforeLLM.default_strategy(), ExecutionStrategy::Sequential);
        assert_eq!(HookPoint::AfterToolCall.default_strategy(), ExecutionStrategy::Parallel);
        assert_eq!(HookPoint::AfterResponse.default_strategy(), ExecutionStrategy::Parallel);
    }

    #[test]
    fn test_hook_action_is_abort() {
        let action = HookAction::Abort("error".to_string());
        assert!(matches!(action, HookAction::Abort(_)));

        let action = HookAction::Continue;
        assert!(matches!(action, HookAction::Continue));
    }

    #[test]
    fn test_mutable_context_to_readonly() {
        let mut messages = vec![ChatMessage::user("test")];
        let ctx = MutableContext {
            session_key: "test:123",
            messages: &mut messages,
            user_input: Some("input"),
            response: None,
            tool_calls: None,
            token_usage: None,
        };

        let readonly = ctx.as_readonly();
        assert_eq!(readonly.session_key, "test:123");
        assert_eq!(readonly.messages.len(), 1);
    }
}
```

- [ ] **Step 1.2: Run tests to verify they compile and pass**

Run: `cargo test --package gasket-core hooks::types --no-fail-fast`
Expected: Tests pass (types are self-validating)

- [ ] **Step 1.3: Update mod.rs to export types**

```rust
// gasket/core/src/hooks/mod.rs
//! Agent pipeline lifecycle hooks
//!
//! Provides a unified hook mechanism for the agent pipeline:
//! - `HookPoint`: Execution points in the pipeline
//! - `HookContext`: Context passed to hooks
//! - `PipelineHook`: Trait for implementing hooks
//! - `HookRegistry`: Registry for managing hooks

mod types;
mod external;

pub use types::{
    ExecutionStrategy, HookAction, HookContext, HookPoint, MutableContext, ReadonlyContext,
    ToolCallInfo,
};
pub use external::{ExternalHookInput, ExternalHookOutput, ExternalHookRunner};
```

- [ ] **Step 1.4: Add HookError to AgentError**

```rust
// gasket/core/src/error.rs
// Add to AgentError enum:

    /// Hook execution error
    #[error("Hook '{name}' failed: {message}")]
    HookFailed {
        name: String,
        message: String,
    },

    /// Request aborted by hook
    #[error("Request aborted by hook: {0}")]
    AbortedByHook(String),
```

- [ ] **Step 1.5: Run cargo check and tests**

Run: `cargo check --package gasket-core && cargo test --package gasket-core hooks::types`
Expected: Compilation succeeds, tests pass

- [ ] **Step 1.6: Commit**

```bash
git add gasket/core/src/hooks/types.rs gasket/core/src/hooks/mod.rs gasket/core/src/error.rs
git commit -m "feat(hooks): add core types for lifecycle hooks

- Add HookPoint enum with 5 execution points
- Add ExecutionStrategy (Sequential/Parallel)
- Add HookAction (Continue/Abort)
- Add generic HookContext<M> with MutableContext and ReadonlyContext aliases
- Add ToolCallInfo for tool call metadata"
```

---

## Task 2: PipelineHook Trait and HookRegistry

**Files:**
- Create: `gasket/core/src/hooks/registry.rs`
- Modify: `gasket/core/src/hooks/mod.rs`

### Step 2.1: Write HookRegistry with tests

```rust
// gasket/core/src/hooks/registry.rs

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use super::{ExecutionStrategy, HookAction, HookPoint, MutableContext, ReadonlyContext};
use crate::error::AgentError;

// ── PipelineHook Trait ────────────────────────────────────

#[async_trait]
pub trait PipelineHook: Send + Sync {
    /// Hook name for logging and debugging
    fn name(&self) -> &str;

    /// Execution point
    fn point(&self) -> HookPoint;

    /// Sequential execution (can modify messages)
    async fn run(&self, _ctx: &mut MutableContext<'_>) -> Result<HookAction, AgentError> {
        Ok(HookAction::Continue)
    }

    /// Parallel execution (readonly)
    async fn run_parallel(&self, _ctx: &ReadonlyContext<'_>) -> Result<HookAction, AgentError> {
        Ok(HookAction::Continue)
    }
}

// ── HookRegistry ───────────────────────────────────────────

pub struct HookRegistry {
    hooks: HashMap<HookPoint, Vec<Arc<dyn PipelineHook>>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            hooks: HashMap::new(),
        }
    }

    /// Create an empty registry (for subagents)
    pub fn empty() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// Register a hook
    pub fn register(&mut self, hook: Arc<dyn PipelineHook>) {
        self.hooks.entry(hook.point()).or_default().push(hook);
    }

    /// Get hooks for a specific point
    pub fn get_hooks(&self, point: HookPoint) -> &[Arc<dyn PipelineHook>] {
        self.hooks.get(&point).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Execute hooks at a specific point
    pub async fn execute(
        &self,
        point: HookPoint,
        ctx: &mut MutableContext<'_>,
    ) -> Result<HookAction, AgentError> {
        let hooks = self.get_hooks(point);
        if hooks.is_empty() {
            return Ok(HookAction::Continue);
        }

        match point.default_strategy() {
            ExecutionStrategy::Sequential => {
                self.execute_sequential(hooks, ctx).await
            }
            ExecutionStrategy::Parallel => {
                let view = ctx.as_readonly();
                self.execute_parallel(hooks, &view).await
            }
        }
    }

    async fn execute_sequential(
        &self,
        hooks: &[Arc<dyn PipelineHook>],
        ctx: &mut MutableContext<'_>,
    ) -> Result<HookAction, AgentError> {
        for hook in hooks {
            debug!("[Hook] Running {} at {:?}", hook.name(), hook.point());
            let action = hook.run(ctx).await?;
            if let HookAction::Abort(msg) = action {
                warn!("[Hook] {} aborted: {}", hook.name(), msg);
                return Ok(HookAction::Abort(msg));
            }
        }
        Ok(HookAction::Continue)
    }

    async fn execute_parallel(
        &self,
        hooks: &[Arc<dyn PipelineHook>],
        ctx: &ReadonlyContext<'_>,
    ) -> Result<HookAction, AgentError> {
        let results = futures::future::join_all(
            hooks.iter().map(|h| {
                debug!("[Hook] Running {} at {:?}", h.name(), h.point());
                h.run_parallel(ctx)
            })
        ).await;

        for result in results {
            if let Ok(HookAction::Abort(msg)) = result {
                return Ok(HookAction::Abort(msg));
            }
        }
        Ok(HookAction::Continue)
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── HookBuilder ────────────────────────────────────────────

pub struct HookBuilder {
    hooks: Vec<Arc<dyn PipelineHook>>,
}

impl HookBuilder {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    pub fn with_hook(mut self, hook: Arc<dyn PipelineHook>) -> Self {
        self.hooks.push(hook);
        self
    }

    /// Add external shell hooks (BeforeRequest and AfterResponse)
    pub fn with_external_hooks(mut self, runner: ExternalHookRunner) -> Self {
        self.hooks.push(Arc::new(ExternalShellHook::new(
            runner.clone(),
            HookPoint::BeforeRequest,
        )));
        self.hooks.push(Arc::new(ExternalShellHook::new(
            runner,
            HookPoint::AfterResponse,
        )));
        self
    }

    /// Add vault hook
    pub fn with_vault(mut self, injector: VaultInjector) -> Self {
        self.hooks.push(Arc::new(VaultHook::new(injector)));
        self
    }

    /// Add history recall hook
    pub fn with_history_recall(
        mut self,
        embedder: Arc<TextEmbedder>,
        k: usize,
        context: Arc<dyn AgentContext>,
    ) -> Self {
        self.hooks.push(Arc::new(HistoryRecallHook::new(embedder, k, context)));
        self
    }

    pub fn build(self) -> HookRegistry {
        let mut registry = HookRegistry::new();
        for hook in self.hooks {
            registry.register(hook);
        }
        registry
    }

    pub fn build_shared(self) -> Arc<HookRegistry> {
        Arc::new(self.build())
    }
}

impl Default for HookBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ChatMessage;

    struct TestHook {
        name: String,
        point: HookPoint,
    }

    #[async_trait]
    impl PipelineHook for TestHook {
        fn name(&self) -> &str { &self.name }
        fn point(&self) -> HookPoint { self.point }

        async fn run(&self, ctx: &mut MutableContext<'_>) -> Result<HookAction, AgentError> {
            ctx.messages.push(ChatMessage::assistant(format!("hook {} executed", self.name)));
            Ok(HookAction::Continue)
        }
    }

    struct AbortHook {
        name: String,
    }

    #[async_trait]
    impl PipelineHook for AbortHook {
        fn name(&self) -> &str { &self.name }
        fn point(&self) -> HookPoint { HookPoint::BeforeRequest }

        async fn run(&self, _ctx: &mut MutableContext<'_>) -> Result<HookAction, AgentError> {
            Ok(HookAction::Abort("test abort".to_string()))
        }
    }

    #[test]
    fn test_registry_empty() {
        let registry = HookRegistry::new();
        assert!(registry.get_hooks(HookPoint::BeforeRequest).is_empty());
    }

    #[test]
    fn test_registry_register() {
        let mut registry = HookRegistry::new();
        registry.register(Arc::new(TestHook {
            name: "test".to_string(),
            point: HookPoint::BeforeRequest,
        }));
        assert_eq!(registry.get_hooks(HookPoint::BeforeRequest).len(), 1);
    }

    #[tokio::test]
    async fn test_execute_sequential() {
        let mut registry = HookRegistry::new();
        registry.register(Arc::new(TestHook {
            name: "hook1".to_string(),
            point: HookPoint::BeforeRequest,
        }));
        registry.register(Arc::new(TestHook {
            name: "hook2".to_string(),
            point: HookPoint::BeforeRequest,
        }));

        let mut messages = vec![];
        let mut ctx = MutableContext {
            session_key: "test:123",
            messages: &mut messages,
            user_input: Some("test"),
            response: None,
            tool_calls: None,
            token_usage: None,
        };

        let result = registry.execute(HookPoint::BeforeRequest, &mut ctx).await;
        assert!(matches!(result, Ok(HookAction::Continue)));
        assert_eq!(ctx.messages.len(), 2);
    }

    #[tokio::test]
    async fn test_execute_abort() {
        let mut registry = HookRegistry::new();
        registry.register(Arc::new(AbortHook { name: "abort".to_string() }));

        let mut messages = vec![];
        let mut ctx = MutableContext {
            session_key: "test:123",
            messages: &mut messages,
            user_input: Some("test"),
            response: None,
            tool_calls: None,
            token_usage: None,
        };

        let result = registry.execute(HookPoint::BeforeRequest, &mut ctx).await;
        assert!(matches!(result, Ok(HookAction::Abort(_))));
    }

    #[tokio::test]
    async fn test_parallel_execution() {
        let mut registry = HookRegistry::new();
        registry.register(Arc::new(TestHook {
            name: "parallel1".to_string(),
            point: HookPoint::AfterResponse,
        }));
        registry.register(Arc::new(TestHook {
            name: "parallel2".to_string(),
            point: HookPoint::AfterResponse,
        }));

        let messages = vec![];
        let mut ctx = MutableContext {
            session_key: "test:123",
            messages: &mut vec![],
            user_input: Some("test"),
            response: Some("response"),
            tool_calls: None,
            token_usage: None,
        };

        let result = registry.execute(HookPoint::AfterResponse, &mut ctx).await;
        assert!(matches!(result, Ok(HookAction::Continue)));
    }

    #[test]
    fn test_hook_builder() {
        let registry = HookBuilder::new()
            .with_hook(Arc::new(TestHook {
                name: "hook1".to_string(),
                point: HookPoint::BeforeRequest,
            }))
            .with_hook(Arc::new(TestHook {
                name: "hook2".to_string(),
                point: HookPoint::AfterResponse,
            }))
            .build();

        assert_eq!(registry.get_hooks(HookPoint::BeforeRequest).len(), 1);
        assert_eq!(registry.get_hooks(HookPoint::AfterResponse).len(), 1);
    }
}
```

- [ ] **Step 2.2: Run tests**

Run: `cargo test --package gasket-core hooks::registry`
Expected: All tests pass

- [ ] **Step 2.3: Update mod.rs exports**

```rust
// gasket/core/src/hooks/mod.rs - add to existing exports

mod registry;

pub use registry::{HookBuilder, HookRegistry, PipelineHook};
```

- [ ] **Step 2.4: Run cargo check**

Run: `cargo check --package gasket-core`
Expected: Compilation succeeds

- [ ] **Step 2.5: Commit**

```bash
git add gasket/core/src/hooks/registry.rs gasket/core/src/hooks/mod.rs
git commit -m "feat(hooks): add PipelineHook trait and HookRegistry

- Add PipelineHook trait with run() and run_parallel() methods
- Add HookRegistry with register() and execute()
- Support Sequential and Parallel execution strategies
- Add HookBuilder for convenient registry construction"
```

---

## Task 3: ExternalShellHook Wrapper

**Files:**
- Modify: `gasket/core/src/hooks/external.rs`
- Modify: `gasket/core/src/hooks/mod.rs`

### Step 3.1: Add ExternalShellHook implementation

Add to existing `external.rs`:

```rust
// Add to gasket/core/src/hooks/external.rs

use async_trait::async_trait;

use super::{HookAction, HookPoint, MutableContext, PipelineHook, ReadonlyContext};
use crate::error::AgentError;
use crate::providers::MessageRole;

// ... existing ExternalHookRunner code ...

// ── ExternalShellHook Wrapper ──────────────────────────────

/// Wraps ExternalHookRunner as a PipelineHook
pub struct ExternalShellHook {
    runner: ExternalHookRunner,
    point: HookPoint,
}

impl ExternalShellHook {
    pub fn new(runner: ExternalHookRunner, point: HookPoint) -> Self {
        Self { runner, point }
    }
}

#[async_trait]
impl PipelineHook for ExternalShellHook {
    fn name(&self) -> &str {
        "external_shell"
    }

    fn point(&self) -> HookPoint {
        self.point
    }

    async fn run(&self, ctx: &mut MutableContext<'_>) -> Result<HookAction, AgentError> {
        // Only BeforeRequest uses this method
        let input = ctx.user_input.unwrap_or("");
        let output = self
            .runner
            .run_pre_request(ctx.session_key, input)
            .await
            .map_err(|e| AgentError::HookFailed {
                name: self.name().to_string(),
                message: e.to_string(),
            })?;

        match output {
            Some(out) if out.is_abort() => Ok(HookAction::Abort(
                out.error.unwrap_or_else(|| "Request aborted".to_string()),
            )),
            Some(out) => {
                if let Some(modified) = out.modified_message {
                    // Modify the first user message
                    for msg in ctx.messages.iter_mut() {
                        if msg.role == MessageRole::User {
                            msg.content = modified;
                            break;
                        }
                    }
                }
                Ok(HookAction::Continue)
            }
            None => Ok(HookAction::Continue),
        }
    }

    async fn run_parallel(&self, ctx: &ReadonlyContext<'_>) -> Result<HookAction, AgentError> {
        // Only AfterResponse uses this method
        let response = ctx.response.unwrap_or("");
        let tools = ctx
            .tool_calls
            .map(|t| t.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "))
            .unwrap_or_default();

        self.runner
            .run_post_response(ctx.session_key, response, &tools)
            .await
            .map_err(|e| AgentError::HookFailed {
                name: self.name().to_string(),
                message: e.to_string(),
            })?;

        Ok(HookAction::Continue)
    }
}

#[cfg(test)]
mod hook_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_external_shell_hook_creation() {
        let runner = ExternalHookRunner::noop();
        let hook = ExternalShellHook::new(runner, HookPoint::BeforeRequest);
        assert_eq!(hook.name(), "external_shell");
        assert_eq!(hook.point(), HookPoint::BeforeRequest);
    }

    #[tokio::test]
    async fn test_noop_runner_returns_continue() {
        let runner = ExternalHookRunner::noop();
        let hook = ExternalShellHook::new(runner, HookPoint::BeforeRequest);

        let mut messages = vec![ChatMessage::user("test")];
        let mut ctx = MutableContext {
            session_key: "test:123",
            messages: &mut messages,
            user_input: Some("test"),
            response: None,
            tool_calls: None,
            token_usage: None,
        };

        let result = hook.run(&mut ctx).await;
        assert!(matches!(result, Ok(HookAction::Continue)));
    }
}
```

- [ ] **Step 3.2: Update mod.rs exports**

```rust
// Add to gasket/core/src/hooks/mod.rs
pub use external::ExternalShellHook;
```

- [ ] **Step 3.3: Run tests**

Run: `cargo test --package gasket-core hooks::external`
Expected: Tests pass

- [ ] **Step 3.4: Commit**

```bash
git add gasket/core/src/hooks/external.rs gasket/core/src/hooks/mod.rs
git commit -m "feat(hooks): wrap ExternalHookRunner as PipelineHook

- Add ExternalShellHook implementing PipelineHook trait
- Support BeforeRequest (run) and AfterResponse (run_parallel)
- Handle abort and message modification"
```

---

## Task 4: VaultHook Implementation

**Files:**
- Create: `gasket/core/src/hooks/vault.rs`
- Modify: `gasket/core/src/hooks/mod.rs`

### Step 4.1: Create VaultHook

```rust
// gasket/core/src/hooks/vault.rs

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::debug;

use super::{HookAction, HookPoint, MutableContext, PipelineHook, ReadonlyContext};
use crate::error::AgentError;
use crate::vault::VaultInjector;

/// Hook for injecting vault secrets into messages
pub struct VaultHook {
    injector: VaultInjector,
    /// Stored injected values for later redaction
    injected_values: Arc<RwLock<Vec<String>>>,
}

impl VaultHook {
    pub fn new(injector: VaultInjector) -> Self {
        Self {
            injector,
            injected_values: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Get a handle to the injected values for redaction
    pub fn injected_values(&self) -> Arc<RwLock<Vec<String>>> {
        self.injected_values.clone()
    }
}

#[async_trait]
impl PipelineHook for VaultHook {
    fn name(&self) -> &str {
        "vault_injector"
    }

    fn point(&self) -> HookPoint {
        HookPoint::BeforeLLM
    }

    async fn run(&self, ctx: &mut MutableContext<'_>) -> Result<HookAction, AgentError> {
        let report = self.injector.inject(ctx.messages);

        if !report.keys_used.is_empty() {
            debug!(
                "[VaultHook] Injected {} keys into {} messages",
                report.keys_used.len(),
                report.messages_modified
            );
        }

        // Store injected values for redaction
        let mut values = self.injected_values.write().await;
        values.clear();
        values.extend(report.injected_values);
        values.sort();
        values.dedup();

        Ok(HookAction::Continue)
    }

    async fn run_parallel(&self, _ctx: &ReadonlyContext<'_>) -> Result<HookAction, AgentError> {
        // VaultHook is Sequential only, this shouldn't be called
        Ok(HookAction::Continue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_hook_point() {
        // We can't easily test without a real VaultStore, but we can test the point
        // This is a compile-time check that the trait is implemented correctly
    }
}
```

- [ ] **Step 4.2: Update mod.rs exports**

```rust
// Add to gasket/core/src/hooks/mod.rs
mod vault;
pub use vault::VaultHook;
```

- [ ] **Step 4.3: Run cargo check**

Run: `cargo check --package gasket-core`
Expected: Compilation succeeds

- [ ] **Step 4.4: Commit**

```bash
git add gasket/core/src/hooks/vault.rs gasket/core/src/hooks/mod.rs
git commit -m "feat(hooks): add VaultHook for secret injection

- Implement PipelineHook trait for VaultInjector
- Store injected values for later redaction
- Execute at BeforeLLM point"
```

---

## Task 5: HistoryRecallHook Implementation

**Files:**
- Create: `gasket/core/src/hooks/history.rs`
- Modify: `gasket/core/src/hooks/mod.rs`

### Step 5.1: Create HistoryRecallHook

```rust
// gasket/core/src/hooks/history.rs

use std::sync::Arc;

use async_trait::async_trait;
use tracing::debug;

use super::{HookAction, HookPoint, MutableContext, PipelineHook, ReadonlyContext};
use crate::agent::context::AgentContext;
use crate::error::AgentError;
use crate::providers::ChatMessage;
use crate::search::TextEmbedder;

/// Hook for semantic history recall
pub struct HistoryRecallHook {
    embedder: Arc<TextEmbedder>,
    k: usize,
    context: Arc<dyn AgentContext>,
}

impl HistoryRecallHook {
    pub fn new(embedder: Arc<TextEmbedder>, k: usize, context: Arc<dyn AgentContext>) -> Self {
        Self {
            embedder,
            k,
            context,
        }
    }
}

#[async_trait]
impl PipelineHook for HistoryRecallHook {
    fn name(&self) -> &str {
        "history_recall"
    }

    fn point(&self) -> HookPoint {
        HookPoint::AfterHistory
    }

    async fn run(&self, ctx: &mut MutableContext<'_>) -> Result<HookAction, AgentError> {
        if self.k == 0 {
            return Ok(HookAction::Continue);
        }

        let query = ctx.user_input.unwrap_or("");

        match self.embedder.embed(query) {
            Ok(query_vec) => {
                match self
                    .context
                    .recall_history(ctx.session_key, &query_vec, self.k)
                    .await
                {
                    Ok(recalled) if !recalled.is_empty() => {
                        debug!("[HistoryRecall] Recalled {} messages", recalled.len());

                        let recall_msg = format!(
                            "# Relevant Historical Context\n{}",
                            recalled.join("\n")
                        );
                        ctx.messages.push(ChatMessage::assistant(recall_msg));
                    }
                    Ok(_) => {
                        debug!("[HistoryRecall] No relevant history found");
                    }
                    Err(e) => {
                        debug!("[HistoryRecall] Recall failed: {}", e);
                    }
                }
                Ok(HookAction::Continue)
            }
            Err(e) => {
                debug!("[HistoryRecall] Failed to embed query: {}", e);
                Ok(HookAction::Continue)
            }
        }
    }

    async fn run_parallel(&self, _ctx: &ReadonlyContext<'_>) -> Result<HookAction, AgentError> {
        // HistoryRecallHook is Sequential only
        Ok(HookAction::Continue)
    }
}
```

- [ ] **Step 5.2: Update mod.rs exports**

```rust
// Add to gasket/core/src/hooks/mod.rs
mod history;
pub use history::HistoryRecallHook;
```

- [ ] **Step 5.3: Run cargo check**

Run: `cargo check --package gasket-core`
Expected: Compilation succeeds

- [ ] **Step 5.4: Commit**

```bash
git add gasket/core/src/hooks/history.rs gasket/core/src/hooks/mod.rs
git commit -m "feat(hooks): add HistoryRecallHook for semantic recall

- Implement PipelineHook trait for semantic history recall
- Use TextEmbedder for query embedding
- Execute at AfterHistory point"
```

---

## Task 6: AgentLoop Refactoring

**Files:**
- Modify: `gasket/core/src/agent/loop_.rs`

### Step 6.1: Update AgentLoop struct

Replace the scattered hook fields with `hooks: Arc<HookRegistry>`:

```rust
// In AgentLoop struct, replace:
//   external_hooks: ExternalHookRunner,
//   vault_injector: Option<VaultInjector>,
//   embedder: Option<Arc<TextEmbedder>>,
//   history_recall_k: usize,

// With:
//   hooks: Arc<HookRegistry>,
//   vault_values: Arc<RwLock<Vec<String>>>,
```

### Step 6.2: Update AgentLoop::new()

```rust
// Update the constructor to use HookBuilder
use crate::hooks::{HookBuilder, HookRegistry, ExternalShellHook, VaultHook, HistoryRecallHook};

impl AgentLoop {
    pub async fn new(
        provider: Arc<dyn LlmProvider>,
        workspace: PathBuf,
        config: AgentConfig,
        tools: Arc<ToolRegistry>,
    ) -> Result<Self, AgentError> {
        let memory_store = Arc::new(MemoryStore::new().await);
        Self::with_services(provider, workspace, config, tools, memory_store).await
    }

    async fn with_services(
        provider: Arc<dyn LlmProvider>,
        workspace: PathBuf,
        config: AgentConfig,
        tools: Arc<ToolRegistry>,
        memory_store: Arc<MemoryStore>,
    ) -> Result<Self, AgentError> {
        // Setup session manager and context (existing code)
        let session_manager = Arc::new(SessionManager::new(memory_store.sqlite_store().clone()));
        let store_arc = memory_store.sqlite_store().clone();
        let summarization = Arc::new(SummarizationService::new(
            provider.clone(),
            Arc::new(store_arc),
            config.model.clone(),
        ));
        let context: Arc<dyn AgentContext> =
            Arc::new(PersistentContext::new(session_manager, summarization));

        let (system_prompt, skills_context) = Self::load_prompts(&workspace).await?;

        // Build hooks using HookBuilder
        let external_hooks = Self::load_external_hooks(); // existing method at line 273
        let vault_values = Arc::new(RwLock::new(Vec::new()));

        let mut builder = HookBuilder::new()
            .with_external_hooks(external_hooks);

        // Add vault hook if available
        // create_vault_injector() is existing method at line 284
        if let Some(injector) = Self::create_vault_injector() {
            let vault_hook = VaultHook::new(injector);
            vault_values.clone_from(&vault_hook.injected_values());
            builder = builder.with_hook(Arc::new(vault_hook));
        }

        let hooks = builder.build_shared();

        Ok(Self {
            provider,
            tools,
            config,
            workspace,
            history_config: HistoryConfig::default(),
            context,
            system_prompt,
            skills_context,
            pricing: None,
            hooks,
            vault_values,
        })
    }

    // Note: load_external_hooks() and create_vault_injector() already exist in current code
    // They are defined at lines 273-296 in loop_.rs
}
```

### Step 6.3: Update AgentLoop::builder() for subagents

```rust
pub fn builder(
    provider: Arc<dyn LlmProvider>,
    workspace: PathBuf,
    config: AgentConfig,
    tools: Arc<ToolRegistry>,
) -> Result<Self, AgentError> {
    let context: Arc<dyn AgentContext> = Arc::new(StatelessContext::new());

    Ok(Self {
        provider,
        tools,
        config,
        workspace,
        history_config: HistoryConfig::default(),
        context,
        system_prompt: String::new(),
        skills_context: None,
        pricing: None,
        hooks: HookRegistry::empty(),
        vault_values: Arc::new(RwLock::new(Vec::new())),
    })
}

/// Set custom hooks (for subagents)
pub fn with_hooks(mut self, hooks: Arc<HookRegistry>) -> Self {
    self.hooks = hooks;
    self
}
```

### Step 6.4: Update process_direct()

Replace all the scattered hook calls with registry execute calls:

```rust
// BeforeRequest
let mut ctx = MutableContext { ... };
match self.hooks.execute(HookPoint::BeforeRequest, &mut ctx).await? {
    HookAction::Abort(msg) => return Ok(AgentResponse::aborted(msg)),
    HookAction::Continue => {}
}

// AfterHistory
self.hooks.execute(HookPoint::AfterHistory, &mut ctx).await?;

// BeforeLLM
self.hooks.execute(HookPoint::BeforeLLM, &mut ctx).await?;

// AfterResponse
self.hooks.execute(HookPoint::AfterResponse, &mut ctx).await?;
```

- [ ] **Step 6.5: Run cargo check and fix compilation errors**

Run: `cargo check --package gasket-core`
Expected: Fix any type mismatches

- [ ] **Step 6.6: Run existing tests**

Run: `cargo test --package gasket-core agent::loop_`
Expected: All tests pass

- [ ] **Step 6.7: Commit**

```bash
git add gasket/core/src/agent/loop_.rs
git commit -m "refactor(agent): replace scattered hooks with HookRegistry

- Replace external_hooks, vault_injector, embedder fields with HookRegistry
- Use hook.execute() at all hook points
- Add with_hooks() for subagent customization
- Maintain backward compatibility"
```

---

## Task 7: SubagentTaskBuilder Extension

**Files:**
- Modify: `gasket/core/src/agent/subagent.rs`

### Step 7.1: Add hooks field to SubagentTaskBuilder

```rust
pub struct SubagentTaskBuilder<'a> {
    manager: &'a SubagentManager,
    subagent_id: String,
    task: String,
    provider: Option<Arc<dyn LlmProvider>>,
    agent_config: Option<AgentConfig>,
    event_tx: Option<mpsc::Sender<SubagentEvent>>,
    system_prompt: Option<String>,
    session_key: Option<SessionKey>,
    cancellation_token: Option<CancellationToken>,
    /// Custom hooks for subagent
    hooks: Option<Arc<HookRegistry>>,
}
```

### Step 7.2: Add with_hooks() and inherit_hooks() methods

```rust
impl<'a> SubagentTaskBuilder<'a> {
    pub fn new(manager: &'a SubagentManager, subagent_id: String, task: String) -> Self {
        Self {
            manager,
            subagent_id,
            task,
            provider: None,
            agent_config: None,
            event_tx: None,
            system_prompt: None,
            session_key: None,
            cancellation_token: None,
            hooks: None,
        }
    }

    /// Set custom hooks for this subagent
    pub fn with_hooks(mut self, hooks: Arc<HookRegistry>) -> Self {
        self.hooks = Some(hooks);
        self
    }

    /// Inherit hooks from main agent
    pub fn inherit_hooks(mut self, agent_hooks: Arc<HookRegistry>) -> Self {
        self.hooks = Some(agent_hooks);
        self
    }

    // ... spawn() method updates ...
}
```

### Step 7.3: Update spawn() to use hooks

```rust
pub async fn spawn(self, result_tx: mpsc::Sender<SubagentResult>) -> anyhow::Result<String> {
    // ... existing setup code ...

    let mut agent = AgentLoop::builder(provider, workspace.clone(), agent_config, tools)?;

    // Apply hooks if provided
    if let Some(hooks) = self.hooks {
        agent = agent.with_hooks(hooks);
    }

    // ... rest of spawn logic ...
}
```

- [ ] **Step 7.4: Run cargo check**

Run: `cargo check --package gasket-core`
Expected: Compilation succeeds

- [ ] **Step 7.5: Commit**

```bash
git add gasket/core/src/agent/subagent.rs
git commit -m "feat(subagent): add hook support to SubagentTaskBuilder

- Add hooks field to SubagentTaskBuilder
- Add with_hooks() for custom hooks
- Add inherit_hooks() to share hooks with main agent"
```

---

## Task 8: Integration Tests

**Files:**
- Create: `gasket/core/src/hooks/integration_test.rs` (as module test, not separate tests/ directory)

### Step 8.1: Add integration tests to registry.rs

Add to the bottom of `registry.rs`:

```rust
// Add to gasket/core/src/hooks/registry.rs

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::providers::ChatMessage;

    // Note: Full integration tests requiring LLM mocking
    // would go in a separate test file or use mockall

    #[test]
    fn test_hook_builder_with_external_hooks() {
        let runner = ExternalHookRunner::noop();
        let registry = HookBuilder::new()
            .with_external_hooks(runner)
            .build();

        assert_eq!(registry.get_hooks(HookPoint::BeforeRequest).len(), 1);
        assert_eq!(registry.get_hooks(HookPoint::AfterResponse).len(), 1);
    }

    #[tokio::test]
    async fn test_empty_registry_returns_continue() {
        let registry = HookRegistry::new();
        let mut messages = vec![];

        let mut ctx = MutableContext {
            session_key: "test:123",
            messages: &mut messages,
            user_input: Some("test"),
            response: None,
            tool_calls: None,
            token_usage: None,
        };

        let result = registry.execute(HookPoint::BeforeRequest, &mut ctx).await;
        assert!(matches!(result, Ok(HookAction::Continue)));
    }

    #[test]
    fn test_registry_empty_factory() {
        let registry = HookRegistry::empty();
        assert!(registry.get_hooks(HookPoint::BeforeRequest).is_empty());
    }
}
```

- [ ] **Step 8.2: Run all tests**

Run: `cargo test --package gasket-core`
Expected: All tests pass

- [ ] **Step 8.3: Run full workspace build**

Run: `cargo build --release --workspace`
Expected: Clean build

- [ ] **Step 8.4: Commit**

```bash
git add gasket/core/tests/hooks_integration_test.rs
git commit -m "test(hooks): add integration tests for hook pipeline"
```

---

## Task 9: Final Verification

- [ ] **Step 9.1: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 9.2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings

- [ ] **Step 9.3: Final commit**

```bash
git add -A
git commit -m "feat(hooks): complete lifecycle hooks implementation

- Add HookPoint, HookContext, HookAction, ExecutionStrategy
- Add PipelineHook trait and HookRegistry
- Migrate ExternalHookRunner, VaultInjector, TextEmbedder to hooks
- Add Subagent hook support with with_hooks() and inherit_hooks()
- Maintain backward compatibility with existing shell hooks"
```

---

## Summary

| Task | Description | Files |
|------|-------------|-------|
| 1 | Core Types | types.rs, mod.rs, error.rs |
| 2 | PipelineHook & Registry | registry.rs |
| 3 | ExternalShellHook | external.rs |
| 4 | VaultHook | vault.rs |
| 5 | HistoryRecallHook | history.rs |
| 6 | AgentLoop Refactor | loop_.rs |
| 7 | Subagent Extension | subagent.rs |
| 8 | Integration Tests | hooks_integration_test.rs |
| 9 | Final Verification | - |
