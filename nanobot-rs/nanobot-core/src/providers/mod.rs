//! LLM Provider system
//!
//! All OpenAI-compatible providers (OpenAI, DashScope, Moonshot, Zhipu, MiniMax)
//! are handled by `OpenAICompatibleProvider` with vendor-specific constructors.
//! Only providers with genuinely different API formats (DeepSeek for reasoning_content,
//! Gemini for native Google format) retain their own modules.

mod base;
mod common;
mod deepseek;
mod gemini;
mod registry;

pub use base::{ChatMessage, ChatRequest, ChatResponse, LlmProvider, ToolCall, ToolDefinition};
pub use common::{OpenAICompatibleProvider, ProviderConfig};
pub use deepseek::DeepSeekProvider;
pub use gemini::GeminiProvider;
pub use registry::{ProviderMetadata, ProviderRegistry};
