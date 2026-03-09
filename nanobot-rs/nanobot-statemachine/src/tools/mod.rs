//! Tools for interacting with the state machine.
//!
//! Provides:
//! - `StateMachineTaskTool`: Create, get, list, and transition tasks
//! - `ReportProgressTool`: Report progress from agents

mod state_machine_task;
mod report_progress;

pub use state_machine_task::StateMachineTaskTool;
pub use report_progress::ReportProgressTool;
