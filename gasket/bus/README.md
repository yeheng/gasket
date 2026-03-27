# gasket-bus

Message bus for inter-component communication in gasket.

## Architecture

Three actors form a clean pipeline with zero locks:

```
Router → Session → Outbound
```

## Dependencies

- `gasket-types` - Shared types (SessionKey, InboundMessage, OutboundMessage)
- `gasket-channels` - Channel implementations
- `tokio` - Async runtime
- `tokio-util` - Tokio utilities
- `async-trait` - Async trait support
- `tracing` - Logging
- `thiserror` - Error handling

## Features

- `webhook` - WebSocket support (via gasket-channels)

## Usage

```rust
use gasket_bus::{MessageHandler, run_session_actor};
```
