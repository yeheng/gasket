//! History API for SqliteStore.

use chrono::Utc;
use tracing::debug;

use super::SqliteStore;

impl SqliteStore {
    /// Read all history entries, ordered by creation time (oldest first).
    pub async fn read_history(&self) -> anyhow::Result<String> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT content FROM history ORDER BY id ASC")
            .fetch_all(&self.pool)
            .await?;

        let mut result = String::new();
        for (content,) in rows {
            result.push_str(&content);
        }
        Ok(result)
    }

    /// Append a new history entry.
    pub async fn append_history(&self, content: &str) -> anyhow::Result<()> {
        let created_at = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO history (content, created_at) VALUES ($1, $2)")
            .bind(content)
            .bind(&created_at)
            .execute(&self.pool)
            .await?;
        debug!("Appended history entry");
        Ok(())
    }

    /// Write (replace) the entire history with new content.
    pub async fn write_history(&self, content: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM history")
            .execute(&self.pool)
            .await?;

        let created_at = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO history (content, created_at) VALUES ($1, $2)")
            .bind(content)
            .bind(&created_at)
            .execute(&self.pool)
            .await?;
        debug!("Wrote history");
        Ok(())
    }

    /// Clear all history entries.
    pub async fn clear_history(&self) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM history")
            .execute(&self.pool)
            .await?;
        debug!("Cleared history");
        Ok(())
    }
}
