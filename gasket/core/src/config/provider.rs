//! Provider configuration schemas
//!
//! LLM provider configuration (OpenAI, OpenRouter, Anthropic, etc.)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Provider API protocol type
///
/// Determines the API format/protocol used to communicate with the provider.
/// Most providers use OpenAI-compatible format, while some have native APIs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    /// OpenAI-compatible API format (most providers)
    #[default]
    Openai,
    /// Anthropic native API format
    Anthropic,
    /// Google Gemini native API format
    Gemini,
}

/// Model-specific configuration including pricing and runtime parameters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelConfig {
    // --- Pricing ---
    /// Price per million input tokens
    #[serde(default, alias = "priceInputPerMillion")]
    pub price_input_per_million: Option<f64>,

    /// Price per million output tokens
    #[serde(default, alias = "priceOutputPerMillion")]
    pub price_output_per_million: Option<f64>,

    /// Currency code (e.g., "USD", "CNY")
    #[serde(default)]
    pub currency: Option<String>,

    // --- Runtime Parameters (override AgentDefaults) ---
    /// Sampling temperature (0.0 - 2.0)
    #[serde(default)]
    pub temperature: Option<f32>,

    /// Maximum tokens to generate
    #[serde(default, alias = "maxTokens")]
    pub max_tokens: Option<u32>,

    /// Maximum tool call iterations
    #[serde(default, alias = "maxIterations")]
    pub max_iterations: Option<u32>,

    /// Number of recent messages to include in context
    #[serde(default, alias = "memoryWindow")]
    pub memory_window: Option<usize>,

    /// Whether to enable thinking/reasoning mode for this model
    #[serde(default, alias = "thinkingEnabled")]
    pub thinking_enabled: Option<bool>,

    /// Whether to enable streaming responses
    #[serde(default)]
    pub streaming: Option<bool>,
}

impl ModelConfig {
    /// Check if this model has complete pricing configuration
    pub fn has_pricing(&self) -> bool {
        self.price_input_per_million.is_some() && self.price_output_per_million.is_some()
    }

    /// Get pricing if complete configuration exists
    pub fn get_pricing(
        &self,
        default_currency: Option<&str>,
    ) -> Option<crate::token_tracker::ModelPricing> {
        match (self.price_input_per_million, self.price_output_per_million) {
            (Some(input), Some(output)) => {
                let currency = self
                    .currency
                    .as_deref()
                    .or(default_currency)
                    .unwrap_or("USD");
                Some(crate::token_tracker::ModelPricing::new(
                    input, output, currency,
                ))
            }
            _ => None,
        }
    }
}

/// Provider configuration (OpenAI, OpenRouter, Anthropic, etc.)
#[derive(Clone, Default)]
pub struct ProviderConfig {
    /// API protocol type (required)
    pub provider_type: ProviderType,

    /// API base URL (required)
    pub api_base: String,

    /// API key for authentication
    pub api_key: Option<String>,

    /// OAuth client ID for providers that support OAuth (e.g., GitHub Copilot)
    pub client_id: Option<String>,

    /// Default currency for model pricing (can be overridden per-model)
    pub default_currency: Option<String>,

    /// Model-specific configurations (including pricing and runtime params)
    pub models: HashMap<String, ModelConfig>,

    /// Whether to enable HTTP proxy for this provider.
    /// When `true` (default), proxy settings from environment variables
    /// (HTTP_PROXY, HTTPS_PROXY, ALL_PROXY) will be used.
    /// When `false`, the provider will bypass all proxies even if
    /// environment variables are set.
    pub proxy_enabled: Option<bool>,
}

impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("provider_type", &self.provider_type)
            .field("api_base", &self.api_base)
            .field("api_key", &self.api_key.as_ref().map(|_| "***REDACTED***"))
            .field(
                "client_id",
                &self.client_id.as_ref().map(|_| "***REDACTED***"),
            )
            .field("default_currency", &self.default_currency)
            .field("models", &self.models)
            .field("proxy_enabled", &self.proxy_enabled)
            .finish()
    }
}

impl ProviderConfig {
    /// Check if proxy is enabled for this provider.
    /// Returns `true` by default (proxy enabled) unless explicitly set to `false`.
    pub fn proxy_enabled(&self) -> bool {
        self.proxy_enabled.unwrap_or(true)
    }

