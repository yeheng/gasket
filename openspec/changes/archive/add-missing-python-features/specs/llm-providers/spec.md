## ADDED Requirements

### Requirement: Provider Registry

The system SHALL provide a unified registry for managing LLM providers.

#### Scenario: Register provider

- **WHEN** a provider is initialized with valid configuration
- **THEN** the system SHALL add it to the Provider Registry
- **AND** make it queryable by name or model prefix

#### Scenario: Detect provider from model name

- **WHEN** a model name includes a prefix (e.g., "deepseek/chat")
- **THEN** the system SHALL extract the provider name from the prefix
- **AND** route the request to the appropriate provider

#### Scenario: Auto-detect provider without prefix

- **WHEN** a model name has no prefix (e.g., "gpt-4")
- **THEN** the system SHALL attempt to match with default provider
- **AND** use the first provider that supports the model

### Requirement: DeepSeek Provider

The system SHALL support DeepSeek as an LLM provider.

#### Scenario: Configure DeepSeek provider

- **WHEN** the configuration includes:
  ```json
  {
    "providers": {
      "deepseek": {
        "apiKey": "${DEEPSEEK_API_KEY}",
        "apiBase": "https://api.deepseek.com/v1"
      }
    }
  }
  ```
- **THEN** the system SHALL initialize a DeepSeek provider
- **AND** support models with prefix "deepseek/"

#### Scenario: Use DeepSeek model

- **WHEN** the agent uses model "deepseek/deepseek-chat"
- **THEN** the system SHALL route the request to DeepSeek API
- **AND** use the configured API key for authentication

#### Scenario: DeepSeek API compatibility

- **WHEN** calling DeepSeek API
- **THEN** the system SHALL use OpenAI-compatible request format
- **AND** handle OpenAI-compatible response format

### Requirement: Gemini Provider

The system SHALL support Google Gemini as an LLM provider.

#### Scenario: Configure Gemini provider

- **WHEN** the configuration includes:
  ```json
  {
    "providers": {
      "gemini": {
        "apiKey": "${GEMINI_API_KEY}"
      }
    }
  }
  ```
- **THEN** the system SHALL initialize a Gemini provider
- **AND** support models with prefix "gemini/"

#### Scenario: Use Gemini model

- **WHEN** the agent uses model "gemini/gemini-pro"
- **THEN** the system SHALL route the request to Gemini API
- **AND** use the configured API key for authentication

#### Scenario: Gemini API adaptation

- **WHEN** calling Gemini API
- **THEN** the system SHALL adapt the request format to Gemini requirements
- **AND** transform the response to standard format

### Requirement: Zhipu Provider

The system SHALL support Zhipu (智谱 GLM) as an LLM provider.

#### Scenario: Configure Zhipu provider

- **WHEN** the configuration includes:
  ```json
  {
    "providers": {
      "zhipu": {
        "apiKey": "${ZHIPU_API_KEY}",
        "apiBase": "https://open.bigmodel.cn/api/paas/v4"
      }
    }
  }
  ```
- **THEN** the system SHALL initialize a Zhipu provider
- **AND** support models with prefix "zhipu/"

#### Scenario: Use Zhipu model

- **WHEN** the agent uses model "zhipu/glm-4"
- **THEN** the system SHALL route the request to Zhipu API
- **AND** handle Zhipu-specific request/response format

### Requirement: DashScope Provider

The system SHALL support Alibaba DashScope as an LLM provider.

#### Scenario: Configure DashScope provider

- **WHEN** the configuration includes:
  ```json
  {
    "providers": {
      "dashscope": {
        "apiKey": "${DASHSCOPE_API_KEY}",
        "apiBase": "https://dashscope.aliyuncs.com/api/v1"
      }
    }
  }
  ```
- **THEN** the system SHALL initialize a DashScope provider
- **AND** support models with prefix "dashscope/"

#### Scenario: Use DashScope model

- **WHEN** the agent uses model "dashscope/qwen-max"
- **THEN** the system SHALL route the request to DashScope API
- **AND** handle DashScope-specific request/response format

### Requirement: Moonshot Provider

The system SHALL support Moonshot (月之暗面) as an LLM provider.

#### Scenario: Configure Moonshot provider

- **WHEN** the configuration includes:
  ```json
  {
    "providers": {
      "moonshot": {
        "apiKey": "${MOONSHOT_API_KEY}",
        "apiBase": "https://api.moonshot.cn/v1"
      }
    }
  }
  ```
- **THEN** the system SHALL initialize a Moonshot provider
- **AND** support models with prefix "moonshot/"

#### Scenario: Use Moonshot model

- **WHEN** the agent uses model "moonshot/moonshot-v1-8k"
- **THEN** the system SHALL route the request to Moonshot API
- **AND** use OpenAI-compatible format

