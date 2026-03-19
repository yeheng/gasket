//! Rig Provider Builder - 工厂函数
//!
//! 根据名称创建对应的 rig provider 实例。

use crate::LlmProvider;

use super::{
    anthropic::RigAnthropicProvider, deepseek::RigDeepSeekProvider, ollama::RigOllamaProvider,
    openai::RigOpenAIProvider, openrouter::RigOpenRouterProvider,
};

/// 根据名称创建 rig provider
///
/// 支持的 provider 类型:
/// - "openai" -> RigOpenAIProvider
/// - "anthropic" -> RigAnthropicProvider
/// - "deepseek" -> RigDeepSeekProvider
/// - "ollama" -> RigOllamaProvider
/// - "openrouter" -> RigOpenRouterProvider
pub fn build_rig_provider(
    provider_type: &str,
    api_key: Option<String>,
    model: &str,
    base_url: Option<String>,
) -> Option<Box<dyn LlmProvider>> {
    match provider_type {
        "openai" => {
            let key = api_key.or_else(|| std::env::var("OPENAI_API_KEY").ok())?;
            Some(Box::new(RigOpenAIProvider::new(
                &key,
                model,
                base_url.as_deref(),
            )))
        }
        "anthropic" => {
            let key = api_key.or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())?;
            Some(Box::new(RigAnthropicProvider::new(
                &key,
                model,
                base_url.as_deref(),
            )))
        }
        "deepseek" => {
            let key = api_key.or_else(|| std::env::var("DEEPSEEK_API_KEY").ok())?;
            Some(Box::new(RigDeepSeekProvider::new(
                &key,
                model,
                base_url.as_deref(),
            )))
        }
        "ollama" => Some(Box::new(RigOllamaProvider::new(base_url.as_deref(), model))),
        "openrouter" => {
            let key = api_key.or_else(|| std::env::var("OPENROUTER_API_KEY").ok())?;
            Some(Box::new(RigOpenRouterProvider::new(
                &key,
                model,
                base_url.as_deref(),
            )))
        }
        _ => None,
    }
}