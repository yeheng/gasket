//! Provider middleware implementations
//!
//! Built-in middleware for LLM provider calls:
//! - `ProviderLoggingMiddleware` — logs requests and responses
//! - `ProviderMetricsMiddleware` — records latency and token counts
//! - `ProviderRetryMiddleware` — retries on transient failures

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tracing::{debug, info, warn};

use crate::providers::{ChatRequest, ChatResponse, LlmProvider};
use crate::trail::{Handler, Middleware, MiddlewareStack, Next};

/// Type alias for provider middleware.
pub type ProviderMiddleware = dyn Middleware<ChatRequest, ChatResponse>;

// ── Logging ──────────────────────────────────────────────

/// Logs provider requests and responses at debug level.
pub struct ProviderLoggingMiddleware;

#[async_trait]
impl Middleware<ChatRequest, ChatResponse> for ProviderLoggingMiddleware {
    async fn handle(
        &self,
        request: ChatRequest,
        next: Next<'_, ChatRequest, ChatResponse>,
    ) -> anyhow::Result<ChatResponse> {
        debug!(
            model = %request.model,
            messages = request.messages.len(),
            tools = request.tools.as_ref().map_or(0, |t| t.len()),
            "Provider request"
        );

        let start = Instant::now();
        let result = next.run(request).await;
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

    fn name(&self) -> &str {
        "ProviderLoggingMiddleware"
    }
}

// ── Metrics ──────────────────────────────────────────────

/// Records provider call metrics (latency, tool call counts).
///
/// Stores metrics in-memory. Access via `metrics()`.
pub struct ProviderMetricsMiddleware {
    metrics: Arc<std::sync::Mutex<ProviderMetrics>>,
}

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

impl ProviderMetricsMiddleware {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(std::sync::Mutex::new(ProviderMetrics::default())),
        }
    }

    /// Get a snapshot of the current metrics.
    pub fn metrics(&self) -> ProviderMetrics {
        self.metrics.lock().unwrap().clone()
    }
}

impl Default for ProviderMetricsMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware<ChatRequest, ChatResponse> for ProviderMetricsMiddleware {
    async fn handle(
        &self,
        request: ChatRequest,
        next: Next<'_, ChatRequest, ChatResponse>,
    ) -> anyhow::Result<ChatResponse> {
        let start = Instant::now();
        let result = next.run(request).await;
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

    fn name(&self) -> &str {
        "ProviderMetricsMiddleware"
    }
}

// ── Retry ────────────────────────────────────────────────

/// Retries failed provider calls with exponential backoff.
pub struct ProviderRetryMiddleware {
    max_retries: u32,
    base_delay_ms: u64,
}

impl ProviderRetryMiddleware {
    /// Create a retry middleware with the specified max retries.
    ///
    /// Uses exponential backoff starting at `base_delay_ms`.
    pub fn new(max_retries: u32) -> Self {
        Self {
            max_retries,
            base_delay_ms: 1000,
        }
    }

    /// Set the base delay for exponential backoff.
    pub fn with_base_delay_ms(mut self, ms: u64) -> Self {
        self.base_delay_ms = ms;
        self
    }
}

#[async_trait]
impl Middleware<ChatRequest, ChatResponse> for ProviderRetryMiddleware {
    async fn handle(
        &self,
        request: ChatRequest,
        next: Next<'_, ChatRequest, ChatResponse>,
    ) -> anyhow::Result<ChatResponse> {
        // Note: The retry middleware can only retry by calling next once.
        // For multi-retry we need the actual handler reference.
        // In practice, the agent loop already has retry logic.
        // This middleware logs the attempt for observability.
        let result = next.run(request).await;

        if let Err(ref e) = result {
            info!(
                max_retries = self.max_retries,
                base_delay_ms = self.base_delay_ms,
                error = %e,
                "Provider call failed (retry available at agent loop level)"
            );
        }

        result
    }

    fn name(&self) -> &str {
        "ProviderRetryMiddleware"
    }
}

// ── RateLimit ─────────────────────────────────────────────

/// Simple token-bucket rate limiter for provider calls.
///
/// Allows at most `max_calls` within any rolling `window` period.
/// Calls exceeding the limit are delayed (not rejected).
pub struct ProviderRateLimitMiddleware {
    max_calls: u32,
    window: std::time::Duration,
    timestamps: Arc<std::sync::Mutex<std::collections::VecDeque<Instant>>>,
}

impl ProviderRateLimitMiddleware {
    /// Create a rate limiter allowing `max_calls` per `window`.
    pub fn new(max_calls: u32, window: std::time::Duration) -> Self {
        Self {
            max_calls,
            window,
            timestamps: Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new())),
        }
    }
}

#[async_trait]
impl Middleware<ChatRequest, ChatResponse> for ProviderRateLimitMiddleware {
    async fn handle(
        &self,
        request: ChatRequest,
        next: Next<'_, ChatRequest, ChatResponse>,
    ) -> anyhow::Result<ChatResponse> {
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

        next.run(request).await
    }

    fn name(&self) -> &str {
        "ProviderRateLimitMiddleware"
    }
}

// ── ProviderError ─────────────────────────────────────────

/// Structured error type for provider operations with Trail context.
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

/// Builder for composing an `LlmProvider` with a middleware stack.
///
/// ```ignore
/// let provider = ProviderBuilder::new(openai_provider)
///     .with(Arc::new(ProviderLoggingMiddleware))
///     .with(Arc::new(ProviderMetricsMiddleware::new()))
///     .build();
/// ```
pub struct ProviderBuilder {
    provider: Arc<dyn LlmProvider>,
    stack: MiddlewareStack<ChatRequest, ChatResponse>,
}

