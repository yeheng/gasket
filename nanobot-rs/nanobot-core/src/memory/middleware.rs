//! Memory middleware infrastructure
//!
//! Provides middleware types and built-in middlewares for wrapping
//! memory store operations.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use tracing::{debug, warn};

use crate::trail::{Middleware, Next};

/// Request type for memory middleware.
#[derive(Debug, Clone)]
pub enum MemoryOperation {
    Read { key: String },
    Write { key: String, value: String },
    Delete { key: String },
    Append { key: String, value: String },
}

/// Response type for memory middleware.
#[derive(Debug, Clone)]
pub enum MemoryResponse {
    /// Result of a read operation.
    Value(Option<String>),
    /// Result of a write/append (success).
    Ok,
    /// Result of a delete (true if existed).
    Deleted(bool),
}

/// Type alias for memory operation middleware.
pub type MemoryMiddleware = dyn Middleware<MemoryOperation, MemoryResponse>;

// ── MemoryLoggingMiddleware ───────────────────────────────

/// Logs memory operations at debug level with timing.
pub struct MemoryLoggingMiddleware;

#[async_trait]
impl Middleware<MemoryOperation, MemoryResponse> for MemoryLoggingMiddleware {
    async fn handle(
        &self,
        request: MemoryOperation,
        next: Next<'_, MemoryOperation, MemoryResponse>,
    ) -> anyhow::Result<MemoryResponse> {
        let op_name = match &request {
            MemoryOperation::Read { key } => format!("read({})", key),
            MemoryOperation::Write { key, .. } => format!("write({})", key),
            MemoryOperation::Delete { key } => format!("delete({})", key),
            MemoryOperation::Append { key, .. } => format!("append({})", key),
        };

        debug!(op = %op_name, "Memory operation");

        let start = Instant::now();
        let result = next.run(request).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => debug!(op = %op_name, elapsed_ms = elapsed.as_millis() as u64, "Memory op complete"),
            Err(e) => warn!(op = %op_name, elapsed_ms = elapsed.as_millis() as u64, error = %e, "Memory op error"),
        }

        result
    }

    fn name(&self) -> &str {
        "MemoryLoggingMiddleware"
    }
}

// ── MemoryMetricsMiddleware ───────────────────────────────

/// Aggregated memory operation metrics.
#[derive(Debug, Clone, Default)]
pub struct MemoryMetrics {
    pub reads: u64,
    pub writes: u64,
    pub deletes: u64,
    pub appends: u64,
    pub errors: u64,
}

/// Records memory operation metrics.
pub struct MemoryMetricsMiddleware {
    metrics: Arc<Mutex<MemoryMetrics>>,
}

impl MemoryMetricsMiddleware {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(MemoryMetrics::default())),
        }
    }

    /// Get a snapshot of current metrics.
    pub fn metrics(&self) -> MemoryMetrics {
        self.metrics.lock().unwrap().clone()
    }
}

impl Default for MemoryMetricsMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware<MemoryOperation, MemoryResponse> for MemoryMetricsMiddleware {
    async fn handle(
        &self,
        request: MemoryOperation,
        next: Next<'_, MemoryOperation, MemoryResponse>,
    ) -> anyhow::Result<MemoryResponse> {
        let op_type = match &request {
            MemoryOperation::Read { .. } => "read",
            MemoryOperation::Write { .. } => "write",
            MemoryOperation::Delete { .. } => "delete",
            MemoryOperation::Append { .. } => "append",
        };

        let result = next.run(request).await;

        let mut m = self.metrics.lock().unwrap();
        match op_type {
            "read" => m.reads += 1,
            "write" => m.writes += 1,
            "delete" => m.deletes += 1,
            "append" => m.appends += 1,
            _ => {}
        }
        if result.is_err() {
            m.errors += 1;
        }

        result
    }

    fn name(&self) -> &str {
        "MemoryMetricsMiddleware"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trail::{Handler, MiddlewareStack};

    struct MockMemoryHandler;

    #[async_trait]
    impl Handler<MemoryOperation, MemoryResponse> for MockMemoryHandler {
        async fn handle(&self, request: MemoryOperation) -> anyhow::Result<MemoryResponse> {
            match request {
                MemoryOperation::Read { .. } => Ok(MemoryResponse::Value(Some("data".to_string()))),
                MemoryOperation::Write { .. } => Ok(MemoryResponse::Ok),
                MemoryOperation::Delete { .. } => Ok(MemoryResponse::Deleted(true)),
                MemoryOperation::Append { .. } => Ok(MemoryResponse::Ok),
            }
        }
    }

    #[test]
    fn test_memory_operation_debug() {
        let op = MemoryOperation::Read {
            key: "test".to_string(),
        };
        let dbg = format!("{:?}", op);
        assert!(dbg.contains("Read"));
        assert!(dbg.contains("test"));
    }

    #[test]
    fn test_memory_response_variants() {
        let r = MemoryResponse::Value(Some("hello".to_string()));
        if let MemoryResponse::Value(Some(v)) = r {
            assert_eq!(v, "hello");
        } else {
            panic!("expected Value(Some)");
        }

        let r = MemoryResponse::Deleted(true);
        if let MemoryResponse::Deleted(d) = r {
            assert!(d);
        } else {
            panic!("expected Deleted");
        }
    }

    #[tokio::test]
    async fn test_logging_middleware() {
        let mut stack = MiddlewareStack::new();
        stack.push(Arc::new(MemoryLoggingMiddleware));

        let handler = MockMemoryHandler;
        let result = stack
            .execute(
                MemoryOperation::Read {
                    key: "test".to_string(),
                },
                &handler,
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_metrics_middleware() {
        let metrics_mw = Arc::new(MemoryMetricsMiddleware::new());
        let mut stack = MiddlewareStack::new();
        stack.push(metrics_mw.clone());

        let handler = MockMemoryHandler;

        let _ = stack
            .execute(
                MemoryOperation::Read {
                    key: "k1".to_string(),
                },
                &handler,
            )
            .await;
        let _ = stack
            .execute(
                MemoryOperation::Write {
                    key: "k2".to_string(),
                    value: "v2".to_string(),
                },
                &handler,
            )
            .await;
        let _ = stack
            .execute(
                MemoryOperation::Delete {
                    key: "k3".to_string(),
                },
                &handler,
            )
            .await;

        let m = metrics_mw.metrics();
        assert_eq!(m.reads, 1);
        assert_eq!(m.writes, 1);
        assert_eq!(m.deletes, 1);
        assert_eq!(m.errors, 0);
    }
}
