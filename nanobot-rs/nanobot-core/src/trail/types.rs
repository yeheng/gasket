//! Core types and traits for the Trail system

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use super::context::TrailContext;

/// A typed attribute value for spans and events.
#[derive(Debug, Clone)]
pub enum Value {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => write!(f, "{}", s),
            Value::Int(i) => write!(f, "{}", i),
            Value::Float(fl) => write!(f, "{}", fl),
            Value::Bool(b) => write!(f, "{}", b),
        }
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Int(i)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Float(f)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

/// Unique identifier for a span within a trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpanId(pub u64);

impl fmt::Display for SpanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// Unique identifier for an entire trace (a tree of spans).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraceId(pub u64);

impl fmt::Display for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// Recorded event within a span.
#[derive(Debug, Clone)]
pub struct EventRecord {
    pub name: String,
    pub attrs: Vec<(String, Value)>,
    pub timestamp: Instant,
}

/// Recorded span data (after the span has ended).
#[derive(Debug, Clone)]
pub struct SpanRecord {
    pub span_id: SpanId,
    pub parent_id: Option<SpanId>,
    pub trace_id: TraceId,
    pub name: String,
    pub attrs: Vec<(String, Value)>,
    pub events: Vec<EventRecord>,
    pub start: Instant,
    pub end: Option<Instant>,
}

impl SpanRecord {
    /// Duration of this span (None if still open).
    pub fn duration(&self) -> Option<std::time::Duration> {
        self.end.map(|e| e.duration_since(self.start))
    }
}

/// Core trait for the Trail execution tracing system.
///
/// Implementations record spans (units of work) and events (point-in-time
/// occurrences) to provide observability into agent execution.
pub trait Trail: Send + Sync {
    /// Start a new span with the given name and attributes.
    /// Returns a `SpanId` that must be passed to `end_span` when the work completes.
    fn start_span(&self, name: &str, attrs: Vec<(String, Value)>) -> SpanId;

    /// Start a child span under an existing parent.
    fn start_child_span(
        &self,
        name: &str,
        parent: SpanId,
        attrs: Vec<(String, Value)>,
    ) -> SpanId;

    /// End a previously started span.
    fn end_span(&self, span_id: SpanId);

    /// Record an event (log-like entry) within the current context.
    fn record_event(&self, name: &str, attrs: Vec<(String, Value)>);

    /// Record an event within a specific span.
    fn record_span_event(&self, span_id: SpanId, name: &str, attrs: Vec<(String, Value)>);

    /// Get the current trail context for propagation.
    fn current_context(&self) -> TrailContext;

    /// Get a completed span record by ID (if available).
    fn get_span(&self, span_id: SpanId) -> Option<SpanRecord>;

    /// Get all completed span records for a trace.
    fn get_trace(&self, trace_id: TraceId) -> Vec<SpanRecord>;
}

// ──────────────────────────────────────────────
//  DefaultTrail – in-memory implementation
// ──────────────────────────────────────────────

/// Internal mutable state for `DefaultTrail`.
struct TrailState {
    spans: HashMap<SpanId, SpanRecord>,
    active_span: Option<SpanId>,
}

/// In-memory Trail implementation that stores all spans and events.
///
/// Suitable for debugging and development. For production, consider
/// a sampled or no-op implementation.
pub struct DefaultTrail {
    trace_id: TraceId,
    next_span_id: AtomicU64,
    state: Mutex<TrailState>,
}

impl DefaultTrail {
    /// Create a new DefaultTrail with a fresh trace ID.
    pub fn new() -> Self {
        static NEXT_TRACE: AtomicU64 = AtomicU64::new(1);
        Self {
            trace_id: TraceId(NEXT_TRACE.fetch_add(1, Ordering::Relaxed)),
            next_span_id: AtomicU64::new(1),
            state: Mutex::new(TrailState {
                spans: HashMap::new(),
                active_span: None,
            }),
        }
    }

