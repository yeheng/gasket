//! Trail context for async propagation using OpenTelemetry

use std::collections::HashMap;

use opentelemetry::Context;

/// Trail context for propagating trace information across async boundaries.
///
/// This wraps OpenTelemetry's `Context` and provides a simple interface
/// for distributed tracing. The context can be passed to LLM providers
/// and other services for end-to-end tracing.
#[derive(Debug, Clone)]
pub struct TrailContext {
    /// The OpenTelemetry context containing span information
    context: Context,
    /// Arbitrary key-value pairs propagated with the context
    baggage: HashMap<String, String>,
}

impl TrailContext {
    /// Create a new context from the current OpenTelemetry span.
    pub fn current() -> Self {
        Self {
            context: Context::current(),
            baggage: HashMap::new(),
        }
    }

    /// Create a new root context (no active span).
    pub fn new() -> Self {
        Self {
            context: Context::new(),
            baggage: HashMap::new(),
        }
    }

    /// Create a context from an existing OpenTelemetry Context.
    pub fn from_context(context: Context) -> Self {
        Self {
            context,
            baggage: HashMap::new(),
        }
    }

    /// Get the underlying OpenTelemetry Context.
    pub fn context(&self) -> &Context {
        &self.context
    }

    /// Get a clone of the underlying OpenTelemetry Context.
    pub fn to_context(&self) -> Context {
        self.context.clone()
    }

    /// Get the trace ID as a string, if available.
    pub fn trace_id(&self) -> Option<String> {
        use opentelemetry::trace::TraceContextExt;
        let span = self.context.span();
        let span_ctx = span.span_context();
        if span_ctx.is_valid() {
            Some(span_ctx.trace_id().to_string())
        } else {
            None
        }
    }

    /// Get the span ID as a string, if available.
    pub fn span_id(&self) -> Option<String> {
        use opentelemetry::trace::TraceContextExt;
        let span = self.context.span();
        let span_ctx = span.span_context();
        if span_ctx.is_valid() {
            Some(span_ctx.span_id().to_string())
        } else {
            None
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

    /// Check if this context has an active span.
    pub fn is_valid(&self) -> bool {
        use opentelemetry::trace::TraceContextExt;
        self.context.span().span_context().is_valid()
    }
}

impl Default for TrailContext {
    fn default() -> Self {
        Self::current()
    }
}
