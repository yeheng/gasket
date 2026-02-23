//! DingTalk (钉钉) webhook handler
//!
//! Provides Axum routes for handling DingTalk callbacks.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::IntoResponse,
    Router,
};
use tokio::sync::{mpsc::Sender, RwLock};
use tracing::{debug, error};

use super::handlers;
use crate::bus::events::InboundMessage;
use crate::channels::dingtalk::{DingTalkCallbackMessage, DingTalkChannel, DingTalkConfig};

/// State for DingTalk webhook routes
#[derive(Clone)]
pub struct DingTalkState {
    pub channel: Arc<RwLock<DingTalkChannel>>,
}

impl DingTalkState {
    /// Create new DingTalk state from a channel
    pub fn new(channel: DingTalkChannel) -> Self {
        Self {
            channel: Arc::new(RwLock::new(channel)),
        }
    }

    /// Create from config and inbound sender
    pub fn from_config(config: DingTalkConfig, inbound_sender: Sender<InboundMessage>) -> Self {
        let channel = DingTalkChannel::new(config, inbound_sender);
        Self::new(channel)
    }
}

/// Create a router for DingTalk webhook endpoints
pub fn create_dingtalk_routes(state: DingTalkState, path: Option<&str>) -> Router {
    let path = path.unwrap_or("/dingtalk/callback");
    Router::new()
        .route(path, axum::routing::get(handle_get).post(handle_post))
        .with_state(state)
}

/// Handle GET request (unexpected for DingTalk)
async fn handle_get(
    _state: State<DingTalkState>,
    _query: Query<serde_json::Value>,
) -> impl IntoResponse {
    // DingTalk doesn't use GET for callbacks
    debug!("DingTalk GET request (unexpected)");
    handlers::bad_request("Use POST for DingTalk webhooks")
}

/// Handle POST request (message callback)
async fn handle_post(
    State(state): State<DingTalkState>,
    _headers: HeaderMap,
    _query: Query<serde_json::Value>,
    body: bytes::Bytes,
) -> impl IntoResponse {
    debug!("DingTalk callback POST request");

    // Parse the callback message
    let message: DingTalkCallbackMessage = match serde_json::from_slice(&body) {
        Ok(m) => m,
        Err(e) => {
            return handlers::bad_request(&format!("Invalid request body: {}", e));
        }
    };

    let channel = state.channel.read().await;

    match channel.handle_callback_message(message).await {
        Ok(()) => {
            debug!("DingTalk callback processed successfully");
            // DingTalk expects a JSON response with success
            handlers::json_response(
                axum::http::StatusCode::OK,
                &serde_json::json!({"msg": "success"}),
            )
        }
        Err(e) => {
            error!("DingTalk callback processing failed: {}", e);
            // Return success anyway to avoid retries for non-recoverable errors
            handlers::json_response(
                axum::http::StatusCode::OK,
                &serde_json::json!({"msg": "success"}),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn create_test_sender() -> Sender<InboundMessage> {
        let (tx, _rx) = mpsc::channel(100);
        tx
    }

    fn create_test_config() -> DingTalkConfig {
        DingTalkConfig {
            webhook_url: "https://oapi.dingtalk.com/robot/send?access_token=test123".to_string(),
            secret: Some("test_secret".to_string()),
            access_token: None,
            allow_from: vec![],
        }
    }

    #[test]
    fn test_dingtalk_state_creation() {
        let config = create_test_config();
        let state = DingTalkState::from_config(config, create_test_sender());
        assert!(Arc::strong_count(&state.channel) >= 1);
    }

    #[test]
    fn test_create_dingtalk_routes_default_path() {
        let config = create_test_config();
        let state = DingTalkState::from_config(config, create_test_sender());
        let _router = create_dingtalk_routes(state, None);
    }

    #[test]
    fn test_create_dingtalk_routes_custom_path() {
        let config = create_test_config();
        let state = DingTalkState::from_config(config, create_test_sender());
        let _router = create_dingtalk_routes(state, Some("/custom/dingtalk"));
    }
}
