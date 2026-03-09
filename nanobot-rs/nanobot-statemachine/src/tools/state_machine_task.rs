//! State machine task tool.
//!
//! Allows agents to interact with the state machine task board:
//! create, get, list, transition, and query flow logs.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::instrument;

use crate::events::StateMachineEvent;
use crate::models::{StateMachineTask, TaskPriority};
use crate::store::StateMachineStore;
use crate::types::StateMachineConfig;
use nanobot_core::tools::{Tool, ToolError, ToolResult};

pub struct StateMachineTaskTool {
    store: StateMachineStore,
    event_tx: mpsc::Sender<StateMachineEvent>,
    config: Arc<StateMachineConfig>,
}

impl StateMachineTaskTool {
    pub fn new(
        store: StateMachineStore,
        event_tx: mpsc::Sender<StateMachineEvent>,
        config: Arc<StateMachineConfig>,
    ) -> Self {
        Self {
            store,
            event_tx,
            config,
        }
    }

    /// Build the list of valid states from the config.
    fn valid_states(&self) -> Vec<&str> {
        let mut states = Vec::new();
        for t in &self.config.transitions {
            if !states.contains(&t.from.as_str()) {
                states.push(t.from.as_str());
            }
            if !states.contains(&t.to.as_str()) {
                states.push(t.to.as_str());
            }
        }
        states
    }

    /// Build the list of unique roles from the config.
    fn valid_roles(&self) -> Vec<&str> {
        let mut roles: Vec<&str> = self
            .config
            .state_roles
            .values()
            .map(|s| s.as_str())
            .collect();
        roles.sort();
        roles.dedup();
        roles
    }
}

#[derive(Deserialize)]
struct TaskArgs {
    action: String,
    // create fields
    title: Option<String>,
    description: Option<String>,
    priority: Option<String>,
    origin_channel: Option<String>,
    origin_chat_id: Option<String>,
    // get / transition / flow_log fields
    task_id: Option<String>,
    // list fields
    state: Option<String>,
    role: Option<String>,
    // transition fields
    to_state: Option<String>,
    agent_role: Option<String>,
    reason: Option<String>,
}

#[async_trait]
impl Tool for StateMachineTaskTool {
    fn name(&self) -> &str {
        "state_machine_task"
    }

    fn description(&self) -> &str {
        "Interact with the state machine task board: create, get, list, transition state, or query flow logs."
    }

    fn parameters(&self) -> Value {
        let valid_states = self.valid_states();
        let valid_roles = self.valid_roles();

        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "get", "list", "transition", "flow_log"],
                    "description": "The action to perform"
                },
                "title": {
                    "type": "string",
                    "description": "Task title (for create)"
                },
                "description": {
                    "type": "string",
                    "description": "Task description (for create)"
                },
                "priority": {
                    "type": "string",
                    "enum": ["low", "normal", "high", "critical"],
                    "description": "Task priority (for create, default: normal)"
                },
                "origin_channel": {
                    "type": "string",
                    "description": "Origin channel for routing results back (for create)"
                },
                "origin_chat_id": {
                    "type": "string",
                    "description": "Origin chat ID (for create)"
                },
                "task_id": {
                    "type": "string",
                    "description": "Task ID (for get, transition, flow_log)"
                },
                "state": {
                    "type": "string",
                    "enum": valid_states,
                    "description": "Filter by state (for list). Valid states are defined by the state machine config."
                },
                "role": {
                    "type": "string",
                    "enum": valid_roles,
                    "description": "Filter by assigned role (for list)"
                },
                "to_state": {
                    "type": "string",
                    "enum": valid_states,
                    "description": "Target state (for transition). Must be a valid transition from current state."
                },
                "agent_role": {
                    "type": "string",
                    "description": "Role performing the transition (for transition)"
                },
                "reason": {
                    "type": "string",
                    "description": "Reason for the transition (for transition)"
                }
            },
            "required": ["action"]
        })
    }

    #[instrument(name = "tool.state_machine_task", skip_all)]
    async fn execute(&self, args: Value) -> ToolResult {
        let args: TaskArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        match args.action.as_str() {
            "create" => self.do_create(args).await,
            "get" => self.do_get(args).await,
            "list" => self.do_list(args).await,
            "transition" => self.do_transition(args).await,
            "flow_log" => self.do_flow_log(args).await,
            other => Err(ToolError::InvalidArguments(format!(
                "Unknown action: {other}"
            ))),
        }
    }
}

