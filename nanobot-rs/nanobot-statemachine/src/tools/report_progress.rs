//! Report progress tool.
//!
//! Allows agents to report progress on tasks, which also updates the heartbeat.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::instrument;

use crate::events::StateMachineEvent;
use crate::store::StateMachineStore;
use nanobot_core::tools::{Tool, ToolError, ToolResult};

pub struct ReportProgressTool {
    store: StateMachineStore,
    event_tx: mpsc::Sender<StateMachineEvent>,
}

impl ReportProgressTool {
    pub fn new(store: StateMachineStore, event_tx: mpsc::Sender<StateMachineEvent>) -> Self {
        Self { store, event_tx }
    }
}

#[derive(Deserialize)]
struct ProgressArgs {
    task_id: String,
    content: String,
    percentage: Option<f32>,
}

#[async_trait]
impl Tool for ReportProgressTool {
    fn name(&self) -> &str {
        "report_progress"
    }

    fn description(&self) -> &str {
        "Report progress on a state machine task. This also updates the task heartbeat to prevent stall detection."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The ID of the task to report progress on"
                },
                "content": {
                    "type": "string",
                    "description": "Human-readable progress description (e.g., 'Completed analysis phase')"
                },
                "percentage": {
                    "type": "number",
                    "description": "Optional completion percentage (0-100)"
                }
            },
            "required": ["task_id", "content"]
        })
    }

    #[instrument(name = "tool.report_progress", skip_all)]
    async fn execute(&self, args: Value) -> ToolResult {
        let args: ProgressArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        // Verify task exists
        let task = self
            .store
            .get_task(&args.task_id)
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Task {} not found", args.task_id)))?;

        // Append progress (this also updates heartbeat)
        self.store
            .append_progress(
                &args.task_id,
                task.assigned_role.as_deref().unwrap_or("unknown"),
                &args.content,
                args.percentage,
            )
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;

        // Emit ProgressReported event
        let _ = self
            .event_tx
            .send(StateMachineEvent::ProgressReported {
                task_id: args.task_id.clone(),
                agent_role: task.assigned_role.unwrap_or_else(|| "unknown".to_string()),
                content: args.content.clone(),
            })
            .await;

        Ok(serde_json::to_string_pretty(&serde_json::json!({
            "status": "reported",
            "task_id": args.task_id,
            "content": args.content,
            "percentage": args.percentage,
        }))
        .unwrap())
    }
}
