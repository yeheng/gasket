//! Trail context for execution tracing using OpenTelemetry
//!
//! This module provides a lightweight context propagation system built on
//! OpenTelemetry. The `TrailContext` wraps OpenTelemetry's `Context` and
//! provides a simple interface for distributed tracing.
//!
//! For span creation, use the `tracing` crate with `#[instrument]`:
//! ```ignore
//! use tracing::{info_span, instrument};
//!
//! #[instrument(skip_all)]
//! async fn my_function(ctx: &TrailContext) {
//!     // Span is automatically created and linked to parent
//! }
//! ```

mod context;
mod middleware;

pub use context::TrailContext;
pub use middleware::{Handler, Middleware, MiddlewareStack, Next};
