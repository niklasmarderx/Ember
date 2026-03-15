//! Error types for ember-storage.
//!
//! This module defines all storage-related errors using thiserror.

use std::path::PathBuf;
use thiserror::Error;

/// Storage errors that can occur during database operations.
#[derive(Error, Debug)]
pub enum StorageError {
    /// Database connection failed.
    #[error("Failed to connect to database: {0}")]
    ConnectionFailed(String),

    /// Database query failed.
    #[error("Database query failed: {0}")]
    QueryFailed(String),

    /// Record not found.
    #[error("Record not found: {entity} with id {id}")]
    NotFound {
        /// Entity type (e.g., "conversation", "memory").
        entity: String,
        /// Record identifier.
        id: String,
    },

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Database migration failed.
    #[error("Migration failed: {0}")]
    MigrationFailed(String),

    /// Invalid database path.
    #[error("Invalid database path: {}", path.display())]
    InvalidPath {
        /// The invalid path.
        path: PathBuf,
    },

    /// Database is locked.
    #[error("Database is locked: {0}")]
    Locked(String),

    /// Storage capacity exceeded.
    #[error("Storage capacity exceeded: {current} > {limit}")]
    CapacityExceeded {
        /// Current size.
        current: usize,
        /// Maximum allowed size.
        limit: usize,
    },

    /// Vector dimension mismatch.
    #[error("Vector dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch {
        /// Expected dimension.
        expected: usize,
        /// Actual dimension.
        actual: usize,
    },

    /// SQLite-specific error.
    #[cfg(feature = "sqlite")]
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Internal storage error.
    #[error("Internal storage error: {0}")]
    Internal(String),

    /// Generic storage error.
    #[error("Storage error: {0}")]
    Storage(String),
}

/// Result type alias for storage operations.
pub type Result<T> = std::result::Result<T, StorageError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = StorageError::NotFound {
            entity: "conversation".to_string(),
            id: "123".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Record not found: conversation with id 123"
        );
    }

    #[test]
    fn test_error_dimension_mismatch() {
        let err = StorageError::DimensionMismatch {
            expected: 1536,
            actual: 768,
        };
        assert_eq!(
            err.to_string(),
            "Vector dimension mismatch: expected 1536, got 768"
        );
    }
}
