# Gasket Three-Layer Architecture Migration Design

> **Status:** Approved
> **Date:** 2026-03-27
> **Author:** Claude + User

## Overview

This document describes the migration strategy to transform `gasket-core` into a pure Facade layer while consolidating all business logic into `gasket-engine`.

## Current State

### Architecture Issues

1. **Circular Dependency Risk**: `gasket-engine` depends on `gasket-core`, while `gasket-core` needs to re-export `gasket-engine`
2. **Incomplete Migration**: Code has been copied to `gasket-engine` but still uses `use gasket_core::*` imports
3. **gasket-core Retains Local Modules**: `agent/`, `tools/`, `skills/`, `hooks/`, `cron/` etc. still exist in core

### Module Location Analysis

| Module | gasket-core | gasket-engine | gasket-history | Status |
|--------|-------------|---------------|----------------|--------|
| `agent/` | ✅ Local | ✅ Copied (uses core) | - | Needs migration |
| `tools/` | ✅ Local | ✅ Copied (uses core) | - | Needs migration |
| `hooks/` | ✅ Local | 📁 Empty dir | - | Needs migration |
| `skills/` | ✅ Local | 📁 Empty dir | - | Needs migration |
| `cron/` | ✅ Local | 📁 Empty dir | - | Needs migration |
| `error.rs` | ✅ Local | ❌ Missing | - | Needs migration |
| `token_tracker.rs` | ✅ Local | ❌ Missing | - | Needs migration |
| `memory/` | ✅ Re-export | ❌ Missing | - | Needs migration |
| `search/` | ✅ Re-export | ❌ Missing | ✅ Implemented | **Already correct** |
| `config/` | ✅ Local | ❌ Missing | - | Needs migration |
| `vault/` | ✅ Local | ❌ Missing | - | Needs migration |

**Key Findings:**
- `gasket-core/src/search/mod.rs` is already a re-export from `gasket-history` and `gasket-semantic`
- `gasket-engine/src/` has empty placeholder directories: `cron/`, `hooks/`, `skills/`
- `config/` is used by `tools/shell.rs` via `gasket_core::config::ExecToolConfig`

### Current Dependencies in gasket-engine

16 places use `use gasket_core::*`:

| File | Import | Module Location |
|------|--------|-----------------|
| `agent/context.rs` | `gasket_core::error::AgentError` | core/error.rs |
| `agent/pipeline.rs` | `gasket_core::error::AgentError` | core/error.rs |
| `agent/skill_loader.rs` | `gasket_core::skills::{SkillsLoader, SkillsRegistry}` | core/skills/ |
| `agent/summarization.rs` | `gasket_core::memory::SqliteStore` | core/memory/ (re-export) |
| `agent/summarization.rs` | `gasket_core::search::*` | Already in history/semantic |
| `agent/subagent.rs` | `gasket_core::hooks::HookRegistry` | core/hooks/ |
| `agent/loop_.rs` | `gasket_core::error::AgentError` | core/error.rs |
| `agent/loop_.rs` | `gasket_core::hooks::*` | core/hooks/ |
| `agent/loop_.rs` | `gasket_core::vault::*` | core/vault/ |
| `agent/executor_core.rs` | `gasket_core::error::AgentError` | core/error.rs |
| `agent/executor_core.rs` | `gasket_core::token_tracker::*` | core/token_tracker.rs |
| `agent/memory.rs` | `gasket_core::memory::SqliteStore` | core/memory/ (re-export) |
| `tools/cron.rs` | `gasket_core::cron::CronService` | core/cron/ |
| `tools/registry.rs` | `gasket_core::search::*` | Already in history/semantic |
| `tools/history_search.rs` | `gasket_core::memory::SqliteStore` | core/memory/ (re-export) |
| `tools/shell.rs` | `gasket_core::config::ExecToolConfig` | core/config/ |

## Target Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      gasket-cli                              │
│                   (Application Layer)                        │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      gasket-core                             │
│                    (Facade Layer)                            │
│  Re-exports: types, bus, history, engine, providers, etc.   │
│  ~50 lines, NO pub mod declarations                          │
└─────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│  gasket-bus   │    │ gasket-history│    │ gasket-engine │
│  (Transport)  │    │  (Storage)    │    │   (Logic)     │
└───────────────┘    └───────────────┘    └───────────────┘
        │                     │                     │
        └─────────────────────┼─────────────────────┘
                              ▼
                    ┌───────────────┐
                    │ gasket-types  │
                    │   (Shared)    │
                    └───────────────┘
