//! Error types for gasket-core public APIs
//!
//! This module defines specific error types using `thiserror` for better
//! error handling and API contracts. Library crates should NOT expose
//! `anyhow::Error` in their public APIs - it's only for internal use.
//!
//! Error chains are preserved through `#[source]` and `#[from]` attributes,
//! enabling full backtrace traversal with `RUST_BACKTRACE=1`.

use thiserror::Error;

/// Errors that can occur during agent processing
#[derive(Debug, Error)]
pub enum AgentError {
    /// Error from the LLM provider
    #[error("LLM provider error: {0}")]
    ProviderError(#[from] ProviderError),

    /// Error during tool execution
    #[error("Tool execution error: {0}")]
    ToolError(String),

    /// Error during session management
    #[error("Session error: {0}")]
    SessionError(String),

    /// Error during context preparation
    #[error("Context preparation error: {0}")]
    ContextError(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// I/O error
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Hook execution error
    #[error("Hook '{name}' failed: {message}")]
    HookFailed { name: String, message: String },

    /// Request aborted by hook
    #[error("Request aborted by hook: {0}")]
    AbortedByHook(String),

    /// Generic error with message
    #[error("{0}")]
    Other(String),

    /// Internal error preserving the full error chain
    #[error(transparent)]
    Internal(Box<dyn std::error::Error + Send + Sync>),
}

/// Errors from LLM providers
#[derive(Debug, Error)]
pub enum ProviderError {
    /// API authentication failed
    #[error("Authentication failed: {0}")]
    AuthError(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded: {0}")]
    RateLimitError(String),

    /// Invalid request
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Model not found
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    /// Network error
    #[error("Network error: {0}")]
    NetworkError(String),

    /// API error with status code
    #[error("API error (status {status_code}): {message}")]
    ApiError { status_code: u16, message: String },

    /// Response parsing error
    #[error("Failed to parse response: {0}")]
    ParseError(String),

    /// Generic provider error
    #[error("{0}")]
    Other(String),

    /// Internal error preserving the full error chain
    #[error(transparent)]
    Internal(Box<dyn std::error::Error + Send + Sync>),
}

/// Errors from channel operations
#[derive(Debug, Error)]
pub enum ChannelError {
    /// Channel not configured
    #[error("Channel '{0}' not configured")]
    NotConfigured(String),

    /// Authentication failed
    #[error("Channel authentication failed: {0}")]
    AuthError(String),

    /// Send message error
    #[error("Failed to send message: {0}")]
    SendError(String),

    /// Receive message error
    #[error("Failed to receive message: {0}")]
    ReceiveError(String),

    /// Invalid message format
    #[error("Invalid message format: {0}")]
    InvalidFormat(String),

    /// Internal error preserving the full error chain
    #[error(transparent)]
    Internal(Box<dyn std::error::Error + Send + Sync>),
}

/// Configuration validation errors
#[derive(Debug, Error)]
pub enum ConfigValidationError {
    /// Provider not available (missing API key for non-local providers)
    #[error("Provider '{0}' is not available (missing API key)")]
    ProviderNotAvailable(String),

    /// Incomplete email configuration
    #[error("Email channel requires either IMAP or SMTP configuration (host, username, password, and from_address for SMTP)")]
    IncompleteEmailConfig,

    /// Missing specific email config field
    #[error("Email configuration missing required field: {0}")]
    MissingEmailField(String),

    /// Invalid channel configuration
    #[error("Channel '{0}' has invalid configuration: {1}")]
    InvalidChannelConfig(String, String),
}

/// Errors from the multi-agent pipeline subsystem
#[derive(Debug, Error)]
pub enum PipelineError {
    /// Pipeline is not enabled in config
    #[error("Pipeline is not enabled")]
    NotEnabled,

    /// Task not found
    #[error("Pipeline task not found: {0}")]
    TaskNotFound(String),

    /// Illegal state transition
    #[error("Invalid state transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },

    /// Caller not allowed to delegate to target
    #[error("Permission denied: role '{caller}' cannot delegate to '{target}'")]
    PermissionDenied { caller: String, target: String },

    /// Too many review round-trips
    #[error("Review limit exceeded for task {0} (max {1})")]
    ReviewLimitExceeded(String, u32),

    /// Task stalled (no heartbeat within timeout)
    #[error("Stall detected for task {0}")]
    StallDetected(String),

    /// Persistence layer error
    #[error("Pipeline store error: {0}")]
    StoreError(String),
}

// ============================================================================
// From<anyhow::Error> — preserve full error chain via Internal variant
// ============================================================================

impl From<anyhow::Error> for AgentError {
    fn from(err: anyhow::Error) -> Self {
        AgentError::Internal(err.into())
    }
}

impl From<anyhow::Error> for ProviderError {
    fn from(err: anyhow::Error) -> Self {
        ProviderError::Internal(err.into())
    }
}

impl From<anyhow::Error> for ChannelError {
    fn from(err: anyhow::Error) -> Self {
        ChannelError::Internal(err.into())
    }
}

impl From<gasket_channels::ChannelConfigError> for ConfigValidationError {
    fn from(err: gasket_channels::ChannelConfigError) -> Self {
        match err {
            gasket_channels::ChannelConfigError::IncompleteEmailConfig => {
                ConfigValidationError::IncompleteEmailConfig
            }
            gasket_channels::ChannelConfigError::InvalidChannelConfig(ch, msg) => {
                ConfigValidationError::InvalidChannelConfig(ch, msg)
            }
        }
    }
}

// ============================================================================
// From<gasket_core::*> — allow migration from core errors
// ============================================================================

impl From<gasket_core::error::AgentError> for AgentError {
    fn from(err: gasket_core::error::AgentError) -> Self {
        // Convert core AgentError to engine AgentError
        match err {
            gasket_core::error::AgentError::ProviderError(e) => AgentError::ProviderError(e.into()),
            gasket_core::error::AgentError::ToolError(e) => AgentError::ToolError(e),
            gasket_core::error::AgentError::SessionError(e) => AgentError::SessionError(e),
            gasket_core::error::AgentError::ContextError(e) => AgentError::ContextError(e),
            gasket_core::error::AgentError::ConfigError(e) => AgentError::ConfigError(e),
            gasket_core::error::AgentError::IoError(e) => AgentError::IoError(e),
            gasket_core::error::AgentError::HookFailed { name, message } => AgentError::HookFailed { name, message },
            gasket_core::error::AgentError::AbortedByHook(e) => AgentError::AbortedByHook(e),
            gasket_core::error::AgentError::Other(e) => AgentError::Other(e),
            gasket_core::error::AgentError::Internal(e) => AgentError::Internal(e),
        }
    }
}

impl From<gasket_core::error::ProviderError> for ProviderError {
    fn from(err: gasket_core::error::ProviderError) -> Self {
        // Convert core ProviderError to engine ProviderError
        match err {
            gasket_core::error::ProviderError::AuthError(e) => ProviderError::AuthError(e),
            gasket_core::error::ProviderError::RateLimitError(e) => ProviderError::RateLimitError(e),
            gasket_core::error::ProviderError::InvalidRequest(e) => ProviderError::InvalidRequest(e),
            gasket_core::error::ProviderError::ModelNotFound(e) => ProviderError::ModelNotFound(e),
            gasket_core::error::ProviderError::NetworkError(e) => ProviderError::NetworkError(e),
            gasket_core::error::ProviderError::ApiError { status_code, message } => ProviderError::ApiError { status_code, message },
            gasket_core::error::ProviderError::ParseError(e) => ProviderError::ParseError(e),
            gasket_core::error::ProviderError::Other(e) => ProviderError::Other(e),
            gasket_core::error::ProviderError::Internal(e) => ProviderError::Internal(e),
        }
    }
}

impl From<gasket_core::error::ChannelError> for ChannelError {
    fn from(err: gasket_core::error::ChannelError) -> Self {
        // Convert core ChannelError to engine ChannelError
        match err {
            gasket_core::error::ChannelError::NotConfigured(e) => ChannelError::NotConfigured(e),
            gasket_core::error::ChannelError::AuthError(e) => ChannelError::AuthError(e),
            gasket_core::error::ChannelError::SendError(e) => ChannelError::SendError(e),
            gasket_core::error::ChannelError::ReceiveError(e) => ChannelError::ReceiveError(e),
            gasket_core::error::ChannelError::InvalidFormat(e) => ChannelError::InvalidFormat(e),
            gasket_core::error::ChannelError::Internal(e) => ChannelError::Internal(e),
        }
    }
}

impl From<gasket_core::error::ConfigValidationError> for ConfigValidationError {
    fn from(err: gasket_core::error::ConfigValidationError) -> Self {
        // Convert core ConfigValidationError to engine ConfigValidationError
        match err {
            gasket_core::error::ConfigValidationError::ProviderNotAvailable(e) => ConfigValidationError::ProviderNotAvailable(e),
            gasket_core::error::ConfigValidationError::IncompleteEmailConfig => ConfigValidationError::IncompleteEmailConfig,
            gasket_core::error::ConfigValidationError::MissingEmailField(e) => ConfigValidationError::MissingEmailField(e),
            gasket_core::error::ConfigValidationError::InvalidChannelConfig(ch, msg) => ConfigValidationError::InvalidChannelConfig(ch, msg),
        }
    }
}

impl From<gasket_core::error::PipelineError> for PipelineError {
    fn from(err: gasket_core::error::PipelineError) -> Self {
        // Convert core PipelineError to engine PipelineError
        match err {
            gasket_core::error::PipelineError::NotEnabled => PipelineError::NotEnabled,
            gasket_core::error::PipelineError::TaskNotFound(e) => PipelineError::TaskNotFound(e),
            gasket_core::error::PipelineError::InvalidTransition { from, to } => PipelineError::InvalidTransition { from, to },
            gasket_core::error::PipelineError::PermissionDenied { caller, target } => PipelineError::PermissionDenied { caller, target },
            gasket_core::error::PipelineError::ReviewLimitExceeded(task, limit) => PipelineError::ReviewLimitExceeded(task, limit),
            gasket_core::error::PipelineError::StallDetected(task) => PipelineError::StallDetected(task),
            gasket_core::error::PipelineError::StoreError(e) => PipelineError::StoreError(e),
        }
    }
}
