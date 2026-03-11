# Design: Dynamic LLM Model Switching

## Context

nanobot currently uses a single LLM provider/model configuration. The main agent processes all tasks with this single model, which leads to:

- Suboptimal cost/performance ratio
- Inability to leverage specialized models
- No delegation mechanism for complex tasks

This design introduces a multi-model architecture where the main agent can delegate tasks to specialized subagents running different models.

## Goals / Non-Goals

### Goals
- Enable runtime model switching via tool calls
- Support multiple model profiles in configuration
- Maintain backward compatibility with existing single-model setup
- Track token usage per model

### Non-Goals
- Auto-selecting models based on task type (LLM decides via tool call)
- Load balancing across multiple providers
- Model fine-tuning or training
- Streaming responses from switched models (initial version)

## Decisions

### Decision 1: Named Model Profiles

**What**: Add `models` map to `agents` config where each key is a model identifier.

**Why**: Provides a clean abstraction over provider/model combinations. Users define logical names (e.g., "coder", "reasoner") instead of provider-specific strings.

**Alternative considered**: Inline provider/model in tool arguments
- Rejected: Security risk (exposes API keys), no validation, harder to manage

### Decision 2: Tool-based Model Switching

**What**: Implement `switch_model` tool that creates a subagent with the specified model.

**Why**: Leverages existing subagent infrastructure, natural LLM interaction pattern.

**Alternative considered**: HTTP endpoint or CLI command
- Rejected: Less integrated, requires external orchestration

### Decision 3: Subagent-based Execution

**What**: Model-switched tasks run as subagents with `StatelessContext`.

**Why**:
- Isolation: No session pollution between models
- Simplicity: Reuses existing `SubagentManager` patterns
- Security: Scoped vault values per request

**Alternative considered**: In-place model swap
- Rejected: Complex state management, potential race conditions

### Decision 4: Provider Registry

**What**: Create `ProviderRegistry` to manage multiple provider instances.

**Why**: Enables lazy initialization and reuse of provider connections.

```rust
pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    configs: HashMap<String, ProviderConfig>,
}
```

### Decision 5: Dynamic Tool Description for LLM Guidance

**What**: Generate `switch_model` tool description dynamically based on configured models.

**Why**: LLM needs to know:
- When to switch models (task complexity, specialization)
- What models are available
- What each model is good at (description + capabilities)

**Implementation**:
```rust
fn generate_tool_description(registry: &ModelRegistry) -> String {
    format!(r#"
Switch to a specialized AI model for specific tasks.

## When to Switch Models
- Complex tasks requiring specialized capabilities
- Tasks that benefit from different reasoning approaches
- When current model's response quality is insufficient

## Available Models
| ID | Description | Capabilities |
|----|-------------|--------------|
{}

## Parameters
- model_id: (optional) Model profile ID. Empty = use default model
- task: The task description for the switched model
- context: Optional additional context

## Example
{{
  "model_id": "coder",
  "task": "Refactor this function",
  "context": "The function is in src/lib.rs"
}}
"#, format_model_table(registry))
}
```

**Alternative considered**: Static description
- Rejected: LLM wouldn't know available models without calling the tool

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Main Agent Loop                         │
│  (uses default model: glm-5)                                │
└────────────────────────┬────────────────────────────────────┘
                         │
                         │ Tool call: switch_model("coder", task)
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                    SwitchModelTool                           │
│  1. Look up model profile from ModelRegistry                │
│  2. Get/create provider from ProviderRegistry               │
│  3. Create subagent with target model                       │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                    Subagent (gpt-4o)                         │
│  - StatelessContext (no session persistence)                │
│  - Fresh tool registry                                       │
│  - Executes task                                             │
│  - Returns response                                          │
└─────────────────────────────────────────────────────────────┘
```

## Data Structures

### ModelProfile
```rust
pub struct ModelProfile {
    /// Provider name (must exist in providers config)
    pub provider: String,

    /// Model identifier for the provider
    pub model: String,

    /// Human-readable description of when to use this model
    pub description: Option<String>,

    /// Capability tags (e.g., "code", "reasoning", "creative")
    pub capabilities: Vec<String>,

    /// Temperature override (optional)
    pub temperature: Option<f32>,

    /// Enable thinking/reasoning mode
    pub thinking_enabled: Option<bool>,

    /// Max tokens override
    pub max_tokens: Option<u32>,
}
```

### ModelRegistry
```rust
pub struct ModelRegistry {
    profiles: HashMap<String, ModelProfile>,
    default_model_id: Option<String>,
}
```

### ProviderRegistry
```rust
pub struct ProviderRegistry {
    configs: HashMap<String, config::ProviderConfig>,
    instances: RwLock<HashMap<String, Arc<dyn LlmProvider>>>,
}
```

## Risks / Trade-offs

### Risk 1: API Key Exposure
- **Risk**: Malicious model switch could use unintended provider
- **Mitigation**: Only allow pre-configured model profiles, validate provider availability

### Risk 2: Cost Escalation
- **Risk**: Switching to expensive models frequently
- **Mitigation**: Token tracking per model, cost logging, configurable limits (future)

### Risk 3: Latency
- **Risk**: Subagent initialization overhead
- **Mitigation**: Provider instance caching, lazy initialization

## Migration Plan

1. **Phase 1**: Add config schema (backward compatible)
2. **Phase 2**: Implement `ModelRegistry` and `ProviderRegistry`
3. **Phase 3**: Add `SwitchModelTool`
4. **Phase 4**: Update `SubagentManager` for multi-model support
5. **Phase 5**: Integration tests and documentation

### Rollback
- Feature is opt-in via config
- Remove `models` section reverts to single-model behavior
- Tool returns error if model switching not configured

## Open Questions

1. **Q**: Should we support model chaining (switch_model within switch_model)?
   **A**: Yes, but with depth limit to prevent infinite loops

2. **Q**: Should subagent have access to the same tools as main agent?
   **A**: Yes, configurable via tool whitelist/blacklist per model profile

3. **Q**: How to handle streaming from switched models?
   **A**: Initial version: accumulate and return complete response. Future: stream events back to main agent callback.
