# tantivy-mcp

A standalone MCP (Model Context Protocol) index server providing full-text search capabilities using the Tantivy search engine.

## Features

- **JSON Document Indexing**: Index arbitrary JSON documents with dynamic schemas
- **Full-Text Search**: BM25 ranking, fuzzy matching, and phrase queries
- **Multi-Index Support**: Create multiple named indexes with different schemas
- **TTL Support**: Automatic document expiration with configurable time-to-live
- **Index Maintenance**: Compaction, backup/restore, and health monitoring
- **MCP Protocol**: Compatible with any MCP client (Claude Code, Cursor, etc.)

## Installation

```bash
# From source
cd tantivy-mcp
cargo install --path .

# Or build release binary
cargo build --release
```

## Usage

### Starting the Server

```bash
# Start with default data directory (~/.local/share/tantivy-mcp/)
tantivy-mcp

# Specify custom index directory
tantivy-mcp --index-dir /path/to/indexes

# Set log level
tantivy-mcp --log-level debug
```

### MCP Configuration

Add to your MCP client configuration:

```json
{
  "mcpServers": {
    "tantivy": {
      "command": "tantivy-mcp",
      "args": []
    }
  }
}
```

## Available Tools

### Index Management

| Tool | Description |
|------|-------------|
| `index_create` | Create a new index with schema |
| `index_drop` | Delete an index |
| `index_list` | List all indexes |
| `index_stats` | Get index statistics and health |

### Document Operations

| Tool | Description |
|------|-------------|
| `document_add` | Add or update a document |
| `document_delete` | Delete a document |
| `document_commit` | Commit pending changes |

### Search

| Tool | Description |
|------|-------------|
| `search` | Full-text and structured search |

### Maintenance

| Tool | Description |
|------|-------------|
| `index_compact` | Compact index (merge segments) |

## Example Usage

### Create an Index

```json
{
  "name": "index_create",
  "arguments": {
    "name": "emails",
    "fields": [
      {"name": "subject", "type": "text"},
      {"name": "body", "type": "text"},
      {"name": "from", "type": "string"},
      {"name": "date", "type": "datetime"},
      {"name": "labels", "type": "string_array"}
    ],
    "default_ttl": "30d"
  }
}
```

### Add a Document

```json
{
  "name": "document_add",
  "arguments": {
    "index": "emails",
    "id": "email-001",
    "fields": {
      "subject": "Hello World",
      "body": "This is the email content...",
      "from": "sender@example.com",
      "date": "2024-01-15T10:30:00Z",
      "labels": ["inbox", "important"]
    }
  }
}
```

### Search

```json
{
  "name": "search",
  "arguments": {
    "index": "emails",
    "query": {
      "text": "hello",
      "filters": [
        {"field": "from", "op": "eq", "value": "sender@example.com"}
      ],
      "limit": 10
    }
  }
}
```

## Field Types

| Type | Description | Indexed | Stored |
|------|-------------|---------|--------|
| `text` | Full-text (tokenized) | Yes | Yes |
| `string` | Exact match only | Yes | Yes |
| `i64` | 64-bit integer | Yes | Yes |
| `f64` | 64-bit float | Yes | Yes |
| `datetime` | ISO 8601 timestamp | Yes | Yes |
| `string_array` | Multiple string values | Yes | Yes |
| `json` | Nested JSON object | No | Yes |

## TTL Format

Time-to-live can be specified with these units:
- `s`, `sec`, `seconds` - Seconds
- `m`, `min`, `minutes` - Minutes
- `h`, `hour`, `hours` - Hours
- `d`, `day`, `days` - Days
- `w`, `week`, `weeks` - Weeks

Examples: `1d`, `7d`, `30d`, `1w`

## Data Storage

Default data directory: `~/.local/share/tantivy-mcp/` (Linux) or `~/Library/Application Support/tantivy-mcp/` (macOS)

Structure:
```
tantivy-mcp/
├── config.json
└── indexes/
    ├── emails/
    │   ├── metadata.json
    │   └── *.tantivy*
    └── documents/
        ├── metadata.json
        └── *.tantivy*
```

## License

MIT License
