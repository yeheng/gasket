//! Provider 构建和查找逻辑

use std::sync::Arc;

use anyhow::Result;
use nanobot_core::config::Config;
use nanobot_core::providers::{LlmProvider, ModelSpec, OpenAICompatibleProvider};

/// Provider information returned by find_provider
pub struct ProviderInfo {
    /// The provider instance
    pub provider: Arc<dyn LlmProvider>,
    /// The model name to use
    pub model: String,
    /// Provider name (e.g., "zhipu", "deepseek")
    pub provider_name: String,
    /// Whether this provider supports thinking/reasoning mode
    pub supports_thinking: bool,
}

/// Local providers that don't require an API key
const LOCAL_PROVIDERS: &[&str] = &["ollama", "litellm"];

/// Build a provider instance from its name and config.
pub fn build_provider(
    name: &str,
    api_key: &str,
    provider_config: &nanobot_core::config::ProviderConfig,
    model: &str,
) -> Arc<dyn LlmProvider> {
    match name {
        // MiniMax requires special handling for group_id header
        "minimax" => Arc::new(OpenAICompatibleProvider::minimax(
            api_key,
            provider_config.api_base.clone(),
            model,
            None,
        )),
        // GitHub Copilot requires special handling for OAuth token management
        "copilot" => Arc::new(nanobot_core::providers::CopilotProvider::new(
            api_key,
            provider_config.api_base.clone(),
            Some(model.to_string()),
        )),
        // All other providers use the generic from_name constructor
        _ => Arc::new(OpenAICompatibleProvider::from_name(
            name,
            api_key,
            provider_config.api_base.clone(),
            Some(model.to_string()),
        )),
    }
}

/// Get the default model name for a provider.
pub fn get_default_model_for_provider(name: &str) -> &'static str {
    match name {
        "deepseek" => "deepseek-chat",
        "openrouter" => "anthropic/claude-4.5-sonnet",
        "anthropic" => "claude-4-6-sonnet",
        "zhipu" => "glm-5",
        "dashscope" => "Qwen/Qwen3.5-397B-A17B",
        "moonshot" => "kimi-k2.5",
        "minimax" => "MiniMax-M2.5",
        "ollama" => "llama3",
        "litellm" => "gpt-4o", // LiteLLM proxies to configured models
        "copilot" => "gpt-4o",
        _ => "gpt-4o",
    }
}

/// Find a configured provider.
///
/// The model field supports `provider_id/model_id` format (parsed via
/// `ModelSpec`) to select a specific provider. For example:
///   - `"deepseek/deepseek-chat"` → use the deepseek provider with model deepseek-chat
///   - `"zhipu/glm-4"`           → use the zhipu provider with model glm-4
///   - `"deepseek-chat"`          → legacy behaviour, use default provider
pub fn find_provider(config: &Config) -> Result<ProviderInfo> {
    let raw_model = config
        .agents
        .defaults
        .model
        .clone()
        .unwrap_or_else(|| "gpt-4o".to_string());

    // Parse once into a strongly-typed ModelSpec
    let spec: ModelSpec = raw_model
        .parse()
        .expect("ModelSpec::from_str is infallible");

    let provider_name = if let Some(name) = spec.provider() {
        if !config.providers.contains_key(name) {
            anyhow::bail!(
                "Provider '{}' specified in model '{}' is not configured",
                name,
                spec
            );
        }
        let provider_config = &config.providers[name];
        let available = LOCAL_PROVIDERS.contains(&name) || provider_config.api_key.is_some();
        if !available {
            anyhow::bail!("Provider '{}' is configured but missing API key", name);
        }
        name.to_string()
    } else {
        let default_order = [
            "openrouter",
            "deepseek",
            "openai",
            "anthropic",
            "litellm",
            "ollama",
        ];

        let mut found = None;
        for &name in &default_order {
            if let Some(provider_config) = config.providers.get(name) {
                let available =
                    LOCAL_PROVIDERS.contains(&name) || provider_config.api_key.is_some();
                if available {
                    found = Some(name.to_string());
                    break;
                }
            }
        }

        if found.is_none() {
            for (name, provider_config) in &config.providers {
                let available =
                    LOCAL_PROVIDERS.contains(&name.as_str()) || provider_config.api_key.is_some();
                if available {
                    found = Some(name.to_string());
                    break;
                }
            }
        }

        found.ok_or_else(|| {
            anyhow::anyhow!(
                "No available provider configured. Run 'nanobot onboard' and add your API key to ~/.nanobot/config.yaml"
            )
        })?
    };

    let provider_config = config.providers.get(&provider_name).unwrap();

    let api_key = if LOCAL_PROVIDERS.contains(&provider_name.as_str()) {
        provider_config.api_key.as_deref().unwrap_or("")
    } else if let Some(key) = &provider_config.api_key {
        key.as_str()
    } else {
        anyhow::bail!("API key not configured for provider '{}'", provider_name);
    };

    let default_model = get_default_model_for_provider(&provider_name);
    let model = if spec.model().is_empty() {
        default_model.to_string()
    } else {
        spec.model().to_string()
    };

    let provider = build_provider(&provider_name, api_key, provider_config, &model);

    let supports_thinking = provider_config.supports_thinking();

    Ok(ProviderInfo {
        provider,
        model,
        provider_name,
        supports_thinking,
    })
}
