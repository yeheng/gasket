# Gasket Three-Layer Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor gasket-core into three independent crates (gasket-bus, gasket-history, gasket-engine) with gasket-core as a Facade layer for backward compatibility.

**Architecture:** Three-layer architecture with clear dependency boundaries:
- `gasket-types`: Shared types (SessionEvent, SessionKey, ChannelType, etc.)
- `gasket-bus`: Message bus (actors, queue) - depends only on gasket-types + tokio
- `gasket-history`: History retrieval and processing - depends on gasket-storage + gasket-semantic
- `gasket-engine`: Core engine (agent, subagent, tools, executor) - depends on bus + history
- `gasket-core`: Facade that re-exports all for backward compatibility

**Tech Stack:** Rust 2021, tokio, async-trait, thiserror

---

## File Structure

### Files to Create:
- `gasket/bus/Cargo.toml` - New crate definition
- `gasket/bus/src/lib.rs` - gasket-bus public API
- `gasket/bus/src/actors.rs` - Router/Session/Outbound actors with trait-based handler
- `gasket/bus/src/queue.rs` - Message queue and lifecycle management
- `gasket/history/Cargo.toml` - New crate definition
- `gasket/history/src/lib.rs` - gasket-history public API
- `gasket/history/src/processor.rs` - Token counting and history truncation
- `gasket/history/src/query.rs` - History query builder
- `gasket/history/src/search/mod.rs` - Semantic search wrapper
- `gasket/engine/Cargo.toml` - New crate definition
- `gasket/engine/src/lib.rs` - gasket-engine public API
- `gasket/engine/src/agent/` - Agent loop, executor, context, subagent
- `gasket/engine/src/tools/` - Tool system
- `gasket/engine/src/bus_adapter.rs` - MessageHandler trait implementation
- `gasket/engine/src/tool_context.rs` - Decoupled ToolContext with SubagentSpawner

### Files to Modify:
- `gasket/types/src/lib.rs` - Add SessionKey, InboundMessage, OutboundMessage, ChannelType
- `gasket/core/Cargo.toml` - Add dependencies on new crates, re-export API
- `gasket/core/src/lib.rs` - Facade pattern with pub use statements
- `gasket/cli/Cargo.toml` - No changes expected (backward compatibility)

### Files to Move:
- `gasket/core/src/bus/events.rs` → `gasket/types/src/events.rs`
- `gasket/core/src/bus/actors.rs` → `gasket/bus/src/actors.rs`
- `gasket/core/src/bus/queue.rs` → `gasket/bus/src/queue.rs`
- `gasket/core/src/agent/history_processor.rs` → `gasket/history/src/processor.rs`
- `gasket/core/src/agent/history_query.rs` → `gasket/history/src/query.rs`
- `gasket/core/src/search/` → `gasket/history/src/search/`
- `gasket/core/src/agent/` → `gasket/engine/src/agent/` (except history_*.rs)
- `gasket/core/src/tools/` → `gasket/engine/src/tools/`

---

## PR 1: gasket-types Preparation

### Task 1.1: Move bus/events types to gasket-types

**Files:**
- Create: `gasket/types/src/events.rs`
- Modify: `gasket/types/src/lib.rs`
- Modify: `gasket/core/src/bus/events.rs` (update imports)
- Modify: `gasket/core/src/bus/mod.rs` (update imports)
- Modify: `gasket/core/src/bus/actors.rs` (update imports)
- Test: `cargo build --workspace`

- [ ] **Step 1: Copy events.rs to gasket-types**

Copy `gasket/core/src/bus/events.rs` content to `gasket/types/src/events.rs`

- [ ] **Step 2: Update gasket/types/src/lib.rs**

```rust
// Add to gasket/types/src/lib.rs
pub mod events;
pub use events::*;
```

- [ ] **Step 3: Update gasket/core/src/bus/events.rs**

```rust
// Re-export from gasket-types for backward compatibility
pub use gasket_types::events::*;
```

- [ ] **Step 4: Run build to verify**

```bash
cd gasket
cargo build --workspace
```

Expected: PASS (may have warnings about unused imports)

- [ ] **Step 5: Commit**

```bash
git add gasket/types/src/events.rs gasket/types/src/lib.rs gasket/core/src/bus/events.rs
git commit -m "refactor(types): move bus events types to gasket-types for shared access"
```

---

## PR 2: Create gasket-bus

