//! Pipeline bootstrap: initializes and starts the pipeline subsystem.
//!
//! This module wires together all pipeline components:
//! - `PipelineStore` for persistence
//! - `OrchestratorActor` for event-driven dispatch
//! - `StallDetector` for timeout monitoring
//! - `PipelineTaskTool` and `ReportProgressTool` for agent interaction

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::info;

use super::config::PipelineConfig;
use super::graph::PipelineGraph;
use super::orchestrator::{OrchestratorActor, PipelineEvent};
use super::permission::PermissionMatrix;
use super::stall_detector::StallDetector;
use super::store::PipelineStore;
use crate::tools::{PipelineTaskTool, ReportProgressTool};
use nanobot_core::agent::subagent::SubagentManager;
use nanobot_core::tools::ToolRegistry;

/// Pipeline subsystem handle.
///
/// Returned by [`bootstrap`] when the pipeline is successfully initialized.
/// Holds references needed for external interaction.
pub struct PipelineHandle {
    /// Sender for pipeline events (cloneable).
    pub event_tx: mpsc::Sender<PipelineEvent>,
    /// The resolved pipeline graph.
    pub graph: Arc<PipelineGraph>,
    /// The store for pipeline entities.
    pub store: PipelineStore,
}

/// Initialize the pipeline subsystem.
///
/// Returns `None` if pipeline is disabled in config.
///
/// # Arguments
///
/// * `config` - Pipeline configuration from the main config file
/// * `pool` - SQLite pool shared with the main store
/// * `subagent_manager` - Manager for spawning sub-agents
/// * `tool_registry` - Tool registry to register pipeline tools
/// * `soul_templates` - Role name → SOUL.md content mapping
///
/// # Example
///
/// ```ignore
/// let handle = pipeline::bootstrap(
///     &config.pipeline.unwrap_or_default(),
///     memory_store.pool().clone(),
///     subagent_manager,
///     &mut tool_registry,
///     soul_templates,
/// ).await?;
/// ```
pub async fn bootstrap(
    config: &PipelineConfig,
    pool: sqlx::SqlitePool,
    subagent_manager: Arc<SubagentManager>,
    tool_registry: &mut ToolRegistry,
    soul_templates: HashMap<String, String>,
) -> anyhow::Result<Option<PipelineHandle>> {
    if !config.enabled {
        info!("Pipeline subsystem disabled");
        return Ok(None);
    }

    info!("Initializing pipeline subsystem...");

    // 1. Resolve the pipeline graph (validates config)
    let graph = config.resolve_graph()?;

    // 2. Initialize the store (creates tables)
    let store = PipelineStore::new(pool);
    store.init_tables().await?;

    // 3. Create event channel
    let (event_tx, event_rx) = mpsc::channel(256);

    // 4. Build permission matrix
    let permission_matrix = PermissionMatrix::default();

    // 5. Spawn the orchestrator
    let orchestrator = OrchestratorActor::new(
        store.clone(),
        permission_matrix,
        subagent_manager,
        config.clone(),
        event_rx,
        soul_templates,
        graph.clone(),
    );
    tokio::spawn(orchestrator.run());

    // 6. Spawn the stall detector
    let detector = StallDetector::new(
        store.clone(),
        event_tx.clone(),
        config.stall_timeout_secs,
        graph.active_states.clone(),
    );
    tokio::spawn(detector.run());

    // 7. Register tools
    let graph_arc = Arc::new(graph.clone());
    tool_registry.register(Box::new(PipelineTaskTool::new(
        store.clone(),
        event_tx.clone(),
        graph_arc.clone(),
    )) as Box<dyn nanobot_core::tools::Tool>);
    tool_registry.register(
        Box::new(ReportProgressTool::new(store.clone(), event_tx.clone()))
            as Box<dyn nanobot_core::tools::Tool>,
    );

    info!(
        "Pipeline subsystem initialized (entry_state={}, terminal_states={:?})",
        graph.entry_state, graph.terminal_states
    );

    Ok(Some(PipelineHandle {
        event_tx,
        graph: graph_arc,
        store,
    }))
}

/// Load soul templates from a directory.
///
/// Reads all `*.md` files from the given directory and maps filename (without extension)
/// to file content. This is used to load role-specific prompts for pipeline agents.
///
/// # Example
///
/// ```ignore
/// let templates = load_soul_templates("~/.nanobot/pipeline_templates");
/// // Returns: {"taizi": "...", "zhongshu": "...", ...}
/// ```
pub fn load_soul_templates(dir: &std::path::Path) -> HashMap<String, String> {
    let mut templates = HashMap::new();

    if !dir.exists() {
        return templates;
    }

    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "md") {
                    if let Some(stem) = path.file_stem() {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            templates.insert(stem.to_string_lossy().to_string(), content);
                        }
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to read soul templates directory: {}", e);
        }
    }

    templates
}
