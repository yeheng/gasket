use conga_rag::testsupport::spawn_mock_embeddings;

use conga::RetryPolicy;
use conga_rag::config::ResolvedEmbedding;
use conga_rag::embed::EmbeddingsClient;

fn cfg() -> ResolvedEmbedding {
    ResolvedEmbedding {
        base_url: "unused".into(),
        api_key: "k".into(),
        model: "mock".into(),
        batch: 2,
        min_interval_ms: 0,
    }
}

fn fast_retry() -> RetryPolicy {
    RetryPolicy {
        max_retries: 3,
        initial_delay_ms: 10,
        max_delay_ms: 50,
        jitter: false,
    }
}

#[tokio::test]
async fn batches_are_split_by_config() {
    let (base, requests) = spawn_mock_embeddings(0).await;
    let client = EmbeddingsClient::with_retry(&cfg(), &base, fast_retry());
    let texts: Vec<String> = (0..5).map(|i| format!("doc text {i}")).collect();
    let out = client.embed_batch(&texts).await.unwrap();
    assert_eq!(out.len(), 5);
    assert!(out.iter().all(|v| v.len() == 4));
    assert_eq!(
        requests.load(std::sync::atomic::Ordering::SeqCst),
        3,
        "5 texts / batch 2 → 3 requests"
    );
}

#[tokio::test]
async fn min_interval_paces_consecutive_batches() {
    // 5 texts / batch 2 → 3 requests with 2 gaps of 120ms: pacing must
    // space them while leaving the request count and results untouched.
    let (base, requests) = spawn_mock_embeddings(0).await;
    let mut c = cfg();
    c.min_interval_ms = 120;
    let client = EmbeddingsClient::with_retry(&c, &base, fast_retry());
    let texts: Vec<String> = (0..5).map(|i| format!("doc text {i}")).collect();
    let started = std::time::Instant::now();
    let out = client.embed_batch(&texts).await.unwrap();
    let elapsed = started.elapsed();
    assert_eq!(out.len(), 5);
    assert_eq!(
        requests.load(std::sync::atomic::Ordering::SeqCst),
        3,
        "pacing must not change the request count"
    );
    assert!(
        elapsed >= std::time::Duration::from_millis(2 * 120),
        "two inter-request gaps must be honored, elapsed {:?}",
        elapsed
    );
}

#[tokio::test]
async fn retry_on_429_then_success() {
    let (base, requests) = spawn_mock_embeddings(1).await;
    let client = EmbeddingsClient::with_retry(&cfg(), &base, fast_retry());
    let out = client.embed_batch(&["one".to_string()]).await.unwrap();
    assert_eq!(out.len(), 1);
    assert!(
        requests.load(std::sync::atomic::Ordering::SeqCst) >= 2,
        "first 429 must be retried"
    );
}

#[tokio::test]
async fn non_retryable_error_propagates_fast() {
    // fail_first=999 keeps failing; retries exhaust and error surfaces
    let (base, requests) = spawn_mock_embeddings(999).await;
    let client = EmbeddingsClient::with_retry(&cfg(), &base, fast_retry());
    let err = client.embed_batch(&["one".to_string()]).await.unwrap_err();
    assert!(err.is_retryable() || err.to_string().contains("429"));
    assert!(requests.load(std::sync::atomic::Ordering::SeqCst) >= 2);
}

#[tokio::test]
async fn http_error_body_is_surfaced() {
    // A 400 must fail fast (no retry) and carry the server's detail message
    // (e.g. Ark's "input limit exceeded") instead of a bare status code.
    let (base, requests) = conga_rag::testsupport::spawn_mock_embeddings_error(
        400,
        r#"{"error":{"code":"InvalidParameter","message":"Embeddings API input limit exceeded: max 10, got 64"}}"#,
    )
    .await;
    let client = EmbeddingsClient::with_retry(&cfg(), &base, fast_retry());
    let err = client.embed_batch(&["one".to_string()]).await.unwrap_err();
    assert!(!err.is_retryable(), "400 must not be retried");
    let msg = err.to_string();
    assert!(msg.contains("400"), "status missing from: {msg}");
    assert!(
        msg.contains("input limit exceeded"),
        "server detail missing from: {msg}"
    );
    assert_eq!(
        requests.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "400 must not be retried"
    );
}
