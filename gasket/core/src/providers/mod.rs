//! LLM Provider system
//!
//! This module re-exports types from the `gasket-providers` crate and keeps
//! `ProviderRegistry` local as the bridge between config and providers.

pub mod registry;

// Re-export the streaming module
pub mod streaming {
    pub use gasket_providers::{parse_sse_stream, sse_lines};
}

// Re-export all base types
pub use gasket_providers::{
    ChatMessage, ChatRequest, ChatResponse, ChatStream, ChatStreamChunk, ChatStreamDelta,
    FinishReason, FunctionCall, FunctionDefinition, LlmProvider, MessageRole, ThinkingConfig,
    ToolCall, ToolCallDelta, ToolDefinition, Usage,
};

// Re-export utility functions
pub use gasket_providers::build_http_client;

// Re-export rig adapters
pub use gasket_providers::{
    build_rig_provider, to_rig_messages, to_rig_tool_def, RigAnthropicProvider, RigDeepSeekProvider,
    RigOllamaProvider, RigOpenAIProvider, RigOpenRouterProvider,
};

// Re-export specialized providers
#[cfg(feature = "provider-copilot")]
pub use gasket_providers::{
    CopilotOAuth, CopilotProvider, CopilotTokenResponse, DeviceCodeResponse,
};

// Re-export model spec
pub use gasket_providers::ModelSpec;

// Re-export registry
pub use registry::ProviderRegistry;