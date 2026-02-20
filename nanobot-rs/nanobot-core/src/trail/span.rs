//! RAII span guard for automatic span lifecycle management

use std::sync::Arc;

use super::types::{SpanId, Trail, Value};

/// RAII guard that automatically ends a span when dropped.
///
/// Use this to ensure spans are always properly closed, even when
/// returning early or encountering errors.
///
/// # Example
/// ```ignore
/// let span = TrailSpan::new(trail.clone(), "my_operation", vec![]);
/// // ... do work ...
/// // span is automatically ended when it goes out of scope
/// ```
pub struct TrailSpan {
    trail: Arc<dyn Trail>,
    span_id: SpanId,
    ended: bool,
}

impl TrailSpan {
    /// Start a new root span.
    pub fn new(trail: Arc<dyn Trail>, name: &str, attrs: Vec<(String, Value)>) -> Self {
        let span_id = trail.start_span(name, attrs);
        Self {
            trail,
            span_id,
            ended: false,
        }
    }

    /// Start a child span under an existing parent.
    pub fn child(
        trail: Arc<dyn Trail>,
        name: &str,
        parent: SpanId,
        attrs: Vec<(String, Value)>,
    ) -> Self {
        let span_id = trail.start_child_span(name, parent, attrs);
        Self {
            trail,
            span_id,
            ended: false,
        }
    }

    /// Get the span ID.
    pub fn id(&self) -> SpanId {
        self.span_id
    }

    /// Record an event within this span.
    pub fn record_event(&self, name: &str, attrs: Vec<(String, Value)>) {
        self.trail.record_span_event(self.span_id, name, attrs);
    }

    /// Manually end the span (otherwise it ends on drop).
    pub fn end(mut self) {
        if !self.ended {
            self.trail.end_span(self.span_id);
            self.ended = true;
        }
    }
}

impl Drop for TrailSpan {
    fn drop(&mut self) {
        if !self.ended {
            self.trail.end_span(self.span_id);
        }
    }
}
