//! Generic middleware infrastructure
//!
//! Provides an onion-model middleware pattern compatible with all nanobot
//! subsystems (providers, channels, tools, memory).

use std::future::Future;
use std::sync::Arc;

use async_trait::async_trait;

/// A generic middleware that wraps an operation of type `Req -> Result<Res>`.
///
/// Middleware implementations receive the request and a `Next` handle.
/// They may:
/// - Inspect or modify the request before forwarding
/// - Short-circuit by returning early without calling `next`
/// - Inspect or modify the response after forwarding
///
/// # Type Parameters
/// - `Req`: The request type (e.g., `ChatRequest`, tool arguments)
/// - `Res`: The response type (e.g., `ChatResponse`, tool result)
#[async_trait]
pub trait Middleware<Req, Res>: Send + Sync
where
    Req: Send + 'static,
    Res: Send + 'static,
{
    /// Process the request, optionally delegating to the next middleware.
    async fn handle(&self, request: Req, next: Next<'_, Req, Res>) -> anyhow::Result<Res>;

    /// Optional name for debugging/logging.
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}

/// Handle to the next middleware (or the final handler) in the chain.
///
/// Calling `next.run(request)` passes the request down the stack.
pub struct Next<'a, Req, Res> {
    inner: NextInner<'a, Req, Res>,
}

enum NextInner<'a, Req, Res> {
    /// More middleware in the chain.
    Middleware {
        current: &'a dyn Middleware<Req, Res>,
        rest: &'a [Arc<dyn Middleware<Req, Res>>],
        handler: &'a dyn Handler<Req, Res>,
    },
    /// Terminal handler (no more middleware).
    Handler(&'a dyn Handler<Req, Res>),
}

impl<'a, Req, Res> Next<'a, Req, Res>
where
    Req: Send + 'static,
    Res: Send + 'static,
{
    /// Run the next step in the middleware chain.
    pub async fn run(self, request: Req) -> anyhow::Result<Res> {
        match self.inner {
            NextInner::Middleware {
                current,
                rest,
                handler,
            } => {
                let next = if rest.is_empty() {
                    Next {
                        inner: NextInner::Handler(handler),
                    }
                } else {
                    Next {
                        inner: NextInner::Middleware {
                            current: rest[0].as_ref(),
                            rest: &rest[1..],
                            handler,
                        },
                    }
                };
                current.handle(request, next).await
            }
            NextInner::Handler(handler) => handler.handle(request).await,
        }
    }
}

/// Terminal handler at the bottom of the middleware stack.
///
/// This is the actual operation being wrapped (e.g., the real LLM call,
/// the actual tool execution).
#[async_trait]
pub trait Handler<Req, Res>: Send + Sync
where
    Req: Send + 'static,
    Res: Send + 'static,
{
    async fn handle(&self, request: Req) -> anyhow::Result<Res>;
}

/// Convenience: any async closure can be a handler.
#[async_trait]
impl<F, Req, Res, Fut> Handler<Req, Res> for F
where
    Req: Send + 'static,
    Res: Send + 'static,
    F: Fn(Req) -> Fut + Send + Sync,
    Fut: Future<Output = anyhow::Result<Res>> + Send,
{
    async fn handle(&self, request: Req) -> anyhow::Result<Res> {
        (self)(request).await
    }
}

/// An ordered stack of middleware with a terminal handler.
///
/// When executed, the request flows through each middleware in order
/// (outermost first), then reaches the terminal handler.
pub struct MiddlewareStack<Req, Res>
where
    Req: Send + 'static,
    Res: Send + 'static,
{
    middlewares: Vec<Arc<dyn Middleware<Req, Res>>>,
}

impl<Req, Res> MiddlewareStack<Req, Res>
where
    Req: Send + 'static,
    Res: Send + 'static,
{
    /// Create a new empty middleware stack.
    pub fn new() -> Self {
        Self {
            middlewares: Vec::new(),
        }
    }

    /// Add a middleware to the end of the stack (innermost position).
    pub fn push(&mut self, middleware: Arc<dyn Middleware<Req, Res>>) {
        self.middlewares.push(middleware);
    }

    /// Insert a middleware at a specific position.
    pub fn insert(&mut self, index: usize, middleware: Arc<dyn Middleware<Req, Res>>) {
        self.middlewares.insert(index, middleware);
    }

    /// Number of middleware in the stack.
    pub fn len(&self) -> usize {
        self.middlewares.len()
    }

    /// Whether the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.middlewares.is_empty()
    }

    /// Execute the middleware stack with the given request and terminal handler.
    pub async fn execute(
        &self,
        request: Req,
        handler: &dyn Handler<Req, Res>,
    ) -> anyhow::Result<Res> {
        if self.middlewares.is_empty() {
            return handler.handle(request).await;
        }

        let first = self.middlewares[0].as_ref();
        let rest = &self.middlewares[1..];

        let next = if rest.is_empty() {
            Next {
                inner: NextInner::Handler(handler),
            }
        } else {
            Next {
                inner: NextInner::Middleware {
                    current: rest[0].as_ref(),
                    rest: &rest[1..],
                    handler,
                },
            }
        };

        first.handle(request, next).await
    }

    /// Get middleware names for debugging.
    pub fn names(&self) -> Vec<&str> {
        self.middlewares.iter().map(|m| m.name()).collect()
    }
}

impl<Req, Res> Default for MiddlewareStack<Req, Res>
where
    Req: Send + 'static,
    Res: Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}
