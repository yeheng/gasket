//! Lightweight subagent execution tracker for parallel task coordination

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use super::loop_::AgentResponse;

/// Default timeout for waiting all results (12 minutes)
const DEFAULT_WAIT_TIMEOUT_SECS: u64 = 720;

/// Subagent execution result
#[derive(Debug, Clone)]
pub struct SubagentResult {
    pub id: String,
    pub task: String,
    pub response: AgentResponse,
    /// Model name used for this execution
    pub model: Option<String>,
}

/// Events emitted during subagent execution for real-time streaming
#[derive(Debug, Clone)]
pub enum SubagentEvent {
    /// Subagent started execution
    Started { id: String, task: String },
    /// Thinking/reasoning content (incremental)
    Thinking { id: String, content: String },
    /// Tool call started
    ToolStart {
        id: String,
        tool_name: String,
        arguments: Option<String>,
    },
    /// Tool call finished
    ToolEnd {
        id: String,
        tool_name: String,
        output: String,
    },
    /// Subagent completed with result
    Completed { id: String, result: SubagentResult },
    /// Subagent encountered an error
    Error { id: String, error: String },
}

/// Tracks multiple subagent executions for parallel coordination
pub struct SubagentTracker {
    results: Arc<RwLock<HashMap<String, SubagentResult>>>,
    result_tx: mpsc::Sender<SubagentResult>,
    result_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<SubagentResult>>>,
    /// Event channel for real-time streaming
    event_tx: mpsc::Sender<SubagentEvent>,
    event_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<SubagentEvent>>>,
}

impl SubagentTracker {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(100);
        let (event_tx, event_rx) = mpsc::channel(256);
        Self {
            results: Arc::new(RwLock::new(HashMap::new())),
            result_tx: tx,
            result_rx: Arc::new(tokio::sync::Mutex::new(rx)),
            event_tx,
            event_rx: Arc::new(tokio::sync::Mutex::new(event_rx)),
        }
    }

    /// Generate a unique subagent ID
    pub fn generate_id() -> String {
        Uuid::new_v4().to_string()
    }

    /// Get a sender for reporting subagent results
    pub fn result_sender(&self) -> mpsc::Sender<SubagentResult> {
        self.result_tx.clone()
    }

    /// Get a sender for streaming events
    pub fn event_sender(&self) -> mpsc::Sender<SubagentEvent> {
        self.event_tx.clone()
    }

    /// Get a cloneable handle to the event receiver
    ///
    /// Returns an Arc<Mutex<Receiver>> that can be used in spawned tasks.
    /// Only one task should actively receive from this at a time.
    pub fn event_receiver(&self) -> Arc<tokio::sync::Mutex<mpsc::Receiver<SubagentEvent>>> {
        self.event_rx.clone()
    }

    /// Receive the next event (non-blocking with timeout)
    pub async fn recv_event_timeout(&self, timeout: Duration) -> Option<SubagentEvent> {
        let mut rx = self.event_rx.lock().await;
        tokio::time::timeout(timeout, rx.recv())
            .await
            .ok()
            .flatten()
    }

    /// Receive all pending events without blocking
    pub async fn drain_events(&self) -> Vec<SubagentEvent> {
        let mut events = Vec::new();
        let mut rx = self.event_rx.lock().await;
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    }

    /// Wait for N subagents to complete with default timeout
    pub async fn wait_for_all(&self, count: usize) -> Vec<SubagentResult> {
        self.wait_for_all_timeout(count, Duration::from_secs(DEFAULT_WAIT_TIMEOUT_SECS))
            .await
    }

    /// Wait for N subagents to complete with custom timeout
    ///
    /// Returns all results collected before timeout. If timeout occurs,
    /// partial results are returned with error markers for missing tasks.
    pub async fn wait_for_all_timeout(
        &self,
        count: usize,
        timeout: Duration,
    ) -> Vec<SubagentResult> {
        let mut collected = Vec::with_capacity(count);

        // Use tokio::select to implement overall timeout
        let collect_future = async {
            let mut rx = self.result_rx.lock().await;
            for i in 0..count {
                match rx.recv().await {
                    Some(result) => {
                        tracing::debug!(
                            "[Tracker] Received result {}/{} from subagent {}",
                            i + 1,
                            count,
                            result.id
                        );
                        self.results
                            .write()
                            .await
                            .insert(result.id.clone(), result.clone());
                        collected.push(result);
                    }
                    None => {
                        // Channel closed, no more results coming
                        tracing::warn!(
                            "[Tracker] Channel closed unexpectedly after receiving {}/{} results. \
                             This usually means all result senders were dropped before tasks completed.",
                            collected.len(),
                            count
                        );
                        break;
                    }
                }
            }
        };

        // Wrap with timeout
        match tokio::time::timeout(timeout, collect_future).await {
            Ok(()) => {
                if collected.len() < count {
                    tracing::warn!(
                        "[Tracker] Only collected {}/{} results (channel closed)",
                        collected.len(),
                        count
                    );
                } else {
                    tracing::debug!("[Tracker] Successfully collected all {} results", count);
                }
                collected
            }
            Err(_) => {
                tracing::warn!(
                    "[Tracker] wait_for_all timed out after {:?}, collected {} of {} results",
                    timeout,
                    collected.len(),
                    count
                );
                collected
            }
        }
    }

    /// Get result by ID (non-blocking)
    pub async fn get_result(&self, id: &str) -> Option<SubagentResult> {
        self.results.read().await.get(id).cloned()
    }

    /// Get count of collected results so far
    pub async fn result_count(&self) -> usize {
        self.results.read().await.len()
    }
}

impl Default for SubagentTracker {
    fn default() -> Self {
        Self::new()
    }
}