```

## Pre-Migration Baseline

Before starting PR1, capture current state:

```bash
# Record current dependency count
cargo tree -p gasket-engine 2>/dev/null | head -50 > /tmp/pre-migration-deps.txt

# Record current import count
grep -r "use gasket_core::" gasket/engine/src/ | wc -l > /tmp/pre-migration-imports.txt
# Expected: 16

# Verify current build works
cargo build --workspace && cargo test --workspace
```

## Migration Strategy: 3 PRs

### PR1: Core Dependency Layer

**Goal**: Migrate basic modules to engine, cut core dependencies for error/token_tracker/memory/config.

**Modules to Migrate**:

| Module | From | To | Used By |
|--------|------|----|---------|
| `error.rs` | `gasket-core/src/error.rs` | `gasket-engine/src/error.rs` | agent/*, tools/* |
| `token_tracker.rs` | `gasket-core/src/token_tracker.rs` | `gasket-engine/src/token_tracker.rs` | executor_core.rs |
| `config/` (partial) | `gasket-core/src/config/exec.rs` | `gasket-engine/src/config/exec.rs` | tools/shell.rs |

**Import Changes** (16 → ~12 remaining):

```rust
// Before
use gasket_core::error::AgentError;
use gasket_core::token_tracker::{ModelPricing, TokenUsage};
use gasket_core::config::ExecToolConfig;

// After
use crate::error::AgentError;
use crate::token_tracker::{ModelPricing, TokenUsage};
use crate::config::ExecToolConfig;
```

**Search/Memory imports** - Change to use gasket-semantic/gasket-storage directly:
```rust
// Before
use gasket_core::memory::SqliteStore;
use gasket_core::search::{top_k_similar, TextEmbedder};

// After
use gasket_storage::SqliteStore;
use gasket_semantic::{top_k_similar, TextEmbedder};
```

**gasket-core Changes**:
- Keep modules but change to re-exports for backward compatibility

```rust
// gasket-core/src/error.rs
pub use gasket_engine::error::*;

// gasket-core/src/token_tracker.rs
pub use gasket_engine::token_tracker::*;
```

**gasket-engine/src/lib.rs after PR1**:
```rust
//! Core execution engine for gasket AI assistant

pub mod agent;
pub mod tools;
pub mod bus_adapter;
pub mod error;
pub mod token_tracker;
pub mod config;

pub use agent::*;
pub use tools::*;
pub use bus_adapter::*;
pub use error::*;
pub use token_tracker::*;
```

**Verification**:
```bash
cargo build --workspace
cargo test --workspace
# Verify dependency reduction
cargo tree -p gasket-engine --invert | grep gasket-core
```

---

### PR2: Extended Functionality Layer

**Goal**: Migrate remaining modules, completely cut `gasket-engine` → `gasket-core` dependency.

**Modules to Migrate**:

| Module | From | To | Notes |
|--------|------|----|-------|
| `hooks/` | `gasket-core/src/hooks/` | `gasket-engine/src/hooks/` | Remove empty dir first |
| `skills/` | `gasket-core/src/skills/` | `gasket-engine/src/skills/` | Remove empty dir first |
| `cron/` | `gasket-core/src/cron/` | `gasket-engine/src/cron/` | Remove empty dir first |
| `vault/` (partial) | `gasket-core/src/vault/` | `gasket-engine/src/vault/` | Only types used by loop_.rs |

**Import Changes** (~12 → 0):

```rust
// Before
use gasket_core::hooks::HookRegistry;
use gasket_core::skills::{SkillsLoader, SkillsRegistry};
use gasket_core::cron::CronService;
use gasket_core::vault::{redact_secrets, VaultInjector, VaultStore};

// After
use crate::hooks::HookRegistry;
use crate::skills::{SkillsLoader, SkillsRegistry};
use crate::cron::CronService;
use crate::vault::{redact_secrets, VaultInjector, VaultStore};
```

**Critical Change**: Remove from `gasket-engine/Cargo.toml`:
```toml
# Remove this line
gasket-core = { path = "../core" }
```

**gasket-engine/src/lib.rs after PR2**:
```rust
//! Core execution engine for gasket AI assistant

pub mod agent;
pub mod tools;
pub mod bus_adapter;
pub mod error;
pub mod token_tracker;
pub mod config;
pub mod hooks;
pub mod skills;
pub mod cron;
pub mod vault;

