# gasket-engine

Core execution engine for gasket AI assistant.

## Components

- `agent/` - Agent loop, executor, context, subagent
- `tools/` - Tool system (spawn, web, file, etc.)
- `bus_adapter/` - MessageHandler trait implementation

## Dependencies

- `gasket-types` - Shared types
- `gasket-bus` - Message bus
- `gasket-history` - History retrieval
- `gasket-storage` - SQLite storage
- `gasket-semantic` - Semantic search
- `gasket-providers` - LLM providers
- `gasket-sandbox` - Code execution sandbox
- `gasket-core` - Core library
- `gasket-vault` - Knowledge vault
- `gasket-channels` - Communication channels
- `tokio` / `tokio-util` - Async runtime
- `async-trait` - Async trait support
- `serde` / `serde_json` - Serialization
- `thiserror` / `anyhow` - Error handling
- `tracing` - Logging
- `dashmap` - Concurrent hash map
- `tiktoken-rs` - Token counting
- `parking_lot` - Synchronization primitives
- `regex` - Pattern matching
- `chrono` / `cron` / `uuid` - Time and identifiers
- `reqwest` - HTTP client
- And more utility crates

## Usage

```rust
use gasket_engine::{AgentLoop, AgentConfig, ToolRegistry};
```
