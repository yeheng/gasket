//! Stall detector for monitoring task activity.
//!
//! Periodically scans for tasks whose heartbeat has exceeded
//! the configured timeout and emits StallDetected events.

use std::collections::HashSet;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, error, info};

use crate::events::StateMachineEvent;
use crate::store::StateMachineStore;

/// Stall detector that monitors task heartbeats.
pub struct StallDetector {
    store: StateMachineStore,
    event_tx: mpsc::Sender<StateMachineEvent>,
    timeout_secs: u64,
    active_states: HashSet<String>,
}

impl StallDetector {
    /// Create a new stall detector.
    pub fn new(
        store: StateMachineStore,
        event_tx: mpsc::Sender<StateMachineEvent>,
        timeout_secs: u64,
        active_states: HashSet<String>,
    ) -> Self {
        Self {
            store,
            event_tx,
            timeout_secs,
            active_states,
        }
    }

    /// Run the stall detection loop. Checks every 30 seconds or 1/3 of timeout, whichever is smaller.
    pub async fn run(self) {
        let check_interval = Duration::from_secs(30.min(self.timeout_secs / 3));
        let mut ticker = interval(check_interval);

        info!(
            "Stall detector started (timeout={}s, check_interval={:?})",
            self.timeout_secs, check_interval
        );

        loop {
            ticker.tick().await;
            if let Err(e) = self.check_for_stalled_tasks().await {
                error!("Stall detector error: {}", e);
            }
        }
    }

    /// Check for stalled tasks and emit events.
    async fn check_for_stalled_tasks(&self) -> anyhow::Result<()> {
        let stalled = self
            .store
            .find_stalled_tasks(self.timeout_secs, &self.active_states)
            .await?;

        if stalled.is_empty() {
            debug!("No stalled tasks found");
            return Ok(());
        }

        info!("Found {} stalled task(s)", stalled.len());

        for task in stalled {
            debug!("Detected stall on task {} (state={})", task.id, task.state);

            let _ = self
                .event_tx
                .send(StateMachineEvent::StallDetected {
                    task_id: task.id.clone(),
                })
                .await;
        }

        Ok(())
    }
}
