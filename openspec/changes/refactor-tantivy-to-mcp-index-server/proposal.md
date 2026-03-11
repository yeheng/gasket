# Change: Create Standalone tantivy-mcp Index Server

## Why

Currently, nanobot-tantivy is tightly coupled with nanobot as a library crate. This proposal creates a **completely independent** MCP index server project that:

1. Has zero dependencies on nanobot-core or nanobot-cli
2. Can be used by ANY MCP-compatible client (Claude Code, Cursor, etc.)
3. Runs as a completely isolated, standalone process
4. Indexes arbitrary JSON documents with dynamic schemas

This separation enables:
- **True Independence**: No coupling with nanobot, can evolve independently
- **Universal Reusability**: Works with any MCP client, not just nanobot
- **Clean Architecture**: Single responsibility - just JSON indexing via MCP
- **Easy Deployment**: Single binary with no external dependencies on nanobot

## What Changes

- **CREATED**: New standalone project `tantivy-mcp/` (sibling to `nanobot-rs/`)
- **ADDED**: MCP protocol implementation (tools/call, tools/list)
- **ADDED**: JSON document indexing with configurable field mappings
- **ADDED**: Multi-index support with namespace isolation
- **ADDED**: Dynamic schema definition via tool parameters
- **ADDED**: CRUD operations for JSON documents
- **UNCHANGED**: nanobot-tantivy remains as-is for nanobot internal use
- **UNCHANGED**: nanobot-core and nanobot-cli have NO changes or references to the new project

## Impact

- Affected specs: json-index-service (new capability)
- Affected code:
  - **NEW** `tantivy-mcp/` - Completely new, isolated project
  - `nanobot-rs/` - No changes whatsoever
  - `nanobot-core/` - No changes
  - `nanobot-cli/` - No changes
- Integration:
  - Users can optionally add tantivy-mcp to their MCP server configuration
  - No automatic loading or dependency from nanobot
  - Discovered at runtime via MCP protocol, not compile time
