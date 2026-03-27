# gasket-history

History retrieval and processing system for gasket.

## Features

- Token counting and history truncation
- Multi-dimensional history query (time, branch, event type)
- Semantic search integration

## Dependencies

- `gasket-types` - Shared types
- `gasket-storage` - SQLite storage
- `gasket-semantic` - Semantic search
- `tokio` - Async runtime
- `chrono` - Date/time handling
- `thiserror` - Error handling
- `tracing` - Logging
- `serde` / `serde_json` - Serialization
- `uuid` - Unique identifiers
- `tiktoken-rs` - Token counting

## Usage

```rust
use gasket_history::{HistoryQuery, process_history, HistoryConfig};
```
