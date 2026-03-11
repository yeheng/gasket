## ADDED Requirements

### Requirement: Standalone Project Independence
The tantivy-mcp project SHALL be completely independent from nanobot with zero compile-time or runtime dependencies on nanobot code.

#### Scenario: Separate Project Directory
- **WHEN** the project is created
- **THEN** it SHALL exist as a sibling directory to nanobot-rs (NOT inside nanobot workspace)
- **AND** have its own independent Cargo.toml
- **AND** NOT be a member of any nanobot workspace

#### Scenario: Independent Data Storage
- **WHEN** the server stores indexes
- **THEN** the default data directory SHALL be `~/.tantivy-mcp/`
- **AND** NOT use `~/.nanobot/` or any nanobot-specific paths

#### Scenario: No nanobot Dependencies
- **WHEN** the project is compiled
- **THEN** the dependency tree SHALL NOT include any nanobot crates
- **AND** function without nanobot installed

### Requirement: MCP Index Server Binary
The system SHALL provide a standalone binary executable that implements the Model Context Protocol (MCP) over stdio transport, enabling any MCP-compatible client to use the indexing service.

#### Scenario: Server Startup
- **WHEN** the binary is executed
- **THEN** it SHALL initialize MCP protocol handlers
- **AND** respond to `initialize` requests with server capabilities
- **AND** advertise available tools via `tools/list`

#### Scenario: Graceful Shutdown
- **WHEN** the server receives EOF on stdin or SIGINT/SIGTERM signal
- **THEN** it SHALL commit any pending writes
- **AND** close all open indexes cleanly

### Requirement: Index Creation and Management
The system SHALL allow clients to create, list, and delete named indexes with custom schemas.

#### Scenario: Create New Index
- **WHEN** client calls `index_create` with index name and field definitions
- **THEN** system SHALL create a new Tantivy index with specified schema
- **AND** persist index metadata to disk
- **AND** return success with index metadata

#### Scenario: Create Index with Duplicate Name
- **WHEN** client calls `index_create` with an existing index name
- **THEN** system SHALL return an error indicating index already exists
- **AND** preserve existing index unchanged

#### Scenario: List Indexes
- **WHEN** client calls `index_list`
- **THEN** system SHALL return all available indexes with their schemas

#### Scenario: Delete Index
- **WHEN** client calls `index_drop` with an index name
- **THEN** system SHALL remove the index directory
- **AND** release all associated resources
- **AND** return success confirmation

### Requirement: Document Indexing
The system SHALL support adding, updating, and deleting JSON documents in named indexes.

#### Scenario: Add Document
- **WHEN** client calls `document_add` with index name, document ID, and field values
- **THEN** system SHALL index the document according to schema
- **AND** store retrievable field values
- **AND** return success with document ID

#### Scenario: Update Existing Document
- **WHEN** client calls `document_add` with an existing document ID
- **THEN** system SHALL replace the existing document atomically
- **AND** update the search index accordingly

#### Scenario: Delete Document
- **WHEN** client calls `document_delete` with index name and document ID
- **THEN** system SHALL remove the document from the index
- **AND** return success confirmation

#### Scenario: Invalid Document Field
- **WHEN** client calls `document_add` with a field not in schema
- **THEN** system SHALL return an error indicating unknown field
- **AND** reject the document

### Requirement: Full-Text Search
The system SHALL provide full-text search across indexed documents with BM25 relevance ranking.

#### Scenario: Basic Text Search
- **WHEN** client calls `search` with index name and text query
- **THEN** system SHALL return matching documents ranked by BM25 score
- **AND** include highlighted snippets when matches are found
- **AND** respect configured limit (default: 10 results)

#### Scenario: Search with Field Filter
- **WHEN** client calls `search` with text query AND field filters
- **THEN** system SHALL apply filters before ranking
- **AND** only return documents matching all filter conditions