    /// Check if this provider supports thinking mode for a specific model.
    ///
    /// Checks the model-level `thinking_enabled` flag first, then falls back
    /// to checking if any model in this provider has thinking enabled.
    pub fn supports_thinking(&self) -> bool {
        // Check if any model has thinking enabled
        self.models
            .values()
            .any(|m| m.thinking_enabled.unwrap_or(false))
    }

    /// Check if thinking is enabled for a specific model
    pub fn thinking_enabled_for_model(&self, model_name: &str) -> bool {
        self.models
            .get(model_name)
            .and_then(|m| m.thinking_enabled)
            .unwrap_or(false)
    }

    /// Check if this provider is available (configured and has required credentials).
    ///
    /// Local providers (ollama, litellm) don't require an API key.
    /// Remote providers require a non-empty API key to be configured.
    pub fn is_available(&self, provider_name: &str) -> bool {
        let is_local = matches!(provider_name, "ollama" | "litellm");
        if is_local {
            return true;
        }
        // Check for non-empty API key
        self.api_key
            .as_ref()
            .is_some_and(|key| !key.trim().is_empty())
    }

    /// Get pricing for a specific model.
    ///
    /// Returns the model's pricing configuration, using default_currency as fallback
    /// if the model doesn't specify its own currency.
    pub fn get_pricing_for_model(
        &self,
        model_name: &str,
    ) -> Option<crate::token_tracker::ModelPricing> {
        let model_cfg = self.models.get(model_name)?;
        model_cfg.get_pricing(self.default_currency.as_deref())
    }

    /// Get runtime parameters for a specific model.
    ///
    /// Returns the model's runtime configuration if it exists.
    pub fn get_model_config(&self, model_name: &str) -> Option<&ModelConfig> {
        self.models.get(model_name)
    }
}

// ============================================================================
// Backward Compatibility - Legacy Provider Config Parsing
// ============================================================================

/// Legacy provider config for backward compatibility.
///
/// Supports both old and new formats:
/// ```yaml
/// # Old format (still supported with migration)
/// providers:
///   openai:
///     api_key: sk-xxx
///     api_base: https://api.openai.com/v1
///     supports_thinking: true
///
/// # New format (recommended)
/// providers:
///   openai:
///     type: openai
///     api_base: "https://api.openai.com/v1"
///     api_key: sk-xxx
///     models:
///       gpt-4o:
///         thinking_enabled: true
/// ```
#[derive(Debug, Clone, Deserialize)]
struct LegacyProviderConfig {
    /// API key for authentication
    #[serde(default, alias = "apiKey")]
    api_key: Option<String>,

    /// API base URL (required in new format)
    #[serde(default, alias = "apiBase")]
    api_base: Option<String>,

    /// Legacy: provider-level thinking support (now per-model)
    #[serde(default, alias = "supportsThinking")]
    supports_thinking: Option<bool>,

    /// OAuth client ID
    #[serde(default, alias = "clientId")]
    client_id: Option<String>,

    /// Default currency for model pricing
    #[serde(default, alias = "defaultCurrency")]
    default_currency: Option<String>,

    /// Legacy: price at provider level (now moved to models)
    #[serde(default, alias = "priceInputPerMillion")]
    price_input_per_million: Option<f64>,

    /// Legacy: price at provider level (now moved to models)
    #[serde(default, alias = "priceOutputPerMillion")]
    price_output_per_million: Option<f64>,

    /// Legacy: currency at provider level (alias for default_currency)
    #[serde(default)]
    currency: Option<String>,

    /// Model-specific configurations
    #[serde(default)]
    models: HashMap<String, ModelConfig>,

    /// Provider protocol type (required in new format)
    #[serde(default, alias = "type")]
    provider_type: ProviderType,

    /// Legacy: API compatibility mode (now replaced by provider_type)
    /// This field is kept for migration but ignored
    #[allow(dead_code)]
    #[serde(default, alias = "apiCompatibility")]
    api_compatibility: Option<String>,

    /// Whether to enable proxy for this provider (default: true)
    #[serde(default, alias = "proxyEnabled")]
    proxy_enabled: Option<bool>,
}

