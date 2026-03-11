# Design: tantivy-mcp Index Server (Standalone)

## Context

This is a **completely new, standalone project** with zero dependencies on nanobot.

### Project Location
```
/Users/yeheng/workspaces/Github/
├── nanobot/                    # Existing nanobot project (unchanged)
│   └── nanobot-rs/
│       ├── nanobot-core/
│       ├── nanobot-cli/
│       └── nanobot-tantivy/    # Remains as internal library
│
└── tantivy-mcp/                # NEW: Standalone project (sibling, not child)
    ├── Cargo.toml
    └── src/
        ├── main.rs
        └── ...
```

### Constraints
- **Zero coupling** with nanobot - no shared code, no workspace membership
- Must support MCP protocol (JSON-RPC 2.0 over stdio)
- Should support concurrent operations safely
- Must persist indexes to disk for durability
- Self-contained binary with minimal external dependencies

### Stakeholders
- **Any MCP client**: Claude Code, Cursor, Continue, or custom MCP implementations
- **nanobot**: Can use it as an external MCP server (optional, runtime configuration)
- **End users**: Benefit from a reusable, universal index service

## Goals / Non-Goals

### Goals
- Create a standalone MCP server binary that provides JSON document indexing
- Support dynamic schema definition for different document types
- Enable full-text search with BM25 ranking
- Support structured queries (field filters, date ranges, tags)
- Provide multi-tenancy via named indexes
- Zero dependency on nanobot project

### Non-Goals
- Distributed indexing (single-node only)
- Real-time replication or clustering
- Vector embeddings (semantic search)
- Complex aggregations or analytics
- Integration with nanobot at compile time

## Decisions

### Decision 1: Project Isolation

**Choice**: Create `tantivy-mcp` as a completely separate project, NOT in nanobot workspace.

**Rationale**:
- True independence: No accidental dependencies through workspace
- Separate versioning: Can evolve at different pace
- Separate release cycle: Deploy without touching nanobot
- Universal access: Any MCP client can use it

**Implementation**:
- Own `Cargo.toml` with own dependencies
- Own CI/CD pipeline
- Own README and documentation
- Published as standalone crate/binary

### Decision 2: MCP Server Architecture

**Choice**: Implement as a single binary with stdio-based JSON-RPC 2.0 communication.

**Rationale**:
- Follows MCP specification for tool servers
- Simple deployment model (single executable)
- Compatible with ALL MCP clients, not just nanobot
- No network configuration required (stdio transport)

**Alternatives considered**:
- HTTP server: Adds complexity, requires port management, security considerations
- gRPC: Overkill for the use case, breaks MCP compatibility

### Decision 3: Schema Design

**Choice**: Use dynamic schema per index, defined via MCP tool parameters.

```json
{
  "index_name": "emails",
  "fields": [
    {"name": "subject", "type": "text", "indexed": true, "stored": true},
    {"name": "body", "type": "text", "indexed": true, "stored": true},
    {"name": "from", "type": "string", "indexed": true, "stored": true},
    {"name": "date", "type": "datetime", "indexed": true, "stored": true},
    {"name": "labels", "type": "string_array", "indexed": true, "stored": true}
  ]
}
```

**Rationale**:
- Flexibility: Different document types have different fields
- Simplicity: Schema is part of the tool call, no separate migration needed
- Transparency: Users can see and modify schema at any time

### Decision 4: Index Storage

**Choice**: One directory per index under a configurable base path (default: `~/.tantivy-mcp/indexes/`).

```
~/.tantivy-mcp/
├── config.json              # Server configuration
└── indexes/
    ├── emails/
    │   ├── .tantivy-meta
    │   └── segment-*
    ├── documents/
    │   ├── .tantivy-meta
    │   └── segment-*
    └── notes/
        ├── .tantivy-meta
        └── segment-*
```

**Rationale**:
- **NOT** using `~/.nanobot/` - completely separate data directory
- Isolation: Each index has its own Tantivy instance
- Portability: Can backup/restore individual indexes
- Simplicity: Maps directly to file system, easy to understand

### Decision 5: MCP Tools Design

**Choice**: Provide thirteen tools organized into three categories:

