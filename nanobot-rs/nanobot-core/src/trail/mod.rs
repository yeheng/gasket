//! Trail system for execution tracing and observability
//!
//! The Trail system provides end-to-end execution tracing inspired by
//! OpenTelemetry's span/event model. It supports:
//! - Hierarchical span tracking
//! - Async context propagation via `TrailContext`
//! - Pluggable middleware infrastructure
//! - Built-in implementations: `DefaultTrail` (in-memory) and `NoopTrail`

mod context;
mod middleware;
mod span;
mod types;

pub use context::TrailContext;
pub use middleware::{Handler, Middleware, MiddlewareStack, Next};
pub use span::TrailSpan;
pub use types::{DefaultTrail, NoopTrail, SpanId, SpanRecord, Trail, TraceId, Value};

#[cfg(test)]
mod tests;