    fn alloc_span_id(&self) -> SpanId {
        SpanId(self.next_span_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Get all recorded spans (for testing/visualization).
    pub fn all_spans(&self) -> Vec<SpanRecord> {
        let state = self.state.lock().unwrap();
        state.spans.values().cloned().collect()
    }

    /// Format the trace as a tree string for debugging.
    pub fn format_tree(&self) -> String {
        let state = self.state.lock().unwrap();
        let roots: Vec<&SpanRecord> = state
            .spans
            .values()
            .filter(|s| s.parent_id.is_none())
            .collect();

        let mut out = String::new();
        for root in roots {
            Self::format_subtree(&state.spans, root, 0, &mut out);
        }
        out
    }

    fn format_subtree(
        spans: &HashMap<SpanId, SpanRecord>,
        span: &SpanRecord,
        depth: usize,
        out: &mut String,
    ) {
        let indent = "  ".repeat(depth);
        let duration = span
            .duration()
            .map(|d| format!("{:.2}ms", d.as_secs_f64() * 1000.0))
            .unwrap_or_else(|| "running".to_string());

        out.push_str(&format!(
            "{}└─ {} [{}] ({})\n",
            indent, span.name, span.span_id, duration
        ));

        for event in &span.events {
            out.push_str(&format!("{}  ● {}\n", indent, event.name));
        }

        let children: Vec<&SpanRecord> = spans
            .values()
            .filter(|s| s.parent_id == Some(span.span_id))
            .collect();

        for child in children {
            Self::format_subtree(spans, child, depth + 1, out);
        }
    }
}

impl Default for DefaultTrail {
    fn default() -> Self {
        Self::new()
    }
}

impl Trail for DefaultTrail {
    fn start_span(&self, name: &str, attrs: Vec<(String, Value)>) -> SpanId {
        let span_id = self.alloc_span_id();
        let mut state = self.state.lock().unwrap();
        let parent_id = state.active_span;

        let record = SpanRecord {
            span_id,
            parent_id,
            trace_id: self.trace_id,
            name: name.to_string(),
            attrs,
            events: Vec::new(),
            start: Instant::now(),
            end: None,
        };
        state.spans.insert(span_id, record);
        state.active_span = Some(span_id);
        span_id
    }

    fn start_child_span(
        &self,
        name: &str,
        parent: SpanId,
        attrs: Vec<(String, Value)>,
    ) -> SpanId {
        let span_id = self.alloc_span_id();
        let mut state = self.state.lock().unwrap();

        let record = SpanRecord {
            span_id,
            parent_id: Some(parent),
            trace_id: self.trace_id,
            name: name.to_string(),
            attrs,
            events: Vec::new(),
            start: Instant::now(),
            end: None,
        };
        state.spans.insert(span_id, record);
        state.active_span = Some(span_id);
        span_id
    }

    fn end_span(&self, span_id: SpanId) {
        let mut state = self.state.lock().unwrap();
        let parent_id = if let Some(span) = state.spans.get_mut(&span_id) {
            span.end = Some(Instant::now());
            span.parent_id
        } else {
            return;
        };

        // Restore parent as active span
        if state.active_span == Some(span_id) {
            state.active_span = parent_id;
        }
    }

    fn record_event(&self, name: &str, attrs: Vec<(String, Value)>) {
        let mut state = self.state.lock().unwrap();
        if let Some(active) = state.active_span {
            if let Some(span) = state.spans.get_mut(&active) {
                span.events.push(EventRecord {
                    name: name.to_string(),
                    attrs,
                    timestamp: Instant::now(),
                });
            }
        }
    }

    fn record_span_event(&self, span_id: SpanId, name: &str, attrs: Vec<(String, Value)>) {
        let mut state = self.state.lock().unwrap();
        if let Some(span) = state.spans.get_mut(&span_id) {
            span.events.push(EventRecord {
                name: name.to_string(),
                attrs,
                timestamp: Instant::now(),
            });
        }
    }

    fn current_context(&self) -> TrailContext {
        let state = self.state.lock().unwrap();
        TrailContext {
            trace_id: self.trace_id,
            span_id: state.active_span.unwrap_or(SpanId(0)),
            baggage: HashMap::new(),
        }
    }

    fn get_span(&self, span_id: SpanId) -> Option<SpanRecord> {
        let state = self.state.lock().unwrap();
        state.spans.get(&span_id).cloned()
    }

    fn get_trace(&self, trace_id: TraceId) -> Vec<SpanRecord> {
        let state = self.state.lock().unwrap();
        state
            .spans
            .values()
            .filter(|s| s.trace_id == trace_id)
            .cloned()
            .collect()
    }
}

// ──────────────────────────────────────────────
//  NoopTrail – disabled tracing
// ──────────────────────────────────────────────

/// No-op Trail implementation that discards all data.
///
/// Use this when tracing is disabled to minimize overhead.
pub struct NoopTrail;

impl NoopTrail {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoopTrail {
    fn default() -> Self {
        Self::new()
    }
}

impl Trail for NoopTrail {
    fn start_span(&self, _name: &str, _attrs: Vec<(String, Value)>) -> SpanId {
        SpanId(0)
    }

    fn start_child_span(
        &self,
        _name: &str,
        _parent: SpanId,
        _attrs: Vec<(String, Value)>,
    ) -> SpanId {
        SpanId(0)
    }

    fn end_span(&self, _span_id: SpanId) {}

    fn record_event(&self, _name: &str, _attrs: Vec<(String, Value)>) {}

    fn record_span_event(&self, _span_id: SpanId, _name: &str, _attrs: Vec<(String, Value)>) {}

    fn current_context(&self) -> TrailContext {
        TrailContext {
            trace_id: TraceId(0),
            span_id: SpanId(0),
            baggage: HashMap::new(),
        }
    }

    fn get_span(&self, _span_id: SpanId) -> Option<SpanRecord> {
        None
    }

    fn get_trace(&self, _trace_id: TraceId) -> Vec<SpanRecord> {
        Vec::new()
    }
}
