# Gasket Architecture

## Overview

Gasket is a modular AI assistant framework built on a three-layer architecture that separates concerns between:

1. **Message Bus** (`gasket-bus`) - Inter-component communication
2. **History & Storage** (`gasket-history`, `gasket-storage`, `gasket-semantic`) - Data persistence and retrieval
3. **Execution Engine** (`gasket-engine`) - Agent loop and tool execution

## Three-Layer Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      gasket-core (Facade)                   │
│  pub use gasket_bus::*; pub use gasket_history::*;         │
│  pub use gasket_engine::*; pub use gasket_providers::*;    │
└─────────────────────────────────────────────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         ▼                    ▼                    ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────────┐
│   gasket-bus    │ │ gasket-history  │ │   gasket-engine     │
│  (消息总线)      │ │   (存储检索)      │ │    (核心引擎)        │
│                 │ │                 │ │                     │
│ 依赖：           │ │ 依赖：           │ │ 依赖：               │
│ • gasket-types  │ │ • gasket-types  │ │ • gasket-bus        │
│ • tokio         │ │ • gasket-storage│ │ • gasket-history    │
│                 │ │ • gasket-semantic│ │ • gasket-providers  │
└─────────────────┘ └─────────────────┘ └─────────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │   gasket-types  │
                    │   (共享类型)     │
                    └─────────────────┘
```

## Component Details

### gasket-types

Base types shared across all components:
- `SessionKey` - Session identification
- `InboundMessage` - Incoming messages from channels
- `OutboundMessage` - Outgoing messages to channels
- `AgentEvent` - Agent lifecycle events

### gasket-bus

Message bus implementing the actor model:

```
Router → Session → Outbound
```

Three actors form a pipeline with zero locks:
- **Router** - Routes inbound messages to appropriate sessions
- **Session** - Manages session state and message processing
- **Outbound** - Sends messages to external channels

### gasket-history

History retrieval and processing:
- Token counting and history truncation
- Multi-dimensional history query (time, branch, event type)
- Semantic search integration

### gasket-storage

SQLite-based storage layer:
- Event storage with FTS5 full-text search
- Session state persistence
- Conversation branching support

### gasket-semantic

Semantic search capabilities:
- Local embedding generation
- Vector search with HNSW
- Integration with gasket-history

### gasket-engine

Core execution engine containing:
- **Agent Loop** - Main agent execution cycle
- **Tool System** - Built-in tools (spawn, web, file, etc.)
- **Bus Adapter** - MessageHandler trait implementation
- **Subagent** - Subagent delegation support

### gasket-providers

LLM provider implementations:
- OpenAI
- Anthropic
- Zhipu (智谱)
- DeepSeek
- And more...

### gasket-sandbox

Code execution sandbox for safe tool execution.

### gasket-vault

Knowledge vault scanner for RAG (Retrieval Augmented Generation).

### gasket-channels

Communication channel implementations:
- Telegram
- Discord
- Slack
- WebSocket
- And more...

## Data Flow

1. Inbound message arrives via channel
2. Router receives and routes to Session
3. Session stores event to gasket-storage
4. Agent loop retrieves history via gasket-history
5. Agent calls LLM via gasket-providers
6. Tools execute via gasket-engine / gasket-sandbox
7. Response stored and sent via Outbound to channel

## Crate Dependencies

```
gasket-types (foundation)
    │
    ├── gasket-bus
    ├── gasket-storage
    ├── gasket-channels
    ├── gasket-semantic
    │
    ├── gasket-history (depends on: types, storage, semantic)
    │
    └── gasket-engine (depends on: types, bus, history, providers, sandbox)

gasket-core (facade - re-exports all public APIs)
```
