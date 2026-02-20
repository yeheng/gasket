//! Tests for the Trail system

use std::sync::Arc;

use super::*;

#[test]
fn test_span_id_display() {
    let id = SpanId(42);
    assert_eq!(format!("{}", id), "000000000000002a");
}

#[test]
fn test_trace_id_display() {
    let id = TraceId(255);
    assert_eq!(format!("{}", id), "00000000000000ff");
}

#[test]
fn test_default_trail_start_end_span() {
    let trail = DefaultTrail::new();

    let span_id = trail.start_span("test_op", vec![("key".to_string(), Value::from("val"))]);
    assert!(trail.get_span(span_id).is_some());

    let span = trail.get_span(span_id).unwrap();
    assert_eq!(span.name, "test_op");
    assert!(span.end.is_none()); // still open

    trail.end_span(span_id);

    let span = trail.get_span(span_id).unwrap();
    assert!(span.end.is_some()); // now closed
    assert!(span.duration().unwrap().as_nanos() > 0);
}

#[test]
fn test_default_trail_child_spans() {
    let trail = DefaultTrail::new();

    let parent = trail.start_span("parent", vec![]);
    let child = trail.start_child_span("child", parent, vec![]);

    let child_span = trail.get_span(child).unwrap();
    assert_eq!(child_span.parent_id, Some(parent));
    assert_eq!(child_span.trace_id, trail.current_context().trace_id);

    trail.end_span(child);
    trail.end_span(parent);
}

#[test]
fn test_default_trail_events() {
    let trail = DefaultTrail::new();

    let span_id = trail.start_span("op", vec![]);
    trail.record_span_event(span_id, "checkpoint", vec![("step".to_string(), Value::from(1i64))]);

    let span = trail.get_span(span_id).unwrap();
    assert_eq!(span.events.len(), 1);
    assert_eq!(span.events[0].name, "checkpoint");

    trail.end_span(span_id);
}

#[test]
fn test_default_trail_record_event_on_active() {
    let trail = DefaultTrail::new();

    let span_id = trail.start_span("op", vec![]);
    trail.record_event("active_event", vec![]);

    let span = trail.get_span(span_id).unwrap();
    assert_eq!(span.events.len(), 1);
    assert_eq!(span.events[0].name, "active_event");

    trail.end_span(span_id);
}

#[test]
fn test_default_trail_get_trace() {
    let trail = DefaultTrail::new();
    let trace_id = trail.current_context().trace_id;

    let s1 = trail.start_span("a", vec![]);
    let s2 = trail.start_child_span("b", s1, vec![]);
    trail.end_span(s2);
    trail.end_span(s1);

    let spans = trail.get_trace(trace_id);
    assert_eq!(spans.len(), 2);
}

#[test]
fn test_default_trail_format_tree() {
    let trail = DefaultTrail::new();

    let root = trail.start_span("request", vec![]);
    let child = trail.start_child_span("llm_call", root, vec![]);
    trail.record_span_event(child, "tokens_used", vec![("count".to_string(), Value::from(100i64))]);
    trail.end_span(child);
    trail.end_span(root);

    let tree = trail.format_tree();
    assert!(tree.contains("request"));
    assert!(tree.contains("llm_call"));
    assert!(tree.contains("tokens_used"));
}

#[test]
fn test_noop_trail() {
    let trail = NoopTrail::new();

    let span_id = trail.start_span("noop", vec![]);
    assert_eq!(span_id, SpanId(0));

    trail.record_event("ignored", vec![]);
    trail.end_span(span_id);

    assert!(trail.get_span(span_id).is_none());
    assert!(trail.get_trace(TraceId(0)).is_empty());
}

#[test]
fn test_trail_context_propagation() {
    let ctx = TrailContext::new(TraceId(42));
    assert!(ctx.is_valid());
    assert_eq!(ctx.trace_id, TraceId(42));

    let child = ctx.child(SpanId(10));
    assert_eq!(child.trace_id, TraceId(42));
    assert_eq!(child.span_id, SpanId(10));
}

#[test]
fn test_trail_context_baggage() {
    let mut ctx = TrailContext::default();
    assert!(!ctx.is_valid());

    ctx.set_baggage("user_id", "abc123");
    assert_eq!(ctx.get_baggage("user_id"), Some("abc123"));
    assert_eq!(ctx.get_baggage("missing"), None);
}