impl From<LegacyProviderConfig> for ProviderConfig {
    fn from(legacy: LegacyProviderConfig) -> Self {
        let mut models = legacy.models;

        // Resolve default_currency: prefer explicit default_currency, then currency
        let default_currency = legacy.default_currency.or(legacy.currency.clone());

        // If legacy provider-level pricing exists, create a "_default" entry
        // This allows backward compatibility for get_pricing_for_model
        if let (Some(input), Some(output)) = (
            legacy.price_input_per_million,
            legacy.price_output_per_million,
        ) {
            models
                .entry("_default".to_string())
                .or_insert_with(|| ModelConfig {
                    price_input_per_million: Some(input),
                    price_output_per_million: Some(output),
                    currency: legacy.currency.clone(),
                    ..Default::default()
                });
        }

        // If legacy supports_thinking is set, propagate to models if no model-level setting
        if let Some(supports_thinking) = legacy.supports_thinking {
            for model in models.values_mut() {
                if model.thinking_enabled.is_none() {
                    model.thinking_enabled = Some(supports_thinking);
                }
            }
        }

        ProviderConfig {
            provider_type: legacy.provider_type,
            api_base: legacy.api_base.unwrap_or_default(),
            api_key: legacy.api_key,
            client_id: legacy.client_id,
            default_currency,
            models,
            proxy_enabled: legacy.proxy_enabled,
        }
    }
}

impl<'de> Deserialize<'de> for ProviderConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let legacy = LegacyProviderConfig::deserialize(deserializer)?;
        Ok(ProviderConfig::from(legacy))
    }
}

