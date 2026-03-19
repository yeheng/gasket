//! Utility functions for providers

use reqwest::Client;
use tracing::info;

/// Build an HTTP client with optional proxy support.
///
/// # Arguments
/// * `proxy_enabled` - If `true`, the client will use proxy settings from
///   environment variables (HTTP_PROXY, HTTPS_PROXY, ALL_PROXY, NO_PROXY).
///   If `false`, all proxy settings are bypassed.
///
/// # Environment Variables (when proxy is enabled)
/// - `HTTP_PROXY` / `http_proxy`: Proxy for HTTP requests
/// - `HTTPS_PROXY` / `https_proxy`: Proxy for HTTPS requests
/// - `ALL_PROXY` / `all_proxy`: Proxy for all requests
/// - `NO_PROXY` / `no_proxy`: Hosts to bypass proxy
pub fn build_http_client(proxy_enabled: bool) -> Client {
    let mut builder = Client::builder();

    if !proxy_enabled {
        // Disable all proxies explicitly
        builder = builder.no_proxy();
        info!("HTTP client created with proxy disabled");
    } else {
        // Default behavior: reqwest automatically reads environment variables
        info!("HTTP client created with proxy enabled (using environment variables)");
    }

    builder.build().unwrap_or_else(|e| {
        tracing::warn!(
            "Failed to build HTTP client with custom settings: {}, using default",
            e
        );
        Client::new()
    })
}