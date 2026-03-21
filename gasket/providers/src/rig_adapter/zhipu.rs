//! Zhipu (智谱) provider adapter using rig's OpenAI-compatible client
//!
//! 智谱 AI 提供 OpenAI 兼容接口，Base URL: https://open.bigmodel.cn/api/paas/v4/
//! 支持的模型：glm-5, glm-4.7, glm-4.6v 等
//!
//! ## 重要：使用 CompletionsClient + CompletionModel API
//!
//! Rig 0.32.0+ 默认使用 Responses API (`/v1/responses`)，但智谱只支持传统的
//! Chat Completions API (`/v1/chat/completions`)。因此我们使用 `CompletionsClient`
//! 而不是默认的 `Client`。
//!
//! 我们使用 `client.completion_model()` 而不是 `client.agent()`，因为：
//! 1. Gasket 的 AgentExecutor 控制多轮对话循环
//! 2. Provider 层只负责单次请求/响应
//! 3. 需要传递完整的历史消息和工具定义

use async_trait::async_trait;
use rig::client::CompletionClient;
use tracing::{debug, info, instrument};

use crate::{ChatMessage, ChatRequest, ChatResponse, ChatStream, LlmProvider, MessageRole};

use super::convert::build_chat_response;
use super::streaming::convert_rig_stream;

/// 智谱 AI 默认 Base URL
/// 注意：如果是 coding 端点，路径为 /api/coding/paas/v4
/// 通用 OpenAI 兼容端点为 /api/paas/v4/
const ZHIPU_BASE_URL: &str = "https://open.bigmodel.cn/api/paas/v4";

/// Zhipu (智谱) provider adapter
///
/// 使用 rig 的 OpenAI-compatible CompletionsClient 访问智谱 AI 的 GLM 系列模型。
///
/// # 注意
///
/// 使用 `CompletionsClient` 而非默认的 `Client`，因为智谱不支持 OpenAI 的 Responses API。
///
/// # Example
///
/// ```ignore
/// use gasket_providers::rig_adapter::RigZhipuProvider;
///
/// let provider = RigZhipuProvider::new("your-api-key", "glm-5");
/// let response = provider.chat(request).await?;
/// ```
pub struct RigZhipuProvider {
    // 使用 CompletionsClient 而非 Client，因为智谱不支持 Responses API
    client: rig::providers::openai::CompletionsClient,
    model_name: String,
}

impl RigZhipuProvider {
    /// 创建新的智谱 provider
    ///
    /// # Arguments
    /// * `api_key` - 智谱 AI API key
    /// * `model` - 模型名称 (e.g., "glm-5", "glm-4.7", "glm-4.6v")
    pub fn new(api_key: &str, model: &str) -> Self {
        // 使用 CompletionsClient::builder() 而非 Client::builder()
        // 这样会使用 /v1/chat/completions 端点而非 /v1/responses
        let client = rig::providers::openai::CompletionsClient::builder()
            .api_key(api_key)
            .base_url(ZHIPU_BASE_URL)
            .build()
            .expect("Failed to build Zhipu client");

        Self {
            client,
            model_name: model.to_string(),
        }
    }

    /// 从环境变量创建 provider
    ///
    /// 读取 `ZHIPU_API_KEY` 环境变量
    pub fn from_env(model: &str) -> Self {
        let api_key =
            std::env::var("ZHIPU_API_KEY").expect("ZHIPU_API_KEY environment variable not set");
        Self::new(&api_key, model)
    }

    /// 使用自定义 base_url 创建 provider
    ///
    /// # Arguments
    /// * `api_key` - 智谱 AI API key
    /// * `model` - 模型名称
    /// * `base_url` - 自定义 base URL
    pub fn with_base_url(api_key: &str, model: &str, base_url: &str) -> Self {
        let client = rig::providers::openai::CompletionsClient::builder()
            .api_key(api_key)
            .base_url(base_url)
            .build()
            .expect("Failed to build Zhipu client");

        Self {
            client,
            model_name: model.to_string(),
        }
    }

