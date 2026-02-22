//! Provider middleware implementations using the decorator pattern.
//!
//! Each middleware is a struct that wraps an `Arc<dyn LlmProvider>` and
//! implements `LlmProvider` itself, enabling static composition:
//!
//! ```ignore
//! let provider = LoggingProvider::wrap(
//!     MetricsProvider::wrap(base_provider)
//! );
//! ```

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tracing::{debug, instrument, warn};

use crate::providers::{ChatRequest, ChatResponse, LlmProvider};

// ── Logging ──────────────────────────────────────────────

/// Logs provider requests and responses at debug level.
pub struct LoggingProvider {
    inner: Arc<dyn LlmProvider>,
}

impl LoggingProvider {
    /// Wrap a provider with logging.
    pub fn wrap(inner: Arc<dyn LlmProvider>) -> Arc<dyn LlmProvider> {
        Arc::new(Self { inner })
    }
}

#[async_trait]
impl LlmProvider for LoggingProvider {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn default_model(&self) -> &str {
        self.inner.default_model()
    }

    #[instrument(name = "provider.logging", skip_all)]
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse> {
        debug!(
            model = %request.model,
            messages = request.messages.len(),
            tools = request.tools.as_ref().map_or(0, |t| t.len()),
            "Provider request"
        );

        let start = Instant::now();
        let result = self.inner.chat(request).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(response) => {
                debug!(
                    elapsed_ms = elapsed.as_millis() as u64,
                    has_content = response.content.is_some(),
                    tool_calls = response.tool_calls.len(),
                    "Provider response"
                );
            }
            Err(e) => {
                warn!(
                    elapsed_ms = elapsed.as_millis() as u64,
                    error = %e,
                    "Provider error"
                );
            }
        }

        result
    }
}

// ── Metrics ──────────────────────────────────────────────

/// Aggregated provider metrics.
#[derive(Debug, Clone, Default)]
pub struct ProviderMetrics {
    pub total_calls: u64,
    pub total_errors: u64,
    pub total_tool_calls: u64,
    pub total_latency_ms: u64,
}

impl ProviderMetrics {
    /// Average latency per call in milliseconds.
    pub fn avg_latency_ms(&self) -> f64 {
        if self.total_calls == 0 {
            0.0
        } else {
            self.total_latency_ms as f64 / self.total_calls as f64
        }
    }
}

/// Records provider call metrics (latency, tool call counts).
pub struct MetricsProvider {
    inner: Arc<dyn LlmProvider>,
    metrics: Arc<std::sync::Mutex<ProviderMetrics>>,
}

impl MetricsProvider {
    /// Wrap a provider with metrics collection.
    ///
    /// Returns a tuple of `(provider, metrics_handle)` so the caller
    /// can query metrics later.
    pub fn wrap(inner: Arc<dyn LlmProvider>) -> (Arc<dyn LlmProvider>, MetricsHandle) {
        let metrics = Arc::new(std::sync::Mutex::new(ProviderMetrics::default()));
        let handle = MetricsHandle {
            metrics: metrics.clone(),
        };
        let provider = Arc::new(Self { inner, metrics });
        (provider, handle)
    }
}

/// Handle for querying metrics from a `MetricsProvider`.
#[derive(Clone)]
pub struct MetricsHandle {
    metrics: Arc<std::sync::Mutex<ProviderMetrics>>,
}

impl MetricsHandle {
    /// Get a snapshot of the current metrics.
    pub fn metrics(&self) -> ProviderMetrics {
        self.metrics.lock().unwrap().clone()
    }
}

#[async_trait]
impl LlmProvider for MetricsProvider {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn default_model(&self) -> &str {
        self.inner.default_model()
    }

    #[instrument(name = "provider.metrics", skip_all)]
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse> {
        let start = Instant::now();
        let result = self.inner.chat(request).await;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        let mut m = self.metrics.lock().unwrap();
        m.total_calls += 1;
        m.total_latency_ms += elapsed_ms;

        match &result {
            Ok(resp) => {
                m.total_tool_calls += resp.tool_calls.len() as u64;
            }
            Err(_) => {
                m.total_errors += 1;
            }
        }

        result
    }
}

