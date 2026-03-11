//! Switch Model Tool for dynamic LLM model delegation
//!
//! Allows the LLM to delegate tasks to specialized models via subagent execution.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tracing::{info, instrument};

use super::base::{Tool, ToolError};
use crate::agent::loop_::{AgentConfig, AgentResponse};
use crate::agent::subagent::SubagentManager;
use crate::config::{ModelProfile, ModelRegistry};
use crate::providers::ProviderRegistry;

/// Switch Model Tool
///
/// Allows the LLM to delegate tasks to specialized models by creating
/// a subagent with the target model configuration.
pub struct SwitchModelTool {
    model_registry: Arc<ModelRegistry>,
    provider_registry: Arc<ProviderRegistry>,
    subagent_manager: Arc<SubagentManager>,
    tool_description: String,
}

impl SwitchModelTool {
    /// Create a new SwitchModelTool
    pub fn new(
        model_registry: Arc<ModelRegistry>,
        provider_registry: Arc<ProviderRegistry>,
        subagent_manager: Arc<SubagentManager>,
    ) -> Self {
        let tool_description = generate_tool_description(&model_registry);
        Self {
            model_registry,
            provider_registry,
            subagent_manager,
            tool_description,
        }
    }

    /// Get model profile, falling back to default if model_id is empty
    fn resolve_model(&self, model_id: &str) -> Result<(String, &ModelProfile), ToolError> {
        if model_id.trim().is_empty() {
            // Empty model_id - use default
            let (id, profile) = self
                .model_registry
                .get_default_profile()
                .ok_or_else(|| {
                    ToolError::ExecutionError(
                        "No default model configured. Please specify a model_id or configure a default model.".to_string()
                    )
                })?;
            Ok((id.to_string(), profile))
        } else {
            // Explicit model ID provided
            let profile = self.model_registry.get_profile(model_id).ok_or_else(|| {
                ToolError::ExecutionError(format!("Model profile not found: {}", model_id))
            })?;
            Ok((model_id.to_string(), profile))
        }
    }

    /// Execute task with the specified model
    async fn execute_with_model(
        &self,
        model_id: &str,
        profile: &ModelProfile,
        task: &str,
        context: Option<&str>,
    ) -> Result<AgentResponse, ToolError> {
        // Check if provider is available
        if !self.provider_registry.is_available(&profile.provider) {
            return Err(ToolError::ExecutionError(format!(
                "Provider '{}' is not available for model '{}'. Check API key configuration.",
                profile.provider, model_id
            )));
        }

        // Build the full task prompt
        let full_task = match context {
            Some(ctx) => format!("{}\n\nContext: {}", task, ctx),
            None => task.to_string(),
        };

        info!(
            "[SwitchModel] Switching to model '{}' (provider: {}) for task: {}",
            model_id,
            profile.provider,
            &task[..task.len().min(100)]
        );

        // Get the provider
        let provider = self
            .provider_registry
            .get_or_create(&profile.provider)
            .map_err(|e| ToolError::ExecutionError(format!("Failed to create provider: {}", e)))?;

        // Build agent config from model profile
        let agent_config = AgentConfig {
            model: profile.model.clone(),
            temperature: profile.temperature.unwrap_or(0.7),
            thinking_enabled: profile.thinking_enabled.unwrap_or(false),
            max_tokens: profile.max_tokens.unwrap_or(4096),
            ..Default::default()
        };

        // Execute via subagent manager
        self.subagent_manager
            .submit_and_wait_with_model(&full_task, None, provider, agent_config)
            .await
            .map_err(|e| ToolError::ExecutionError(format!("Model execution failed: {}", e)))
    }
}

#[async_trait]
impl Tool for SwitchModelTool {
    fn name(&self) -> &str {
        "switch_model"
    }

    fn description(&self) -> &str {
        &self.tool_description
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "model_id": {
                    "type": "string",
                    "description": "The model profile ID to switch to. Leave empty to use the default model. Available models are listed in the tool description."
                },
                "task": {
                    "type": "string",
                    "description": "The task description for the switched model to execute"
                },
                "context": {
                    "type": "string",
                    "description": "Optional additional context or background information for the task"
                }
            },
            "required": ["task"]
        })
    }

    #[instrument(name = "tool.switch_model", skip_all)]
    async fn execute(&self, args: Value) -> Result<String, ToolError> {
        let args: SwitchModelArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        // Validate task is not empty
        if args.task.trim().is_empty() {
            return Err(ToolError::InvalidArguments(
                "task cannot be empty".to_string(),
            ));
        }

        // Resolve model (use default if empty)
        let model_id_str = args.model_id.as_deref().unwrap_or("");
        let (model_id, profile) = self.resolve_model(model_id_str)?;

        // Execute the task
        let response = self
            .execute_with_model(&model_id, profile, &args.task, args.context.as_deref())
            .await?;

        // Format response
        let mut result = if let Some(ref reasoning) = response.reasoning_content {
            if !reasoning.is_empty() {
                format!(
                    "[Model: {} - Reasoning]\n{}\n\n[Response]\n{}",
                    model_id, reasoning, response.content
                )
            } else {
                format!("[Model: {}]\n{}", model_id, response.content)
            }
        } else {
            format!("[Model: {}]\n{}", model_id, response.content)
        };

        // Add tools used if any
        if !response.tools_used.is_empty() {
            result.push_str(&format!(
                "\n\n[Tools used: {}]",
                response.tools_used.join(", ")
            ));
        }

        Ok(result)
    }
}