#### Core Operations (6 tools)
| Tool | Description |
|------|-------------|
| `index_create` | Create a new index with schema and optional TTL |
| `index_drop` | Delete an index |
| `index_list` | List all indexes |
| `document_add` | Add/update documents with optional TTL |
| `document_delete` | Remove documents |
| `search` | Full-text and structured search |

#### Maintenance Operations (6 tools)
| Tool | Description |
|------|-------------|
| `index_compact` | Merge segments, remove deleted documents |
| `index_expire` | Remove expired documents manually |
| `index_rebuild` | Rebuild index (optionally with new schema) |
| `index_backup` | Create index snapshot |
| `index_restore` | Restore index from backup |
| `index_stats` | Get index health and usage statistics |

#### Configuration (1 tool)
| Tool | Description |
|------|-------------|
| `index_configure` | Configure auto-maintenance settings |

**Rationale**:
- CRUD completeness: All operations needed for index management
- Maintenance: Proactive index health management prevents degradation
- MCP best practices: Tools are discoverable and self-documenting

### Decision 6: TTL and Expiration Design

**Choice**: Support both index-level default TTL and document-level TTL override.

```json
// Index with default TTL
{
  "name": "logs",
  "fields": [...],
  "default_ttl": "7d"    // Documents expire after 7 days
}

// Document with custom TTL
{
  "index": "logs",
  "id": "log-001",
  "fields": {...},
  "ttl": "1d"            // Override: expire after 1 day
}
```

**Implementation**:
- Store `_expires_at` as a hidden datetime field
- Automatic cleanup runs periodically (configurable, default: 1 hour)
- Manual cleanup via `index_expire` tool

**Rationale**:
- Use case: Log data, temporary caches, time-sensitive documents
- Flexibility: Document-level TTL allows exceptions to index default

### Decision 7: Automatic Maintenance

**Choice**: Implement configurable automatic maintenance with sensible defaults.

| Task | Trigger | Default |
|------|---------|---------|
| Auto-compaction | Deleted ratio > 20% OR segments > 10 | Enabled |
| Auto-expiration | Periodic interval | 1 hour |
| Health check | On stats query | Always |

**Configuration Example**:
```json
{
  "auto_compact": {
    "enabled": true,
    "deleted_ratio_threshold": 0.2,
    "max_segments": 10,
    "schedule": "0 2 * * *"   // Cron: 2 AM daily
  },
  "auto_expire": {
    "enabled": true,
    "interval_seconds": 3600
  }
}
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    tantivy-mcp Server                        │
│                  (Standalone Binary)                         │
│                                                              │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐  │
│  │   Stdio      │───▶│   JSON-RPC   │───▶│   Tool       │  │
│  │   Transport  │    │   Handler    │    │   Router     │  │
│  └──────────────┘    └──────────────┘    └──────┬───────┘  │
│                                                  │          │
│  ┌──────────────────────────────────────────────▼───────┐  │
│  │                   Index Manager                       │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │  │
│  │  │   emails    │  │  documents  │  │   notes     │  │  │
│  │  │   index     │  │   index     │  │   index     │  │  │
│  │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  │  │
│  └─────────┼────────────────┼────────────────┼─────────┘  │
│            │                │                │            │
│  ┌─────────▼────────────────▼────────────────▼─────────┐  │
│  │                   Tantivy Engine                     │  │
│  │  • BM25 ranking  • Fuzzy matching  • Faceted search  │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                              │
                    ┌─────────▼─────────┐
                    │  ~/.tantivy-mcp/  │
                    │    (disk)         │
                    └───────────────────┘
```

## Data Model

### Index Schema

```rust
struct IndexSchema {
    name: String,
    fields: Vec<FieldDef>,
    created_at: DateTime<Utc>,
}

struct FieldDef {
    name: String,
    field_type: FieldType,
    indexed: bool,    // Include in search index
    stored: bool,     // Return in search results
}

enum FieldType {
    Text,           // Full-text indexed (tokenized)
    String,         // Exact match (not tokenized)
    I64,            // Integer
    F64,            // Float
    DateTime,       // ISO 8601 timestamp
    StringArray,    // Multiple string values (tags, labels)
    Json,           // Nested JSON (stored only, not indexed)
}
```

### Document Format

```json
{
  "id": "unique-doc-id",
  "fields": {
    "subject": "Hello World",
    "body": "This is the content...",
    "from": "user@example.com",
    "date": "2024-01-15T10:30:00Z",
    "labels": ["inbox", "important"]
  }
}
```

