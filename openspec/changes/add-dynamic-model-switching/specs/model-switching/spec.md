# Model Switching Capability

## ADDED Requirements

### Requirement: Model Profile Configuration

The system SHALL support named model profiles in the agent configuration.

A model profile MUST specify:
- `provider`: The provider name (must exist in providers config)
- `model`: The model identifier for the provider

A model profile MAY specify:
- `description`: Human-readable description of when to use this model (for LLM guidance)
- `capabilities`: List of capability tags (e.g., "code", "reasoning", "creative", "fast")
- `temperature`: Temperature override (0.0-2.0)
- `thinking_enabled`: Enable deep reasoning mode
- `max_tokens`: Maximum tokens override

#### Scenario: Valid model profile configuration
- **WHEN** user configures model profiles under `agents.models`
- **THEN** the system parses and validates each profile
- **AND** profiles are available for model switching

#### Scenario: Invalid provider reference
- **WHEN** a model profile references a non-existent provider
- **THEN** the system logs a warning at startup
- **AND** the profile is marked as unavailable

#### Scenario: Model profile with description
- **WHEN** a model profile includes a `description` field
- **THEN** the description is included in the tool's available models list
- **AND** the LLM can use this to decide when to switch

### Requirement: Model Registry

The system SHALL provide a ModelRegistry to manage model profiles.

The ModelRegistry MUST:
- Store all configured model profiles by ID
- Provide lookup by model ID
- List all available model IDs
- Track the default model ID

#### Scenario: Model profile lookup
- **WHEN** code requests a model profile by ID
- **THEN** the registry returns the profile if it exists
- **OR** returns None if the profile doesn't exist

#### Scenario: List available models
- **WHEN** code requests available models
- **THEN** the registry returns all configured model IDs

### Requirement: Provider Registry

The system SHALL provide a ProviderRegistry to manage LLM provider instances.

The ProviderRegistry MUST:
- Store provider configurations
- Lazily create provider instances on first access
- Cache created instances for reuse
- Provide thread-safe access to instances

#### Scenario: Provider instance creation
- **WHEN** code requests a provider by name
- **THEN** the registry returns a cached instance if available
- **OR** creates a new instance, caches it, and returns it

#### Scenario: Provider creation failure
- **WHEN** provider creation fails (e.g., invalid API key)
- **THEN** the registry returns an error
- **AND** does not cache the failed instance

### Requirement: switch_model Tool

The system SHALL provide a `switch_model` tool for LLM-initiated model delegation.

The tool MUST accept parameters:
- `model_id` (optional): The target model profile ID. If empty, uses the default model.
- `task` (required): The task description to execute
- `context` (optional): Additional context for the task

The tool MUST:
- Use the default model if `model_id` is empty or not provided
- Validate the model ID exists (when provided)
- Create a subagent with the target model configuration
- Execute the task synchronously with a timeout
- Return the subagent's response

The tool MUST NOT:
- Allow switching to unavailable models
- Expose provider API keys
- Persist session state for the subagent

#### Scenario: Successful model switch
- **WHEN** LLM calls `switch_model` with valid parameters
- **THEN** a subagent is created with the target model
- **AND** the task is executed
- **AND** the response is returned to the calling agent

#### Scenario: Model not found
- **WHEN** LLM calls `switch_model` with unknown model_id
- **THEN** the tool returns an error: "Model profile not found: {model_id}"
- **AND** no subagent is created

#### Scenario: Provider unavailable
- **WHEN** LLM calls `switch_model` for a model whose provider is unavailable
- **THEN** the tool returns an error: "Provider unavailable: {provider}"
- **AND** no subagent is created

#### Scenario: Task timeout
- **WHEN** the switched model task exceeds the timeout (10 minutes)
- **THEN** the tool returns an error: "Task timed out after 600 seconds"
- **AND** the subagent is terminated

### Requirement: Subagent with Model Selection

The SubagentManager SHALL support creating subagents with specific model configurations.

The SubagentManager MUST:
- Accept a model profile when creating a subagent
- Create an AgentConfig from the model profile
- Use the specified provider instead of the default

#### Scenario: Subagent with custom model
- **WHEN** `submit_with_model()` is called with a model profile
- **THEN** a subagent is created with the specified model
- **AND** uses the specified provider
- **AND** applies model-specific settings (temperature, thinking, etc.)

### Requirement: Backward Compatibility

The system SHALL maintain backward compatibility with existing configurations.

When no model profiles are configured:
- The system MUST function as before (single model)
- The `switch_model` tool MUST return an informative error

#### Scenario: No model profiles configured
- **WHEN** `switch_model` is called but no models are configured
- **THEN** the tool returns: "Model switching not configured. Add model profiles to agents.models in config."

#### Scenario: Empty model_id uses default
- **WHEN** `switch_model` is called with empty or missing model_id
- **THEN** the tool uses the default model
- **AND** executes the task with the default configuration
- **AND** returns the response normally

### Requirement: LLM Guidance for Model Switching

The system SHALL provide clear guidance to the LLM about when and how to switch models.

The `switch_model` tool description MUST include:
1. **When to switch** - Guidance on task types that benefit from model switching
2. **Available models** - Dynamic list from configuration with descriptions
3. **Best practices** - How to effectively use model switching

#### Scenario: Dynamic tool description
- **GIVEN** models are configured with descriptions and capabilities
- **WHEN** the tool definition is generated for the LLM
- **THEN** the description contains a formatted list of available models
- **AND** each model shows its ID, description, and capabilities
- **AND** the LLM can use this to decide which model to switch to

#### Scenario: Tool description format
- **WHEN** the `switch_model` tool is registered
- **THEN** its description includes:
  - Purpose explanation
  - When to use guidance
  - Available models table (ID | Description | Capabilities)
  - Parameter descriptions with examples

#### Scenario: No models configured guidance
- **WHEN** no model profiles are configured
- **THEN** the tool description explains how to configure model profiles
- **AND** suggests checking the configuration file

### Requirement: Model Capability Matching

The system SHALL support capability tags to help LLMs match tasks to appropriate models.

Capability tags MUST be defined as a standard taxonomy including:
- `code`: Code generation, refactoring, debugging
- `reasoning`: Complex analysis, multi-step logic, math
- `creative`: Creative writing, brainstorming, ideation
- `fast`: Quick responses for simple tasks
- `vision`: Image understanding, multimodal tasks
- `research`: Information gathering, summarization, search

#### Scenario: Capability-based model discovery
- **WHEN** the LLM needs to find a model for a coding task
- **THEN** the tool description shows which models have the "code" capability
- **AND** the LLM can select an appropriate model

#### Scenario: Multiple matching capabilities
- **WHEN** a task requires both code and reasoning
- **THEN** the LLM can see models with both capabilities
- **AND** choose based on description and available features

### Requirement: Tool Description Template

The `switch_model` tool description SHALL follow a structured template.

The template MUST include the following sections:
1. Purpose explanation ("Switch to a specialized AI model for specific tasks")
2. "When to Switch Models" guidance section
3. "Available Models" table with ID, Description, and Capabilities columns
4. "Parameters" section describing model_id, task, and context
5. "Example" section with a sample tool call

#### Scenario: Template renders correctly
- **WHEN** the tool description is generated using the template
- **THEN** the output is valid markdown
- **AND** contains all required sections (When to Switch, Available Models, Parameters, Example)
- **AND** the Available Models table is populated from configuration

#### Scenario: Template with configured models
- **GIVEN** three models are configured: coder, reasoner, fast
- **WHEN** the tool description is generated
- **THEN** the "Available Models" table lists all three
- **AND** each row shows the model's description and capabilities
