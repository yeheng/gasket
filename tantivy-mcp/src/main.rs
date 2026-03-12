//! Tantivy MCP - Standalone MCP Index Server
//!
//! A Model Context Protocol (MCP) server providing full-text search capabilities
//! using the Tantivy search engine.
//!
//! # Running Modes
//!
//! - `stdio`: Run as a subprocess communicating via stdin/stdout (default)
//! - `server`: Run as an HTTP server using axum

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use directories::UserDirs;
use tantivy_mcp::index::IndexManager;
use tantivy_mcp::maintenance::{JobRegistry, MaintenanceConfig, MaintenanceScheduler};
use tantivy_mcp::mcp::{McpHandler, ToolRegistry};
use tantivy_mcp::register_tools;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Running mode for the MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunMode {
    /// Run as a subprocess communicating via stdin/stdout (default)
    #[default]
    Stdio,
    /// Run as an HTTP server
    Server,
}

impl std::str::FromStr for RunMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "stdio" => Ok(RunMode::Stdio),
            "server" => Ok(RunMode::Server),
            _ => Err(format!(
                "Invalid run mode '{}'. Expected 'stdio' or 'server'",
                s
            )),
        }
    }
}

/// Tantivy MCP Index Server
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Index storage directory
    #[arg(short, long)]
    index_dir: Option<PathBuf>,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Enable automatic maintenance (compaction, expiration)
    #[arg(long, default_value = "true")]
    auto_maintain: bool,

    /// Maintenance interval in seconds
    #[arg(long, default_value = "3600")]
    maintenance_interval: u64,

    /// Running mode: 'stdio' (subprocess via stdin/stdout) or 'server' (HTTP server)
    #[arg(short, long, default_value = "stdio", value_name = "TYPE")]
    r#type: RunMode,

    /// HTTP server bind address (only used when --type=server)
    #[arg(long, default_value = "127.0.0.1:3000")]
    bind: String,

    /// Enable logging to file
    #[arg(long)]
    log_file: bool,

    /// Log file path (default: <index_dir>/tantivy-mcp.log)
    #[arg(long)]
    log_file_path: Option<PathBuf>,

    /// Maximum number of log files to keep (for rotation)
    #[arg(long, default_value = "7")]
    log_max_files: usize,
}

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub manager: Arc<IndexManager>,
    pub cancel_token: CancellationToken,
}

#[tokio::main]
async fn main() -> tantivy_mcp::Result<()> {
    let args = Args::parse();

    // Determine index directory early (needed for log file path)
    let index_dir = args.index_dir.clone().unwrap_or_else(|| {
        UserDirs::new()
            .map(|dirs| dirs.home_dir().join(".nanobot/tantivy"))
            .unwrap_or_else(|| PathBuf::from(".nanobot/tantivy"))
    });

    // Initialize logging
    init_logging(&args, &index_dir)?;

    info!("Starting tantivy-mcp {}", env!("CARGO_PKG_VERSION"));
    info!("Running mode: {:?}", args.r#type);

    info!("Index directory: {:?}", index_dir);

    // Create job registry first
    let job_registry = Arc::new(JobRegistry::new());

    // Create index manager with job registry
    let manager = IndexManager::new(&index_dir, job_registry.clone());
    manager.load_indexes()?;

    // Wrap in Arc for shared ownership
    let manager = Arc::new(manager);

    // Create cancellation token for graceful shutdown
    let cancel_token = CancellationToken::new();

    // Start maintenance scheduler if enabled
    let (scheduler_handle, scheduler_token) = if args.auto_maintain {
        let config = MaintenanceConfig {
            auto_compact: true,
            deleted_ratio_threshold: 0.2,
            max_segments: 10,
            auto_expire: true,
            expire_interval_secs: args.maintenance_interval,
        };
        let scheduler = MaintenanceScheduler::new(manager.clone(), config);
        let (handle, token) = scheduler.start();
        info!(
            "Maintenance scheduler started (interval: {}s)",
            args.maintenance_interval
        );
        (Some(handle), Some(token))
    } else {
        (None, None)
    };

    // Run based on mode
    let result = match args.r#type {
        RunMode::Stdio => {
            run_stdio_mode(manager, cancel_token, scheduler_token, scheduler_handle).await
        }
        RunMode::Server => {
            run_http_mode(
                args.bind,
                manager,
                cancel_token,
                scheduler_token,
                scheduler_handle,
            )
            .await
        }
    };

    info!("Shutting down tantivy-mcp");
    result
}

