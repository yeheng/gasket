//! SQLite store: documents + chunks + sqlite-vec vec0 KNN, single file.

use std::path::{Path, PathBuf};

use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

/// Search hit, `score` = 1 − cosine distance (higher is better).
#[derive(Debug, Clone)]
pub struct Hit {
    pub source: String,
    pub path: String,
    pub ordinal: usize,
    pub content: String,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct DocRow {
    pub path: PathBuf,
    pub mtime: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceStat {
    pub source: String,
    pub docs: i64,
    pub chunks: i64,
}

pub struct Store {
    pool: SqlitePool,
}

/// Entry-point prototype `sqlite3_auto_extension` expects.
type AutoExtensionEntry = unsafe extern "C" fn(
    *mut libsqlite3_sys::sqlite3,
    *mut *mut std::os::raw::c_char,
    *const libsqlite3_sys::sqlite3_api_routines,
) -> std::os::raw::c_int;

/// Register the sqlite-vec extension exactly once per process, BEFORE any
/// connection is opened (sqlite3_auto_extension applies to future opens).
/// Relies on cargo resolving this `libsqlite3_sys` to the same crate
/// instance sqlx links against — see the workspace Cargo.toml note.
fn register_vec_once() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        // Safety: documented sqlite-vec registration pattern — the extension
        // init function has the auto-extension entry-point ABI; transmuting
        // the fn pointer makes every later-opened connection load vec0.
        libsqlite3_sys::sqlite3_auto_extension(Some(std::mem::transmute::<
            *const (),
            AutoExtensionEntry,
        >(
            sqlite_vec::sqlite3_vec_init as *const ()
        )));
    });
}