/// Arguments for the switch_model tool
#[derive(Debug, Deserialize)]
struct SwitchModelArgs {
    /// Optional model profile ID (empty = use default)
    model_id: Option<String>,

    /// Required task description
    task: String,

    /// Optional context/background information
    context: Option<String>,
}

/// Generate the tool description with available models
fn generate_tool_description(registry: &ModelRegistry) -> String {
    let models_section = if registry.is_empty() {
        "No models configured. Add model profiles to `agents.models` in your config file.\n\n\
         Example:\n\
         ```yaml\n\
         agents:\n\
           models:\n\
             coder:\n\
               provider: openai\n\
               model: gpt-4o\n\
               description: \"Best for code generation\"\n\
               capabilities: [\"code\"]\n\
         ```"
        .to_string()
    } else {
        let mut table = "| ID | Description | Capabilities |\n".to_string();
        table.push_str("|----|-------------|--------------|\n");

        for model_id in registry.list_available_models() {
            if let Some(profile) = registry.get_profile(model_id) {
                let desc = profile
                    .description
                    .as_deref()
                    .unwrap_or("No description")
                    .lines()
                    .next()
                    .unwrap_or("No description");

                let caps = if profile.capabilities.is_empty() {
                    "-".to_string()
                } else {
                    profile.capabilities.join(", ")
                };

                table.push_str(&format!("| {} | {} | {} |\n", model_id, desc, caps));
            }
        }

        table
    };

    format!(
        r#"Switch to a specialized AI model for specific tasks.

## When to Switch Models
- **Complex tasks** requiring specialized capabilities (code, reasoning, creative)
- **Different perspectives** - some models excel at certain reasoning approaches
- **When current model's response quality is insufficient**

## Available Models
{}

## Parameters
- **model_id** (optional): The model profile ID. Leave empty to use the default model.
- **task** (required): The task description for the switched model to execute.
- **context** (optional): Additional context or background information.

## Example Usage
```json
{{"task": "Refactor this function", "model_id": "coder", "context": "The function is in src/lib.rs"}}
```

## Notes
- The switched model executes the task as a subagent with full tool access
- Response includes the model ID used for transparency
- If model_id is empty, the default model is used"#,
        models_section
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_registry() -> ModelRegistry {
        let mut registry = ModelRegistry::new();

        registry.insert(
            "coder".to_string(),
            ModelProfile {
                provider: "openai".to_string(),
                model: "gpt-4o".to_string(),
                description: Some("Code generation expert".to_string()),
                capabilities: vec!["code".to_string(), "reasoning".to_string()],
                temperature: Some(0.3),
                thinking_enabled: None,
                max_tokens: None,
            },
        );

        registry.insert(
            "fast".to_string(),
            ModelProfile {
                provider: "zhipu".to_string(),
                model: "glm-4-flash".to_string(),
                description: Some("Quick responses".to_string()),
                capabilities: vec!["fast".to_string()],
                temperature: Some(0.7),
                thinking_enabled: None,
                max_tokens: None,
            },
        );

        registry.set_default_model_id(Some("fast".to_string()));

        registry
    }

    #[test]
    fn test_generate_tool_description() {
        let registry = create_test_registry();
        let desc = generate_tool_description(&registry);

        // Check that description contains guidance and model info
        assert!(desc.contains("Switch to a specialized")); // Main header
        assert!(desc.contains("coder"));
        assert!(desc.contains("fast"));
        assert!(desc.contains("Code generation expert"));
        assert!(desc.contains("code, reasoning"));
        assert!(desc.contains("When to Switch Models"));
    }

    #[test]
    fn test_generate_tool_description_empty() {
        let registry = ModelRegistry::new();
        let desc = generate_tool_description(&registry);

        assert!(desc.contains("No models configured"));
        assert!(desc.contains("agents.models"));
    }

    #[test]
    fn test_parameters_schema() {
        // This tests the schema structure
        let params = serde_json::json!({
            "type": "object",
            "properties": {
                "model_id": { "type": "string" },
                "task": { "type": "string" },
                "context": { "type": "string" }
            },
            "required": ["task"]
        });

        assert_eq!(params["required"], serde_json::json!(["task"]));
    }
}