#### Scenario: Search with Date Range
- **WHEN** client calls `search` with date range filter on a datetime field
- **THEN** system SHALL only return documents within the specified date range

#### Scenario: Search with Pagination
- **WHEN** client calls `search` with offset and limit parameters
- **THEN** system SHALL return results starting from the offset
- **AND** limit total results to the specified count

#### Scenario: Search Non-Existent Index
- **WHEN** client calls `search` on a non-existent index
- **THEN** system SHALL return an error indicating index not found

### Requirement: Schema Definition
The system SHALL support dynamic schema definition with multiple field types.

#### Scenario: Text Field Type
- **WHEN** schema defines a field with type "text"
- **THEN** the field SHALL be tokenized and full-text indexed
- **AND** support phrase queries and highlighting

#### Scenario: String Field Type
- **WHEN** schema defines a field with type "string"
- **THEN** the field SHALL be indexed for exact matching only
- **AND** NOT be tokenized

#### Scenario: DateTime Field Type
- **WHEN** schema defines a field with type "datetime"
- **THEN** the field SHALL accept ISO 8601 formatted dates
- **AND** support range queries (gte, lte, gt, lt)

#### Scenario: StringArray Field Type
- **WHEN** schema defines a field with type "string_array"
- **THEN** the field SHALL accept arrays of strings
- **AND** support filtering on any array element

### Requirement: Index Persistence
The system SHALL persist all indexes to disk and restore them on restart.

#### Scenario: Index Persistence on Commit
- **WHEN** documents are added or deleted
- **THEN** changes SHALL be persisted to disk
- **AND** survive server restart

#### Scenario: Index Recovery on Startup
- **WHEN** server starts and indexes directory exists
- **THEN** system SHALL load existing indexes
- **AND** make them immediately available for queries

### Requirement: Index Maintenance - Compaction
The system SHALL provide index compaction to optimize storage and query performance by merging segments and removing deleted documents.

#### Scenario: Manual Index Compaction
- **WHEN** client calls `index_compact` with an index name
- **THEN** system SHALL merge all index segments into a single optimized segment
- **AND** permanently remove previously deleted documents
- **AND** return compaction statistics (bytes saved, segments merged)

#### Scenario: Compaction on Non-Existent Index
- **WHEN** client calls `index_compact` on a non-existent index
- **THEN** system SHALL return an error indicating index not found

#### Scenario: Compaction During Active Writes
- **WHEN** client calls `index_compact` while documents are being added
- **THEN** system SHALL queue the compaction request
- **AND** execute after pending writes complete
- **OR** return an error if concurrent modification is detected

### Requirement: Index Maintenance - Expiration
The system SHALL support automatic document expiration based on time-to-live (TTL) configuration.

#### Scenario: Index Creation with TTL
- **WHEN** client calls `index_create` with a `default_ttl` parameter (e.g., 7d, 30d)
- **THEN** system SHALL store the TTL configuration for the index
- **AND** automatically mark documents for deletion after TTL expires

#### Scenario: Document with Custom TTL
- **WHEN** client calls `document_add` with a `ttl` parameter
- **THEN** system SHALL use the document-specific TTL instead of index default
- **AND** expire the document after the specified duration

#### Scenario: Expired Document Cleanup
- **WHEN** the `index_expire` tool is called or automatic cleanup runs
- **THEN** system SHALL delete all documents past their expiration time
- **AND** return count of expired documents removed

#### Scenario: TTL Not Configured
- **WHEN** an index has no TTL configured
- **THEN** documents SHALL NOT expire automatically
- **AND** `index_expire` SHALL return 0 documents removed

### Requirement: Index Maintenance - Statistics
The system SHALL provide detailed statistics about index health and resource usage.

#### Scenario: Get Index Statistics
- **WHEN** client calls `index_stats` with an index name
- **THEN** system SHALL return:
  - Total document count
  - Index size on disk (bytes)
  - Number of segments
  - Deleted documents count (not yet compacted)
  - Last modified timestamp
  - Index health status

