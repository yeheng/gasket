## 1. Project Setup (Stand-alone)

- [ ] 1.1 Create new directory `tantivy-mcp/` at project sibling level (NOT inside nanobot-rs)
- [ ] 1.2 Create `Cargo.toml` with independent project configuration (NOT workspace member)
- [ ] 1.3 Add binary target `src/main.rs`
- [ ] 1.4 Create library module structure (`src/lib.rs`, `src/mcp/`, `src/index/`, `src/maintenance/`)
- [ ] 1.5 Define error types in `src/error.rs`
- [ ] 1.6 Add README.md with installation and usage instructions

## 2. MCP Protocol Layer

- [ ] 2.1 Implement JSON-RPC 2.0 message types (`src/mcp/types.rs`)
- [ ] 2.2 Implement stdio transport (`src/mcp/transport.rs`)
- [ ] 2.3 Implement request/response handling (`src/mcp/handler.rs`)
- [ ] 2.4 Implement tool registration and dispatch (`src/mcp/tools.rs`)
- [ ] 2.5 Add `initialize` and `tools/list` handlers

## 3. Index Management

- [ ] 3.1 Define schema types (`IndexSchema`, `FieldDef`, `FieldType`) in `src/index/schema.rs`
- [ ] 3.2 Implement `IndexManager` for managing multiple indexes (`src/index/manager.rs`)
- [ ] 3.3 Implement index creation with Tantivy schema mapping
- [ ] 3.4 Implement index listing and deletion
- [ ] 3.5 Add index persistence (save/load metadata from disk)
- [ ] 3.6 Add TTL field to index schema (hidden `_expires_at` field)
- [ ] 3.7 Store index configuration (default_ttl, auto_compact settings)

## 4. Document Operations

- [ ] 4.1 Implement document addition with schema validation (`src/index/document.rs`)
- [ ] 4.2 Implement document update (upsert by ID)
- [ ] 4.3 Implement document deletion
- [ ] 4.4 Add support for all field types (text, string, i64, f64, datetime, string_array, json)
- [ ] 4.5 Implement commit logic with proper error handling
- [ ] 4.6 Implement TTL calculation and `_expires_at` field population

## 5. Search Implementation

- [ ] 5.1 Implement query parser for text queries (`src/index/search.rs`)
- [ ] 5.2 Implement field filter queries (eq, ne, gte, lte, gt, lt)
- [ ] 5.3 Implement date range filtering
- [ ] 5.4 Implement pagination (offset, limit)
- [ ] 5.5 Implement result sorting (by score or field)
- [ ] 5.6 Implement snippet highlighting for text fields

## 6. Maintenance Operations

- [ ] 6.1 Implement `index_compact` tool - merge segments and remove deleted docs (`src/maintenance/compact.rs`)
- [ ] 6.2 Implement `index_expire` tool - remove expired documents (`src/maintenance/expire.rs`)
- [ ] 6.3 Implement `index_rebuild` tool - recreate index with optional new schema (`src/maintenance/rebuild.rs`)
- [ ] 6.4 Implement `index_backup` tool - create consistent snapshot (`src/maintenance/backup.rs`)
- [ ] 6.5 Implement `index_restore` tool - restore from backup (`src/maintenance/backup.rs`)
- [ ] 6.6 Implement `index_stats` tool - return health and usage statistics (`src/maintenance/stats.rs`)

## 7. Automatic Maintenance

- [ ] 7.1 Create maintenance scheduler module (`src/maintenance/scheduler.rs`)
- [ ] 7.2 Implement auto-compaction trigger (deleted ratio threshold, segment count)
- [ ] 7.3 Implement auto-expiration background task (periodic cleanup)
- [ ] 7.4 Implement `index_configure` tool for maintenance settings
- [ ] 7.5 Implement `maintenance_status` tool
- [ ] 7.6 Add maintenance task persistence and recovery

## 8. MCP Tools (Core)

- [ ] 8.1 Implement `index_create` tool (with TTL support)
- [ ] 8.2 Implement `index_drop` tool
- [ ] 8.3 Implement `index_list` tool
- [ ] 8.4 Implement `document_add` tool (with TTL support)
- [ ] 8.5 Implement `document_delete` tool
- [ ] 8.6 Implement `search` tool
- [ ] 8.7 Add JSON schema for each tool's input parameters

## 9. Binary Entry Point

- [ ] 9.1 Implement `main.rs` with CLI argument parsing (clap)
- [ ] 9.2 Add `--index-dir` option for custom index directory
- [ ] 9.3 Add `--config` option for configuration file
- [ ] 9.4 Implement graceful shutdown handling (SIGINT/SIGTERM)
- [ ] 9.5 Add logging/tracing support with configurable level
- [ ] 9.6 Start maintenance scheduler on startup

## 10. Testing and Documentation

- [ ] 10.1 Add unit tests for schema validation
- [ ] 10.2 Add unit tests for document indexing with TTL
- [ ] 10.3 Add unit tests for compaction
- [ ] 10.4 Add unit tests for expiration
- [ ] 10.5 Add integration tests for MCP protocol
- [ ] 10.6 Add usage examples in README.md
- [ ] 10.7 Document tool schemas and examples
- [ ] 10.8 Add example MCP client configuration

## 11. Release Preparation

- [ ] 11.1 Set up CI/CD pipeline (GitHub Actions)
- [ ] 11.2 Add release workflow for binary distribution
- [ ] 11.3 Publish to crates.io (optional)
- [ ] 11.4 Create release documentation

## Notes

- **NO changes to nanobot-core or nanobot-cli**
- **NO workspace membership** - this is a completely separate project
- Default data directory: `~/.tantivy-mcp/` (NOT `~/.nanobot/`)
- Integration with nanobot is done via MCP configuration, not code
- Maintenance tasks run in background with minimal impact on query performance
