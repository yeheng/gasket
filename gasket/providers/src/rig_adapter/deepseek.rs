//! DeepSeek provider adapter using rig
//!
//! DeepSeek 的特色是支持 reasoning_content (链式思考)，
//! 通过 rig 的 Reasoning 类型支持此功能。

use async_trait::async_trait;
use tracing::instrument;

use crate::{ChatRequest, ChatResponse, ChatStream, LlmProvider, MessageRole};

use super::streaming::convert_rig_multi_turn_stream;

/// DeepSeek provider adapter using rig
///
/// DeepSeek 的特色是支持 reasoning_content (链式思考)，
/// 通过 rig 的 Reasoning 类型支持此功能。
pub struct RigDeepSeekProvider {
    client: rig::providers::deepseek::Client,
    model_name: String,
}

impl RigDeepSeekProvider {
    /// 创建新的 DeepSeek provider
    ///
    /// # Arguments
    /// * `api_key` - DeepSeek API key
    /// * `model` - 模型名称 (e.g., "deepseek-chat", "deepseek-reasoner")
    /// * `base_url` - 可选的自定义 API base URL
    pub fn new(api_key: &str, model: &str, base_url: Option<&str>) -> Self {
        let mut builder = rig::providers::deepseek::Client::builder().api_key(api_key);

        if let Some(url) = base_url {
            builder = builder.base_url(url);
        }

        let client = builder.build().expect("Failed to build DeepSeek client");
        Self {
            client,
            model_name: model.to_string(),
        }
    }

    /// 从环境变量创建 provider
    pub fn from_env(model: &str) -> Self {
        let api_key = std::env::var("DEEPSEEK_API_KEY")
            .expect("DEEPSEEK_API_KEY environment variable not set");
        let base_url = std::env::var("DEEPSEEK_BASE_URL").ok();
        Self::new(&api_key, model, base_url.as_deref())
    }
}

#[async_trait]
impl LlmProvider for RigDeepSeekProvider {
    fn name(&self) -> &str {
        "deepseek"
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
            .map_err(|e| anyhow::anyhow!("DeepSeek chat error: {}", e))?;

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