#### Scenario: Get All Indexes Statistics
- **WHEN** client calls `index_stats` without specifying an index
- **THEN** system SHALL return aggregated statistics for all indexes
- **AND** include per-index breakdown

#### Scenario: Health Status Indicators
- **WHEN** index statistics are retrieved
- **THEN** health status SHALL indicate:
  - "healthy" - normal operation
  - "needs_compaction" - high deleted document ratio (>20%)
  - "warning" - approaching size limits
  - "error" - index corruption detected

### Requirement: Index Maintenance - Rebuild
The system SHALL support rebuilding an index from scratch to fix corruption or apply schema changes.

#### Scenario: Rebuild Index
- **WHEN** client calls `index_rebuild` with an index name
- **THEN** system SHALL create a new index with the same schema
- **AND** re-index all documents from the original index
- **AND** replace the original index atomically

#### Scenario: Rebuild with New Schema
- **WHEN** client calls `index_rebuild` with an index name and new field definitions
- **THEN** system SHALL apply the new schema
- **AND** preserve document fields that exist in both schemas
- **AND** drop fields not in new schema
- **AND** use default values for new required fields

#### Scenario: Rebuild Non-Existent Index
- **WHEN** client calls `index_rebuild` on a non-existent index
- **THEN** system SHALL return an error indicating index not found

### Requirement: Index Maintenance - Backup and Restore
The system SHALL support backup and restore operations for index data.

#### Scenario: Backup Index
- **WHEN** client calls `index_backup` with index name and backup path
- **THEN** system SHALL create a consistent snapshot of the index
- **AND** store it at the specified path
- **AND** return backup metadata (timestamp, size, document count)

#### Scenario: Restore Index
- **WHEN** client calls `index_restore` with backup path
- **THEN** system SHALL restore the index from backup
- **AND** overwrite existing index with same name if exists
- **AND** return restore confirmation

#### Scenario: Backup Non-Existent Index
- **WHEN** client calls `index_backup` on a non-existent index
- **THEN** system SHALL return an error indicating index not found

### Requirement: Automatic Maintenance Scheduling
The system SHALL support configurable automatic maintenance tasks.

#### Scenario: Configure Auto-Compaction
- **WHEN** client calls `index_configure` with `auto_compact` settings
- **THEN** system SHALL automatically compact the index when:
  - Deleted document ratio exceeds threshold (default: 20%)
  - Number of segments exceeds limit (default: 10)
  - On schedule (e.g., daily at 2 AM)

#### Scenario: Configure Auto-Expiration
- **WHEN** an index has TTL configured
- **THEN** system SHALL automatically run expiration cleanup:
  - On startup (once)
  - Periodically (configurable interval, default: 1 hour)

#### Scenario: Maintenance Status Query
- **WHEN** client calls `maintenance_status`
- **THEN** system SHALL return:
  - Next scheduled maintenance time
  - Last maintenance execution time and results
  - Pending maintenance tasks

### Requirement: Error Handling
The system SHALL provide clear error messages for all failure conditions.

#### Scenario: Invalid JSON Input
- **WHEN** client sends malformed JSON
- **THEN** system SHALL return JSON-RPC parse error
- **AND** include error details in response

#### Scenario: Missing Required Parameter
- **WHEN** client calls a tool without required parameters
- **THEN** system SHALL return invalid params error
- **AND** specify which parameter is missing

#### Scenario: Index Write Failure
- **WHEN** disk write fails during document indexing
- **THEN** system SHALL return internal error
- **AND** preserve index consistency (no partial writes)

#### Scenario: Maintenance Operation Failure
- **WHEN** a maintenance operation fails (compaction, backup, etc.)
- **THEN** system SHALL preserve the original index state
- **AND** return detailed error information
- **AND** log the failure for debugging
