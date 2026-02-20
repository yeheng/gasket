//! Tool middleware infrastructure
//!
//! Provides middleware types for wrapping tool executions,
//! plus built-in middlewares for logging, permission checks, and timeouts.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, warn};

use crate::trail::{Middleware, Next};

/// A tool invocation request used as the middleware request type.
#[derive(Debug, Clone)]
pub struct ToolInvocation {
    /// The tool name.
    pub name: String,

    /// The tool arguments (JSON).
    pub args: Value,
}

/// Type alias for tool execution middleware.
pub type ToolMiddleware = dyn Middleware<ToolInvocation, String>;

// ── ToolLoggingMiddleware ─────────────────────────────────

/// Logs tool invocations at debug level with timing.
pub struct ToolLoggingMiddleware;

#[async_trait]
impl Middleware<ToolInvocation, String> for ToolLoggingMiddleware {
    async fn handle(
        &self,
        request: ToolInvocation,
        next: Next<'_, ToolInvocation, String>,
    ) -> anyhow::Result<String> {
        debug!(
            tool = %request.name,
            args_size = request.args.to_string().len(),
            "Tool invocation"
        );

        let start = Instant::now();
        let result = next.run(request).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(output) => {
                debug!(
                    elapsed_ms = elapsed.as_millis() as u64,
                    output_len = output.len(),
                    "Tool completed"
                );
            }
            Err(e) => {
                warn!(
                    elapsed_ms = elapsed.as_millis() as u64,
                    error = %e,
                    "Tool error"
                );
            }
        }

        result
    }

    fn name(&self) -> &str {
        "ToolLoggingMiddleware"
    }
}

// ── ToolPermissionMiddleware ──────────────────────────────

/// Restricts tool access by allowlist/blocklist.
///
/// If the blocklist is non-empty, any tool in the blocklist is rejected.
/// If the allowlist is non-empty, only tools in the allowlist are allowed.
/// If both are empty, all tools pass through.
pub struct ToolPermissionMiddleware {
    allowed_tools: HashSet<String>,
    blocked_tools: HashSet<String>,
}

impl ToolPermissionMiddleware {
    /// Create a permission middleware with both allow and block lists.
    pub fn new(
        allowed: impl IntoIterator<Item = String>,
        blocked: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            allowed_tools: allowed.into_iter().collect(),
            blocked_tools: blocked.into_iter().collect(),
        }
    }

    /// Create a middleware that only allows the specified tools.
    pub fn allow_only(tools: impl IntoIterator<Item = String>) -> Self {
        Self::new(tools, Vec::<String>::new())
    }

    /// Create a middleware that blocks the specified tools.
    pub fn block(tools: impl IntoIterator<Item = String>) -> Self {
        Self::new(Vec::<String>::new(), tools)
    }
}

#[async_trait]
impl Middleware<ToolInvocation, String> for ToolPermissionMiddleware {
    async fn handle(
        &self,
        request: ToolInvocation,
        next: Next<'_, ToolInvocation, String>,
    ) -> anyhow::Result<String> {
        // Check blocklist first
        if self.blocked_tools.contains(&request.name) {
            warn!(tool = %request.name, "Tool blocked by permission policy");
            anyhow::bail!("Permission denied: tool '{}' is blocked", request.name);
        }

        // Check allowlist (empty = allow all)
        if !self.allowed_tools.is_empty() && !self.allowed_tools.contains(&request.name) {
            warn!(tool = %request.name, "Tool not in allowlist");
            anyhow::bail!("Permission denied: tool '{}' is not allowed", request.name);
        }

        next.run(request).await
    }

    fn name(&self) -> &str {
        "ToolPermissionMiddleware"
    }
}

// ── ToolTimeoutMiddleware ─────────────────────────────────

/// Enforces a maximum execution time for tool calls.
///
/// If the tool doesn't complete within the timeout, the call is cancelled
/// and an error is returned.
pub struct ToolTimeoutMiddleware {
    timeout: std::time::Duration,
}

impl ToolTimeoutMiddleware {
    /// Create a timeout middleware with the specified duration.
    pub fn new(timeout: std::time::Duration) -> Self {
        Self { timeout }
    }
}

#[async_trait]
impl Middleware<ToolInvocation, String> for ToolTimeoutMiddleware {
    async fn handle(
        &self,
        request: ToolInvocation,
        next: Next<'_, ToolInvocation, String>,
    ) -> anyhow::Result<String> {
        let tool_name = request.name.clone();
        match tokio::time::timeout(self.timeout, next.run(request)).await {
            Ok(result) => result,
            Err(_) => {
                warn!(
                    tool = %tool_name,
                    timeout_ms = self.timeout.as_millis() as u64,
                    "Tool execution timed out"
                );
                anyhow::bail!(
                    "Tool '{}' timed out after {}ms",
                    tool_name,
                    self.timeout.as_millis()
                );
            }
        }
    }

    fn name(&self) -> &str {
        "ToolTimeoutMiddleware"
    }
}

// ── ToolMetricsMiddleware ─────────────────────────────────

