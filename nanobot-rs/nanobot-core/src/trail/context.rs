//! Trail context for async propagation

use std::collections::HashMap;

use super::types::{SpanId, TraceId};

/// Trail context for propagating trace information across async boundaries.
///
/// Contains the current trace and span IDs plus arbitrary baggage items
/// that travel with the request through the system.
#[derive(Debug, Clone)]
pub struct TrailContext {
    /// The trace this context belongs to.
    pub trace_id: TraceId,

    /// The current span within the trace.
    pub span_id: SpanId,

    /// Arbitrary key-value pairs propagated with the context.
    pub baggage: HashMap<String, String>,
}

impl TrailContext {
    /// Create a new root context with a given trace ID.
    pub fn new(trace_id: TraceId) -> Self {
        Self {
            trace_id,
            span_id: SpanId(0),
            baggage: HashMap::new(),
        }
    }

    /// Create a child context from this one, with a new span ID.
    pub fn child(&self, span_id: SpanId) -> Self {
        Self {
            trace_id: self.trace_id,
            span_id,
            baggage: self.baggage.clone(),
        }
    }

    /// Set a baggage item.
    pub fn set_baggage(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.baggage.insert(key.into(), value.into());
    }

    /// Get a baggage item.
    pub fn get_baggage(&self, key: &str) -> Option<&str> {
        self.baggage.get(key).map(|s| s.as_str())
    }

    /// Check if this is a valid (non-zero) context.
    pub fn is_valid(&self) -> bool {
        self.trace_id.0 != 0
    }
}

impl Default for TrailContext {
    fn default() -> Self {
        Self {
            trace_id: TraceId(0),
            span_id: SpanId(0),
            baggage: HashMap::new(),
        }
    }
}
