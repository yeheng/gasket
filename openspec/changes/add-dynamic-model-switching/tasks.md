# Tasks: Dynamic LLM Model Switching

## 1. Configuration Schema

- [x] 1.1 Add `ModelProfile` struct to `config/agent.rs`
  - Fields: `provider`, `model`, `description`, `capabilities`, `temperature`, `thinking_enabled`, `max_tokens`
  - Add validation method `validate()`

- [x] 1.2 Add `models: HashMap<String, ModelProfile>` to `AgentsConfig`
  - Update `AgentsConfig` struct
  - Add test for parsing model profiles with descriptions and capabilities

- [x] 1.3 Update config loader to validate model profiles
  - Check that referenced providers exist
  - Validate model profile fields

## 2. Model Registry

- [x] 2.1 Create `ModelRegistry` in `config/` module
  - Store model profiles by ID
  - Provide `get_profile(id: &str) -> Option<&ModelProfile>`
  - Add `list_available_models() -> Vec<&str>`
  - Add `get_default_profile() -> Option<&ModelProfile>`

- [x] 2.2 Integrate `ModelRegistry` with config loading
  - Initialize from `AgentsConfig.models`
  - Set default model ID from `AgentsConfig.defaults.model`

- [x] 2.3 Add unit tests for `ModelRegistry`
  - Test profile lookup
  - Test missing profile handling
  - Test default profile fallback

## 3. Provider Registry

- [x] 3.1 Create `ProviderRegistry` in `providers/` module
  - Store provider configs and instances
  - Lazy initialization of provider instances
  - Thread-safe access via `RwLock`

- [x] 3.2 Implement `get_or_create_provider(name: &str) -> Result<Arc<dyn LlmProvider>>`
  - Check cache first
  - Create provider if not cached
  - Handle provider creation errors

- [x] 3.3 Add provider factory method based on config
  - Support all existing provider types (OpenAI, Gemini, Copilot, etc.)
  - Use `OpenAICompatibleProvider` for standard providers

## 4. SwitchModel Tool

- [x] 4.1 Create `SwitchModelTool` in `tools/` module
  - Parameters: `model_id` (optional, defaults to current), `task` (required), `context` (optional)
  - Generate JSON schema for parameters

- [x] 4.2 Implement dynamic tool description generator
  - Create `generate_tool_description(registry: &ModelRegistry) -> String`
  - Include "When to Switch" guidance section
  - Include formatted table of available models (ID, Description, Capabilities)
  - Include usage examples

- [x] 4.3 Implement tool execution
  - If `model_id` is empty, use default model
  - Look up model profile from registry
  - Get provider from provider registry
  - Create subagent with target model config
  - Execute task synchronously (with timeout)
  - Return subagent response

- [x] 4.4 Add error handling
  - Model not found (when explicit model_id is provided)
  - Provider not available
  - Subagent execution timeout
  - Subagent execution error

- [x] 4.5 Add tool metadata
  - Category: "agent"
  - Requires approval: false
  - Is mutating: false

## 5. LLM Guidance System

- [x] 5.1 Define capability tag taxonomy
  - Standard tags: `code`, `reasoning`, `creative`, `fast`, `vision`, `research`
  - Document each tag's meaning and use cases

- [x] 5.2 Implement model table formatter
  - Format models as markdown table for tool description
  - Include ID, description (truncated if too long), capabilities
  - Handle models without description/capabilities gracefully

- [x] 5.3 Create guidance text templates
  - "When to Switch Models" section
  - Parameter descriptions with examples
  - Best practices for model selection

## 6. SubagentManager Enhancement

- [x] 6.1 Add `submit_with_model()` method
  - Accept model profile as parameter
  - Create `AgentConfig` from model profile
  - Use specified provider instead of default

- [x] 6.2 Add `submit_and_wait_with_model()` method
  - Synchronous version with model selection
  - Return `AgentResponse` directly

- [ ] 6.3 Update subagent tool registry creation
  - Support tool filtering per model profile (future enhancement)

## 7. CLI Integration

- [x] 7.1 Update tool registration in CLI
  - Register `SwitchModelTool` with dependencies
  - Pass `ModelRegistry` and `ProviderRegistry` references

- [x] 7.2 Add startup logging
  - Log available models on startup with their descriptions
  - Warn if model profiles reference unavailable providers

## 8. Testing

- [x] 8.1 Add unit tests for `SwitchModelTool`
  - Test successful model switch
  - Test empty model_id uses default
  - Test missing model error (when explicit model_id provided)
  - Test unavailable provider error

- [x] 8.2 Add unit tests for tool description generator
  - Test description includes all configured models
  - Test description with missing descriptions/capabilities
  - Test description with no models configured

- [ ] 8.3 Add integration tests
  - Test multi-model conversation flow
  - Test model profile configuration parsing

- [ ] 8.4 Add config validation tests
  - Test valid model profiles
  - Test invalid provider reference
  - Test missing required fields

## 9. Documentation

- [ ] 9.1 Update README with model switching feature
  - Configuration example with descriptions and capabilities
  - Tool usage example
  - Capability tag reference

- [ ] 9.2 Add example config file
  - Multiple providers
  - Multiple model profiles with descriptions
  - Common use cases (coder, reasoner, fast, etc.)

## Dependencies

- Task 2.* depends on Task 1.* (config schema)
- Task 4.* depends on Task 2.* and 3.* (registries)
- Task 5.* depends on Task 1.* (capability definitions)
- Task 6.* depends on Task 2.* (model profiles)
- Task 7.* depends on Task 4.*, 5.* and 6.* (tool, guidance, manager)
- Task 8.* depends on all previous tasks

## Parallelizable Work

- Tasks 1.* and 3.* can be done in parallel
- Tasks 5.* can be done in parallel with 2.* and 3.*
- Tasks 8.* and 9.* can be done in parallel after core implementation
