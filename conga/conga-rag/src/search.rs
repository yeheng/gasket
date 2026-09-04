//! Retrieval: query embedding + KNN.

use crate::config::RagConfig;
use crate::embed::EmbeddingsClient;
use crate::store::{Hit, Store};

pub async fn run_search(
    cfg: &RagConfig,
    query: &str,
    k: usize,
    source: Option<&str>,
) -> anyhow::Result<Vec<Hit>> {
    let resolved = cfg.resolve_embedding()?;
    let client = EmbeddingsClient::new(&resolved);
    let qv = client.embed_query(query).await?;
    let store = Store::open(&cfg.store_path()).await?;
    let mut hits = store.knn(&qv, k, source).await?;
    // Present paths relative to their source root (spec §9: 相对路径).
    for h in &mut hits {
        if let Some(src) = cfg.sources.get(&h.source) {
            if let Ok(rel) = std::path::Path::new(&h.path).strip_prefix(&src.path) {
                h.path = rel
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/");
            }
        }
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn search_returns_ranked_hits() {
        let dir = tempfile::tempdir().unwrap();
        let dbdir = tempfile::tempdir().unwrap();
        let (base, _req) = crate::testsupport::spawn_mock_embeddings(0).await;
        let cfg =
            crate::pipeline::tests::cfg_with_store(dir.path(), &dbdir.path().join("t.db"), &base);
        crate::pipeline::run_ingest(&cfg, None, false)
            .await
            .unwrap();
        std::fs::write(dir.path().join("m.md"), "match target").unwrap();
        std::fs::write(dir.path().join("o.md"), "unrelated").unwrap();
        crate::pipeline::run_ingest(&cfg, None, false)
            .await
            .unwrap();

        let hits = run_search(&cfg, "match", 2, None).await.unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].path, "m.md", "Hit.path 是源根相对路径");
        assert!(hits[0].content.contains("match"));
    }

    /// Regression: rag_search opens the store per call while rag_remember /
    /// evolve ingest opens it again — in the app these overlap on tokio
    /// threads. Store::open must tolerate a concurrent writer instead of
    /// failing fast with SQLITE_BUSY.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_search_and_ingest_on_same_store_do_not_error() {
        let dir = tempfile::tempdir().unwrap();
        let dbdir = tempfile::tempdir().unwrap();
        let (base, _req) = crate::testsupport::spawn_mock_embeddings(0).await;
        let cfg =
            crate::pipeline::tests::cfg_with_store(dir.path(), &dbdir.path().join("t.db"), &base);
        std::fs::write(dir.path().join("seed.md"), "seed match").unwrap();
        crate::pipeline::run_ingest(&cfg, None, false)
            .await
            .unwrap();

        std::fs::write(dir.path().join("new.md"), "new match target").unwrap();
        let mut handles = Vec::new();
        for _ in 0..8 {
            let c = cfg.clone();
            handles.push(tokio::spawn(async move {
                run_search(&c, "match", 2, None).await
            }));
        }
        let c = cfg.clone();
        let ingest =
            tokio::spawn(async move { crate::pipeline::run_ingest(&c, None, false).await });
        for h in handles {
            h.await
                .unwrap()
                .expect("search must not fail under concurrency");
        }
        ingest
            .await
            .unwrap()
            .expect("ingest must not fail under concurrency");
    }
}
