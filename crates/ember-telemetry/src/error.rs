//! Telemetry error types

use thiserror::Error;

/// Telemetry errors
#[derive(Debug, Error)]
pub enum TelemetryError {
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Storage error
    #[error("Storage error: {0}")]
    Storage(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Network error (for remote reporting)
    #[cfg(feature = "remote")]
    #[error("Network error: {0}")]
    Network(String),
}