### Search Query Format

```json
{
  "index": "emails",
  "query": {
    "text": "hello world",
    "filters": [
      {"field": "from", "op": "eq", "value": "user@example.com"},
      {"field": "date", "op": "gte", "value": "2024-01-01"}
    ],
    "limit": 10,
    "offset": 0,
    "sort": {"field": "date", "order": "desc"}
  }
}
```

## Project Structure

```
tantivy-mcp/
├── Cargo.toml              # Independent project, NOT workspace member
├── README.md               # Standalone documentation
├── src/
│   ├── main.rs             # Binary entry point
│   ├── lib.rs              # Library exports
│   ├── error.rs            # Error types
│   ├── mcp/
│   │   ├── mod.rs
│   │   ├── types.rs        # JSON-RPC types
│   │   ├── transport.rs    # Stdio transport
│   │   ├── handler.rs      # Request handling
│   │   └── tools.rs        # Tool definitions
│   └── index/
│       ├── mod.rs
│       ├── schema.rs       # Schema types
│       ├── manager.rs      # Index manager
│       ├── document.rs     # Document operations
│       └── search.rs       # Search implementation
├── tests/
│   ├── integration_test.rs
│   └── mcp_test.rs
└── .github/
    └── workflows/
        └── ci.yml          # Independent CI
```

## Dependencies

```toml
[dependencies]
# Async runtime
tokio = { version = "1", features = ["rt-multi-thread", "macros", "io-std", "signal"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Error handling
thiserror = "1"
anyhow = "1"

# Time
chrono = { version = "0.4", features = ["serde"] }

# Search engine
tantivy = "0.25"

# CLI (optional, for configuration)
clap = { version = "4", features = ["derive"] }
```

**Note**: No dependencies on any nanobot crates.

## Risks / Trade-offs

### Risk: Duplicate Code
- **Risk**: Some code patterns might be similar to nanobot-tantivy
- **Mitigation**: Accept minimal duplication for true independence; patterns can be extracted to a shared crate later if needed

### Risk: Schema Migration Complexity
- **Risk**: Changing schema after data is indexed requires re-indexing
- **Mitigation**: Document clearly; provide `index_rebuild` tool for migration scenarios

### Risk: Memory Usage with Large Indexes
- **Risk**: Tantivy loads index metadata into memory; large indexes may consume RAM
- **Mitigation**: Set sensible defaults; document memory requirements

### Trade-off: Independence vs Code Reuse
- **Trade-off**: Not sharing code with nanobot-tantivy
- **Benefit**: True independence, no version coupling, separate evolution
- **Cost**: May duplicate some utility code

## Usage Example

### Starting the Server

```bash
# Install
cargo install tantivy-mcp

# Run with default config
tantivy-mcp

# Run with custom index directory
tantivy-mcp --index-dir /path/to/indexes
```

### MCP Configuration (for any MCP client)

```json
{
  "mcpServers": {
    "tantivy": {
      "command": "tantivy-mcp",
      "args": ["--index-dir", "~/.tantivy-mcp/indexes"]
    }
  }
}
```

### Tool Usage Examples

```json
// Create an index
{
  "name": "index_create",
  "arguments": {
    "name": "emails",
    "fields": [
      {"name": "subject", "type": "text", "indexed": true, "stored": true},
      {"name": "body", "type": "text", "indexed": true, "stored": true}
    ]
  }
}

// Add a document
{
  "name": "document_add",
  "arguments": {
    "index": "emails",
    "id": "email-001",
    "fields": {
      "subject": "Hello World",
      "body": "This is the email body..."
    }
  }
}

// Search
{
  "name": "search",
  "arguments": {
    "index": "emails",
    "query": {"text": "hello"}
  }
}
```

## Open Questions

1. **Default index directory?**
   - Current: `~/.tantivy-mcp/indexes`
   - Alternative: Platform-specific (XDG on Linux, Application Support on macOS)

2. **Should we support batch operations?**
   - Current design: Single document per call
   - Alternative: Add `documents_add` for batch indexing
   - Decision: Start with single; add batch if needed

3. **How to handle index locking?**
   - Tantivy's IndexWriter needs exclusive access
   - Suggestion: Queue writes in memory, commit on interval or explicit call