#[test]
fn test_trail_span_raii() {
    let trail: Arc<dyn Trail> = Arc::new(DefaultTrail::new());

    let span_id;
    {
        let span = TrailSpan::new(trail.clone(), "scoped_op", vec![]);
        span_id = span.id();
        span.record_event("inside", vec![]);
        // span dropped here
    }

    let record = trail.get_span(span_id).unwrap();
    assert!(record.end.is_some()); // auto-ended on drop
    assert_eq!(record.events.len(), 1);
}

#[test]
fn test_trail_span_manual_end() {
    let trail: Arc<dyn Trail> = Arc::new(DefaultTrail::new());

    let span = TrailSpan::new(trail.clone(), "manual", vec![]);
    let span_id = span.id();
    span.end(); // manual end

    let record = trail.get_span(span_id).unwrap();
    assert!(record.end.is_some());
}

#[test]
fn test_value_types() {
    let _s: Value = "hello".into();
    let _i: Value = 42i64.into();
    let _f: Value = 3.14f64.into();
    let _b: Value = true.into();

    assert_eq!(format!("{}", Value::String("test".into())), "test");
    assert_eq!(format!("{}", Value::Int(42)), "42");
    assert_eq!(format!("{}", Value::Float(1.5)), "1.5");
    assert_eq!(format!("{}", Value::Bool(false)), "false");
}

// Middleware tests
mod middleware_tests {
    use super::super::middleware::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    /// A simple logging middleware for testing.
    struct LogMiddleware {
        log: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Middleware<String, String> for LogMiddleware {
        async fn handle(
            &self,
            request: String,
            next: Next<'_, String, String>,
        ) -> anyhow::Result<String> {
            self.log.lock().unwrap().push(format!("before:{}", request));
            let response = next.run(request).await?;
            self.log
                .lock()
                .unwrap()
                .push(format!("after:{}", response));
            Ok(response)
        }

        fn name(&self) -> &str {
            "LogMiddleware"
        }
    }

    /// A middleware that uppercases the request.
    struct UpperMiddleware;

    #[async_trait]
    impl Middleware<String, String> for UpperMiddleware {
        async fn handle(
            &self,
            request: String,
            next: Next<'_, String, String>,
        ) -> anyhow::Result<String> {
            next.run(request.to_uppercase()).await
        }
    }

    /// A handler that echoes the request.
    struct EchoHandler;

    #[async_trait]
    impl Handler<String, String> for EchoHandler {
        async fn handle(&self, request: String) -> anyhow::Result<String> {
            Ok(format!("echo:{}", request))
        }
    }

    #[tokio::test]
    async fn test_middleware_stack_empty() {
        let stack = MiddlewareStack::<String, String>::new();
        let handler = EchoHandler;

        let result = stack.execute("hello".to_string(), &handler).await.unwrap();
        assert_eq!(result, "echo:hello");
    }

    #[tokio::test]
    async fn test_middleware_stack_single() {
        let mut stack = MiddlewareStack::<String, String>::new();
        stack.push(Arc::new(UpperMiddleware));

        let handler = EchoHandler;
        let result = stack.execute("hello".to_string(), &handler).await.unwrap();
        assert_eq!(result, "echo:HELLO");
    }

    #[tokio::test]
    async fn test_middleware_stack_chain() {
        let log = Arc::new(Mutex::new(Vec::new()));

        let mut stack = MiddlewareStack::<String, String>::new();
        stack.push(Arc::new(LogMiddleware { log: log.clone() }));
        stack.push(Arc::new(UpperMiddleware));

        let handler = EchoHandler;
        let result = stack.execute("hello".to_string(), &handler).await.unwrap();
        assert_eq!(result, "echo:HELLO");

        let entries = log.lock().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], "before:hello");
        assert_eq!(entries[1], "after:echo:HELLO");
    }

    #[tokio::test]
    async fn test_middleware_names() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut stack = MiddlewareStack::<String, String>::new();
        stack.push(Arc::new(LogMiddleware { log }));

        let names = stack.names();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], "LogMiddleware");
    }

    #[tokio::test]
    async fn test_middleware_stack_insert() {
        let mut stack = MiddlewareStack::<String, String>::new();
        stack.push(Arc::new(UpperMiddleware));

        let log = Arc::new(Mutex::new(Vec::new()));
        stack.insert(0, Arc::new(LogMiddleware { log: log.clone() }));

        assert_eq!(stack.len(), 2);
        let names = stack.names();
        assert_eq!(names[0], "LogMiddleware");
    }
}
