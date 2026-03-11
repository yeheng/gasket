# Change: Add Dynamic LLM Model Switching

## Why

Currently, nanobot uses a single LLM model for all interactions. This creates several limitations:

1. **Cost inefficiency**: Simple tasks don't need expensive models (e.g., GPT-4o, Claude Opus)
2. **Capability mismatch**: Some tasks need specialized models (e.g., code generation vs. creative writing vs. deep reasoning)
3. **No model delegation**: The main agent cannot delegate complex tasks to more capable models

Users and LLMs should be able to dynamically switch to different models based on task requirements, using a subagent pattern where the main agent acts as an orchestrator.

## What Changes

### 1. Configuration Extension
- Add `models` section to `agents` config for defining named model profiles
- Each model profile specifies: provider, model name, and optional overrides (thinking_enabled, temperature, etc.)

### 2. New Tool: `switch_model`
- Allow LLM to delegate tasks to a different model via subagent
- Parameters: `model_id` (optional, defaults to current model), `task` (required), `context` (optional)
- Tool description includes dynamic list of available models with descriptions and capabilities
- Returns the subagent's response directly

### 3. Model Registry
- Create `ModelRegistry` to manage available model configurations
- Support runtime model lookup by ID
- Validate model availability at startup

### 4. SubagentManager Enhancement
- Support creating subagents with different providers/models
- Add method `submit_with_model()` for model-specific task execution

### 5. LLM Guidance System
- Dynamic tool description with available models and their capabilities
- Model profiles include `description` and `capabilities` fields
- Tool description template with "When to Switch" guidance
- Capability tags for task-to-model matching

## Impact

- **Affected specs**: None (new capability)
- **Affected code**:
  - `nanobot-core/src/config/agent.rs` - Add model profiles config
  - `nanobot-core/src/agent/subagent.rs` - Enhance for multi-model support
  - `nanobot-core/src/tools/` - New `switch_model` tool
  - `nanobot-core/src/config/loader.rs` - Load model profiles
  - `nanobot-cli/src/commands/` - Update tool registration

## Example Usage

### Configuration
```yaml
providers:
  openai:
    api_key: sk-xxx
  openrouter:
    api_key: sk-or-xxx
  zhipu:
    api_key: xxx

agents:
  defaults:
    model: zhipu/glm-5
  models:
    # Main orchestrator - fast and cheap
    orchestrator:
      provider: zhipu
      model: glm-5
      thinking_enabled: true
      description: "Fast and cost-effective for orchestration and simple tasks"
      capabilities: ["fast", "general"]

    # Code specialist
    coder:
      provider: openai
      model: gpt-4o
      temperature: 0.3
      description: "Best for code generation, refactoring, and debugging"
      capabilities: ["code", "reasoning"]

    # Deep reasoning for complex analysis
    reasoner:
      provider: openrouter
      model: anthropic/claude-opus-4
      thinking_enabled: true
      description: "Deep reasoning for complex analysis and multi-step problems"
      capabilities: ["reasoning", "creative", "research"]

    # Fast responses for simple tasks
    fast:
      provider: zhipu
      model: glm-4-flash
      temperature: 0.7
      description: "Quick responses for simple questions and tasks"
      capabilities: ["fast"]
```

### LLM Tool Call
```json
{
  "name": "switch_model",
  "arguments": {
    "model_id": "coder",
    "task": "Refactor this function to use async/await pattern",
    "context": "The function is in src/database.rs and handles PostgreSQL queries"
  }
}
```

## Design Considerations

1. **Security**: Model switching should respect provider API key availability
2. **Cost tracking**: Each model switch should track token usage separately
3. **Error handling**: Graceful fallback if requested model is unavailable
4. **Context passing**: Optional context parameter for task handoff
