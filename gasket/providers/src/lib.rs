//! LLM Provider abstractions and implementations for gasket
//!
//! This crate provides a unified interface for interacting with various LLM providers
//! through the `LlmProvider` trait.
//!
//! # Rig Framework Integration
//!
//! The `rig_adapter` module provides integration with the [rig](https://github.com/0xPlaygrounds/rig)
//! framework, enabling access to 20+ LLM providers through a unified interface.
//!
//! ## Supported Rig Providers
//!
//! - OpenAI (GPT-4o, GPT-4-turbo, etc.)
//! - Anthropic (Claude 3.5 Sonnet, Claude 3 Opus, etc.)
//! - DeepSeek (with reasoning_content support)
//! - Ollama (local models)
//! - OpenRouter (unified access to multiple providers)
//! - And more...
//!
//! # Specialized Providers
//!
//! - **Copilot**: GitHub Copilot with OAuth token management (feature-gated)

mod base;
#[cfg(feature = "provider-copilot")]
mod copilot;
#[cfg(feature = "provider-copilot")]
mod copilot_oauth;
mod model_spec;
mod streaming;
mod utils;

// Rig adapter module (directory-based)
pub mod rig_adapter;

// Re-export base types
pub use base::{
    ChatMessage, ChatRequest, ChatResponse, ChatStream, ChatStreamChunk, ChatStreamDelta,
    FinishReason, FunctionCall, FunctionDefinition, LlmProvider, MessageRole, ThinkingConfig,
    ToolCall, ToolCallDelta, ToolDefinition, Usage,
};

// Re-export streaming types
pub use streaming::{parse_sse_stream, sse_lines};

// Re-export utility functions
pub use utils::build_http_client;

// Re-export model spec
pub use model_spec::ModelSpec;

// Re-export specialized providers
#[cfg(feature = "provider-copilot")]
pub use copilot::CopilotProvider;
#[cfg(feature = "provider-copilot")]
pub use copilot_oauth::{CopilotOAuth, CopilotTokenResponse, DeviceCodeResponse};

// Re-export rig adapters
pub use rig_adapter::{
    build_rig_provider, to_rig_messages, to_rig_tool_def, RigAnthropicProvider,
    RigDeepSeekProvider, RigOllamaProvider, RigOpenAIProvider, RigOpenRouterProvider,
};