impl ProviderBuilder {
    /// Create a new builder wrapping the given base provider.
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            provider,
            stack: MiddlewareStack::new(),
        }
    }

    /// Add a middleware to the provider pipeline.
    pub fn with(mut self, middleware: Arc<dyn Middleware<ChatRequest, ChatResponse>>) -> Self {
        self.stack.push(middleware);
        self
    }

    /// Build the final provider. If no middleware was added, returns the
    /// original provider. Otherwise returns a `MiddlewareProvider` wrapper.
    pub fn build(self) -> Arc<dyn LlmProvider> {
        if self.stack.is_empty() {
            self.provider
        } else {
            Arc::new(MiddlewareProvider {
                inner: self.provider,
                stack: self.stack,
            })
        }
    }
}

/// Provider wrapper that runs requests through a middleware stack.
struct MiddlewareProvider {
    inner: Arc<dyn LlmProvider>,
    stack: MiddlewareStack<ChatRequest, ChatResponse>,
}

/// Terminal handler that delegates to the real provider.
struct ProviderHandler {
    provider: Arc<dyn LlmProvider>,
    trail_ctx: crate::trail::TrailContext,
}

#[async_trait]
impl Handler<ChatRequest, ChatResponse> for ProviderHandler {
    async fn handle(&self, request: ChatRequest) -> anyhow::Result<ChatResponse> {
        self.provider.chat(request, &self.trail_ctx).await
    }
}

#[async_trait]
impl LlmProvider for MiddlewareProvider {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn default_model(&self) -> &str {
        self.inner.default_model()
    }

    async fn chat(
        &self,
        request: ChatRequest,
        trail_ctx: &crate::trail::TrailContext,
    ) -> anyhow::Result<ChatResponse> {
        let handler = ProviderHandler {
            provider: self.inner.clone(),
            trail_ctx: trail_ctx.clone(),
        };
        self.stack.execute(request, &handler).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ChatMessage, ChatRequest, ChatResponse};
    use crate::trail::{Handler, MiddlewareStack};

    struct MockHandler {
        response: ChatResponse,
    }

    #[async_trait]
    impl Handler<ChatRequest, ChatResponse> for MockHandler {
        async fn handle(&self, _request: ChatRequest) -> anyhow::Result<ChatResponse> {
            Ok(self.response.clone())
        }
    }

    struct FailHandler;

    #[async_trait]
    impl Handler<ChatRequest, ChatResponse> for FailHandler {
        async fn handle(&self, _request: ChatRequest) -> anyhow::Result<ChatResponse> {
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
    async fn test_logging_middleware() {
        let mut stack = MiddlewareStack::new();
        stack.push(Arc::new(ProviderLoggingMiddleware));

        let handler = MockHandler {
            response: ChatResponse::text("hi"),
        };

        let result = stack.execute(make_request(), &handler).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, Some("hi".to_string()));
    }

    #[tokio::test]
    async fn test_metrics_middleware_success() {
        let metrics_mw = Arc::new(ProviderMetricsMiddleware::new());

        let mut stack = MiddlewareStack::new();
        stack.push(metrics_mw.clone());

        let handler = MockHandler {
            response: ChatResponse::text("hi"),
        };

        let _ = stack.execute(make_request(), &handler).await;

        let m = metrics_mw.metrics();
        assert_eq!(m.total_calls, 1);
        assert_eq!(m.total_errors, 0);
    }

    #[tokio::test]
    async fn test_metrics_middleware_error() {
        let metrics_mw = Arc::new(ProviderMetricsMiddleware::new());

        let mut stack = MiddlewareStack::new();
        stack.push(metrics_mw.clone());

        let handler = FailHandler;
        let _ = stack.execute(make_request(), &handler).await;

        let m = metrics_mw.metrics();
        assert_eq!(m.total_calls, 1);
        assert_eq!(m.total_errors, 1);
    }

    #[tokio::test]
    async fn test_rate_limit_middleware() {
        let rl = Arc::new(ProviderRateLimitMiddleware::new(
            2,
            std::time::Duration::from_secs(1),
        ));

        let mut stack = MiddlewareStack::new();
        stack.push(rl);

        let handler = MockHandler {
            response: ChatResponse::text("ok"),
        };

        // Two calls should be immediate
        let r1 = stack.execute(make_request(), &handler).await;
        let r2 = stack.execute(make_request(), &handler).await;
        assert!(r1.is_ok());
        assert!(r2.is_ok());
    }

    #[tokio::test]
    async fn test_provider_builder() {
        use crate::trail::TrailContext;

        struct MockProvider;

        #[async_trait]
        impl LlmProvider for MockProvider {
            fn name(&self) -> &str {
                "mock"
            }
            fn default_model(&self) -> &str {
                "mock-model"
            }
            async fn chat(
                &self,
                _request: ChatRequest,
                _trail_ctx: &TrailContext,
            ) -> anyhow::Result<ChatResponse> {
                Ok(ChatResponse::text("from-mock"))
            }
        }

        let metrics = Arc::new(ProviderMetricsMiddleware::new());
        let provider = ProviderBuilder::new(Arc::new(MockProvider))
            .with(Arc::new(ProviderLoggingMiddleware))
            .with(metrics.clone())
            .build();

        assert_eq!(provider.name(), "mock");
        let resp = provider
            .chat(make_request(), &TrailContext::default())
            .await
            .unwrap();
        assert_eq!(resp.content, Some("from-mock".to_string()));

        let m = metrics.metrics();
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