pub use agent::*;
pub use tools::*;
pub use bus_adapter::*;
pub use error::*;
pub use token_tracker::*;
pub use hooks::*;
pub use skills::*;
pub use cron::*;
pub use vault::*;
```

**Verification**:
```bash
cargo build --workspace
cargo tree -p gasket-engine 2>/dev/null | grep -c gasket-core
# Expected: 0
cargo test --workspace
```

---

### PR3: Facade Transformation

**Goal**: Delete local implementations in `gasket-core`, convert to pure Facade.

**Files/Directories to Delete**:
```
gasket/core/src/agent/       # Moved to engine
gasket/core/src/tools/       # Moved to engine
gasket/core/src/hooks/       # Moved to engine
gasket/core/src/skills/      # Moved to engine
gasket/core/src/cron/        # Moved to engine
gasket/core/src/vault/       # Moved to engine
gasket/core/src/error.rs     # Moved to engine
gasket/core/src/token_tracker.rs  # Moved to engine
gasket/core/src/config/      # Moved to engine
gasket/core/src/memory/      # Already re-export
gasket/core/src/search/      # Already re-export
gasket/core/src/channels/    # Already re-export
gasket/core/src/providers/   # Already re-export
gasket/core/src/heartbeat/   # Needs evaluation (keep or move?)
```

**New `gasket-core/src/lib.rs`** (~50 lines, NO `pub mod` declarations):

```rust
//! gasket-core: Facade for gasket AI assistant framework
//!
//! This crate re-exports all gasket crates for backward compatibility.
//! It provides a single entry point for all gasket functionality.
//!
//! NOTE: This crate contains NO local implementations.
//! All functionality is provided by the underlying crates.

// Core types (canonical source)
pub use gasket_types::*;

// Message bus
pub use gasket_bus::*;

// History processing
pub use gasket_history::*;

// Core engine (agent, tools, hooks, skills, etc.)
pub use gasket_engine::*;

// LLM Providers
pub use gasket_providers::*;

// Communication Channels
pub use gasket_channels::*;

// Supporting crates
pub use gasket_vault::*;
pub use gasket_storage as storage;
pub use gasket_semantic as semantic;
```

**gasket-core/Cargo.toml Changes**:
```toml
[dependencies]
# Only internal crate dependencies needed for re-exports
gasket-types = { path = "../types" }
gasket-bus = { path = "../bus" }
gasket-history = { path = "../history" }
gasket-engine = { path = "../engine" }
gasket-providers = { path = "../providers" }
gasket-channels = { path = "../channels" }
gasket-vault = { path = "../vault" }
gasket-storage = { path = "../storage" }
gasket-semantic = { path = "../semantic", features = ["local-embedding"] }

# Feature flags remain for pass-through
[features]
default = []
telegram = ["gasket-channels/telegram"]
discord = ["gasket-channels/discord"]
# ... etc
```

**Verification**:
```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace

# Verify no local modules
grep -c "pub mod" gasket/core/src/lib.rs
# Expected: 0

# Verify CLI works
cargo run --package gasket-cli -- agent -m "test"

# Verify clean dependency graph
cargo tree -p gasket-core
```

---

## Dependency Graph After Migration

```
gasket-types     (no internal deps)
    ↑
gasket-storage   (depends on types)
gasket-semantic  (depends on types)
gasket-vault     (depends on types)
    ↑
gasket-bus       (depends on types)
gasket-history   (depends on types, storage, semantic)
gasket-engine    (depends on types, bus, history, storage, semantic, providers, sandbox, vault)
    ↑
gasket-core      (depends on all, re-exports all - NO implementations)
    ↑
gasket-cli       (depends on core)
```

## Feature Flag Considerations

`gasket-engine/Cargo.toml` has these feature flags that must continue to work:
- `smart-model-selection`
- `provider-gemini` → passes through to `gasket-providers/provider-gemini`
- `provider-copilot` → passes through to `gasket-providers/provider-copilot`
- `all-providers` → passes through to `gasket-providers/all-providers`

After removing `gasket-core` dependency, verify these still work correctly.

## Rollback Plan

Each PR is independently revertible:
1. If PR1 causes issues → revert PR1, fix in isolation
2. If PR2 causes issues → revert PR2, PR1 still valid
3. If PR3 causes issues → revert PR3, keep PR1+PR2

## Success Criteria

- [ ] `gasket-engine` has zero dependency on `gasket-core` (verify with `cargo tree`)
- [ ] `gasket-core/src/lib.rs` has NO `pub mod` declarations (~50 lines of re-exports only)
- [ ] All tests pass (`cargo test --workspace`)
- [ ] No circular dependencies in `cargo tree`
- [ ] CLI works without modification
- [ ] Feature flags still work correctly
- [ ] Build time does not increase significantly
