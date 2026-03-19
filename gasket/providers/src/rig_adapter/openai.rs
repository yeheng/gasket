//! OpenAI provider adapter using rig
//!
//! 支持通过 rig 框架访问 OpenAI API，包括 GPT-4o, GPT-4-turbo 等模型。

use async_trait::async_trait;
use tracing::instrument;

use crate::{ChatRequest, ChatResponse, ChatStream, LlmProvider, MessageRole};

use super::streaming::convert_rig_multi_turn_stream;

/// OpenAI provider adapter using rig
///
/// 支持通过 rig 框架访问 OpenAI API，包括 GPT-4o, GPT-4-turbo 等模型。
///
/// # Example
///
/// ```ignore
/// use gasket_providers::rig_adapter::RigOpenAIProvider;
///
/// let provider = RigOpenAIProvider::new("your-api-key", "gpt-4o", None);
/// let response = provider.chat(request).await?;
/// ```
pub struct RigOpenAIProvider {
    client: rig::providers::openai::Client,
    model_name: String,
}

impl RigOpenAIProvider {
    /// 创建新的 OpenAI provider
    ///
    /// # Arguments
    /// * `api_key` - OpenAI API key
    /// * `model` - 模型名称 (e.g., "gpt-4o", "gpt-4-turbo")
    /// * `base_url` - 可选的自定义 API base URL
    pub fn new(api_key: &str, model: &str, base_url: Option<&str>) -> Self {
        let mut builder = rig::providers::openai::Client::builder().api_key(api_key);

        if let Some(url) = base_url {
            builder = builder.base_url(url);
        }

        let client = builder.build().expect("Failed to build OpenAI client");
        Self {
            client,
            model_name: model.to_string(),
        }
    }

    /// 从环境变量创建 provider
    ///
    /// 读取 `OPENAI_API_KEY` 和可选的 `OPENAI_BASE_URL` 环境变量
    pub fn from_env(model: &str) -> Self {
        let api_key =
            std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY environment variable not set");
        let base_url = std::env::var("OPENAI_BASE_URL").ok();
        Self::new(&api_key, model, base_url.as_deref())
    }
}

#[async_trait]
impl LlmProvider for RigOpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn default_model(&self) -> &str {
        &self.model_name
    }

    #[instrument(skip(self, request), fields(provider = %self.name(), model = %request.model))]
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse> {
        use rig::client::CompletionClient;
        use rig::completion::Prompt;

        // 获取 system message 作为 preamble
        let preamble = request
            .messages
            .iter()
            .find(|m| m.role == MessageRole::System)
            .and_then(|m| m.content.clone());

        // 获取 user prompt
        let prompt_msg = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
            .and_then(|m| m.content.clone())
            .unwrap_or_default();

        // 构建 agent
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
            .map_err(|e| anyhow::anyhow!("OpenAI chat error: {}", e))?;

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
