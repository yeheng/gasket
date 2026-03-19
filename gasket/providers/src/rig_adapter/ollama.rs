//! Ollama 本地模型 provider adapter

use async_trait::async_trait;
use tracing::instrument;

use crate::{ChatRequest, ChatResponse, ChatStream, LlmProvider, MessageRole};

use super::streaming::convert_rig_multi_turn_stream;

/// Ollama 本地模型 provider adapter
pub struct RigOllamaProvider {
    client: rig::providers::ollama::Client,
    model_name: String,
}

impl RigOllamaProvider {
    /// 创建新的 Ollama provider
    ///
    /// # Arguments
    /// * `base_url` - Ollama 服务地址 (默认: "http://localhost:11434")
    /// * `model` - 模型名称 (e.g., "llama3", "mistral")
    pub fn new(base_url: Option<&str>, model: &str) -> Self {
        use rig::client::Nothing;

        let url = base_url.unwrap_or("http://localhost:11434");

        let client = rig::providers::ollama::Client::builder()
            .base_url(url)
            .api_key(Nothing)
            .build()
            .expect("Failed to build Ollama client");

        Self {
            client,
            model_name: model.to_string(),
        }
    }
}

#[async_trait]
impl LlmProvider for RigOllamaProvider {
    fn name(&self) -> &str {
        "ollama"
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

        let agent = agent_builder.build();

        let response = agent
            .prompt(&prompt_msg)
            .await
            .map_err(|e| anyhow::anyhow!("Ollama chat error: {}", e))?;

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

        let agent = agent_builder.build();

        let stream = agent.stream_prompt(&prompt_msg).await;

        Ok(convert_rig_multi_turn_stream(stream))
    }
}