/// f32 slice → little-endian byte blob (sqlite-vec binary format).
fn f32_le_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS meta(
     key TEXT PRIMARY KEY, value TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS documents(
     id INTEGER PRIMARY KEY,
     source TEXT NOT NULL,
     path TEXT NOT NULL,
     mtime INTEGER NOT NULL,
     content_hash TEXT NOT NULL,
     chunk_count INTEGER NOT NULL,
     updated_at INTEGER NOT NULL,
     UNIQUE(source, path));
 CREATE TABLE IF NOT EXISTS chunks(
     rowid INTEGER PRIMARY KEY,
     doc_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
     ordinal INTEGER NOT NULL,
     content TEXT NOT NULL);";

impl Store {
    pub async fn open(path: &Path) -> anyhow::Result<Store> {
        register_vec_once();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建库目录失败 {}", parent.display()))?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path.to_path_buf())
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        // One connection: the store is used by a single sequential pipeline
        // (ingest or search), so there is nothing to gain from a pool and
        // no SQLITE_BUSY contention to tolerate.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .with_context(|| format!("打开库失败 {}", path.display()))?;
        sqlx::raw_sql(SCHEMA).execute(&pool).await?;
        Ok(Store { pool })
    }

    async fn meta_get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(
            sqlx::query_scalar::<_, String>("SELECT value FROM meta WHERE key = ?1")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    async fn meta_set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO meta(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn fingerprint(&self) -> Option<(String, usize)> {
        let model = self.meta_get("embedding_model").await.ok()??;
        let dim: usize = self.meta_get("embedding_dim").await.ok()??.parse().ok()?;
        Some((model, dim))
    }

    pub async fn ensure_vec(&self, dim: usize, model: &str) -> anyhow::Result<()> {
        if let Some((m, d)) = self.fingerprint().await {
            anyhow::ensure!(
                m == model && d == dim,
                "embedding 指纹变更:库中 {m}[{d}],请求 {model}[{dim}]。请使用 --rebuild 重建索引"
            );
            return Ok(());
        }
        // Audited dynamic SQL: `dim` is a usize interpolated into the
        // vec0 table declaration; no user input reaches this string.
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(embedding float[{dim}] distance_metric=cosine)"
        )))
        .execute(&self.pool)
        .await?;
        self.meta_set("embedding_model", model).await?;
        self.meta_set("embedding_dim", &dim.to_string()).await?;
        Ok(())
    }

    async fn has_vec_table(&self) -> bool {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='vec_chunks'",
        )
        .fetch_one(&self.pool)
        .await
        .map(|n| n > 0)
        .unwrap_or(false)
    }

    pub async fn chunk_count(&self) -> anyhow::Result<i64> {
        if !self.has_vec_table().await {
            return Ok(0);
        }
        Ok(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM vec_chunks")
                .fetch_one(&self.pool)
                .await?,
        )
    }

    pub async fn docs_for_source(&self, source: &str) -> anyhow::Result<Vec<DocRow>> {
        let rows: Vec<(String, i64, String)> =
            sqlx::query_as("SELECT path, mtime, content_hash FROM documents WHERE source = ?1")
                .bind(source)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows
            .into_iter()
            .map(|(path, mtime, content_hash)| DocRow {
                path: PathBuf::from(path),
                mtime,
                content_hash,
            })
            .collect())
    }

    pub async fn touch_mtime(&self, source: &str, path: &Path, mtime: i64) -> anyhow::Result<()> {
        sqlx::query("UPDATE documents SET mtime = ?1 WHERE source = ?2 AND path = ?3")
            .bind(mtime)
            .bind(source)
            .bind(path.to_string_lossy())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Document-level upsert in ONE transaction: delete old rows (chunks via
    /// FK cascade, vectors explicitly), then insert fresh.
    pub async fn upsert_doc(
        &self,
        source: &str,
        path: &Path,
        mtime: i64,
        hash: &str,
        chunks: &[(usize, String, Vec<f32>)],
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        let p = path.to_string_lossy();
        sqlx::query(
            "DELETE FROM vec_chunks WHERE rowid IN (
                 SELECT c.rowid FROM chunks c JOIN documents d ON d.id = c.doc_id
                 WHERE d.source = ?1 AND d.path = ?2)",
        )
        .bind(source)
        .bind(&*p)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM documents WHERE source = ?1 AND path = ?2")
            .bind(source)
            .bind(&*p)
            .execute(&mut *tx)
            .await?;
        let doc_res = sqlx::query(
            "INSERT INTO documents(source, path, mtime, content_hash, chunk_count, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(source)
        .bind(&*p)
        .bind(mtime)
        .bind(hash)
        .bind(chunks.len() as i64)
        .bind(chrono::Utc::now().timestamp())
        .execute(&mut *tx)
        .await?;
        let doc_id = doc_res.last_insert_rowid();
        for (ordinal, content, embedding) in chunks {
            let chunk_res =
                sqlx::query("INSERT INTO chunks(doc_id, ordinal, content) VALUES(?1, ?2, ?3)")
                    .bind(doc_id)
                    .bind(*ordinal as i64)
                    .bind(content)
                    .execute(&mut *tx)
                    .await?;
            let rowid = chunk_res.last_insert_rowid();
            sqlx::query("INSERT INTO vec_chunks(rowid, embedding) VALUES(?1, ?2)")
                .bind(rowid)
                .bind(f32_le_blob(embedding))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Delete documents of `source` whose path is not in `live`. Returns count.
    pub async fn remove_missing(&self, source: &str, live: &[PathBuf]) -> anyhow::Result<usize> {
        let mut tx = self.pool.begin().await?;
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, path FROM documents WHERE source = ?1")
                .bind(source)
                .fetch_all(&mut *tx)
                .await?;
        let mut removed = 0;
        for (id, path) in rows {
            if !live.iter().any(|p| p.to_string_lossy() == path) {
                sqlx::query(
                    "DELETE FROM vec_chunks WHERE rowid IN (
                         SELECT c.rowid FROM chunks c WHERE c.doc_id = ?1)",
                )
                .bind(id)
                .execute(&mut *tx)
                .await?;
                sqlx::query("DELETE FROM documents WHERE id = ?1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                removed += 1;
            }
        }
        tx.commit().await?;
        Ok(removed)
    }

    /// KNN over vec0. Errors when the index is empty (no vec table).
    pub async fn knn(
        &self,
        query: &[f32],
        k: usize,
        source: Option<&str>,
    ) -> anyhow::Result<Vec<Hit>> {
        if !self.has_vec_table().await {
            anyhow::bail!("索引为空:请先运行 conga-rag ingest");
        }
        // Source filter is bound as ?3 (never string-interpolated). vec0
        // pre-filters KNN candidates via `rowid IN (SELECT ...)`.
        let (filter, src): (&str, Option<&str>) = match source {
            Some(s) => (
                " AND rowid IN (SELECT c.rowid FROM chunks c \
                 JOIN documents d ON d.id = c.doc_id WHERE d.source = ?3)",
                Some(s),
            ),
            None => ("", None),
        };
        let sql = format!(
            "SELECT k.rowid, k.distance, d.source, d.path, c.ordinal, c.content
             FROM (SELECT rowid, distance FROM vec_chunks
                   WHERE embedding MATCH ?1 AND k = ?2{filter}) k
             JOIN chunks c ON c.rowid = k.rowid
             JOIN documents d ON d.id = c.doc_id
             ORDER BY k.distance"
        );
        // Audited dynamic SQL: `filter` is a static fragment; user input
        // only ever travels through bound parameters (?1..?3).
        let rows: Vec<(i64, f64, String, String, i64, String)> = match src {
            Some(s) => {
                sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
                    .bind(f32_le_blob(query))
                    .bind(k as i64)
                    .bind(s)
                    .fetch_all(&self.pool)
                    .await?
            }
            None => {
                sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
                    .bind(f32_le_blob(query))
                    .bind(k as i64)
                    .fetch_all(&self.pool)
                    .await?
            }
        };
        Ok(rows
            .into_iter()
            .map(|(_, distance, source, path, ordinal, content)| Hit {
                score: 1.0 - distance,
                source,
                path,
                ordinal: ordinal as usize,
                content,
            })
            .collect())
    }

    pub async fn stats(&self) -> anyhow::Result<Vec<SourceStat>> {
        let rows: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT d.source, count(DISTINCT d.id), count(c.rowid)
             FROM documents d LEFT JOIN chunks c ON c.doc_id = d.id
             GROUP BY d.source ORDER BY d.source",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(source, docs, chunks)| SourceStat {
                source,
                docs,
                chunks,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("t.db")).await.unwrap();
        (dir, s)
    }

    fn dim4(v: [f32; 4]) -> Vec<f32> {
        v.to_vec()
    }

    #[tokio::test]
    async fn open_creates_schema() {
        let (_d, s) = store().await;
        assert_eq!(s.chunk_count().await.unwrap(), 0);
        assert!(s.fingerprint().await.is_none());
    }

    #[tokio::test]
    async fn ensure_vec_is_idempotent_and_locks_fingerprint() {
        let (_d, s) = store().await;
        s.ensure_vec(4, "m1").await.unwrap();
        s.ensure_vec(4, "m1").await.unwrap(); // idempotent
        assert_eq!(s.fingerprint().await.unwrap(), ("m1".to_string(), 4));
        let err = s.ensure_vec(8, "m2").await.unwrap_err();
        assert!(err.to_string().contains("--rebuild"), "err: {err}");
    }

    #[tokio::test]
    async fn upsert_and_knn_roundtrip() {
        let (_d, s) = store().await;
        s.ensure_vec(4, "m1").await.unwrap();
        s.upsert_doc(
            "src",
            Path::new("/n/a.md"),
            1,
            "h1",
            &[
                (0, "alpha content".into(), dim4([1.0, 0.0, 0.0, 0.0])),
                (1, "beta content".into(), dim4([0.0, 1.0, 0.0, 0.0])),
            ],
        )
        .await
        .unwrap();
        s.upsert_doc(
            "src",
            Path::new("/n/b.md"),
            1,
            "h2",
            &[(0, "gamma content".into(), dim4([0.0, 0.0, 1.0, 0.0]))],
        )
        .await
        .unwrap();

        let hits = s.knn(&dim4([1.0, 0.0, 0.0, 0.0]), 2, None).await.unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].path, "/n/a.md");
        assert_eq!(hits[0].ordinal, 0);
        assert_eq!(hits[0].content, "alpha content");
        assert!(
            hits[0].score > 0.99,
            "identical vector → score≈1: {}",
            hits[0].score
        );
        assert!(hits[0].score >= hits[1].score);

        // source filter narrows results
        let _ = s
            .upsert_doc(
                "other",
                Path::new("/n/c.md"),
                1,
                "h3",
                &[(0, "delta content".into(), dim4([1.0, 0.0, 0.0, 0.0]))],
            )
            .await
            .unwrap();
        let only = s
            .knn(&dim4([1.0, 0.0, 0.0, 0.0]), 5, Some("other"))
            .await
            .unwrap();
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].source, "other");
    }

    #[tokio::test]
    async fn upsert_replaces_previous_chunks() {
        let (_d, s) = store().await;
        s.ensure_vec(4, "m1").await.unwrap();
        s.upsert_doc(
            "src",
            Path::new("/n/a.md"),
            1,
            "h1",
            &[(0, "old".into(), dim4([1.0, 0.0, 0.0, 0.0]))],
        )
        .await
        .unwrap();
        s.upsert_doc(
            "src",
            Path::new("/n/a.md"),
            2,
            "h1b",
            &[
                (0, "new".into(), dim4([0.0, 0.9, 0.0, 0.0])),
                (1, "new2".into(), dim4([0.0, 0.9, 0.1, 0.0])),
            ],
        )
        .await
        .unwrap();
        let hits = s.knn(&dim4([1.0, 0.0, 0.0, 0.0]), 10, None).await.unwrap();
        assert_eq!(hits.len(), 2, "old vector must be gone");
        assert!(hits.iter().all(|h| h.content.starts_with("new")));
        assert_eq!(s.chunk_count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn remove_missing_deletes_only_absent() {
        let (_d, s) = store().await;
        s.ensure_vec(4, "m1").await.unwrap();
        s.upsert_doc(
            "src",
            Path::new("/n/a.md"),
            1,
            "h1",
            &[(0, "a".into(), dim4([1.0, 0.0, 0.0, 0.0]))],
        )
        .await
        .unwrap();
        s.upsert_doc(
            "src",
            Path::new("/n/gone.md"),
            1,
            "h2",
            &[(0, "g".into(), dim4([0.0, 1.0, 0.0, 0.0]))],
        )
        .await
        .unwrap();
        let removed = s
            .remove_missing("src", &[PathBuf::from("/n/a.md")])
            .await
            .unwrap();
        assert_eq!(removed, 1);
        assert_eq!(s.chunk_count().await.unwrap(), 1);
        let rows = s.docs_for_source("src").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, Path::new("/n/a.md"));
    }

    #[tokio::test]
    async fn knn_without_vec_table_is_error() {
        let (_d, s) = store().await;
        let err = s.knn(&dim4([0.0; 4]), 3, None).await.unwrap_err();
        assert!(err.to_string().contains("ingest"), "err: {err}");
    }

    #[tokio::test]
    async fn stats_groups_by_source() {
        let (_d, s) = store().await;
        s.ensure_vec(4, "m1").await.unwrap();
        s.upsert_doc(
            "a",
            Path::new("/x/1"),
            1,
            "h",
            &[(0, "c".into(), dim4([1.0, 0.0, 0.0, 0.0]))],
        )
        .await
        .unwrap();
        s.upsert_doc(
            "b",
            Path::new("/x/2"),
            1,
            "h",
            &[
                (0, "c".into(), dim4([0.0, 1.0, 0.0, 0.0])),
                (1, "c2".into(), dim4([0.0, 0.0, 1.0, 0.0])),
            ],
        )
        .await
        .unwrap();
        let st = s.stats().await.unwrap();
        assert_eq!(st.len(), 2);
        let b = st.iter().find(|x| x.source == "b").unwrap();
        assert_eq!((b.docs, b.chunks), (1, 2));
    }

    #[tokio::test]
    async fn touch_mtime_updates_row() {
        let (_d, s) = store().await;
        s.ensure_vec(4, "m1").await.unwrap();
        s.upsert_doc(
            "s",
            Path::new("/n/a"),
            10,
            "h",
            &[(0, "c".into(), dim4([1.0, 0.0, 0.0, 0.0]))],
        )
        .await
        .unwrap();
        s.touch_mtime("s", Path::new("/n/a"), 99).await.unwrap();
        let rows = s.docs_for_source("s").await.unwrap();
        assert_eq!(rows[0].mtime, 99);
        assert_eq!(rows[0].content_hash, "h");
    }
}