/// Initialize logging with optional file output
fn init_logging(args: &Args, index_dir: &PathBuf) -> tantivy_mcp::Result<()> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&args.log_level));

    if args.log_file {
        // Determine log file path
        let log_path = args
            .log_file_path
            .clone()
            .unwrap_or_else(|| index_dir.join("tantivy-mcp.log"));

        // Ensure parent directory exists
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                tantivy_mcp::Error::ConfigError(format!(
                    "Failed to create log directory {:?}: {}",
                    parent, e
                ))
            })?;
        }

        // Create rolling file appender with daily rotation
        let log_dir = log_path.parent().unwrap_or(std::path::Path::new("."));
        let log_filename = log_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("tantivy-mcp");

        let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix(log_filename)
            .filename_suffix("log")
            .max_log_files(args.log_max_files)
            .build(log_dir)
            .map_err(|e| {
                tantivy_mcp::Error::ConfigError(format!("Failed to create log file: {}", e))
            })?;

        // Wrap in non-blocking writer
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

        // Create layers: stderr + file
        let stderr_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true);

        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false)
            .with_target(true)
            .with_thread_ids(false)
            .with_line_number(true);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(stderr_layer)
            .with(file_layer)
            .init();

        info!("Logging to file: {:?}", log_path);
        info!("Log rotation: daily, max files: {}", args.log_max_files);
    } else {
        // Stderr only
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .init();
    }

    Ok(())
}

/// Run in stdio mode (subprocess communicating via stdin/stdout)
async fn run_stdio_mode(
    manager: Arc<IndexManager>,
    cancel_token: CancellationToken,
    scheduler_token: Option<CancellationToken>,
    scheduler_handle: Option<tokio::task::JoinHandle<()>>,
) -> tantivy_mcp::Result<()> {
    info!("Running in stdio mode");

    // Create tool registry and register tools
    let mut tools = ToolRegistry::new();
    register_tools(&mut tools, manager);

    // Create MCP handler
    let mut handler = McpHandler::new(tools);

    // Set up graceful shutdown
    let shutdown_token = cancel_token.clone();
    let shutdown = setup_shutdown_handler();

    // Run MCP server in a separate task
    let server_token = cancel_token.clone();
    let server_task = tokio::spawn(async move { handler.run(server_token).await });

    // Wait for either server completion or shutdown signal
    tokio::select! {
        result = server_task => {
            if let Some(ref token) = scheduler_token {
                token.cancel();
            }
            match result {
                Ok(Ok(())) => info!("MCP server completed normally"),
                Ok(Err(e)) => {
                    tracing::error!("MCP server error: {}", e);
                    return Err(e);
                }
                Err(e) => {
                    tracing::error!("MCP server task panicked: {}", e);
                    return Err(tantivy_mcp::Error::McpError(format!("Server panic: {}", e)));
                }
            }
        }
        _ = shutdown => {
            info!("Received shutdown signal");
            shutdown_token.cancel();
            if let Some(token) = scheduler_token {
                token.cancel();
            }
        }
    }

    // Wait for maintenance scheduler to stop
    wait_for_scheduler(scheduler_handle).await;

    Ok(())
}

/// Run in HTTP server mode using axum
async fn run_http_mode(
    bind: String,
    manager: Arc<IndexManager>,
    cancel_token: CancellationToken,
    scheduler_token: Option<CancellationToken>,
    scheduler_handle: Option<tokio::task::JoinHandle<()>>,
) -> tantivy_mcp::Result<()> {
    use axum::routing::{get, post};
    use axum::Router;
    use tower_http::cors::{Any, CorsLayer};

    info!("Running in HTTP server mode");

    // Create shared state
    let state = AppState {
        manager,
        cancel_token: cancel_token.clone(),
    };

    // Build router with state
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/mcp", post(handle_mcp_request))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    // Parse bind address
    let addr: SocketAddr = bind
        .parse()
        .map_err(|e| tantivy_mcp::Error::ConfigError(format!("Invalid bind address: {}", e)))?;

    info!("HTTP server listening on {}", addr);

    // Create the server with graceful shutdown
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| tantivy_mcp::Error::ConfigError(format!("Failed to bind: {}", e)))?;

    // Set up graceful shutdown
    let shutdown_token = cancel_token.clone();
    let shutdown = setup_shutdown_handler();

    // Run server
    tokio::select! {
        result = axum::serve(listener, app) => {
            if let Err(e) = result {
                tracing::error!("HTTP server error: {}", e);
            }
            if let Some(token) = scheduler_token {
                token.cancel();
            }
        }
        _ = shutdown => {
            info!("Received shutdown signal");
            shutdown_token.cancel();
            if let Some(token) = scheduler_token {
                token.cancel();
            }
        }
    }

    // Wait for maintenance scheduler to stop
    wait_for_scheduler(scheduler_handle).await;

    info!("HTTP server stopped");

    Ok(())
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