### Task 2.1: Create gasket-bus crate structure

**Files:**
- Create: `gasket/bus/Cargo.toml`
- Create: `gasket/bus/src/lib.rs`
- Create: `gasket/bus/src/actors.rs`
- Create: `gasket/bus/src/queue.rs`
- Modify: `gasket/Cargo.toml` (add workspace member)

- [ ] **Step 1: Create gasket/bus/Cargo.toml**

```toml
[package]
name = "gasket-bus"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
description = "Message bus for gasket AI assistant"

[dependencies]
gasket-types = { path = "../types" }
tokio.workspace = true
tokio-util.workspace = true
async-trait.workspace = true
tracing.workspace = true
thiserror.workspace = true

[features]
default = []
```

- [ ] **Step 2: Add to workspace members in gasket/Cargo.toml**

```toml
[workspace]
resolver = "2"
members = [
    "types",
    "vault",
    "storage",
    "semantic",
    "core",
    "cli",
    "providers",
    "channels",
    "sandbox",
    "tantivy",
    "bus",  # Add this line
]
```

- [ ] **Step 3: Create gasket/bus/src/lib.rs**

```rust
//! Message bus for inter-component communication
//!
//! Three actors form a clean pipeline with zero locks:
//! Router → Session → Outbound

pub mod actors;
pub mod queue;

pub use actors::{run_outbound_actor, run_router_actor, run_session_actor, MessageHandler};
pub use gasket_types::events::*;
pub use queue::MessageBus;
```

- [ ] **Step 4: Commit**

```bash
git add gasket/bus/ gasket/Cargo.toml
git commit -m "feat(bus): create gasket-bus crate structure"
```

### Task 2.2: Move actors.rs to gasket-bus with trait-based handler

**Files:**
- Modify: `gasket/bus/src/actors.rs`
- Modify: `gasket/core/src/bus/actors.rs` (re-export)

- [ ] **Step 1: Copy and refactor actors.rs**

Copy `gasket/core/src/bus/actors.rs` to `gasket/bus/src/actors.rs` and refactor:

```rust
// Add MessageHandler trait
#[async_trait]
pub trait MessageHandler: Send + Sync {
    async fn handle_message(
        &self,
        session_key: &gasket_types::SessionKey,
        message: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send>>;
}

// Update run_session_actor to use trait
pub async fn run_session_actor<H: MessageHandler>(
    session_key: SessionKey,
    mut rx: mpsc::Receiver<InboundMessage>,
    outbound_tx: mpsc::Sender<OutboundMessage>,
    handler: Arc<H>,
    subagent_manager: Option<Arc<SubagentManager>>, // Keep for now, will refactor in PR4
    idle_timeout: Duration,
) { ... }
```

- [ ] **Step 2: Update gasket/core/src/bus/actors.rs**

```rust
// Re-export for backward compatibility
pub use gasket_bus::actors::*;
```

- [ ] **Step 3: Run build**

```bash
cargo build --workspace
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add gasket/bus/src/actors.rs gasket/core/src/bus/actors.rs
git commit -m "refactor(bus): move actors to gasket-bus with MessageHandler trait"
```

### Task 2.3: Move queue.rs to gasket-bus

**Files:**
- Copy: `gasket/core/src/bus/queue.rs` → `gasket/bus/src/queue.rs`
- Modify: `gasket/core/src/bus/queue.rs` (re-export)

- [ ] **Step 1: Copy queue.rs**

```bash
cp gasket/core/src/bus/queue.rs gasket/bus/src/queue.rs
```

- [ ] **Step 2: Update gasket/core/src/bus/queue.rs**

```rust
pub use gasket_bus::queue::*;
```

- [ ] **Step 3: Update gasket/bus/src/lib.rs exports**

Ensure `pub use queue::MessageBus;` is present

- [ ] **Step 4: Run build**

```bash
cargo build --workspace
```

- [ ] **Step 5: Commit**

```bash
git add gasket/bus/src/queue.rs gasket/core/src/bus/queue.rs
git commit -m "refactor(bus): move queue to gasket-bus"
```

### Task 2.4: Update gasket-core to use gasket-bus

**Files:**
- Modify: `gasket/core/Cargo.toml`
- Modify: `gasket/core/src/bus/mod.rs`

- [ ] **Step 1: Add gasket-bus dependency**