/// Aggregated tool execution metrics.
#[derive(Debug, Clone, Default)]
pub struct ToolMetrics {
    pub total_calls: u64,
    pub total_errors: u64,
    pub total_latency_ms: u64,
}

/// Records tool execution metrics (call count, errors, latency).
pub struct ToolMetricsMiddleware {
    metrics: Arc<Mutex<ToolMetrics>>,
}

impl ToolMetricsMiddleware {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(ToolMetrics::default())),
        }
    }

    /// Get a snapshot of current metrics.
    pub fn metrics(&self) -> ToolMetrics {
        self.metrics.lock().unwrap().clone()
    }
}

impl Default for ToolMetricsMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware<ToolInvocation, String> for ToolMetricsMiddleware {
    async fn handle(
        &self,
        request: ToolInvocation,
        next: Next<'_, ToolInvocation, String>,
    ) -> anyhow::Result<String> {
        let start = Instant::now();
        let result = next.run(request).await;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        let mut m = self.metrics.lock().unwrap();
        m.total_calls += 1;
        m.total_latency_ms += elapsed_ms;
        if result.is_err() {
            m.total_errors += 1;
        }

        result
    }

    fn name(&self) -> &str {
        "ToolMetricsMiddleware"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trail::{Handler, MiddlewareStack};

    fn make_invocation(name: &str) -> ToolInvocation {
        ToolInvocation {
            name: name.to_string(),
            args: serde_json::json!({"path": "/tmp/test.txt"}),
        }
    }

    struct EchoHandler;

    #[async_trait]
    impl Handler<ToolInvocation, String> for EchoHandler {
        async fn handle(&self, request: ToolInvocation) -> anyhow::Result<String> {
            Ok(format!("executed: {}", request.name))
        }
    }

    struct SlowHandler;

    #[async_trait]
    impl Handler<ToolInvocation, String> for SlowHandler {
        async fn handle(&self, _request: ToolInvocation) -> anyhow::Result<String> {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            Ok("done".to_string())
        }
    }

    #[test]
    fn test_tool_invocation() {
        let inv = ToolInvocation {
            name: "read_file".to_string(),
            args: serde_json::json!({"path": "/tmp/test.txt"}),
        };
        assert_eq!(inv.name, "read_file");
        assert_eq!(inv.args["path"], "/tmp/test.txt");
    }

    #[tokio::test]
    async fn test_logging_middleware() {
        let mut stack = MiddlewareStack::new();
        stack.push(Arc::new(ToolLoggingMiddleware));

        let handler = EchoHandler;
        let result = stack.execute(make_invocation("read_file"), &handler).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("read_file"));
    }

    #[tokio::test]
    async fn test_permission_allow_only() {
        let perm = ToolPermissionMiddleware::allow_only(vec!["read_file".to_string()]);
        let mut stack = MiddlewareStack::new();
        stack.push(Arc::new(perm));

        let handler = EchoHandler;

        // Allowed tool passes
        let r1 = stack.execute(make_invocation("read_file"), &handler).await;
        assert!(r1.is_ok());

        // Not-allowed tool rejected
        let r2 = stack.execute(make_invocation("exec"), &handler).await;
        assert!(r2.is_err());
        assert!(r2.unwrap_err().to_string().contains("not allowed"));
    }

    #[tokio::test]
    async fn test_permission_block() {
        let perm = ToolPermissionMiddleware::block(vec!["exec".to_string()]);
        let mut stack = MiddlewareStack::new();
        stack.push(Arc::new(perm));

        let handler = EchoHandler;

        // Non-blocked tool passes
        let r1 = stack.execute(make_invocation("read_file"), &handler).await;
        assert!(r1.is_ok());

        // Blocked tool rejected
        let r2 = stack.execute(make_invocation("exec"), &handler).await;
        assert!(r2.is_err());
        assert!(r2.unwrap_err().to_string().contains("blocked"));
    }

    #[tokio::test]
    async fn test_timeout_middleware_passes() {
        let timeout = ToolTimeoutMiddleware::new(std::time::Duration::from_secs(5));
        let mut stack = MiddlewareStack::new();
        stack.push(Arc::new(timeout));

        let handler = EchoHandler;
        let result = stack.execute(make_invocation("read_file"), &handler).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_timeout_middleware_expires() {
        let timeout = ToolTimeoutMiddleware::new(std::time::Duration::from_millis(100));
        let mut stack = MiddlewareStack::new();
        stack.push(Arc::new(timeout));

        let handler = SlowHandler;
        let result = stack.execute(make_invocation("slow_tool"), &handler).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn test_metrics_middleware() {
        let metrics_mw = Arc::new(ToolMetricsMiddleware::new());
        let mut stack = MiddlewareStack::new();
        stack.push(metrics_mw.clone());

        let handler = EchoHandler;
        let _ = stack.execute(make_invocation("t1"), &handler).await;
        let _ = stack.execute(make_invocation("t2"), &handler).await;

        let m = metrics_mw.metrics();
        assert_eq!(m.total_calls, 2);
        assert_eq!(m.total_errors, 0);
    }
}
