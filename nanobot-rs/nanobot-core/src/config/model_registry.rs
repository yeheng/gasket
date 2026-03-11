//! Model Registry for managing model profiles
//!
//! Provides lookup and management for named model profiles that can be
//! used for dynamic model switching via the `switch_model` tool.

use std::collections::HashMap;

use super::agent::{AgentsConfig, ModelProfile};

/// Registry for managing model profiles
///
/// Stores model profiles by ID and provides lookup methods.
/// The default model ID is extracted from the agent config's model field
/// (format: "provider/model" -> extracts model ID or uses as-is).
#[derive(Debug, Clone)]
pub struct ModelRegistry {
    /// Model profiles indexed by ID
    profiles: HashMap<String, ModelProfile>,

    /// Default model ID (extracted from agents.defaults.model)
    default_model_id: Option<String>,
}

impl ModelRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            profiles: HashMap::new(),
            default_model_id: None,
        }
    }

    /// Create a registry from agent configuration
    pub fn from_config(config: &AgentsConfig) -> Self {
        let mut registry = Self::new();

        // Add all model profiles
        for (id, profile) in &config.models {
            registry.profiles.insert(id.clone(), profile.clone());
        }

        // Extract default model ID from agents.defaults.model
        // Format can be "provider/model" or just a model profile ID
        if let Some(ref model) = config.defaults.model {
            // If the model string matches a profile ID, use it
            // Otherwise, check if it's a provider/model format
            if registry.profiles.contains_key(model) {
                registry.default_model_id = Some(model.clone());
            } else {
                // Try to find a profile that matches the provider/model pattern
                // For now, we'll store the full string as the default
                registry.default_model_id = Some(model.clone());
            }
        }

        registry
    }

    /// Get a model profile by ID
    pub fn get_profile(&self, id: &str) -> Option<&ModelProfile> {
        self.profiles.get(id)
    }

    /// Get the default model profile
    ///
    /// Returns the profile for the default model ID if it exists in the profiles map.
    /// If not found, returns None.
    pub fn get_default_profile(&self) -> Option<(&str, &ModelProfile)> {
        self.default_model_id
            .as_ref()
            .and_then(|id| self.profiles.get(id).map(|p| (id.as_str(), p)))
    }

    /// Get the default model ID
    pub fn get_default_model_id(&self) -> Option<&str> {
        self.default_model_id.as_deref()
    }

    /// List all available model IDs
    pub fn list_available_models(&self) -> Vec<&str> {
        self.profiles.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a model profile exists
    pub fn contains(&self, id: &str) -> bool {
        self.profiles.contains_key(id)
    }

    /// Get the number of registered profiles
    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }

    /// Add a model profile
    pub fn insert(&mut self, id: String, profile: ModelProfile) {
        self.profiles.insert(id, profile);
    }

    /// Set the default model ID
    pub fn set_default_model_id(&mut self, id: Option<String>) {
        self.default_model_id = id;
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::agent::AgentDefaults;

    fn create_test_config() -> AgentsConfig {
        let mut models = HashMap::new();

        models.insert(
            "coder".to_string(),
            ModelProfile {
                provider: "openai".to_string(),
                model: "gpt-4o".to_string(),
                description: Some("Code expert".to_string()),
                capabilities: vec!["code".to_string()],
                temperature: Some(0.3),
                thinking_enabled: None,
                max_tokens: None,
            },
        );

        models.insert(
            "fast".to_string(),
            ModelProfile {
                provider: "zhipu".to_string(),
                model: "glm-4-flash".to_string(),
                description: Some("Fast responses".to_string()),
                capabilities: vec!["fast".to_string()],
                temperature: Some(0.7),
                thinking_enabled: None,
                max_tokens: None,
            },
        );

        AgentsConfig {
            defaults: AgentDefaults {
                model: Some("coder".to_string()),
                ..Default::default()
            },
            models,
        }
    }

    #[test]
    fn test_registry_from_config() {
        let config = create_test_config();
        let registry = ModelRegistry::from_config(&config);

        assert_eq!(registry.len(), 2);
        assert!(registry.contains("coder"));
        assert!(registry.contains("fast"));
        assert_eq!(registry.get_default_model_id(), Some("coder"));
    }

    #[test]
    fn test_get_profile() {
        let config = create_test_config();
        let registry = ModelRegistry::from_config(&config);

        let profile = registry.get_profile("coder").unwrap();
        assert_eq!(profile.provider, "openai");
        assert_eq!(profile.model, "gpt-4o");
        assert_eq!(profile.temperature, Some(0.3));
    }

    #[test]
    fn test_missing_profile() {
        let config = create_test_config();
        let registry = ModelRegistry::from_config(&config);

        assert!(registry.get_profile("unknown").is_none());
    }

    #[test]
    fn test_default_profile() {
        let config = create_test_config();
        let registry = ModelRegistry::from_config(&config);

        let (id, profile) = registry.get_default_profile().unwrap();
        assert_eq!(id, "coder");
        assert_eq!(profile.provider, "openai");
    }

    #[test]
    fn test_empty_registry() {
        let registry = ModelRegistry::new();

        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(registry.get_default_model_id().is_none());
        assert!(registry.get_default_profile().is_none());
    }

    #[test]
    fn test_list_available_models() {
        let config = create_test_config();
        let registry = ModelRegistry::from_config(&config);

        let mut models = registry.list_available_models();
        models.sort();

        assert_eq!(models, vec!["coder", "fast"]);
    }
}
