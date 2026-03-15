//! Error types for ember-web.
//!
//! This module defines web-specific errors and error responses.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Web server errors.
#[derive(Error, Debug)]
pub enum WebError {
    /// Request validation failed.
    #[error("Validation error: {0}")]
    Validation(String),

    /// Resource not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// Unauthorized request.
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Forbidden request.
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// Rate limit exceeded.
    #[error("Rate limit exceeded")]
    RateLimited,

    /// Internal server error.
    #[error("Internal error: {0}")]
    Internal(String),

    /// Bad request.
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// Service unavailable.
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    /// Timeout error.
    #[error("Request timeout")]
    Timeout,

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias for web operations.
pub type Result<T> = std::result::Result<T, WebError>;

/// Error response sent to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error code.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
    /// Optional details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    /// Create a new error response.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    /// Add details to the error response.
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let (status, error_response) = match &self {
            WebError::Validation(msg) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse::new("VALIDATION_ERROR", msg),
            ),
            WebError::NotFound(msg) => {
                (StatusCode::NOT_FOUND, ErrorResponse::new("NOT_FOUND", msg))
            }
            WebError::Unauthorized(msg) => (
                StatusCode::UNAUTHORIZED,
                ErrorResponse::new("UNAUTHORIZED", msg),
            ),
            WebError::Forbidden(msg) => {
                (StatusCode::FORBIDDEN, ErrorResponse::new("FORBIDDEN", msg))
            }
            WebError::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                ErrorResponse::new("RATE_LIMITED", "Too many requests"),
            ),
            WebError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse::new("INTERNAL_ERROR", msg),
            ),
            WebError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse::new("BAD_REQUEST", msg),
            ),
            WebError::ServiceUnavailable(msg) => (
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorResponse::new("SERVICE_UNAVAILABLE", msg),
            ),
            WebError::Timeout => (
                StatusCode::REQUEST_TIMEOUT,
                ErrorResponse::new("TIMEOUT", "Request timed out"),
            ),
            WebError::Serialization(e) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse::new("SERIALIZATION_ERROR", e.to_string()),
            ),
            WebError::Io(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse::new("IO_ERROR", e.to_string()),
            ),
        };

        (status, Json(error_response)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_response_creation() {
        let response = ErrorResponse::new("TEST_ERROR", "Something went wrong");
        assert_eq!(response.code, "TEST_ERROR");
        assert_eq!(response.message, "Something went wrong");
        assert!(response.details.is_none());
    }

    #[test]
    fn test_error_response_with_details() {
        let response = ErrorResponse::new("TEST_ERROR", "Something went wrong")
            .with_details(serde_json::json!({"field": "email", "reason": "invalid format"}));

        assert!(response.details.is_some());
    }

    #[test]
    fn test_error_serialization() {
        let response = ErrorResponse::new("NOT_FOUND", "Resource not found");
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("NOT_FOUND"));
    }
}
