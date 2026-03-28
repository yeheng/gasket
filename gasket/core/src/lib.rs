//! gasket-core: Facade for gasket AI assistant framework
//!
//! This crate provides a unified API for the gasket assistant framework.
//! All implementation is delegated to specialized crates.

// Local modules that still exist
pub mod bus;
pub mod channels;
pub mod config;
pub mod heartbeat;
pub mod providers;
pub mod tools;
pub mod vault;

// Re-export types from gasket-types (canonical source)
pub use gasket_types::*;

// Re-export from gasket-engine (the main implementation crate)
pub use gasket_engine::{
    // Agent types
    AgentConfig, AgentContext, AgentExecutor, AgentLoop, AgentResponse, ExecutionResult,
    ExecutorOptions, StreamEvent,
    // Tool types
    CronTool, EditFileTool, ExecTool, HistorySearchTool, ListDirTool, MemorySearchTool,
    MessageTool, ReadFileTool, SpawnParallelTool, SpawnTool, ToolRegistry, WebFetchTool,
    WebSearchTool, WriteFileTool,
    // Config
    config_dir,
    // Error types
    AgentError, ChannelError, ConfigValidationError, PipelineError, ProviderError,
    // Token tracking
    SessionTokenStats,
    // Subagent
    SubagentManager, run_subagent, SessionKeyGuard,
    // Memory
    MemoryStore,
    // Pipeline
    PipelineContext, process_message,
    // Compression
    CompressionActor, EmbeddingService, SummarizationService,
    // Tracker
    SubagentTracker, TrackerError,
};

// Re-export tool config types from engine
pub use gasket_engine::{
    CommandPolicyConfig, ExecToolConfig, ResourceLimitsConfig, SandboxConfig, ToolsConfig,
    WebToolsConfig,
};

// Re-export skills types from engine
pub use gasket_engine::{Skill, SkillsLoader, SkillMetadata, SkillsRegistry};

// Re-export hooks types from engine
pub use gasket_engine::{HookRegistry, HookPoint, HookAction, MutableContext, HookContext, PipelineHook};

// Re-export cron types from engine
pub use gasket_engine::{CronJob, CronService};

// Re-export vault types from engine
pub use gasket_engine::{InjectionReport, VaultInjector};

// Re-export bus types (via local bus module which re-exports from gasket_bus)
pub use gasket_bus::{
    events as bus_events, run_outbound_actor, run_router_actor, run_session_actor, MessageBus,
    MessageHandler,
};

// Re-export history types
pub use gasket_history::{
    count_tokens, process_history, HistoryConfig, HistoryQuery, HistoryQueryBuilder, HistoryResult,
    HistoryRetriever, ProcessedHistory, QueryOrder, ResultMeta, SemanticQuery, TimeRange,
};

// Re-export providers
pub use gasket_providers::{
    build_http_client, parse_json_args, streaming, ChatMessage, ChatRequest, ChatResponse,
    ChatStream, ChatStreamChunk, ChatStreamDelta, FinishReason, FunctionCall, FunctionDefinition,
    LlmProvider, MessageRole, ModelSpec, OpenAICompatibleProvider, ProviderBuildError,
    ProviderConfig, ProviderResult, ThinkingConfig, ToolCall, ToolCallDelta, ToolDefinition, Usage,
};

#[cfg(feature = "provider-gemini")]
pub use gasket_providers::GeminiProvider;
#[cfg(feature = "provider-copilot")]
pub use gasket_providers::{
    CopilotOAuth, CopilotProvider, CopilotTokenResponse, DeviceCodeResponse,
};

// Re-export channels
pub use gasket_channels::{
    base, log_inbound, middleware, outbound, Channel, ChannelConfigError, ChannelType,
    ChannelsConfig, DingTalkConfig, DiscordConfig, EmailConfig, FeishuConfig, InboundMessage,
    InboundSender, MediaAttachment, OutboundMessage, OutboundSender, OutboundSenderRegistry,
    SessionKey, SessionKeyParseError, SimpleAuthChecker, SimpleRateLimiter, SlackConfig,
    TelegramConfig, WebSocketMessage,
};

// Re-export webhook (feature-gated)
#[cfg(any(
    feature = "webhook",
    feature = "dingtalk",
    feature = "feishu",
    feature = "wecom"
))]
pub use gasket_channels::webhook;

// Re-export vault base types from gasket_vault crate
pub use gasket_vault::{
    contains_placeholders, contains_secrets, extract_keys, redact_message_secrets, redact_secrets,
    replace_placeholders, scan_placeholders, AtomicTimestamp, EncryptedData, KdfParams,
    Placeholder, VaultCrypto, VaultEntryV2, VaultError, VaultFileV2, VaultMetadata, VaultStore,
};

// Re-export semantic for local embedding support
pub use gasket_semantic as semantic;

// Re-export storage
pub use gasket_storage as storage;