    /// 将 gasket ChatRequest 转换为 rig CompletionRequestBuilder
    fn build_request(
        &self,
        request: ChatRequest,
    ) -> rig::completion::CompletionRequestBuilder<rig::providers::openai::CompletionModel> {
        use rig::completion::{CompletionModel, ToolDefinition};

        // 获取 CompletionModel
        let model = self.client.completion_model(&self.model_name);

        // 找到最后一条用户消息作为 prompt
        let prompt_msg = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
            .and_then(|m| m.content.clone())
            .unwrap_or_default();

        let mut builder = model.completion_request(&prompt_msg);

        // 添加 system message 作为 preamble
        if let Some(system_msg) = request
            .messages
            .iter()
            .find(|m| m.role == MessageRole::System)
            .and_then(|m| m.content.clone())
        {
            builder = builder.preamble(system_msg);
        }

        // 添加历史消息（排除 system 和最后一条 user）
        let history: Vec<_> = request
            .messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .take(request.messages.len().saturating_sub(1))
            .filter_map(convert_message_to_rig)
            .collect();

        if !history.is_empty() {
            builder = builder.messages(history);
        }

        // 添加工具定义
        if let Some(ref tools) = request.tools {
            let rig_tools: Vec<_> = tools
                .iter()
                .map(|t| ToolDefinition {
                    name: t.function.name.clone(),
                    description: t.function.description.clone(),
                    parameters: t.function.parameters.clone(),
                })
                .collect();
            if !rig_tools.is_empty() {
                let tool_count = rig_tools.len();
                builder = builder.tools(rig_tools);
                debug!("[RigZhipu] Added {} tools to request", tool_count);
            }
        }

        if let Some(temp) = request.temperature {
            builder = builder.temperature(temp as f64);
        }

        if let Some(max_tokens) = request.max_tokens {
            builder = builder.max_tokens(max_tokens as u64);
        }

        builder
    }
}

/// 将 gasket ChatMessage 转换为 rig Message
fn convert_message_to_rig(msg: &ChatMessage) -> Option<rig::message::Message> {
    use rig::message::{AssistantContent, Message};
    use rig::OneOrMany;

    match msg.role {
        MessageRole::User => {
            let content = msg.content.clone().unwrap_or_default();
            Some(Message::user(content))
        }
        MessageRole::Assistant => {
            if let Some(ref tool_calls) = msg.tool_calls {
                let mut contents: Vec<AssistantContent> = tool_calls
                    .iter()
                    .map(|tc| {
                        AssistantContent::tool_call(
                            &tc.id,
                            &tc.function.name,
                            tc.function.arguments.clone(),
                        )
                    })
                    .collect();

                if let Some(ref text) = msg.content {
                    if !text.is_empty() {
                        contents.insert(0, AssistantContent::text(text));
                    }
                }

                Some(Message::Assistant {
                    id: None,
                    content: OneOrMany::many(contents)
                        .unwrap_or_else(|_| OneOrMany::one(AssistantContent::text(""))),
                })
            } else {
                Some(Message::assistant(msg.content.clone().unwrap_or_default()))
            }
        }
        MessageRole::System => None,
        MessageRole::Tool => {
            let tool_call_id = msg.tool_call_id.clone().unwrap_or_default();
            let content = msg.content.clone().unwrap_or_default();
            Some(Message::tool_result(tool_call_id, content))
        }
    }
}

#[async_trait]
impl LlmProvider for RigZhipuProvider {
    fn name(&self) -> &str {
        "zhipu"
    }

    fn default_model(&self) -> &str {
        &self.model_name
    }

    #[instrument(skip(self, request), fields(provider = %self.name(), model = %request.model))]
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse> {
        let builder = self.build_request(request);
        let response = builder.send().await.map_err(|e| {
            info!("[RigZhipu] Chat error: {}", e);
            anyhow::anyhow!("Zhipu chat error: {}", e)
        })?;

        info!(
            "[RigZhipu] Chat response: choice_count={}, usage={:?}",
            response.choice.len(),
            response.usage
        );

        Ok(build_chat_response(
            response.choice.into_iter().collect(),
            Some(response.usage),
        ))
    }

    #[instrument(skip(self, request), fields(provider = %self.name(), model = %request.model))]
    async fn chat_stream(&self, request: ChatRequest) -> anyhow::Result<ChatStream> {
        debug!("[RigZhipu] Starting stream request");

        let builder = self.build_request(request);
        let stream = builder.stream().await.map_err(|e| {
            info!("[RigZhipu] Stream error: {}", e);
            anyhow::anyhow!("Zhipu stream error: {}", e)
        })?;

        debug!("[RigZhipu] Stream started successfully");
        Ok(convert_rig_stream(stream))
    }
}
