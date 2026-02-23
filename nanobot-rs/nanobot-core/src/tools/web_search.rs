//! Web search tool

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tracing::{info, instrument};

use super::base::simple_schema;
use super::{Tool, ToolError, ToolResult};

/// Web search tool
pub struct WebSearchTool {
    client: Client,
    config: Option<crate::config::WebToolsConfig>,
}

impl WebSearchTool {
    /// Create a new web search tool
    pub fn new(config: Option<crate::config::WebToolsConfig>) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    async fn search_brave(&self, query: &str, count: usize) -> ToolResult {
        let api_key = self
            .config
            .as_ref()
            .and_then(|c| c.brave_api_key.as_ref())
            .ok_or_else(|| ToolError::ExecutionError("Brave API key not configured".to_string()))?;

        let url = format!(
            "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
            urlencoding::encode(query),
            count
        );

        let response = self
            .client
            .get(&url)
            .header("X-Subscription-Token", api_key)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| ToolError::ExecutionError(format!("Brave API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ToolError::ExecutionError(format!(
                "Brave Search API error (status {}): {}",
                status, body
            )));
        }

        let search_response: BraveSearchResponse = response.json().await.map_err(|e| {
            ToolError::ExecutionError(format!("Failed to parse Brave API response: {}", e))
        })?;

        let mut result = String::new();
        for (i, r) in search_response.web.results.iter().enumerate() {
            result.push_str(&format!(
                "{}. **{}**\n   {}\n   URL: {}\n\n",
                i + 1,
                r.title,
                r.description,
                r.url
            ));
        }

        if result.is_empty() {
            result = "No results found.".to_string();
        }

        Ok(result)
    }

    async fn search_tavily(&self, query: &str, count: usize) -> ToolResult {
        let api_key = self
            .config
            .as_ref()
            .and_then(|c| c.tavily_api_key.as_ref())
            .ok_or_else(|| {
                ToolError::ExecutionError("Tavily API key not configured".to_string())
            })?;

        let body = serde_json::json!({
            "api_key": api_key,
            "query": query,
            "max_results": count,
            "search_depth": "basic"
        });

        let response = self
            .client
            .post("https://api.tavily.com/search")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionError(format!("Tavily API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ToolError::ExecutionError(format!(
                "Tavily API error (status {}): {}",
                status, body
            )));
        }

        let search_response: TavilySearchResponse = response.json().await.map_err(|e| {
            ToolError::ExecutionError(format!("Failed to parse Tavily API response: {}", e))
        })?;

        let mut result = String::new();
        for (i, r) in search_response.results.iter().enumerate() {
            result.push_str(&format!(
                "{}. **{}**\n   {}\n   URL: {}\n\n",
                i + 1,
                r.title,
                r.content,
                r.url
            ));
        }

        if result.is_empty() {
            result = "No results found.".to_string();
        }

        Ok(result)
    }

    async fn search_exa(&self, query: &str, count: usize) -> ToolResult {
        let api_key = self
            .config
            .as_ref()
            .and_then(|c| c.exa_api_key.as_ref())
            .ok_or_else(|| ToolError::ExecutionError("Exa API key not configured".to_string()))?;

        let body = serde_json::json!({
            "query": query,
            "numResults": count,
            "contents": { "text": true }
        });

        let response = self
            .client
            .post("https://api.exa.ai/search")
            .header("x-api-key", api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionError(format!("Exa API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ToolError::ExecutionError(format!(
                "Exa API error (status {}): {}",
                status, body
            )));
        }

        let search_response: ExaSearchResponse = response.json().await.map_err(|e| {
            ToolError::ExecutionError(format!("Failed to parse Exa API response: {}", e))
        })?;

        let mut result = String::new();
        for (i, r) in search_response.results.iter().enumerate() {
            let title = r.title.as_deref().unwrap_or("No title");
            let text = r.text.as_deref().unwrap_or("No description");
            result.push_str(&format!(
                "{}. **{}**\n   {}\n   URL: {}\n\n",
                i + 1,
                title,
                text.chars().take(300).collect::<String>(),
                r.url
            ));
        }

        if result.is_empty() {
            result = "No results found.".to_string();
        }

        Ok(result)
    }

    async fn search_firecrawl(&self, query: &str, count: usize) -> ToolResult {
        let api_key = self
            .config
            .as_ref()
            .and_then(|c| c.firecrawl_api_key.as_ref())
            .ok_or_else(|| {
                ToolError::ExecutionError("Firecrawl API key not configured".to_string())
            })?;

        let body = serde_json::json!({
            "query": query,
            "limit": count
        });

        let response = self
            .client
            .post("https://api.firecrawl.dev/v1/search")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                ToolError::ExecutionError(format!("Firecrawl API request failed: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ToolError::ExecutionError(format!(
                "Firecrawl API error (status {}): {}",
                status, body
            )));
        }

        let search_response: FirecrawlSearchResponse = response.json().await.map_err(|e| {
            ToolError::ExecutionError(format!("Failed to parse Firecrawl API response: {}", e))
        })?;

        let mut result = String::new();
        for (i, r) in search_response.data.iter().enumerate() {
            let title = r.title.as_deref().unwrap_or("No title");
            let desc = r.description.as_deref().unwrap_or("No description");
            result.push_str(&format!(
                "{}. **{}**\n   {}\n   URL: {}\n\n",
                i + 1,
                title,
                desc,
                r.url
            ));
        }

        if result.is_empty() {
            result = "No results found.".to_string();
        }

        Ok(result)
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web using the configured provider (Brave, Tavily, Exa, Firecrawl)"
    }

    fn parameters(&self) -> Value {
        simple_schema(&[
            ("query", "string", true, "Search query string"),
            (
                "count",
                "number",
                false,
                "Number of results to return (default 5)",
            ),
        ])
    }

    #[instrument(name = "tool.web_search", skip_all)]
    async fn execute(&self, args: Value) -> ToolResult {
        #[derive(Deserialize)]
        struct Args {
            query: String,
            #[serde(default = "default_count")]
            count: usize,
        }

        fn default_count() -> usize {
            5
        }

        let args: Args =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let provider = self
            .config
            .as_ref()
            .and_then(|c| c.search_provider.as_deref())
            .unwrap_or("brave")
            .to_lowercase();

        info!(
            "[WebSearch] Using '{}' API to search for: {}",
            provider, args.query
        );

        match provider.as_str() {
            "tavily" => self.search_tavily(&args.query, args.count).await,
            "exa" => self.search_exa(&args.query, args.count).await,
            "firecrawl" => self.search_firecrawl(&args.query, args.count).await,
            _ => self.search_brave(&args.query, args.count).await,
        }
    }
}

/// Brave Search API response
#[derive(Debug, Deserialize)]
struct BraveSearchResponse {
    web: BraveWebResults,
}

#[derive(Debug, Deserialize)]
struct BraveWebResults {
    results: Vec<BraveResult>,
}

#[derive(Debug, Deserialize)]
struct BraveResult {
    title: String,
    description: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct TavilySearchResponse {
    results: Vec<TavilyResult>,
}

#[derive(Debug, Deserialize)]
struct TavilyResult {
    title: String,
    content: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct ExaSearchResponse {
    results: Vec<ExaResult>,
}

#[derive(Debug, Deserialize)]
struct ExaResult {
    title: Option<String>,
    text: Option<String>,
    url: String,
}

#[derive(Debug, Deserialize)]
struct FirecrawlSearchResponse {
    data: Vec<FirecrawlResult>,
}

#[derive(Debug, Deserialize)]
struct FirecrawlResult {
    title: Option<String>,
    description: Option<String>,
    url: String,
}

// URL encoding helper
mod urlencoding {
    pub fn encode(s: &str) -> String {
        url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
    }
}
