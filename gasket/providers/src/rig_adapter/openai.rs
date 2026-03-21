//! OpenAI provider adapter using rig
//!
//! 支持通过 rig 框架访问 OpenAI API，包括 GPT-4o, GPT-4-turbo 等模型。
//!
//! ## 重要：使用 CompletionsClient + CompletionModel API
//!
//! Rig 0.32.0+ 默认使用 Responses API (`/v1/responses`)，但为了更好的兼容性，
//! 我们使用 `CompletionsClient` 来使用传统的 Chat Completions API。
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
    // 使用 CompletionsClient 而非 Client，以确保使用 Chat Completions API
    client: rig::providers::openai::CompletionsClient,
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
        let mut builder = rig::providers::openai::CompletionsClient::builder().api_key(api_key);
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

    /// 将 gasket ChatRequest 转换为 rig CompletionRequestBuilder
    ///
    /// 这是关键方法，正确处理：
    /// - 系统消息 -> preamble
    /// - 历史消息 -> chat_history
    /// - 工具定义 -> tools
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

        // 构建请求
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
                debug!("[RigOpenAI] Added {} tools to request", tool_count);
            }
        }

        // 添加参数
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
                // 带工具调用的 assistant 消息
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

                // 如果有 content，也添加文本内容
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
        MessageRole::System => {
            // System 消息通过 preamble 处理，这里返回 None
            None
        }
        MessageRole::Tool => {
            // 工具结果消息
            let tool_call_id = msg.tool_call_id.clone().unwrap_or_default();
            let content = msg.content.clone().unwrap_or_default();
            Some(Message::tool_result(tool_call_id, content))
        }
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
        let builder = self.build_request(request);
        let response = builder.send().await.map_err(|e| {
            info!("[RigOpenAI] Chat error: {}", e);
            anyhow::anyhow!("OpenAI chat error: {}", e)
        })?;

        info!(
            "[RigOpenAI] Chat response: choice_count={}, usage={:?}",
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
        debug!("[RigOpenAI] Starting stream request");

        let builder = self.build_request(request);
        let stream = builder.stream().await.map_err(|e| {
            info!("[RigOpenAI] Stream error: {}", e);
            anyhow::anyhow!("OpenAI stream error: {}", e)
        })?;

        debug!("[RigOpenAI] Stream started successfully");
        Ok(convert_rig_stream(stream))
    }
}
