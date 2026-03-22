//! Provider Registry for managing multiple LLM provider instances
//!
//! Provides lazy initialization and caching of provider instances with thread-safe access.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use tracing::{debug, info, warn};

use crate::config::{Config, ProviderType};
#[cfg(feature = "provider-gemini")]
use gasket_providers::GeminiProvider;
use gasket_providers::LlmProvider;
use gasket_providers::{OpenAICompatibleProvider, ProviderConfig};

/// Registry for managing LLM provider instances
///
/// Provides:
/// - Lazy initialization of provider instances
/// - Caching of created instances for reuse
/// - Thread-safe access via RwLock
pub struct ProviderRegistry {
    /// Provider configurations from the config file
    configs: HashMap<String, crate::config::ProviderConfig>,

    /// Cached provider instances
    instances: RwLock<HashMap<String, Arc<dyn LlmProvider>>>,

    /// Default provider name (extracted from agent config)
    default_provider: Option<String>,
}

impl ProviderRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
            instances: RwLock::new(HashMap::new()),
            default_provider: None,
        }
    }

    /// Create a registry from the root configuration
    pub fn from_config(config: &Config) -> Self {
        let mut registry = Self::new();

        // Add all provider configurations
        for (name, provider_config) in &config.providers {
            registry
                .configs
                .insert(name.clone(), provider_config.clone());
            debug!("Registered provider config: {}", name);
        }

        // Extract default provider from agent defaults model
        // Format: "provider/model" or just a model ID
        if let Some(ref model) = config.agents.defaults.model {
            let provider_name: Option<&str> = model.split('/').next();
            if let Some(name) = provider_name {
                registry.default_provider = Some(name.to_string());
            }
        }

        registry
    }

    /// Get or create a provider by name
    ///
    /// Returns a cached instance if available, otherwise creates a new one.
    pub fn get_or_create(&self, name: &str) -> anyhow::Result<Arc<dyn LlmProvider>> {
        // Check cache first (read lock)
        {
            let instances = self.instances.read().unwrap();
            if let Some(provider) = instances.get(name) {
                debug!("Using cached provider instance: {}", name);
                return Ok(provider.clone());
            }
        }

        // Create new instance (write lock)
        let provider = self.create_provider(name)?;

        {
            let mut instances = self.instances.write().unwrap();
            instances.insert(name.to_string(), provider.clone());
        }

        info!("Created and cached provider instance: {}", name);
        Ok(provider)
    }

    /// Create a new provider instance
    fn create_provider(&self, name: &str) -> anyhow::Result<Arc<dyn LlmProvider>> {
        let config = self
            .configs
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Provider not found: {}", name))?;

        // Check if provider is available
        if !config.is_available(name) {
            anyhow::bail!("Provider {} is not available (missing API key)", name);
        }

        // Validate api_base is configured
        if config.api_base.is_empty() {
            anyhow::bail!(
                "Provider '{}' is missing required 'api_base' configuration. \
                 Please add 'api_base' to your provider config in ~/.gasket/config.yaml",
                name
            );
        }

        // Create provider based on provider_type
        let provider: Arc<dyn LlmProvider> = match config.provider_type {
            #[cfg(feature = "provider-gemini")]
            ProviderType::Gemini => {
                let api_key = config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Gemini API key not configured"))?;
                Arc::new(GeminiProvider::with_config(
                    api_key.clone(),
                    Some(config.api_base.clone()),
                    None, // Use default model
                    config.proxy_enabled(),
                ))
            }
            ProviderType::Anthropic | ProviderType::Openai => {
                // Use OpenAI-compatible provider for Anthropic and OpenAI types
                // (Anthropic's /v1 endpoint is OpenAI-compatible)
                let api_key = config.api_key.as_deref().unwrap_or("");

                let provider_config = ProviderConfig {
                    name: name.to_string(),
                    api_base: config.api_base.clone(),
                    api_key: api_key.to_string(),
                    default_model: "default".to_string(),
                    extra_headers: HashMap::new(),
                    proxy_enabled: config.proxy_enabled(),
                };

                Arc::new(OpenAICompatibleProvider::new(provider_config))
            }
            #[cfg(not(feature = "provider-gemini"))]
            ProviderType::Gemini => {
                anyhow::bail!(
                    "Gemini provider is not compiled in. Rebuild with --features provider-gemini"
                );
            }
        };

        Ok(provider)
    }

    /// Check if a provider is configured
    pub fn contains(&self, name: &str) -> bool {
        self.configs.contains_key(name)
    }

    /// Check if a provider is available (configured and has credentials)
    pub fn is_available(&self, name: &str) -> bool {
        self.configs.get(name).is_some_and(|c| c.is_available(name))
    }

    /// List all configured provider names
    pub fn list_providers(&self) -> Vec<&str> {
        self.configs.keys().map(|s| s.as_str()).collect()
    }

    /// List available provider names (configured and have credentials)
    pub fn list_available_providers(&self) -> Vec<&str> {
        self.configs
            .iter()
            .filter(|(name, config)| config.is_available(name))
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Get the default provider name
    pub fn get_default_provider(&self) -> Option<&str> {
        self.default_provider.as_deref()
    }

    /// Check if the registry is empty (no providers configured)
    pub fn is_empty(&self) -> bool {
        self.configs.is_empty()
    }

    /// Get the number of configured providers
    pub fn len(&self) -> usize {
        self.configs.len()
    }

    /// Get provider configuration
    pub fn get_config(&self, name: &str) -> Option<&crate::config::ProviderConfig> {
        self.configs.get(name)
    }

    /// Clear cached instances (useful for testing or config reload)
    pub fn clear_cache(&self) {
        let mut instances = self.instances.write().unwrap();
        instances.clear();
        debug!("Cleared provider instance cache");
    }

    /// Log warnings for model profiles that reference unavailable providers
    pub fn validate_model_profiles(&self, registry: &crate::config::ModelRegistry) {
        for model_id in registry.list_available_models() {
            if let Some(profile) = registry.get_profile(model_id) {
                if !self.is_available(&profile.provider) {
                    warn!(
                        "Model profile '{}' references unavailable provider '{}'",
                        model_id, profile.provider
                    );
                }
            }
        }
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ModelConfig, ProviderConfig};

    #[test]
    fn test_empty_registry() {
        let registry = ProviderRegistry::new();

        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(registry.list_providers().is_empty());
    }

    #[test]
    fn test_contains_provider() {
        let mut registry = ProviderRegistry::new();
        registry.configs.insert(
            "test".to_string(),
            ProviderConfig {
                api_key: Some("test-key".to_string()),
                api_base: "https://api.example.com/v1".to_string(),
                ..Default::default()
            },
        );

        assert!(registry.contains("test"));
        assert!(!registry.contains("other"));
    }

    #[test]
    fn test_is_available() {
        let mut registry = ProviderRegistry::new();

        // Provider with API key
        registry.configs.insert(
            "openai".to_string(),
            ProviderConfig {
                api_key: Some("sk-test".to_string()),
                api_base: "https://api.openai.com/v1".to_string(),
                ..Default::default()
            },
        );

        // Provider without API key
        registry.configs.insert(
            "empty".to_string(),
            ProviderConfig {
                api_key: None,
                api_base: "https://api.example.com/v1".to_string(),
                ..Default::default()
            },
        );

        // Local provider (doesn't need API key)
        registry.configs.insert(
            "ollama".to_string(),
            ProviderConfig {
                api_base: "http://localhost:11434/v1".to_string(),
                ..Default::default()
            },
        );

        assert!(registry.is_available("openai"));
        assert!(!registry.is_available("empty"));
        assert!(registry.is_available("ollama")); // Local provider
    }

    #[test]
    fn test_provider_with_gemini_type() {
        let mut registry = ProviderRegistry::new();
        registry.configs.insert(
            "gemini".to_string(),
            ProviderConfig {
                provider_type: ProviderType::Gemini,
                api_key: Some("test-key".to_string()),
                api_base: "https://generativelanguage.googleapis.com/v1beta".to_string(),
                ..Default::default()
            },
        );

        assert!(registry.contains("gemini"));
        assert!(registry.is_available("gemini"));
    }

    #[test]
    fn test_provider_with_anthropic_type() {
        let mut registry = ProviderRegistry::new();
        registry.configs.insert(
            "anthropic".to_string(),
            ProviderConfig {
                provider_type: ProviderType::Anthropic,
                api_key: Some("sk-ant-test".to_string()),
                api_base: "https://api.anthropic.com/v1".to_string(),
                ..Default::default()
            },
        );

        assert!(registry.contains("anthropic"));
        assert!(registry.is_available("anthropic"));
    }

    #[test]
    fn test_provider_with_model_configs() {
        let mut registry = ProviderRegistry::new();
        let mut models = HashMap::new();
        models.insert(
            "deepseek-reasoner".to_string(),
            ModelConfig {
                thinking_enabled: Some(true),
                max_tokens: Some(8192),
                ..Default::default()
            },
        );

        registry.configs.insert(
            "deepseek".to_string(),
            ProviderConfig {
                provider_type: ProviderType::Openai,
                api_key: Some("sk-test".to_string()),
                api_base: "https://api.deepseek.com/v1".to_string(),
                models,
                ..Default::default()
            },
        );

        let config = registry.get_config("deepseek").unwrap();
        assert!(config.thinking_enabled_for_model("deepseek-reasoner"));
        assert!(!config.thinking_enabled_for_model("other-model"));
    }
}
