//! `MemoryStore` trait implementation for SqliteStore (FTS5-backed search).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{QueryBuilder, Row, SqlitePool};
use tracing::debug;

use super::SqliteStore;
use crate::memory::store::{MemoryEntry, MemoryMetadata, MemoryQuery, MemoryStore};

#[async_trait]
impl MemoryStore for SqliteStore {
    async fn save(&self, entry: &MemoryEntry) -> anyhow::Result<()> {
        let metadata_json = serde_json::to_string(&entry.metadata)?;
        let created = entry.created_at.to_rfc3339();
        let updated = entry.updated_at.to_rfc3339();

        sqlx::query(
            "INSERT OR REPLACE INTO memories (id, content, metadata, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&entry.id)
        .bind(&entry.content)
        .bind(&metadata_json)
        .bind(&created)
        .bind(&updated)
        .execute(&self.pool)
        .await?;

        // Sync tags: delete old, insert new
        sqlx::query("DELETE FROM memory_tags WHERE memory_id = $1")
            .bind(&entry.id)
            .execute(&self.pool)
            .await?;
        for tag in &entry.metadata.tags {
            sqlx::query("INSERT INTO memory_tags (memory_id, tag) VALUES ($1, $2)")
                .bind(&entry.id)
                .bind(tag)
                .execute(&self.pool)
                .await?;
        }

        debug!("Saved memory entry: {}", entry.id);
        Ok(())
    }

    async fn get(&self, id: &str) -> anyhow::Result<Option<MemoryEntry>> {
        let row: Option<SqliteRow> = sqlx::query(
            "SELECT id, content, metadata, created_at, updated_at FROM memories WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(row_to_entry(&row)?)),
            None => Ok(None),
        }
    }

    async fn delete(&self, id: &str) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM memories WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn search(&self, query: &MemoryQuery) -> anyhow::Result<Vec<MemoryEntry>> {
        search_impl(&self.pool, query).await
    }
}

fn row_to_entry(row: &SqliteRow) -> anyhow::Result<MemoryEntry> {
    let id: String = row.get("id");
    let content: String = row.get("content");
    let metadata_json: String = row.get("metadata");
    let created_str: String = row.get("created_at");
    let updated_str: String = row.get("updated_at");

    let metadata: MemoryMetadata = serde_json::from_str(&metadata_json)?;
    let created_at = DateTime::parse_from_rfc3339(&created_str)?.with_timezone(&Utc);
    let updated_at = DateTime::parse_from_rfc3339(&updated_str)?.with_timezone(&Utc);

    Ok(MemoryEntry {
        id,
        content,
        metadata,
        created_at,
        updated_at,
    })
}

/// Build and execute the search query dynamically using `sqlx::QueryBuilder`.
async fn search_impl(pool: &SqlitePool, query: &MemoryQuery) -> anyhow::Result<Vec<MemoryEntry>> {
    let mut builder: QueryBuilder<'_, sqlx::Sqlite> = if let Some(text) = &query.text {
        let mut b = QueryBuilder::new(
            "SELECT m.id, m.content, m.metadata, m.created_at, m.updated_at \
             FROM memories m \
             JOIN memories_fts f ON m.id = f.id \
             WHERE f.content MATCH ",
        );
        b.push_bind(text.clone());
        b
    } else {
        QueryBuilder::new(
            "SELECT m.id, m.content, m.metadata, m.created_at, m.updated_at \
             FROM memories m WHERE 1=1",
        )
    };

    // Filter by source
    if let Some(source) = &query.source {
        builder.push(" AND json_extract(m.metadata, '$.source') = ");
        builder.push_bind(source.clone());
    }

    // Filter by tags (AND semantics: entry must have ALL tags)
    for tag in &query.tags {
        builder
            .push(" AND EXISTS (SELECT 1 FROM memory_tags t WHERE t.memory_id = m.id AND t.tag = ");
        builder.push_bind(tag.clone());
        builder.push(")");
    }

    // Order by updated_at descending for deterministic results
    builder.push(" ORDER BY m.updated_at DESC");

    // Limit / offset
    if let Some(limit) = query.limit {
        builder.push(" LIMIT ");
        builder.push_bind(limit as i64);
    }
    if let Some(offset) = query.offset {
        if query.limit.is_none() {
            builder.push(" LIMIT -1");
        }
        builder.push(" OFFSET ");
        builder.push_bind(offset as i64);
    }

    let rows = builder.build().fetch_all(pool).await?;

    let mut entries = Vec::with_capacity(rows.len());
    for row in &rows {
        entries.push(row_to_entry(row)?);
    }

    Ok(entries)
}