### Requirement: MiniMax Provider

The system SHALL support MiniMax as an LLM provider.

#### Scenario: Configure MiniMax provider

- **WHEN** the configuration includes:
  ```json
  {
    "providers": {
      "minimax": {
        "apiKey": "${MINIMAX_API_KEY}",
        "groupId": "${MINIMAX_GROUP_ID}",
        "apiBase": "https://api.minimax.chat/v1"
      }
    }
  }
  ```
- **THEN** the system SHALL initialize a MiniMax provider
- **AND** support models with prefix "minimax/"

#### Scenario: Use MiniMax model

- **WHEN** the agent uses model "minimax/abab5.5-chat"
- **THEN** the system SHALL route the request to MiniMax API
- **AND** handle MiniMax-specific authentication (groupId)

### Requirement: vLLM Provider

The system SHALL support local vLLM server as an LLM provider.

#### Scenario: Configure vLLM provider

- **WHEN** the configuration includes:
  ```json
  {
    "providers": {
      "vllm": {
        "apiBase": "http://localhost:8000/v1"
      }
    }
  }
  ```
- **THEN** the system SHALL initialize a vLLM provider
- **AND** support models with prefix "vllm/"

#### Scenario: Use vLLM model

- **WHEN** the agent uses model "vllm/llama-2-70b"
- **THEN** the system SHALL route the request to local vLLM server
- **AND** use OpenAI-compatible format

#### Scenario: vLLM without API key

- **WHEN** vLLM provider is configured without apiKey
- **THEN** the system SHALL still accept the configuration
- **AND** send requests without authentication header

### Requirement: Groq Provider

The system SHALL support Groq as an LLM provider for both text and transcription.

#### Scenario: Configure Groq provider

- **WHEN** the configuration includes:
  ```json
  {
    "providers": {
      "groq": {
        "apiKey": "${GROQ_API_KEY}",
        "apiBase": "https://api.groq.com/openai/v1"
      }
    }
  }
  ```
- **THEN** the system SHALL initialize a Groq provider
- **AND** support models with prefix "groq/"

#### Scenario: Use Groq for text generation

- **WHEN** the agent uses model "groq/llama2-70b-4096"
- **THEN** the system SHALL route the request to Groq API
- **AND** use OpenAI-compatible format

### Requirement: AiHubMix Provider

The system SHALL support AiHubMix as a gateway provider.

#### Scenario: Configure AiHubMix provider

- **WHEN** the configuration includes:
  ```json
  {
    "providers": {
      "aihubmix": {
        "apiKey": "${AIHUBMIX_API_KEY}",
        "apiBase": "https://aihubmix.com/v1"
      }
    }
  }
  ```
- **THEN** the system SHALL initialize an AiHubMix provider
- **AND** support models with prefix "aihubmix/"

#### Scenario: Use AiHubMix gateway

- **WHEN** the agent uses model "aihubmix/claude-3-opus"
- **THEN** the system SHALL route the request through AiHubMix gateway
- **AND** use standard OpenAI format

### Requirement: Model Prefix Handling

The system SHALL support model name prefixes for provider routing.

#### Scenario: Extract provider from prefix

- **WHEN** model name is "deepseek/deepseek-chat"
- **THEN** the system SHALL extract provider as "deepseek"
- **AND** model as "deepseek-chat"

#### Scenario: Model without prefix

- **WHEN** model name is "gpt-4" without prefix
- **THEN** the system SHALL use default provider selection logic
- **AND** search for provider that supports this model

#### Scenario: Unknown provider prefix

- **WHEN** model prefix doesn't match any configured provider
- **THEN** the system SHALL log a warning
- **AND** fall back to default provider

### Requirement: Provider Configuration Validation

The system SHALL validate provider configurations.

#### Scenario: Missing API key

- **WHEN** a provider requires apiKey but it's not configured
- **THEN** the system SHALL log an error
- **AND** mark the provider as unavailable

#### Scenario: Invalid API base URL

- **WHEN** apiBase is not a valid URL
- **THEN** the system SHALL log a validation error
- **AND** refuse to initialize the provider

#### Scenario: Provider-specific required fields

- **WHEN** MiniMax provider is configured without groupId
- **THEN** the system SHALL log an error about missing required field
- **AND** mark the provider as unavailable

### Requirement: Provider Priority and Selection

The system SHALL support provider priority for model routing.

#### Scenario: Multiple providers for same model

- **WHEN** multiple providers support the same model
- **THEN** the system SHALL use provider priority order
- **AND** prefer explicitly configured default provider

#### Scenario: Fallback provider

- **WHEN** primary provider fails
- **THEN** the system MAY attempt fallback provider (if configured)
- **AND** log the fallback attempt
