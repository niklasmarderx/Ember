//! Error types for ember-core.

use thiserror::Error;

/// Result type alias for ember-core operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in ember-core.
#[derive(Error, Debug)]
pub enum Error {
    /// LLM provider error
    #[error("LLM error: {0}")]
    Llm(#[from] ember_llm::Error),

    /// Agent configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Agent is not initialized
    #[error("Agent not initialized: {0}")]
    NotInitialized(String),

    /// Tool execution error
    #[error("Tool execution failed: {tool} - {message}")]
    ToolExecution {
        /// Name of the tool that failed
        tool: String,
        /// Error message
        message: String,
    },

    /// Context window exceeded
    #[error("Context window exceeded: {current} tokens > {max} max")]
    ContextOverflow {
        /// Current token count
        current: usize,
        /// Maximum allowed tokens
        max: usize,
    },

    /// Memory operation error
    #[error("Memory error: {0}")]
    Memory(String),

    /// Conversation not found
    #[error("Conversation not found: {0}")]
    ConversationNotFound(uuid::Uuid),

    /// Agent loop limit exceeded
    #[error("Agent loop limit exceeded: {iterations} iterations")]
    LoopLimitExceeded {
        /// Number of iterations performed
        iterations: usize,
    },

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid state transition
    #[error("Invalid state transition: {from} -> {to}")]
    InvalidStateTransition {
        /// Current state
        from: String,
        /// Attempted target state
        to: String,
    },

    /// Timeout error
    #[error("Operation timed out after {seconds}s")]
    Timeout {
        /// Timeout duration in seconds
        seconds: u64,
    },

    /// Cancelled operation
    #[error("Operation was cancelled")]
    Cancelled,

    /// Agent/Orchestrator error
    #[error("Agent error: {0}")]
    Agent(String),

    /// Not implemented error
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Resource exhausted (limits reached)
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Simple timeout error with message
    #[error("Timeout: {0}")]
    TimeoutMsg(String),
}

impl Error {
    /// Create a configuration error.
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Create a not initialized error.
    pub fn not_initialized(msg: impl Into<String>) -> Self {
        Self::NotInitialized(msg.into())
    }

    /// Create a tool execution error.
    pub fn tool_execution(tool: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ToolExecution {
            tool: tool.into(),
            message: message.into(),
        }
    }

    /// Create a memory error.
    pub fn memory(msg: impl Into<String>) -> Self {
        Self::Memory(msg.into())
    }

    /// Check if this error is recoverable.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::ToolExecution { .. } | Self::Timeout { .. } | Self::TimeoutMsg(_) | Self::Memory(_)
        )
    }

    /// Check if this error is due to rate limiting.
    pub fn is_rate_limited(&self) -> bool {
        if let Self::Llm(llm_err) = self {
            llm_err.is_rate_limited()
        } else {
            false
        }
    }
}

// Provide constructor aliases for Error variants used by new modules
impl Error {
    /// Alias for Timeout variant (used by task_planner and streaming)
    #[allow(non_snake_case)]
    pub fn Timeout(msg: impl Into<String>) -> Self {
        Self::TimeoutMsg(msg.into())
    }
}
