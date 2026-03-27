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

### Current Dependencies in gasket-engine

16 places use `use gasket_core::*`:

| File | Import |
|------|--------|
| `agent/context.rs` | `gasket_core::error::AgentError` |
| `agent/pipeline.rs` | `gasket_core::error::AgentError` |
| `agent/skill_loader.rs` | `gasket_core::skills::{SkillsLoader, SkillsRegistry}` |
| `agent/summarization.rs` | `gasket_core::memory::SqliteStore`, `gasket_core::search::*` |
| `agent/subagent.rs` | `gasket_core::hooks::HookRegistry` |
| `agent/loop_.rs` | `gasket_core::error::AgentError`, `gasket_core::hooks::*`, `gasket_core::vault::*` |
| `agent/executor_core.rs` | `gasket_core::error::AgentError`, `gasket_core::token_tracker::*` |
| `agent/memory.rs` | `gasket_core::memory::SqliteStore` |
| `tools/cron.rs` | `gasket_core::cron::CronService` |
| `tools/registry.rs` | `gasket_core::search::*` |
| `tools/history_search.rs` | `gasket_core::memory::SqliteStore` |
| `tools/shell.rs` | `gasket_core::config::ExecToolConfig` |

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

## Migration Strategy: 3 PRs

### PR1: Core Dependency Layer

**Goal**: Migrate basic modules to engine, cut core dependencies for error/memory/search/token_tracker.

**Modules to Migrate**:

| Module | From | To | Used By |
|--------|------|----|---------|
| `error.rs` | `gasket-core/src/error.rs` | `gasket-engine/src/error.rs` | agent/*, tools/* |
| `token_tracker.rs` | `gasket-core/src/token_tracker.rs` | `gasket-engine/src/token_tracker.rs` | executor_core.rs |
| `memory/` | `gasket-core/src/memory/` | `gasket-engine/src/memory/` | summarization.rs, tools/* |
| `search/` | `gasket-core/src/search/` | `gasket-engine/src/search/` | summarization.rs, tools/* |

**Import Changes** (16 → ~8 remaining):

```rust
// Before
use gasket_core::error::AgentError;
use gasket_core::memory::SqliteStore;
use gasket_core::search::{top_k_similar, TextEmbedder};
use gasket_core::token_tracker::{ModelPricing, TokenUsage};

// After
use crate::error::AgentError;
use crate::memory::SqliteStore;
use crate::search::{top_k_similar, TextEmbedder};
use crate::token_tracker::{ModelPricing, TokenUsage};
```

**gasket-core Changes**:
- Keep modules but change to re-exports for backward compatibility

```rust
// gasket-core/src/error.rs
pub use gasket_engine::error::*;

// gasket-core/src/memory/mod.rs
pub use gasket_engine::memory::*;

// etc.
```

**Verification**:
- `cargo build --workspace` passes
- `cargo test --workspace` passes

---

### PR2: Extended Functionality Layer

**Goal**: Migrate remaining modules, completely cut `gasket-engine` → `gasket-core` dependency.

**Modules to Migrate**:

| Module | From | To | Used By |
|--------|------|----|---------|
| `hooks/` | `gasket-core/src/hooks/` | `gasket-engine/src/hooks/` | loop_.rs, subagent.rs |
| `skills/` | `gasket-core/src/skills/` | `gasket-engine/src/skills/` | skill_loader.rs |
| `cron/` | `gasket-core/src/cron/` | `gasket-engine/src/cron/` | tools/cron.rs |
| `vault/` (partial) | `gasket-core/src/vault/` | `gasket-engine/src/vault/` | loop_.rs |

**Import Changes** (~8 → 0):

```rust
// Before
use gasket_core::hooks::HookRegistry;
use gasket_core::skills::{SkillsLoader, SkillsRegistry};
use gasket_core::cron::CronService;
use gasket_core::vault::{redact_secrets, VaultInjector, VaultStore};
use gasket_core::config::ExecToolConfig;

// After
use crate::hooks::HookRegistry;
use crate::skills::{SkillsLoader, SkillsRegistry};
use crate::cron::CronService;
use crate::vault::{redact_secrets, VaultInjector, VaultStore};
use crate::config::ExecToolConfig;
```

**Critical Change**: Remove from `gasket-engine/Cargo.toml`:
```toml
# Remove this line
gasket-core = { path = "../core" }
```

**Verification**:
- `cargo build --workspace` passes
- `cargo tree -p gasket-engine` shows no `gasket-core` dependency
- `cargo test --workspace` passes

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
gasket/core/src/error.rs     # Moved to engine
gasket/core/src/token_tracker.rs  # Moved to engine
gasket/core/src/memory/      # Moved to engine
gasket/core/src/search/      # Already in history
```

**New `gasket-core/src/lib.rs`** (~100 lines):

```rust
//! gasket-core: Facade for gasket AI assistant framework
//!
//! This crate re-exports all gasket crates for backward compatibility.
//! It provides a single entry point for all gasket functionality.

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
- Remove most direct dependencies
- Keep only internal crate dependencies

**Verification**:
- `cargo build --workspace` passes
- `cargo test --workspace` passes
- `cargo clippy --workspace` has no new warnings
- CLI still works: `cargo run --package gasket-cli -- agent -m "test"`
- `cargo tree` shows clean dependency graph (no cycles)

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
gasket-engine    (depends on types, bus, history, storage, semantic, providers, sandbox)
    ↑
gasket-core      (depends on all, re-exports all)
    ↑
gasket-cli       (depends on core)
```

## Rollback Plan

Each PR is independently revertible:
1. If PR1 causes issues → revert PR1, fix in isolation
2. If PR2 causes issues → revert PR2, PR1 still valid
3. If PR3 causes issues → revert PR3, keep PR1+PR2

## Success Criteria

- [ ] `gasket-engine` has zero dependency on `gasket-core`
- [ ] `gasket-core` is ~100 lines of re-exports
- [ ] All tests pass
- [ ] No circular dependencies in `cargo tree`
- [ ] CLI works without modification
- [ ] Build time does not increase significantly
