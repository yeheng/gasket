//! Events for the state machine.
//!
//! These events drive the state machine lifecycle and are processed
//! by the state machine engine.

use serde::{Deserialize, Serialize};

/// Events processed by the state machine engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StateMachineEvent {
    /// A new task was created.
    TaskCreated {
        task_id: String,
        session_id: Option<String>,
    },

    /// A task transitioned to a new state.
    TaskTransitioned {
        task_id: String,
        from_state: String,
        to_state: String,
        agent_role: String,
    },

    /// An agent reported progress.
    ProgressReported {
        task_id: String,
        agent_role: String,
        content: String,
    },

    /// Stall detector found a stalled task.
    StallDetected { task_id: String },

    /// Request to transition a task (from external source).
    TransitionRequest {
        task_id: String,
        to_state: String,
        agent_role: String,
        reason: Option<String>,
    },

    /// Request to create a task (from external source).
    CreateTaskRequest {
        title: String,
        description: String,
        priority: Option<String>,
        origin_channel: Option<String>,
        origin_chat_id: Option<String>,
        session_id: Option<String>,
    },
}

impl StateMachineEvent {
    /// Get the task_id associated with this event, if any.
    pub fn task_id(&self) -> Option<&str> {
        match self {
            Self::TaskCreated { task_id, .. } => Some(task_id),
            Self::TaskTransitioned { task_id, .. } => Some(task_id),
            Self::ProgressReported { task_id, .. } => Some(task_id),
            Self::StallDetected { task_id } => Some(task_id),
            Self::TransitionRequest { task_id, .. } => Some(task_id),
            Self::CreateTaskRequest { .. } => None,
        }
    }
}
