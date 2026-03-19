//! Rig provider adapter - 将 rig providers 包装为 LlmProvider
//!
//! rig 的 CompletionModel trait 不是 dyn-safe (要求 Self: Sized)，
//! 所以我们需要保留 gasket 的 LlmProvider trait 作为统一接口，
//! 并创建薄的 adapter 层将 rig providers 包装进来。
//!
//! # 支持的 Providers
//!
//! - OpenAI (Responses API 和 Completions API)
//! - DeepSeek (支持 reasoning_content)
//! - Anthropic
//! - Ollama (本地模型)
//! - OpenRouter (统一访问多个提供商)

// 子模块
mod anthropic;
mod builder;
mod convert;
mod deepseek;
mod ollama;
mod openai;
mod openrouter;
mod streaming;

// Re-export 公共接口
pub use anthropic::RigAnthropicProvider;
pub use builder::build_rig_provider;
pub use convert::{build_chat_response, to_rig_messages, to_rig_tool_def};
pub use deepseek::RigDeepSeekProvider;
pub use ollama::RigOllamaProvider;
pub use openai::RigOpenAIProvider;
pub use openrouter::RigOpenRouterProvider;
