//! Web fetch tool for downloading web content

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, instrument};

use super::base::simple_schema;
use super::{Tool, ToolError, ToolResult};

/// Web fetch tool for downloading web content
pub struct WebFetchTool {
    client: Client,
    timeout_secs: u64,
    max_size: usize,
}

impl WebFetchTool {
    /// Create a new web fetch tool
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            timeout_secs: 120,
            max_size: 10_000_000, // 10 MB
        }
    }

    /// Set timeout in seconds
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Set max response size in bytes
    pub fn with_max_size(mut self, max_size: usize) -> Self {
        self.max_size = max_size;
        self
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch and extract text content from a web page"
    }

    fn parameters(&self) -> Value {
        simple_schema(&[
            ("url", "string", true, "URL of the web page to fetch"),
            (
                "prompt",
                "string",
                false,
                "Optional prompt describing what to extract from the page",
            ),
        ])
    }

    #[instrument(name = "tool.web_fetch", skip_all)]
    async fn execute(&self, args: Value) -> ToolResult {
        #[derive(Deserialize)]
        struct Args {
            url: String,
            #[serde(default)]
            prompt: Option<String>,
        }

        let args: Args =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        debug!("Fetching URL: {}", args.url);

        let response = self
            .client
            .get(&args.url)
            .header("User-Agent", "Mozilla/5.0 (compatible; nanobot/2.0)")
            .send()
            .await
            .map_err(|e| {
                ToolError::ExecutionError(format!("Failed to fetch URL '{}': {}", args.url, e))
            })?;

        if !response.status().is_success() {
            return Err(ToolError::ExecutionError(format!(
                "HTTP error {} when fetching '{}'",
                response.status(),
                args.url
            )));
        }

        // Get content type before consuming response
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body = response.text().await.map_err(|e| {
            ToolError::ExecutionError(format!(
                "Failed to read response body from '{}': {}",
                args.url, e
            ))
        })?;

        // Simple text extraction for HTML
        let text = if content_type.contains("text/html") {
            strip_html(&body)
        } else {
            body
        };

        // Truncate if too long (UTF-8 safe)
        let truncated = if text.len() > 8000 {
            let safe_len = text
                .char_indices()
                .nth(8000)
                .map(|(i, _)| i)
                .unwrap_or(text.len());
            format!(
                "{}...\n\n[Content truncated, {} chars total]",
                &text[..safe_len],
                text.len()
            )
        } else if let Some(prompt) = &args.prompt {
            format!("Prompt: {}\n\nContent:\n{}", prompt, text)
        } else {
            text
        };

        Ok(truncated)
    }
}

/// Strip HTML tags and convert to plain text.
///
/// Uses the well-tested `html2text` crate for robust HTML parsing,
/// handling edge cases like malformed tags, entities, and nested structures.
fn strip_html(html: &str) -> String {
    match html2text::from_read(html.as_bytes(), 10000) {
        Ok(text) => text
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Err(_) => {
            // Fallback: strip tags with a simple regex-like approach.
            // The previous code just joined whitespace, preserving all <script>/<style> content.
            // This at least removes tags and truncates to a reasonable length.
            let stripped = strip_tags_naive(html);
            let max = 2000.min(stripped.len());
            let mut end = max;
            while !stripped.is_char_boundary(end) && end > 0 {
                end -= 1;
            }
            if end < stripped.len() {
                format!(
                    "[HTML parsing failed. Showing raw snippet:]\n{}...",
                    &stripped[..end]
                )
            } else {
                format!("[HTML parsing failed. Showing raw snippet:]\n{}", stripped)
            }
        }
    }
}

/// Naive tag stripping: remove <script>/<style> blocks entirely, then strip remaining tags.
fn strip_tags_naive(html: &str) -> String {
    // Remove <script>...</script> and <style>...</style> blocks (case-insensitive)
    let mut result = String::with_capacity(html.len());
    let lower = html.to_lowercase();
    let bytes = html.as_bytes();
    let lower_bytes = lower.as_bytes();

    let mut i = 0;
    while i < bytes.len() {
        if lower_bytes[i] == b'<' {
            // Check for <script or <style
            let rest = &lower[i..];
            if rest.starts_with("<script") || rest.starts_with("<style") {
                let tag = if rest.starts_with("<script") {
                    "</script>"
                } else {
                    "</style>"
                };
                if let Some(end_pos) = lower[i..].find(tag) {
                    i += end_pos + tag.len();
                    continue;
                }
            }
            // Strip any other tag
            if let Some(end_pos) = html[i..].find('>') {
                i += end_pos + 1;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }

    // Collapse whitespace
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}