// ── RateLimit ─────────────────────────────────────────────

/// Simple token-bucket rate limiter for provider calls.
///
/// Allows at most `max_calls` within any rolling `window` period.
/// Calls exceeding the limit are delayed (not rejected).
pub struct RateLimitProvider {
    inner: Arc<dyn LlmProvider>,
    max_calls: u32,
    window: std::time::Duration,
    timestamps: std::sync::Mutex<std::collections::VecDeque<Instant>>,
}

impl RateLimitProvider {
    /// Wrap a provider with rate limiting.
    pub fn wrap(
        inner: Arc<dyn LlmProvider>,
        max_calls: u32,
        window: std::time::Duration,
    ) -> Arc<dyn LlmProvider> {
        Arc::new(Self {
            inner,
            max_calls,
            window,
            timestamps: std::sync::Mutex::new(std::collections::VecDeque::new()),
        })
    }
}

#[async_trait]
impl LlmProvider for RateLimitProvider {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn default_model(&self) -> &str {
        self.inner.default_model()
    }

    #[instrument(name = "provider.rate_limit", skip_all)]
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse> {
        // Wait until we're within the rate limit
        loop {
            let should_wait = {
                let mut ts = self.timestamps.lock().unwrap();
                let now = Instant::now();

                // Remove expired timestamps
                while let Some(&front) = ts.front() {
                    if now.duration_since(front) > self.window {
                        ts.pop_front();
                    } else {
                        break;
                    }
                }

                if ts.len() < self.max_calls as usize {
                    ts.push_back(now);
                    false
                } else {
                    true
                }
            };

            if should_wait {
                debug!("Rate limit reached, waiting 100ms");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            } else {
                break;
            }
        }

        self.inner.chat(request).await
    }
}

// ── ProviderError ─────────────────────────────────────────

/// Structured error type for provider operations.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// The provider API returned an HTTP error.
    #[error("API error ({status}): {message}")]
    ApiError {
        status: u16,
        message: String,
        provider: String,
    },

    /// The API response could not be parsed.
    #[error("Parse error: {message}")]
    ParseError {
        message: String,
        body: Option<String>,
    },

    /// The request timed out.
    #[error("Timeout after {elapsed_ms}ms")]
    Timeout { elapsed_ms: u64, provider: String },

    /// Rate limit exceeded.
    #[error("Rate limited by {provider}")]
    RateLimited { provider: String },

    /// Authentication failure.
    #[error("Auth error for {provider}: {message}")]
    AuthError { provider: String, message: String },

    /// Other/unknown errors.
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl ProviderError {
    /// Get the provider name associated with this error (if any).
    pub fn provider(&self) -> Option<&str> {
        match self {
            Self::ApiError { provider, .. }
            | Self::Timeout { provider, .. }
            | Self::RateLimited { provider }
            | Self::AuthError { provider, .. } => Some(provider),
            _ => None,
        }
    }

    /// Whether this error is likely transient and retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Timeout { .. }
                | Self::RateLimited { .. }
                | Self::ApiError { status: 429, .. }
                | Self::ApiError {
                    status: 500..=599,
                    ..
                }
        )
    }
}

// ── ProviderBuilder ────────────────────────────────────────

/// Builder for composing provider decorators.
///
/// ```ignore
/// let provider = ProviderBuilder::new(base_provider)
///     .with_logging()
///     .with_metrics()
///     .build();
/// ```
pub struct ProviderBuilder {
    provider: Arc<dyn LlmProvider>,
    metrics_handle: Option<MetricsHandle>,
}

