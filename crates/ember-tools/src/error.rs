//! Error types for ember-tools.

use thiserror::Error;

/// Result type alias for ember-tools operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in ember-tools.
#[derive(Error, Debug)]
pub enum Error {
    /// Tool not found
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    /// Invalid tool arguments
    #[error("Invalid arguments for tool '{tool}': {message}")]
    InvalidArguments {
        /// Tool name
        tool: String,
        /// Error message
        message: String,
    },

    /// Tool execution failed
    #[error("Tool execution failed: {tool} - {message}")]
    ExecutionFailed {
        /// Tool name
        tool: String,
        /// Error message
        message: String,
    },

    /// Shell command failed
    #[error("Shell command failed with exit code {code}: {stderr}")]
    ShellCommandFailed {
        /// Exit code
        code: i32,
        /// Standard error output
        stderr: String,
    },

    /// Shell command timed out
    #[error("Shell command timed out after {seconds}s")]
    ShellTimeout {
        /// Timeout in seconds
        seconds: u64,
    },

    /// Filesystem error
    #[error("Filesystem error: {0}")]
    Filesystem(String),

    /// Path not allowed (security)
    #[error("Path not allowed: {0}")]
    PathNotAllowed(String),

    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// Permission denied
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    HttpRequest(String),

    /// JSON parsing error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Tool is disabled
    #[error("Tool '{0}' is disabled")]
    ToolDisabled(String),

    /// Missing required parameter
    #[error("Missing required parameter: {0}")]
    MissingParameter(String),

    /// Invalid parameter value
    #[error("Invalid parameter '{name}': {reason}")]
    InvalidParameter {
        /// Parameter name
        name: String,
        /// Reason for invalidity
        reason: String,
    },
}

impl Error {
    /// Create an invalid arguments error.
    pub fn invalid_arguments(tool: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidArguments {
            tool: tool.into(),
            message: message.into(),
        }
    }

    /// Create an execution failed error.
    pub fn execution_failed(tool: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ExecutionFailed {
            tool: tool.into(),
            message: message.into(),
        }
    }

    /// Create a filesystem error.
    pub fn filesystem(message: impl Into<String>) -> Self {
        Self::Filesystem(message.into())
    }

    /// Create a path not allowed error.
    pub fn path_not_allowed(path: impl Into<String>) -> Self {
        Self::PathNotAllowed(path.into())
    }
}