```toml
# gasket/core/Cargo.toml
[dependencies]
gasket-bus = { path = "../bus" }
# ... existing dependencies
```

- [ ] **Step 2: Update gasket/core/src/bus/mod.rs**

```rust
pub use gasket_bus::*;
```

- [ ] **Step 3: Run build**

```bash
cargo build --workspace
```

- [ ] **Step 4: Commit**

```bash
git add gasket/core/Cargo.toml gasket/core/src/bus/mod.rs
git commit -m "refactor(core): use gasket-bus as dependency"
```

---

## PR 3: Create gasket-history

### Task 3.1: Create gasket-history crate structure

**Files:**
- Create: `gasket/history/Cargo.toml`
- Create: `gasket/history/src/lib.rs`
- Create: `gasket/history/src/processor.rs`
- Create: `gasket/history/src/query.rs`
- Create: `gasket/history/src/search/mod.rs`
- Modify: `gasket/Cargo.toml` (add workspace member)

- [ ] **Step 1: Create gasket/history/Cargo.toml**

```toml
[package]
name = "gasket-history"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
description = "History retrieval and processing for gasket"

[dependencies]
gasket-types = { path = "../types" }
gasket-storage = { path = "../storage" }
gasket-semantic = { path = "../semantic" }
tokio.workspace = true
chrono.workspace = true
thiserror.workspace = true
tracing.workspace = true
tiktoken-rs = "0.9"

[features]
default = []
```

- [ ] **Step 2: Add to workspace**

```toml
# gasket/Cargo.toml
members = [
    "types", "vault", "storage", "semantic", "core", "cli",
    "providers", "channels", "sandbox", "tantivy", "bus",
    "history",  # Add this line
]
```

- [ ] **Step 3: Create gasket/history/src/lib.rs**

```rust
//! History retrieval and processing system

pub mod processor;
pub mod query;
pub mod search;

pub use processor::{count_tokens, process_history, HistoryConfig, ProcessedHistory};
pub use query::{
    HistoryQuery, HistoryQueryBuilder, HistoryResult, HistoryRetriever,
    QueryOrder, ResultMeta, SemanticQuery, TimeRange,
};
pub use search::*;
```

- [ ] **Step 4: Commit**

```bash
git add gasket/history/ gasket/Cargo.toml
git commit -m "feat(history): create gasket-history crate structure"
```

### Task 3.2: Move history_processor.rs

**Files:**
- Copy: `gasket/core/src/agent/history_processor.rs` → `gasket/history/src/processor.rs`
- Modify: `gasket/core/src/agent/history_processor.rs` (re-export)

- [ ] **Step 1: Copy file**

```bash
cp gasket/core/src/agent/history_processor.rs gasket/history/src/processor.rs
```

- [ ] **Step 2: Update imports in gasket/history/src/processor.rs**

```rust
// Update to use gasket-types
use gasket_types::SessionEvent;
```

- [ ] **Step 3: Add re-export in gasket/core/src/agent/history_processor.rs**

```rust
pub use gasket_history::processor::*;
```

- [ ] **Step 4: Run build**

```bash
cargo build --workspace
```

- [ ] **Step 5: Commit**

```bash
git add gasket/history/src/processor.rs gasket/core/src/agent/history_processor.rs
git commit -m "refactor(history): move history_processor to gasket-history"
```

### Task 3.3: Move history_query.rs

**Files:**
- Copy: `gasket/core/src/agent/history_query.rs` → `gasket/history/src/query.rs`
- Modify: `gasket/core/src/agent/history_query.rs` (re-export)

- [ ] **Step 1: Copy file**

```bash
cp gasket/core/src/agent/history_query.rs gasket/history/src/query.rs
```

- [ ] **Step 2: Update imports**

```rust
use gasket_types::{SessionEvent, EventTypeCategory};
```

- [ ] **Step 3: Add re-export**

```rust
pub use gasket_history::query::*;
```

- [ ] **Step 4: Commit**

```bash
git add gasket/history/src/query.rs gasket/core/src/agent/history_query.rs
git commit -m "refactor(history): move history_query to gasket-history"
```

### Task 3.4: Move search module

**Files:**
- Copy: `gasket/core/src/search/` → `gasket/history/src/search/`
- Modify: `gasket/core/src/search/mod.rs` (re-export)

- [ ] **Step 1: Copy directory**

```bash
cp -r gasket/core/src/search gasket/history/src/search
```