/// Handle MCP JSON-RPC request over HTTP
async fn handle_mcp_request(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::Json(request): axum::Json<tantivy_mcp::mcp::JsonRpcRequest>,
) -> axum::Json<tantivy_mcp::mcp::JsonRpcResponse> {
    use serde_json::json;
    use tantivy_mcp::mcp::{JsonRpcError, JsonRpcResponse, ToolResult};

    info!("Handling MCP request: {}", request.method);

    let response = match request.method.as_str() {
        "initialize" => {
            let result = tantivy_mcp::mcp::InitializeResult {
                protocol_version: "2024-11-05".to_string(),
                capabilities: tantivy_mcp::mcp::ServerCapabilities {
                    tools: Some(tantivy_mcp::mcp::ToolsCapability {}),
                },
                server_info: tantivy_mcp::mcp::ServerInfo {
                    name: "tantivy-mcp".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
            };
            JsonRpcResponse::success(request.id, serde_json::to_value(result).unwrap())
        }
        "notifications/initialized" => {
            // No response for notifications
            return axum::Json(JsonRpcResponse::success(
                request.id,
                json!({"status": "initialized"}),
            ));
        }
        "tools/list" => {
            // Create a temporary tool registry for listing
            let mut tools = ToolRegistry::new();
            register_tools(&mut tools, state.manager.clone());
            let tools_list = tools.list_tools();
            JsonRpcResponse::success(request.id, json!({ "tools": tools_list }))
        }
        "tools/call" => {
            let params = match &request.params {
                Some(p) => p,
                None => {
                    return axum::Json(JsonRpcResponse::error(
                        request.id,
                        JsonRpcError::invalid_params("Missing params"),
                    ));
                }
            };

            let tool_name = match params.get("name").and_then(|v| v.as_str()) {
                Some(name) => name,
                None => {
                    return axum::Json(JsonRpcResponse::error(
                        request.id,
                        JsonRpcError::invalid_params("Missing tool name"),
                    ));
                }
            };

            let arguments = params.get("arguments").cloned();

            info!("Calling tool: {}", tool_name);

            // Create a temporary tool registry for calling
            let mut tools = ToolRegistry::new();
            register_tools(&mut tools, state.manager.clone());

            match tools.call_tool(tool_name, arguments) {
                Ok(result) => {
                    JsonRpcResponse::success(request.id, serde_json::to_value(result).unwrap())
                }
                Err(e) => {
                    let error_result = ToolResult::error(e.to_string());
                    JsonRpcResponse::success(
                        request.id,
                        serde_json::to_value(error_result).unwrap(),
                    )
                }
            }
        }
        method => JsonRpcResponse::error(request.id, JsonRpcError::method_not_found(method)),
    };

    axum::Json(response)
}

/// Wait for maintenance scheduler to stop gracefully
async fn wait_for_scheduler(scheduler_handle: Option<tokio::task::JoinHandle<()>>) {
    if let Some(handle) = scheduler_handle {
        if tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .is_ok()
        {
            info!("Maintenance scheduler stopped");
        } else {
            tracing::warn!("Maintenance scheduler stop timed out");
        }
    }
}

/// Setup graceful shutdown handler.
async fn setup_shutdown_handler() {
    #[cfg(unix)]
    {
        let ctrl_c = async {
            signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C handler");
        };

        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to install signal handler")
                .recv()
                .await;
        };

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }
    }

    #[cfg(not(unix))]
    {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    }
}
