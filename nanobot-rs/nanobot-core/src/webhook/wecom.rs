//! WeCom (企业微信) webhook handler
//!
//! Provides Axum routes for handling WeCom callbacks.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::IntoResponse,
    routing::get,
    Router,
};
use tokio::sync::{mpsc::Sender, RwLock};
use tracing::{debug, error, info};

use super::handlers;
use crate::bus::events::InboundMessage;
use crate::channels::wecom::{WeComCallbackBody, WeComCallbackQuery, WeComChannel, WeComConfig};

/// State for WeCom webhook routes
#[derive(Clone)]
pub struct WeComState {
    pub channel: Arc<RwLock<WeComChannel>>,
}

impl WeComState {
    /// Create new WeCom state from a channel
    pub fn new(channel: WeComChannel) -> Self {
        Self {
            channel: Arc::new(RwLock::new(channel)),
        }
    }

    /// Create from config and inbound sender
    pub fn from_config(config: WeComConfig, inbound_sender: Sender<InboundMessage>) -> Self {
        let channel = WeComChannel::new(config, inbound_sender);
        Self::new(channel)
    }
}

/// Create a router for WeCom webhook endpoints
pub fn create_wecom_routes(state: WeComState, path: Option<&str>) -> Router {
    let path = path.unwrap_or("/wecom/callback");
    Router::new()
        .route(path, get(handle_get).post(handle_post))
        .with_state(state)
}

/// Handle GET request (URL verification)
async fn handle_get(
    State(state): State<WeComState>,
    Query(query): Query<serde_json::Value>,
) -> impl IntoResponse {
    debug!("WeCom URL verification request: {:?}", query);

    // Parse query parameters
    let callback_query: WeComCallbackQuery = match serde_json::from_value(query) {
        Ok(q) => q,
        Err(e) => {
            return handlers::bad_request(&format!("Invalid query parameters: {}", e));
        }
    };

    let channel = state.channel.read().await;

    match channel.verify_url(&callback_query) {
        Ok(echostr) => {
            info!("WeCom URL verification successful");
            handlers::success(&echostr)
        }
        Err(e) => {
            error!("WeCom URL verification failed: {}", e);
            handlers::bad_request(&format!("Verification failed: {}", e))
        }
    }
}

/// Handle POST request (message callback)
async fn handle_post(
    State(state): State<WeComState>,
    _headers: HeaderMap,
    Query(query): Query<serde_json::Value>,
    body: bytes::Bytes,
) -> impl IntoResponse {
    debug!("WeCom callback POST request");

    // Parse query parameters
    let callback_query: WeComCallbackQuery = match serde_json::from_value(query) {
        Ok(q) => q,
        Err(e) => {
            return handlers::bad_request(&format!("Invalid query parameters: {}", e));
        }
    };

    // Parse body
    let callback_body: WeComCallbackBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => {
            return handlers::bad_request(&format!("Invalid request body: {}", e));
        }
    };

    let channel = state.channel.read().await;

    match channel
        .handle_callback_message(&callback_query, &callback_body)
        .await
    {
        Ok(()) => {
            debug!("WeCom callback processed successfully");
            // WeCom expects "success" as response
            handlers::success("success")
        }
        Err(e) => {
            error!("WeCom callback processing failed: {}", e);
            // Still return success to avoid retries for non-recoverable errors
            handlers::success("success")
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

    fn create_test_config() -> WeComConfig {
        WeComConfig {
            corpid: "ww_test123".to_string(),
            corpsecret: "test_secret".to_string(),
            agent_id: 1000001,
            token: Some("test_token".to_string()),
            encoding_aes_key: Some("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY".to_string()),
            allow_from: vec![],
        }
    }

    #[test]
    fn test_wecom_state_creation() {
        let config = create_test_config();
        let state = WeComState::from_config(config, create_test_sender());
        assert!(Arc::strong_count(&state.channel) >= 1);
    }

    #[test]
    fn test_create_wecom_routes_default_path() {
        let config = create_test_config();
        let state = WeComState::from_config(config, create_test_sender());
        let _router = create_wecom_routes(state, None);
    }

    #[test]
    fn test_create_wecom_routes_custom_path() {
        let config = create_test_config();
        let state = WeComState::from_config(config, create_test_sender());
        let _router = create_wecom_routes(state, Some("/custom/wecom"));
    }
}
