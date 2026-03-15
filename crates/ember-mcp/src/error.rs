//! Error types for ember-mcp

use thiserror::Error;

/// Result type alias for MCP operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during MCP operations
#[derive(Debug, Error)]
pub enum Error {
    /// Tool not found in registry
    #[error("MCP tool not found: {0}")]
    ToolNotFound(String),

    /// Resource not found
    #[error("MCP resource not found: {uri}")]
    ResourceNotFound {
        /// Resource URI
        uri: String,
    },

    /// Invalid request format
    #[error("Invalid MCP request: {0}")]
    InvalidRequest(String),

    /// Invalid response format
    #[error("Invalid MCP response: {0}")]
    InvalidResponse(String),

    /// Serialization/deserialization error
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Tool execution failed
    #[error("Tool execution failed: {tool} - {message}")]
    ToolExecutionFailed {
        /// Tool name
        tool: String,
        /// Error message
        message: String,
    },

    /// Server not initialized
    #[error("MCP server not initialized")]
    NotInitialized,

    /// Server already initialized
    #[error("MCP server already initialized")]
    AlreadyInitialized,

    /// Protocol version mismatch
    #[error("Protocol version mismatch: expected {expected}, got {actual}")]
    VersionMismatch {
        /// Expected version
        expected: String,
        /// Actual version
        actual: String,
    },

    /// Capability not supported
    #[error("Capability not supported: {0}")]
    CapabilityNotSupported(String),

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Internal error
    #[error("Internal MCP error: {0}")]
    Internal(String),

    /// Protocol error (client-specific)
    #[error("MCP protocol error: {message}")]
    Protocol {
        /// Error message
        message: String,
    },

    /// Transport error (client-specific)
    #[error("MCP transport error: {message}")]
    Transport {
        /// Error message
        message: String,
    },

    /// Tool execution error (client-specific)
    #[error("Tool '{tool}' execution failed: {message}")]
    ToolExecution {
        /// Tool name
        tool: String,
        /// Error message
        message: String,
    },
}

impl Error {
    /// Create a new tool not found error
    pub fn tool_not_found(name: impl Into<String>) -> Self {
        Self::ToolNotFound(name.into())
    }

    /// Create a new resource not found error
    pub fn resource_not_found(uri: impl Into<String>) -> Self {
        Self::ResourceNotFound { uri: uri.into() }
    }

    /// Create a new tool execution failed error
    pub fn tool_failed(tool: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ToolExecutionFailed {
            tool: tool.into(),
            message: message.into(),
        }
    }

    /// Create a new invalid request error
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::InvalidRequest(message.into())
    }

    /// Create a new internal error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::tool_not_found("shell");
        assert!(err.to_string().contains("shell"));
    }

    #[test]
    fn test_tool_failed() {
        let err = Error::tool_failed("read_file", "Permission denied");
        assert!(err.to_string().contains("read_file"));
        assert!(err.to_string().contains("Permission denied"));
    }
}