impl ProviderBuilder {
    /// Create a new builder wrapping the given base provider.
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            provider,
            metrics_handle: None,
        }
    }

    /// Add logging decorator.
    pub fn with_logging(self) -> Self {
        Self {
            provider: LoggingProvider::wrap(self.provider),
            metrics_handle: self.metrics_handle,
        }
    }

    /// Add metrics decorator. Call `metrics_handle()` after `build()` to
    /// retrieve the handle.
    pub fn with_metrics(self) -> Self {
        let (provider, handle) = MetricsProvider::wrap(self.provider);
        Self {
            provider,
            metrics_handle: Some(handle),
        }
    }

    /// Add rate limiting decorator.
    pub fn with_rate_limit(self, max_calls: u32, window: std::time::Duration) -> Self {
        Self {
            provider: RateLimitProvider::wrap(self.provider, max_calls, window),
            metrics_handle: self.metrics_handle,
        }
    }

    /// Build the final provider.
    pub fn build(self) -> Arc<dyn LlmProvider> {
        self.provider
    }

    /// Build and return both the provider and the metrics handle (if metrics were enabled).
    pub fn build_with_metrics(self) -> (Arc<dyn LlmProvider>, Option<MetricsHandle>) {
        (self.provider, self.metrics_handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ChatMessage, ChatRequest, ChatResponse};

    struct MockProvider;

    #[async_trait]
    impl LlmProvider for MockProvider {
        fn name(&self) -> &str {
            "mock"
        }
        fn default_model(&self) -> &str {
            "mock-model"
        }
        async fn chat(&self, _request: ChatRequest) -> anyhow::Result<ChatResponse> {
            Ok(ChatResponse::text("from-mock"))
        }
    }

    struct FailProvider;

    #[async_trait]
    impl LlmProvider for FailProvider {
        fn name(&self) -> &str {
            "fail"
        }
        fn default_model(&self) -> &str {
            "fail-model"
        }
        async fn chat(&self, _request: ChatRequest) -> anyhow::Result<ChatResponse> {
            anyhow::bail!("mock error")
        }
    }

    fn make_request() -> ChatRequest {
        ChatRequest {
            model: "test-model".to_string(),
            messages: vec![ChatMessage::user("hello")],
            tools: None,
            temperature: None,
            max_tokens: None,
        }
    }

    #[tokio::test]
    async fn test_logging_provider() {
        let provider = LoggingProvider::wrap(Arc::new(MockProvider));
        let result = provider.chat(make_request()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, Some("from-mock".to_string()));
    }

    #[tokio::test]
    async fn test_metrics_provider_success() {
        let (provider, handle) = MetricsProvider::wrap(Arc::new(MockProvider));
        let _ = provider.chat(make_request()).await;

        let m = handle.metrics();
        assert_eq!(m.total_calls, 1);
        assert_eq!(m.total_errors, 0);
    }

    #[tokio::test]
    async fn test_metrics_provider_error() {
        let (provider, handle) = MetricsProvider::wrap(Arc::new(FailProvider));
        let _ = provider.chat(make_request()).await;

        let m = handle.metrics();
        assert_eq!(m.total_calls, 1);
        assert_eq!(m.total_errors, 1);
    }

    #[tokio::test]
    async fn test_rate_limit_provider() {
        let provider = RateLimitProvider::wrap(
            Arc::new(MockProvider),
            2,
            std::time::Duration::from_secs(1),
        );

        let r1 = provider.chat(make_request()).await;
        let r2 = provider.chat(make_request()).await;
        assert!(r1.is_ok());
        assert!(r2.is_ok());
    }

    #[tokio::test]
    async fn test_provider_builder() {
        let (provider, metrics_handle) = ProviderBuilder::new(Arc::new(MockProvider))
            .with_logging()
            .with_metrics()
            .build_with_metrics();

        assert_eq!(provider.name(), "mock");
        let resp = provider.chat(make_request()).await.unwrap();
        assert_eq!(resp.content, Some("from-mock".to_string()));

        let m = metrics_handle.unwrap().metrics();
        assert_eq!(m.total_calls, 1);
    }

    #[test]
    fn test_provider_error_retryable() {
        let err = ProviderError::RateLimited {
            provider: "openai".to_string(),
        };
        assert!(err.is_retryable());
        assert_eq!(err.provider(), Some("openai"));

        let err = ProviderError::ApiError {
            status: 400,
            message: "bad request".to_string(),
            provider: "openai".to_string(),
        };
        assert!(!err.is_retryable());

        let err = ProviderError::ApiError {
            status: 503,
            message: "service unavailable".to_string(),
            provider: "openai".to_string(),
        };
        assert!(err.is_retryable());
    }
}
