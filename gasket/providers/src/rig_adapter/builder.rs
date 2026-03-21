//! Rig Provider Builder - 工厂函数
//!
//! 根据 provider name 创建对应的 rig provider 实例。
//! 基于 API 协议类型（而非 provider 名字）来选择实现。

use crate::LlmProvider;

use super::{
    anthropic::RigAnthropicProvider, deepseek::RigDeepSeekProvider, ollama::RigOllamaProvider,
    openai::RigOpenAIProvider, openrouter::RigOpenRouterProvider, zhipu::RigZhipuProvider,
};

/// 根据 provider name 创建 rig provider
///
/// # 参数
/// * `name` - provider 的名称（如 "openai"、"tencent"、"deepseek"）
/// * `api_key` - API 密钥（可选，会从环境变量 fallback）
/// * `model` - 模型名称
/// * `base_url` - 自定义 API 端点（可选）
///
/// # 返回
/// 返回对应的 provider 实例，如果 provider 不受支持则返回 None
///
/// # 设计原则
/// - 基于 **API 协议类型** 选择实现，而非 provider 名字
/// - 新 provider 只要指定正确的协议类型即可立即支持
/// - provider name 仅用于：1) 特定 provider 的特殊处理 2) 环境变量 fallback
///
/// # 支持的协议类型
/// - OpenAI 原生接口 -> RigOpenAIProvider
/// - Anthropic 原生接口 -> RigAnthropicProvider
/// - DeepSeek (OpenAI 兼容，支持 reasoning_content) -> RigDeepSeekProvider
/// - 智谱 (OpenAI 兼容) -> RigZhipuProvider
/// - Ollama (本地) -> RigOllamaProvider
/// - OpenRouter -> RigOpenRouterProvider
/// - 其他 OpenAI 兼容 provider (Tencent、Gemini 等) -> RigOpenAIProvider
pub fn build_rig_provider(
    name: &str,
    api_key: Option<String>,
    model: &str,
    base_url: Option<String>,
) -> Option<Box<dyn LlmProvider>> {
    // 首先处理有特殊实现的特殊 provider
    match name {
        // Anthropic 原生接口
        "anthropic" => {
            let key = api_key.or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())?;
            return Some(Box::new(RigAnthropicProvider::new(
                &key,
                model,
                base_url.as_deref(),
            )));
        }
        // DeepSeek: OpenAI 兼容但有特殊 reasoning_content 支持
        "deepseek" => {
            let key = api_key.or_else(|| std::env::var("DEEPSEEK_API_KEY").ok())?;
            return Some(Box::new(RigDeepSeekProvider::new(
                &key,
                model,
                base_url.as_deref(),
            )));
        }
        // 智谱：OpenAI 兼容，使用 Completions API
        "zhipu" => {
            let key = api_key.or_else(|| std::env::var("ZHIPU_API_KEY").ok())?;
            let provider = if let Some(url) = &base_url {
                RigZhipuProvider::with_base_url(&key, model, url)
            } else {
                RigZhipuProvider::new(&key, model)
            };
            return Some(Box::new(provider));
        }
        // Ollama: 本地模型，不需要 API key
        "ollama" => {
            return Some(Box::new(RigOllamaProvider::new(base_url.as_deref(), model)));
        }
        // OpenRouter: 统一接口
        "openrouter" => {
            let key = api_key.or_else(|| std::env::var("OPENROUTER_API_KEY").ok())?;
            return Some(Box::new(RigOpenRouterProvider::new(
                &key,
                model,
                base_url.as_deref(),
            )));
        }
        _ => {}
    }

    // 其他 provider 使用 OpenAI 兼容接口处理
    // 包括：openai、tencent、gemini、以及其他自定义 provider
    let env_var_name = format!("{}_API_KEY", name.to_uppercase());
    let key = api_key.or_else(|| std::env::var(&env_var_name).ok())?;

    // 确定默认 API 端点
    let default_url = match name {
        "openai" => "https://api.openai.com/v1",
        "tencent" => "https://api.hunyuan.tencentcloud.com/v1",
        "gemini" => "https://generativelanguage.googleapis.com/v1beta/openapi",
        _ => base_url.as_deref().unwrap_or("https://api.openai.com/v1"),
    };

    let final_url = base_url.as_deref().unwrap_or(default_url);

    Some(Box::new(RigOpenAIProvider::new(
        &key,
        model,
        Some(final_url),
    )))
}
