//! Error types for ember-plugins.
//!
//! This module defines all plugin-related errors using thiserror.

use std::path::PathBuf;
use thiserror::Error;

/// Plugin errors that can occur during plugin operations.
#[derive(Error, Debug)]
pub enum PluginError {
    /// Plugin not found.
    #[error("Plugin not found: {0}")]
    NotFound(String),

    /// Plugin already loaded.
    #[error("Plugin already loaded: {0}")]
    AlreadyLoaded(String),

    /// Invalid plugin format.
    #[error("Invalid plugin format: {0}")]
    InvalidFormat(String),

    /// Plugin load failed.
    #[error("Failed to load plugin from {}: {reason}", path.display())]
    LoadFailed {
        /// Path to the plugin file.
        path: PathBuf,
        /// Reason for failure.
        reason: String,
    },

    /// Plugin initialization failed.
    #[error("Plugin initialization failed: {0}")]
    InitFailed(String),

    /// Plugin function not found.
    #[error("Function not found in plugin: {plugin}::{function}")]
    FunctionNotFound {
        /// Plugin name.
        plugin: String,
        /// Function name.
        function: String,
    },

    /// Plugin execution failed.
    #[error("Plugin execution failed: {0}")]
    ExecutionFailed(String),

    /// Plugin timed out.
    #[error("Plugin execution timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// Memory limit exceeded.
    #[error("Plugin memory limit exceeded: {current} > {limit} bytes")]
    MemoryExceeded {
        /// Current memory usage.
        current: usize,
        /// Maximum allowed memory.
        limit: usize,
    },

    /// Invalid plugin manifest.
    #[error("Invalid plugin manifest: {0}")]
    InvalidManifest(String),

    /// Plugin version mismatch.
    #[error("Plugin version mismatch: expected {expected}, got {actual}")]
    VersionMismatch {
        /// Expected version.
        expected: String,
        /// Actual version.
        actual: String,
    },

    /// WASM compilation error.
    #[cfg(feature = "wasmtime")]
    #[error("WASM compilation error: {0}")]
    WasmCompilation(String),

    /// WASM runtime error.
    #[cfg(feature = "wasmtime")]
    #[error("WASM runtime error: {0}")]
    WasmRuntime(#[from] anyhow::Error),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Internal plugin error.
    #[error("Internal plugin error: {0}")]
    Internal(String),
}

/// Result type alias for plugin operations.
pub type Result<T> = std::result::Result<T, PluginError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = PluginError::NotFound("my-plugin".to_string());
        assert_eq!(err.to_string(), "Plugin not found: my-plugin");
    }

    #[test]
    fn test_function_not_found_error() {
        let err = PluginError::FunctionNotFound {
            plugin: "calculator".to_string(),
            function: "divide".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Function not found in plugin: calculator::divide"
        );
    }

    #[test]
    fn test_version_mismatch_error() {
        let err = PluginError::VersionMismatch {
            expected: "1.0".to_string(),
            actual: "2.0".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Plugin version mismatch: expected 1.0, got 2.0"
        );
    }
}
