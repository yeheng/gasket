//! conga WebSocket Gateway - bridges the existing Vue 3 frontend to the
//! conga agent loop via WebSocket + JSON.
//!
//! ## Architecture
//!
//! Each WebSocket connection is one session with one `Host`. The main tokio
//! task loops on incoming messages. When a `"message"` arrives it runs the
//! turn inline (`run_turn`) and multiplexes it against incoming frames
//! (cancel, approvals) in a secondary select loop.
//!
//! ## Wire protocol (frontend ↔ gateway)
//!
//! ### Client -> Server
//! ```json
//! {"type":"message","content":"...","trace_id":"..."}
//! {"type":"cancel"}
//! ```
//!
//! ### Server -> Client (streamed per turn)
//! ```json
//! {"type":"thinking","content":"..."}
//! {"type":"tool_start","name":"...","arguments":"..."}
//! {"type":"tool_end","name":"...","output":"..."}
//! {"type":"content","content":"..."}
//! {"type":"error","content":"...","message":"..."}
//! {"type":"done"}
//! ```
//!
//! ### 契约核对表（前端 `useChatSession.ts` / `types/index.ts` 全部消息类型）
//!
//! | 消息 | 方向 | 状态 |
//! |---|---|---|
//! | `message` / `cancel` | C->S | ✅ 已实现 |
//! | `approval_request` / `approval_response` | 双向 | ✅ 已实现（本任务） |
//! | `thinking` / `tool_start` / `tool_end` / `content` / `error` / `done` | S->C | ✅ 已实现 |
//! | `subagent_*`（10 种） | S->C | ✅ 已实现（core 子 agent 编排 + gateway 事件转发 + 前端渲染） |

use std::sync::Arc;

use axum::http::{HeaderValue, Method};
use axum::routing::{delete, get, post, put};
use axum::Router;
use dashmap::DashMap;
use tracing::{info, warn};

use crate::api::{
    compact_context, delete_session, get_cache_stats, get_commands, get_context, get_messages,
    get_settings, list_sessions, put_settings, rename_session, search_sessions,
};
use crate::state::AppState;
use crate::ws::ws_handler;

mod api;
mod auth;
mod state;
mod wire;
mod ws;

/// Cross-origin callers allowed by default: the Vite dev server (1420) is a
/// different origin than the gateway (3000), so browser-mode dev genuinely
/// needs CORS. In production the bundled frontend is served same-origin by
/// the gateway itself and needs none.
///
/// This used to be `CorsLayer::permissive()`, which let ANY website read
/// authenticated responses and widened the DNS-rebinding surface. Extra
/// origins (e.g. a `TAURI_DEV_HOST` LAN address) can be added with
/// `CONGA_GATEWAY_CORS_ORIGINS`.
const DEFAULT_CORS_ORIGINS: &[&str] = &["http://localhost:1420", "http://127.0.0.1:1420"];

fn cors_layer() -> tower_http::cors::CorsLayer {
    let mut origins: Vec<HeaderValue> = DEFAULT_CORS_ORIGINS
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();
    if let Ok(extra) = std::env::var("CONGA_GATEWAY_CORS_ORIGINS") {
        for raw in extra.split(',') {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }
            match raw.parse() {
                Ok(v) => origins.push(v),
                Err(_) => warn!("ignoring unparseable CONGA_GATEWAY_CORS_ORIGINS entry: {raw}"),
            }
        }
    }
    tower_http::cors::CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
        ])
}

// ── Axum server ────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let _ = dotenvy::dotenv();

    let (auth_token, token_source) = match auth::load_or_create_token() {
        Ok((t, src)) => (Arc::new(t), src),
        Err(e) => {
            eprintln!("conga-gateway: cannot establish auth token: {e}");
            std::process::exit(1);
        }
    };
    // Never log the token value itself — only where it came from.
    info!("gateway auth: token required for /ws and /api/* (source: {token_source})");

    let state = Arc::new(AppState {
        sessions: DashMap::new(),
        store_root: conga::JsonlStorage::default_root().base_dir_clone(),
        index_db: conga::storage::config_dir().join("index.db"),
        auth_token,
    });
    let frontend_dist =
        std::env::var("CONGA_GATEWAY_STATIC_DIR").unwrap_or_else(|_| "../web/dist".to_string());

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/sessions", get(list_sessions))
        .route("/api/commands", get(get_commands))
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/sessions/search", get(search_sessions))
        .route("/api/sessions/{key}/context", get(get_context))
        .route("/api/sessions/{key}/context/compact", post(compact_context))
        .route("/api/sessions/{key}/messages", get(get_messages))
        .route("/api/sessions/{key}/cache", get(get_cache_stats))
        .route("/api/sessions/{key}/name", put(rename_session))
        .route("/api/sessions/{key}", delete(delete_session))
        .fallback_service(
            tower_http::services::ServeDir::new(&frontend_dist).not_found_service(
                tower_http::services::ServeFile::new(format!("{frontend_dist}/index.html")),
            ),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_token,
        ))
        .layer(cors_layer())
        .with_state(state);

    let port = std::env::var("CONGA_GATEWAY_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    // Loopback by default: this process runs the agent's bash tool, so
    // exposing it to the LAN is a remote-code-execution decision the
    // operator must make explicitly via CONGA_GATEWAY_HOST (the Dockerfile
    // sets 0.0.0.0, where the container network is the intended boundary).
    let host = std::env::var("CONGA_GATEWAY_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    if host != "127.0.0.1" && host != "localhost" && host != "::1" {
        warn!(
            "listening on {host} — anyone who can reach this address can run commands as this user"
        );
    }

    let addr = format!("{host}:{port}");
    info!("conga-gateway listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    axum::serve(listener, app).await.expect("server error");
}
