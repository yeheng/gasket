//! LLM Provider system
//!
//! This module re-exports types from the `gasket-providers` crate and keeps
//! `ProviderRegistry` local as the bridge between config and providers.

pub mod registry;

// Re-export the streaming module
pub mod streaming {
    pub use gasket_providers::streaming::*;
}

// Re-export all base types
pub use gasket_providers::{
    ChatMessage, ChatRequest, ChatResponse, ChatStream, ChatStreamChunk, ChatStreamDelta,
    FinishReason, FunctionCall, FunctionDefinition, LlmProvider, MessageRole, ThinkingConfig,
    ToolCall, ToolCallDelta, ToolDefinition, Usage,
};

// Re-export common types
pub use gasket_providers::{
    build_http_client, get_default_api_base, get_default_model, parse_json_args,
    OpenAICompatibleProvider, ProviderBuildError, ProviderConfig, ProviderResult,
};

// Re-export specialized providers
#[cfg(feature = "provider-gemini")]
pub use gasket_providers::GeminiProvider;
#[cfg(feature = "provider-copilot")]
pub use gasket_providers::{
    CopilotOAuth, CopilotProvider, CopilotTokenResponse, DeviceCodeResponse,
};

// Re-export model spec
pub use gasket_providers::ModelSpec;

// Re-export registry
pub use registry::ProviderRegistry;