impl Serialize for ProviderConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut s = serializer.serialize_struct("ProviderConfig", 7)?;
        s.serialize_field("type", &self.provider_type)?;
        s.serialize_field("apiBase", &self.api_base)?;
        s.serialize_field("apiKey", &self.api_key)?;
        s.serialize_field("clientId", &self.client_id)?;
        s.serialize_field("defaultCurrency", &self.default_currency)?;
        s.serialize_field("models", &self.models)?;
        s.serialize_field("proxyEnabled", &self.proxy_enabled)?;
        s.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_config_has_pricing() {
        let complete = ModelConfig {
            price_input_per_million: Some(1.0),
            price_output_per_million: Some(2.0),
            currency: Some("USD".to_string()),
            ..Default::default()
        };
        assert!(complete.has_pricing());

        let partial = ModelConfig {
            price_input_per_million: Some(1.0),
            price_output_per_million: None,
            ..Default::default()
        };
        assert!(!partial.has_pricing());
    }

    #[test]
    fn test_model_config_get_pricing() {
        let config = ModelConfig {
            price_input_per_million: Some(1.0),
            price_output_per_million: Some(2.0),
            currency: Some("CNY".to_string()),
            ..Default::default()
        };
        let pricing = config.get_pricing(None).unwrap();
        assert_eq!(pricing.price_input_per_million, 1.0);
        assert_eq!(pricing.price_output_per_million, 2.0);
        assert_eq!(pricing.currency, "CNY");

        // Default currency fallback
        let config = ModelConfig {
            price_input_per_million: Some(1.0),
            price_output_per_million: Some(2.0),
            currency: None,
            ..Default::default()
        };
        let pricing = config.get_pricing(Some("EUR")).unwrap();
        assert_eq!(pricing.currency, "EUR");

        // Ultimate fallback to USD
        let pricing = config.get_pricing(None).unwrap();
        assert_eq!(pricing.currency, "USD");
    }

    #[test]
    fn test_model_config_runtime_params() {
        let config = ModelConfig {
            temperature: Some(0.7),
            max_tokens: Some(4096),
            max_iterations: Some(10),
            memory_window: Some(20),
            thinking_enabled: Some(true),
            streaming: Some(false),
            ..Default::default()
        };

        assert_eq!(config.temperature, Some(0.7));
        assert_eq!(config.max_tokens, Some(4096));
        assert_eq!(config.max_iterations, Some(10));
        assert_eq!(config.memory_window, Some(20));
        assert_eq!(config.thinking_enabled, Some(true));
        assert_eq!(config.streaming, Some(false));
    }

    #[test]
    fn test_new_format_model_pricing() {
        let yaml = r#"
type: openai
api_base: "https://api.example.com/v1"
api_key: sk-xxx
default_currency: CNY
models:
  deepseek-chat:
    price_input_per_million: 0.5
    price_output_per_million: 1.0
  deepseek-reasoner:
    price_input_per_million: 2.0
    price_output_per_million: 8.0
    currency: USD
    thinking_enabled: true
    max_tokens: 8192
"#;
        let provider: ProviderConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(provider.provider_type, ProviderType::Openai);
        assert_eq!(provider.api_base, "https://api.example.com/v1");

        // deepseek-chat uses default currency
        let pricing = provider.get_pricing_for_model("deepseek-chat").unwrap();
        assert_eq!(pricing.price_input_per_million, 0.5);
        assert_eq!(pricing.price_output_per_million, 1.0);
        assert_eq!(pricing.currency, "CNY");

        // deepseek-reasoner has its own currency and runtime params
        let pricing = provider.get_pricing_for_model("deepseek-reasoner").unwrap();
        assert_eq!(pricing.price_input_per_million, 2.0);
        assert_eq!(pricing.price_output_per_million, 8.0);
        assert_eq!(pricing.currency, "USD");

        let model_cfg = provider.get_model_config("deepseek-reasoner").unwrap();
        assert_eq!(model_cfg.thinking_enabled, Some(true));
        assert_eq!(model_cfg.max_tokens, Some(8192));

        // Unknown model returns None
        assert!(provider.get_pricing_for_model("unknown").is_none());
    }

    #[test]
    fn test_backward_compatible_provider_pricing() {
        // Old format with provider-level pricing (no type/api_base required for backward compat)
        let yaml = r#"
api_key: sk-xxx
api_base: https://api.example.com/v1
price_input_per_million: 3.0
price_output_per_million: 15.0
currency: USD
"#;
        let provider: ProviderConfig = serde_yaml::from_str(yaml).unwrap();

        // Should create a _default entry
        assert!(provider.models.contains_key("_default"));

        // Verify _default model config
        let default_model = provider.models.get("_default").unwrap();
        assert_eq!(default_model.price_input_per_million, Some(3.0));
        assert_eq!(default_model.price_output_per_million, Some(15.0));
        assert_eq!(default_model.currency, Some("USD".to_string()));

        // default_currency should also be set
        assert_eq!(provider.default_currency, Some("USD".to_string()));
    }

    #[test]
    fn test_backward_compatible_with_models() {
        // Old format with both provider-level and model-level pricing
        let yaml = r#"
api_key: sk-xxx
api_base: https://api.example.com/v1
price_input_per_million: 0.5
price_output_per_million: 1.0
currency: CNY
models:
  deepseek-reasoner:
    price_input_per_million: 2.0
    price_output_per_million: 8.0
"#;
        let provider: ProviderConfig = serde_yaml::from_str(yaml).unwrap();

        // Model-level pricing
        let pricing = provider.get_pricing_for_model("deepseek-reasoner").unwrap();
        assert_eq!(pricing.price_input_per_million, 2.0);
        assert_eq!(pricing.price_output_per_million, 8.0);
        // Uses default_currency since model doesn't specify
        assert_eq!(pricing.currency, "CNY");

        // Provider-level pricing stored in _default
        let default_model = provider.models.get("_default").unwrap();
        assert_eq!(default_model.price_input_per_million, Some(0.5));
        assert_eq!(default_model.price_output_per_million, Some(1.0));
    }

    #[test]
    fn test_camel_case_aliases() {
        let yaml = r#"
type: openai
apiBase: "https://api.example.com/v1"
apiKey: sk-xxx
clientId: my-client-id
defaultCurrency: EUR
models:
  gpt-4o:
    priceInputPerMillion: 2.5
    priceOutputPerMillion: 10.0
    maxTokens: 4096
    thinkingEnabled: true
"#;
        let provider: ProviderConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(provider.provider_type, ProviderType::Openai);
        assert_eq!(provider.api_base, "https://api.example.com/v1");
        assert_eq!(provider.api_key, Some("sk-xxx".to_string()));
        assert_eq!(provider.client_id, Some("my-client-id".to_string()));
        assert_eq!(provider.default_currency, Some("EUR".to_string()));

        let pricing = provider.get_pricing_for_model("gpt-4o").unwrap();
        assert_eq!(pricing.price_input_per_million, 2.5);
        assert_eq!(pricing.price_output_per_million, 10.0);
        assert_eq!(pricing.currency, "EUR");

        let model_cfg = provider.get_model_config("gpt-4o").unwrap();
        assert_eq!(model_cfg.max_tokens, Some(4096));
        assert_eq!(model_cfg.thinking_enabled, Some(true));
    }

    #[test]
    fn test_provider_is_available() {
        // Remote provider without API key
        let provider = ProviderConfig {
            api_key: None,
            ..Default::default()
        };
        assert!(!provider.is_available("openai"));

        // Remote provider with empty API key
        let provider = ProviderConfig {
            api_key: Some("".to_string()),
            ..Default::default()
        };
        assert!(!provider.is_available("openai"));

        // Remote provider with valid API key
        let provider = ProviderConfig {
            api_key: Some("sk-xxx".to_string()),
            ..Default::default()
        };
        assert!(provider.is_available("openai"));

        // Local provider (ollama) doesn't need API key
        let provider = ProviderConfig::default();
        assert!(provider.is_available("ollama"));

        // Local provider (litellm) doesn't need API key
        assert!(provider.is_available("litellm"));
    }

    #[test]
    fn test_supports_thinking() {
        // Provider with thinking model
        let provider = ProviderConfig {
            models: {
                let mut m = HashMap::new();
                m.insert(
                    "deepseek-reasoner".to_string(),
                    ModelConfig {
                        thinking_enabled: Some(true),
                        ..Default::default()
                    },
                );
                m
            },
            ..Default::default()
        };
        assert!(provider.supports_thinking());
        assert!(provider.thinking_enabled_for_model("deepseek-reasoner"));
        assert!(!provider.thinking_enabled_for_model("other-model"));

        // Provider without thinking models
        let provider = ProviderConfig {
            models: {
                let mut m = HashMap::new();
                m.insert(
                    "gpt-4o".to_string(),
                    ModelConfig {
                        thinking_enabled: Some(false),
                        ..Default::default()
                    },
                );
                m
            },
            ..Default::default()
        };
        assert!(!provider.supports_thinking());
    }

    #[test]
    fn test_serialization() {
        let provider = ProviderConfig {
            provider_type: ProviderType::Openai,
            api_base: "https://api.openai.com/v1".to_string(),
            api_key: Some("sk-xxx".to_string()),
            client_id: None,
            default_currency: Some("USD".to_string()),
            models: {
                let mut m = HashMap::new();
                m.insert(
                    "gpt-4o".to_string(),
                    ModelConfig {
                        price_input_per_million: Some(2.5),
                        price_output_per_million: Some(10.0),
                        thinking_enabled: Some(true),
                        ..Default::default()
                    },
                );
                m
            },
            proxy_enabled: Some(true),
        };

        let yaml = serde_yaml::to_string(&provider).unwrap();
        assert!(yaml.contains("type: openai"));
        assert!(yaml.contains("apiBase: https://api.openai.com/v1"));
        assert!(yaml.contains("gpt-4o"));
    }

    #[test]
    fn test_empty_provider() {
        let yaml = "";
        let provider: ProviderConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(provider.api_key.is_none());
        assert!(provider.api_base.is_empty());
        assert!(provider.client_id.is_none());
        assert!(provider.default_currency.is_none());
        assert!(provider.models.is_empty());
        assert_eq!(provider.provider_type, ProviderType::Openai);
    }

    #[test]
    fn test_provider_type_serialization() {
        assert_eq!(
            serde_yaml::to_string(&ProviderType::Openai).unwrap().trim(),
            "openai"
        );
        assert_eq!(
            serde_yaml::to_string(&ProviderType::Anthropic)
                .unwrap()
                .trim(),
            "anthropic"
        );
        assert_eq!(
            serde_yaml::to_string(&ProviderType::Gemini).unwrap().trim(),
            "gemini"
        );
    }

    #[test]
    fn test_provider_type_deserialization() {
        assert_eq!(
            serde_yaml::from_str::<ProviderType>("openai").unwrap(),
            ProviderType::Openai
        );
        assert_eq!(
            serde_yaml::from_str::<ProviderType>("anthropic").unwrap(),
            ProviderType::Anthropic
        );
        assert_eq!(
            serde_yaml::from_str::<ProviderType>("gemini").unwrap(),
            ProviderType::Gemini
        );
    }

    #[test]
    fn test_all_provider_types() {
        let yaml = r#"
type: anthropic
api_base: "https://api.anthropic.com/v1"
api_key: sk-ant-xxx
"#;
        let provider: ProviderConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(provider.provider_type, ProviderType::Anthropic);

        let yaml = r#"
type: gemini
api_base: "https://generativelanguage.googleapis.com/v1beta"
api_key: xxx
"#;
        let provider: ProviderConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(provider.provider_type, ProviderType::Gemini);
    }
}
