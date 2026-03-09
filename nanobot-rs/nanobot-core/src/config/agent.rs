//! Agent configuration schemas
//!
//! Default agent settings and behavior configuration

use serde::{Deserialize, Serialize};

/// Agents configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentsConfig {
    /// Default agent settings
    #[serde(default)]
    pub defaults: AgentDefaults,
}

/// Default agent settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefaults {
    /// Model to use
    #[serde(default)]
    pub model: Option<String>,

    /// Temperature for generation
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Maximum tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Maximum tool call iterations
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,

    /// Memory window size
    #[serde(default = "default_memory_window")]
    pub memory_window: usize,

    /// Enable thinking/reasoning mode for deep reasoning models (GLM-5, DeepSeek R1, etc.)
    #[serde(default)]
    pub thinking_enabled: bool,

    /// Enable streaming mode for progressive output (default: true)
    #[serde(default = "default_streaming")]
    pub streaming: bool,
}

impl Default for AgentDefaults {
    fn default() -> Self {
        Self {
            model: None,
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            max_iterations: default_max_iterations(),
            memory_window: default_memory_window(),
            thinking_enabled: false,
            streaming: default_streaming(),
        }
    }
}

// Default value functions
fn default_temperature() -> f32 {
    0.7
}
fn default_max_tokens() -> u32 {
    4096
}
fn default_max_iterations() -> u32 {
    20
}
fn default_memory_window() -> usize {
    50
}
fn default_streaming() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_defaults() {
        let yaml = r#"
defaults:
  model: anthropic/claude-opus-4-5
  temperature: 0.5
  max_tokens: 8192
"#;
        let agents: AgentsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            agents.defaults.model,
            Some("anthropic/claude-opus-4-5".to_string())
        );
        assert_eq!(agents.defaults.temperature, 0.5);
        assert_eq!(agents.defaults.max_tokens, 8192);
        // Default values
        assert_eq!(agents.defaults.max_iterations, 20);
        assert_eq!(agents.defaults.memory_window, 50);
        assert!(agents.defaults.streaming);
        assert!(!agents.defaults.thinking_enabled);
    }

    #[test]
    fn test_agent_defaults_empty() {
        let yaml = "";
        let agents: AgentsConfig = serde_yaml::from_str(yaml).unwrap();
        // All defaults
        assert!(agents.defaults.model.is_none());
        assert_eq!(agents.defaults.temperature, 0.7);
        assert_eq!(agents.defaults.max_tokens, 4096);
    }
}
