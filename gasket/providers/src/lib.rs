//! LLM Provider abstractions and implementations for gasket
//!
//! All OpenAI-compatible providers (OpenAI, DashScope, Moonshot, Zhipu, MiniMax)
//! are handled by `OpenAICompatibleProvider` with vendor-specific constructors.
//! Only providers with genuinely different API formats (DeepSeek for reasoning_content,
//! Gemini for native Google format, Copilot for OAuth token management) retain
//! their own modules.

mod base;
mod common;
#[cfg(feature = "provider-copilot")]
mod copilot;
#[cfg(feature = "provider-copilot")]
mod copilot_oauth;
#[cfg(feature = "provider-gemini")]
mod gemini;
mod model_spec;
pub mod streaming;

// Re-export base types
pub use base::{
    ChatMessage, ChatRequest, ChatResponse, ChatStream, ChatStreamChunk, ChatStreamDelta,
    FinishReason, FunctionCall, FunctionDefinition, LlmProvider, MessageRole, ThinkingConfig,
    ToolCall, ToolCallDelta, ToolDefinition, Usage,
};

// Re-export common types
pub use common::{
    build_http_client, get_default_api_base, get_default_model, parse_json_args,
    OpenAICompatibleProvider, ProviderBuildError, ProviderConfig, ProviderResult,
};

// Re-export specialized providers
#[cfg(feature = "provider-copilot")]
pub use copilot::CopilotProvider;
#[cfg(feature = "provider-copilot")]
pub use copilot_oauth::{CopilotOAuth, CopilotTokenResponse, DeviceCodeResponse};
#[cfg(feature = "provider-gemini")]
pub use gemini::GeminiProvider;

// Re-export model spec
pub use model_spec::ModelSpec;