- [ ] **Step 2: Update imports in gasket/history/src/search/*.rs**

```rust
use gasket_types::...;
```

- [ ] **Step 3: Add re-export**

```rust
// gasket/core/src/search/mod.rs
pub use gasket_history::search::*;
```

- [ ] **Step 4: Commit**

```bash
git add gasket/history/src/search/ gasket/core/src/search/mod.rs
git commit -m "refactor(history): move search module to gasket-history"
```

### Task 3.5: Update gasket-core dependencies

**Files:**
- Modify: `gasket/core/Cargo.toml`
- Modify: `gasket/core/src/agent/mod.rs`

- [ ] **Step 1: Add gasket-history dependency**

```toml
# gasket/core/Cargo.toml
gasket-history = { path = "../history" }
```

- [ ] **Step 2: Update agent/mod.rs exports**

```rust
pub use gasket_history::*;
```

- [ ] **Step 3: Run build**

```bash
cargo build --workspace
```

- [ ] **Step 4: Commit**

```bash
git add gasket/core/Cargo.toml gasket/core/src/agent/mod.rs
git commit -m "refactor(core): use gasket-history as dependency"
```

---

## PR 4: Create gasket-engine + Facade

### Task 4.1: Create gasket-engine crate

**Files:**
- Create: `gasket/engine/Cargo.toml`
- Create: `gasket/engine/src/lib.rs`
- Create: `gasket/engine/src/agent/` (move from core)
- Create: `gasket/engine/src/tools/` (move from core)
- Modify: `gasket/Cargo.toml`

- [ ] **Step 1: Create gasket/engine/Cargo.toml**

```toml
[package]
name = "gasket-engine"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
description = "Core execution engine for gasket"

[dependencies]
gasket-types = { path = "../types" }
gasket-bus = { path = "../bus" }
gasket-history = { path = "../history" }
gasket-storage = { path = "../storage" }
gasket-semantic = { path = "../semantic" }
gasket-providers = { path = "../providers" }
gasket-sandbox = { path = "../sandbox" }
tokio.workspace = true
tokio-util.workspace = true
async-trait.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
anyhow.workspace = true
tracing.workspace = true
dashmap = "6"
tiktoken-rs = "0.9"
parking_lot = "0.12"
regex = "1"
which = "8.0"
html2text = "0.16"
bytes = "1"
cron.workspace = true
uuid.workspace = true
url.workspace = true
urlencoding = "2.1"
serde_yaml = "0.9.33"
json5 = "1.3"
base64 = "0.22"
futures = "0.3"
tokio-stream = "0.1.18"

[features]
default = []
smart-model-selection = []
provider-gemini = ["gasket-providers/provider-gemini"]
provider-copilot = ["gasket-providers/provider-copilot"]
all-providers = ["gasket-providers/all-providers"]
```

- [ ] **Step 2: Add to workspace**

```toml
# gasket/Cargo.toml
members = [
    ..., "bus", "history",
    "engine",  # Add this line
]
```

- [ ] **Step 3: Create gasket/engine/src/lib.rs**

```rust
//! Core execution engine for gasket AI assistant

pub mod agent;
pub mod tools;
pub mod bus_adapter;

pub use agent::*;
pub use tools::*;
pub use bus_adapter::*;
```

- [ ] **Step 4: Commit**

```bash
git add gasket/engine/ gasket/Cargo.toml
git commit -m "feat(engine): create gasket-engine crate structure"
```

### Task 4.2: Move agent module (except history_*.rs)

**Files:**
- Copy: `gasket/core/src/agent/*` → `gasket/engine/src/agent/*` (except history_*.rs)
- Modify: imports in moved files

- [ ] **Step 1: Copy agent files**

```bash
# Copy all except history_processor.rs and history_query.rs
rsync -av gasket/core/src/agent/ gasket/engine/src/agent/ \
  --exclude 'history_processor.rs' \
  --exclude 'history_query.rs'
```

- [ ] **Step 2: Update imports in gasket/engine/src/agent/*.rs**

```rust
// Update to use gasket-history
use gasket_history::{HistoryConfig, HistoryQuery, ...};
```

- [ ] **Step 3: Add re-exports in gasket/core/src/agent/mod.rs**

```rust
pub use gasket_engine::agent::*;
```

- [ ] **Step 4: Commit**

```bash
git add gasket/engine/src/agent/ gasket/core/src/agent/mod.rs
git commit -m "refactor(engine): move agent module to gasket-engine"
```

### Task 4.3: Move tools module

**Files:**
- Copy: `gasket/core/src/tools/` → `gasket/engine/src/tools/`

- [ ] **Step 1: Copy tools directory**

```bash
cp -r gasket/core/src/tools gasket/engine/src/tools
```

- [ ] **Step 2: Update imports**

```rust
use gasket_engine::agent::...;
use gasket_history::...;
```

- [ ] **Step 3: Add re-export**

```rust
// gasket/core/src/tools/mod.rs
pub use gasket_engine::tools::*;
```

- [ ] **Step 4: Commit**

```bash
git add gasket/engine/src/tools/ gasket/core/src/tools/mod.rs
git commit -m "refactor(engine): move tools module to gasket-engine"
```

### Task 4.4: Implement MessageHandler trait

**Files:**
- Create: `gasket/engine/src/bus_adapter.rs`

- [ ] **Step 1: Create bus_adapter.rs**

```rust
//! Adapter for integrating with gasket-bus

use std::sync::Arc;
use async_trait::async_trait;
use gasket_bus::MessageHandler;
use gasket_types::{SessionKey, InboundMessage};
use crate::agent::AgentLoop;

pub struct EngineHandler {
    agent_loop: Arc<AgentLoop>,
}

impl EngineHandler {
    pub fn new(agent_loop: Arc<AgentLoop>) -> Self {
        Self { agent_loop }
    }
}

#[async_trait]
impl MessageHandler for EngineHandler {
    async fn handle_message(
        &self,
        session_key: &SessionKey,
        message: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send>> {
        // Delegate to AgentLoop
        let response = self.agent_loop
            .process_message(session_key, message)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
        Ok(response.content)
    }
}
```

- [ ] **Step 2: Export in gasket/engine/src/lib.rs**

```rust
pub use bus_adapter::*;
```

- [ ] **Step 3: Commit**

```bash
git add gasket/engine/src/bus_adapter.rs
git commit -m "feat(engine): implement MessageHandler trait for bus integration"
```

### Task 4.5: Refactor ToolContext with SubagentSpawner trait

**Files:**
- Create: `gasket/engine/src/tool_context.rs`
- Modify: `gasket/engine/src/tools/base.rs`
- Modify: `gasket/engine/src/tools/spawn.rs`

- [ ] **Step 1: Define SubagentSpawner trait**

```rust
// gasket/engine/src/tool_context.rs
use std::sync::Arc;
use crate::agent::SubagentResult;

#[async_trait]
pub trait SubagentSpawner: Send + Sync {
    async fn spawn(
        &self,
        task: String,
        model_id: Option<String>,
    ) -> Result<SubagentResult, Box<dyn std::error::Error + Send>>;
}
```

- [ ] **Step 2: Update ToolContext**

```rust
pub struct ToolContext {
    pub session_key: Option<SessionKey>,
    pub outbound_tx: Option<mpsc::Sender<OutboundMessage>>,
    pub spawner: Option<Box<dyn SubagentSpawner>>,
}
```

- [ ] **Step 3: Implement for SubagentManager**

```rust
impl SubagentSpawner for SubagentManager {
    async fn spawn(...) -> Result<SubagentResult, ...> {
        // Existing spawn logic
    }
}
```

- [ ] **Step 4: Update spawn.rs to use trait**

```rust
// Use ctx.spawner instead of direct SubagentManager
if let Some(spawner) = &ctx.spawner {
    let result = spawner.spawn(task, model_id).await?;
    ...
}
```

- [ ] **Step 5: Commit**

```bash
git add gasket/engine/src/tool_context.rs gasket/engine/src/tools/base.rs gasket/engine/src/tools/spawn.rs
git commit -m "refactor(engine): decouple tools from SubagentManager with trait"
```

### Task 4.6: Convert gasket-core to Facade

**Files:**
- Modify: `gasket/core/Cargo.toml`
- Modify: `gasket/core/src/lib.rs`

- [ ] **Step 1: Update gasket/core/Cargo.toml**

```toml
[dependencies]
gasket-types = { path = "../types" }
gasket-bus = { path = "../bus" }
gasket-history = { path = "../history" }
gasket-engine = { path = "../engine" }
gasket-providers = { path = "../providers" }
gasket-channels = { path = "../channels" }
# ... keep existing dependencies
```

- [ ] **Step 2: Update gasket/core/src/lib.rs**

```rust
//! gasket-core: Facade for gasket AI assistant framework

pub use gasket_types::*;
pub use gasket_bus::*;
pub use gasket_history::*;
pub use gasket_engine::*;

// Re-export providers and channels
pub use gasket_providers::*;
pub use gasket_channels::*;

// Keep existing re-exports for backward compatibility
#[cfg(feature = "webhook")]
pub use gasket_channels::webhook;

pub use gasket_engine::config::Config;
pub use gasket_engine::error::{AgentError, PipelineError, ProviderError};
pub use gasket_channels::error::ChannelError;
// ... etc
```

- [ ] **Step 3: Run full build**

```bash
cargo build --workspace
```

- [ ] **Step 4: Commit**

```bash
git add gasket/core/Cargo.toml gasket/core/src/lib.rs
git commit -m "refactor(core): convert to facade pattern re-exporting all crates"
```

---

## PR 5: Cleanup and Documentation

### Task 5.1: Remove duplicate files

**Files:**
- Remove: `gasket/core/src/bus/events.rs` (now in gasket-types)
- Remove: `gasket/core/src/bus/actors.rs` (now in gasket-bus)
- Remove: `gasket/core/src/bus/queue.rs` (now in gasket-bus)
- Remove: `gasket/core/src/agent/history_processor.rs` (now in gasket-history)
- Remove: `gasket/core/src/agent/history_query.rs` (now in gasket-history)
- Remove: `gasket/core/src/search/` (now in gasket-history)

- [ ] **Step 1: Remove duplicate files**

```bash
rm gasket/core/src/bus/events.rs
rm gasket/core/src/bus/actors.rs
rm gasket/core/src/bus/queue.rs
rm gasket/core/src/agent/history_processor.rs
rm gasket/core/src/agent/history_query.rs
rm -rf gasket/core/src/search/
```

- [ ] **Step 2: Update mod.rs files that reference removed modules**

```rust
// gasket/core/src/bus/mod.rs - now just re-exports
pub use gasket_bus::*;

// gasket/core/src/agent/mod.rs
pub use gasket_engine::agent::*;
pub use gasket_history::{
    count_tokens, process_history, HistoryConfig,
    HistoryQuery, HistoryQueryBuilder, ...
};

// Remove search module reference
```

- [ ] **Step 3: Run build**

```bash
cargo build --workspace
```

- [ ] **Step 4: Commit**

```bash
git add gasket/core/src/bus/ gasket/core/src/agent/ gasket/core/src/search/
git commit -m "cleanup(core): remove duplicate files after migration"
```

### Task 5.2: Update documentation

**Files:**
- Modify: `gasket/README.md` (if exists)
- Modify: `gasket/core/README.md` (if exists)
- Create: `gasket/bus/README.md`
- Create: `gasket/history/README.md`
- Create: `gasket/engine/README.md`
- Modify: `docs/architecture.md`

- [ ] **Step 1: Create crate README files**

Each crate gets a README.md explaining its purpose and dependencies.

- [ ] **Step 2: Update docs/architecture.md**

Add the three-layer architecture diagram and explain the dependency structure.

- [ ] **Step 3: Commit**

```bash
git add gasket/*/README.md docs/architecture.md
git commit -m "docs: add three-layer architecture documentation"
```

### Task 5.3: Run full test suite

**Files:**
- All test files

- [ ] **Step 1: Run all tests**

```bash
cargo test --workspace
```

Expected: All tests pass

- [ ] **Step 2: Fix any failures**

Address any test failures caused by the refactoring.

- [ ] **Step 3: Commit**

```bash
git add <any fixed files>
git commit -m "test: fix tests after three-layer refactoring"
```

---

## Verification Checklist

After all PRs are merged:

- [ ] `cargo build --workspace` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace` has no new warnings
- [ ] CLI can still run: `cargo run --package gasket-cli -- agent -m "test"`
- [ ] `cargo tree` shows clean dependency graph (no cycles)
- [ ] gasket-bus does not depend on gasket-engine
- [ ] gasket-history does not depend on gasket-engine or gasket-bus

---

## Rollback Plan

If any PR causes issues:

1. Revert the specific PR
2. Continue with remaining PRs that don't depend on it
3. Fix issues in isolation before proceeding

Each PR is designed to be independently revertible.
