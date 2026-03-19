//! Anthropic Claude provider adapter using rig

use async_trait::async_trait;
use tracing::instrument;

use crate::{ChatRequest, ChatResponse, ChatStream, LlmProvider, MessageRole};

use super::streaming::convert_rig_multi_turn_stream;

/// Anthropic Claude provider adapter using rig
pub struct RigAnthropicProvider {
    client: rig::providers::anthropic::Client,
    model_name: String,
}

impl RigAnthropicProvider {
    /// 创建新的 Anthropic provider
    pub fn new(api_key: &str, model: &str, base_url: Option<&str>) -> Self {
        let mut builder = rig::providers::anthropic::Client::builder().api_key(api_key);

        if let Some(url) = base_url {
            builder = builder.base_url(url);
        }

        let client = builder.build().expect("Failed to build Anthropic client");
        Self {
            client,
            model_name: model.to_string(),
        }
    }

    /// 从环境变量创建 provider
    pub fn from_env(model: &str) -> Self {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .expect("ANTHROPIC_API_KEY environment variable not set");
        let base_url = std::env::var("ANTHROPIC_BASE_URL").ok();
        Self::new(&api_key, model, base_url.as_deref())
    }
}

#[async_trait]
impl LlmProvider for RigAnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn default_model(&self) -> &str {
        &self.model_name
    }

    #[instrument(skip(self, request), fields(provider = %self.name(), model = %request.model))]
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse> {
        use rig::client::CompletionClient;
        use rig::completion::Prompt;

        let preamble = request
            .messages
            .iter()
            .find(|m| m.role == MessageRole::System)
            .and_then(|m| m.content.clone());

        let prompt_msg = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
            .and_then(|m| m.content.clone())
            .unwrap_or_default();

        let mut agent_builder = self.client.agent(&self.model_name);

        if let Some(p) = preamble {
            agent_builder = agent_builder.preamble(&p);
        }

        if let Some(temp) = request.temperature {
            agent_builder = agent_builder.temperature(temp as f64);
        }

        if let Some(max_tokens) = request.max_tokens {
            agent_builder = agent_builder.max_tokens(max_tokens as u64);
        }

        let agent = agent_builder.build();

        let response = agent
            .prompt(&prompt_msg)
            .await
            .map_err(|e| anyhow::anyhow!("Anthropic chat error: {}", e))?;

        Ok(ChatResponse::text(response))
    }

    #[instrument(skip(self, request), fields(provider = %self.name(), model = %request.model))]
    async fn chat_stream(&self, request: ChatRequest) -> anyhow::Result<ChatStream> {
        use rig::client::CompletionClient;
        use rig::streaming::StreamingPrompt;

        let preamble = request
            .messages
            .iter()
            .find(|m| m.role == MessageRole::System)
            .and_then(|m| m.content.clone());

        let prompt_msg = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
            .and_then(|m| m.content.clone())
            .unwrap_or_default();

        let mut agent_builder = self.client.agent(&self.model_name);

        if let Some(p) = preamble {
            agent_builder = agent_builder.preamble(&p);
        }

        if let Some(temp) = request.temperature {
            agent_builder = agent_builder.temperature(temp as f64);
        }

        if let Some(max_tokens) = request.max_tokens {
            agent_builder = agent_builder.max_tokens(max_tokens as u64);
        }

        let agent = agent_builder.build();

        let stream = agent.stream_prompt(&prompt_msg).await;

        Ok(convert_rig_multi_turn_stream(stream))
    }
}
