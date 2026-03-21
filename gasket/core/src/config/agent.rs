//! Agent configuration schemas
//!
//! Default agent settings and behavior configuration

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agents configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentsConfig {
    /// Default agent settings
    #[serde(default)]
    pub defaults: AgentDefaults,

    /// Named model profiles for dynamic model switching
    /// Key is the model profile ID (e.g., "coder", "reasoner")
    #[serde(default)]
    pub models: HashMap<String, ModelProfile>,
}

/// Model profile for dynamic model switching
///
/// Defines a named configuration for a specific model that can be
/// switched to at runtime via the `switch_model` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    /// Provider name (must exist in providers config)
    pub provider: String,

    /// Model identifier for the provider
    pub model: String,

    /// Human-readable description of when to use this model (for LLM guidance)
    #[serde(default)]
    pub description: Option<String>,

    /// Capability tags (e.g., "code", "reasoning", "creative", "fast")
    #[serde(default)]
    pub capabilities: Vec<String>,

    /// Temperature override (optional, uses ModelConfig or global default if not set)
    #[serde(default)]
    pub temperature: Option<f32>,

    /// Enable thinking/reasoning mode
    #[serde(default)]
    pub thinking_enabled: Option<bool>,

    /// Max tokens override
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

impl ModelProfile {
    /// Validate the model profile configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.provider.trim().is_empty() {
            return Err("provider cannot be empty".to_string());
        }
        if self.model.trim().is_empty() {
            return Err("model cannot be empty".to_string());
        }
        if let Some(temp) = self.temperature {
            if !(0.0..=2.0).contains(&temp) {
                return Err(format!(
                    "temperature must be between 0.0 and 2.0, got {}",
                    temp
                ));
            }
        }
        if let Some(tokens) = self.max_tokens {
            if tokens == 0 {
                return Err("max_tokens must be greater than 0".to_string());
            }
        }
        Ok(())
    }
}

/// Default agent settings - simplified to only model reference
///
/// Runtime configuration (temperature, max_tokens, etc.) is now defined
/// in ModelConfig under each ProviderConfig, allowing per-model settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentDefaults {
    /// Model to use - format: "provider_id/model_id" or "model_profile_id"
    /// Examples:
    /// - "openai/gpt-4o" - direct provider/model reference
    /// - "coder" - references a ModelProfile by ID
    #[serde(default)]
    pub model: Option<String>,
}

/// Global default values for runtime configuration
///
/// These are used as fallbacks when not specified in ModelConfig or ModelProfile.
impl AgentDefaults {
    /// Default temperature for generation
    pub const DEFAULT_TEMPERATURE: f32 = 0.7;

    /// Default maximum tokens to generate
    pub const DEFAULT_MAX_TOKENS: u32 = 4096;

    /// Default maximum tool call iterations
    pub const DEFAULT_MAX_ITERATIONS: u32 = 20;

    /// Default memory window size
    pub const DEFAULT_MEMORY_WINDOW: usize = 50;

    /// Default streaming mode
    pub const DEFAULT_STREAMING: bool = true;

    /// Default thinking mode
    pub const DEFAULT_THINKING_ENABLED: bool = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_defaults_model_only() {
        let yaml = r#"
defaults:
  model: openai/gpt-4o
"#;
        let agents: AgentsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(agents.defaults.model, Some("openai/gpt-4o".to_string()));
    }

    #[test]
    fn test_agent_defaults_empty() {
        let yaml = "";
        let agents: AgentsConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(agents.defaults.model.is_none());
    }

    #[test]
    fn test_agent_defaults_constants() {
        assert_eq!(AgentDefaults::DEFAULT_TEMPERATURE, 0.7);
        assert_eq!(AgentDefaults::DEFAULT_MAX_TOKENS, 4096);
        assert_eq!(AgentDefaults::DEFAULT_MAX_ITERATIONS, 20);
        assert_eq!(AgentDefaults::DEFAULT_MEMORY_WINDOW, 50);
        assert!(AgentDefaults::DEFAULT_STREAMING);
        assert!(!AgentDefaults::DEFAULT_THINKING_ENABLED);
    }

    #[test]
    fn test_model_profile_parsing() {
        let yaml = r#"
defaults:
  model: zhipu/glm-5
models:
  coder:
    provider: openai
    model: gpt-4o
    description: "Best for code generation and debugging"
    capabilities:
      - code
      - reasoning
    temperature: 0.3
  reasoner:
    provider: openrouter
    model: anthropic/claude-opus-4
    description: "Deep reasoning for complex analysis"
    capabilities:
      - reasoning
      - creative
    thinking_enabled: true
"#;
        let agents: AgentsConfig = serde_yaml::from_str(yaml).unwrap();

        // Check models map
        assert_eq!(agents.models.len(), 2);

        // Check coder profile
        let coder = agents.models.get("coder").unwrap();
        assert_eq!(coder.provider, "openai");
        assert_eq!(coder.model, "gpt-4o");
        assert_eq!(
            coder.description,
            Some("Best for code generation and debugging".to_string())
        );
        assert_eq!(coder.capabilities, vec!["code", "reasoning"]);
        assert_eq!(coder.temperature, Some(0.3));
        assert_eq!(coder.thinking_enabled, None);

        // Check reasoner profile
        let reasoner = agents.models.get("reasoner").unwrap();
        assert_eq!(reasoner.provider, "openrouter");
        assert_eq!(reasoner.model, "anthropic/claude-opus-4");
        assert_eq!(reasoner.thinking_enabled, Some(true));
    }

    #[test]
    fn test_model_profile_validation() {
        // Valid profile
        let valid = ModelProfile {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            description: None,
            capabilities: vec![],
            temperature: Some(0.5),
            thinking_enabled: None,
            max_tokens: Some(4096),
        };
        assert!(valid.validate().is_ok());

        // Empty provider
        let invalid_provider = ModelProfile {
            provider: "".to_string(),
            model: "gpt-4o".to_string(),
            description: None,
            capabilities: vec![],
            temperature: None,
            thinking_enabled: None,
            max_tokens: None,
        };
        assert!(invalid_provider.validate().is_err());

        // Invalid temperature
        let invalid_temp = ModelProfile {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            description: None,
            capabilities: vec![],
            temperature: Some(3.0),
            thinking_enabled: None,
            max_tokens: None,
        };
        assert!(invalid_temp.validate().is_err());
    }

    #[test]
    fn test_models_empty_by_default() {
        let yaml = r#"
defaults:
  model: test-model
"#;
        let agents: AgentsConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(agents.models.is_empty());
    }
}