impl StateMachineTaskTool {
    async fn do_create(&self, args: TaskArgs) -> ToolResult {
        let title = args
            .title
            .ok_or_else(|| ToolError::InvalidArguments("title is required".into()))?;
        let now = Utc::now();
        let id = uuid::Uuid::new_v4().to_string();

        let task = StateMachineTask {
            id: id.clone(),
            title,
            description: args.description.unwrap_or_default(),
            state: "pending".to_string(),
            priority: args
                .priority
                .as_deref()
                .map(TaskPriority::from_str_lossy)
                .unwrap_or_default(),
            assigned_role: None,
            review_count: 0,
            retry_count: 0,
            last_heartbeat: now,
            created_at: now,
            updated_at: now,
            result: None,
            origin_channel: args.origin_channel,
            origin_chat_id: args.origin_chat_id,
            session_id: None,
        };

        self.store
            .create_task(&task)
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;

        // Emit TaskCreated event to trigger initial dispatch
        let _ = self
            .event_tx
            .send(StateMachineEvent::TaskCreated {
                task_id: id.clone(),
                session_id: None,
            })
            .await;

        Ok(serde_json::to_string_pretty(&serde_json::json!({
            "status": "created",
            "task_id": id,
        }))
        .unwrap())
    }

    async fn do_get(&self, args: TaskArgs) -> ToolResult {
        let id = args
            .task_id
            .ok_or_else(|| ToolError::InvalidArguments("task_id is required".into()))?;

        let task = self
            .store
            .get_task(&id)
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Task {id} not found")))?;

        Ok(serde_json::to_string_pretty(&task).unwrap())
    }

    async fn do_list(&self, args: TaskArgs) -> ToolResult {
        let tasks = if let Some(state_str) = &args.state {
            self.store
                .list_tasks_by_state(state_str)
                .await
                .map_err(|e| ToolError::ExecutionError(e.to_string()))?
        } else if let Some(role) = &args.role {
            self.store
                .list_tasks_by_role(role)
                .await
                .map_err(|e| ToolError::ExecutionError(e.to_string()))?
        } else {
            // Default: list all tasks in common active states
            let mut all_tasks = Vec::new();
            // Get all unique states from config
            let states: Vec<&str> = self.valid_states();
            for state in states {
                if let Ok(tasks) = self.store.list_tasks_by_state(state).await {
                    all_tasks.extend(tasks);
                }
            }
            all_tasks
        };

        Ok(serde_json::to_string_pretty(&tasks).unwrap())
    }

    async fn do_transition(&self, args: TaskArgs) -> ToolResult {
        let id = args
            .task_id
            .ok_or_else(|| ToolError::InvalidArguments("task_id is required".into()))?;
        let to_state = args
            .to_state
            .ok_or_else(|| ToolError::InvalidArguments("to_state is required".into()))?;
        let agent_role = args
            .agent_role
            .ok_or_else(|| ToolError::InvalidArguments("agent_role is required".into()))?;

        // Fetch current task to validate transition
        let task = self
            .store
            .get_task(&id)
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Task {id} not found")))?;

        // Validate the transition is allowed
        if !self.config.can_transition(&task.state, &to_state) {
            let allowed = self.config.allowed_transitions(&task.state);
            return Err(ToolError::ExecutionError(format!(
                "Invalid transition: '{}' → '{}'. Allowed transitions from '{}': {:?}",
                task.state, to_state, task.state, allowed
            )));
        }

        let ok = self
            .store
            .update_task_state(&id, &task.state, &to_state, Some(&agent_role))
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;

        if !ok {
            return Err(ToolError::ExecutionError(
                "Concurrent modification: task state changed".into(),
            ));
        }

        // Write audit log
        self.store
            .append_flow_log(
                &id,
                &task.state,
                &to_state,
                &agent_role,
                args.reason.as_deref(),
            )
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;

        // Emit TaskTransitioned event
        let _ = self
            .event_tx
            .send(StateMachineEvent::TaskTransitioned {
                task_id: id.clone(),
                from_state: task.state.clone(),
                to_state: to_state.clone(),
                agent_role: agent_role.clone(),
            })
            .await;

        Ok(serde_json::to_string_pretty(&serde_json::json!({
            "status": "transitioned",
            "task_id": id,
            "from": task.state,
            "to": to_state,
        }))
        .unwrap())
    }

    async fn do_flow_log(&self, args: TaskArgs) -> ToolResult {
        let id = args
            .task_id
            .ok_or_else(|| ToolError::InvalidArguments("task_id is required".into()))?;

        let log = self
            .store
            .get_flow_log(&id)
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;

        Ok(serde_json::to_string_pretty(&log).unwrap())
    }
}
