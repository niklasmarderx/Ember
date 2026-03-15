//! Error types for ember-llm

use thiserror::Error;

/// Result type alias for ember-llm operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during LLM operations
#[derive(Debug, Error)]
pub enum Error {
    /// API key is missing for the specified provider
    #[error("API key not found for provider '{provider}'. Set {env_var} environment variable or configure in ember.toml")]
    ApiKeyMissing {
        /// The provider name
        provider: String,
        /// The environment variable to set
        env_var: String,
    },

    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    /// API returned an error response
    #[error("API error from {provider}: {message} (status: {status})")]
    ApiError {
        /// The provider name
        provider: String,
        /// HTTP status code
        status: u16,
        /// Error message from the API
        message: String,
    },

    /// Failed to parse API response
    #[error("Failed to parse response: {0}")]
    ParseError(#[from] serde_json::Error),

    /// Model not found or not available
    #[error("Model '{model}' not found or not available on {provider}")]
    ModelNotFound {
        /// The model name
        model: String,
        /// The provider name
        provider: String,
    },

    /// Rate limit exceeded
    #[error("Rate limit exceeded for {provider}. Retry after {retry_after:?} seconds")]
    RateLimitExceeded {
        /// The provider name
        provider: String,
        /// Optional retry-after duration in seconds
        retry_after: Option<u64>,
    },

    /// Context length exceeded
    #[error("Context length exceeded: {message}")]
    ContextLengthExceeded {
        /// Error message
        message: String,
    },

    /// Invalid request parameters
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Provider is not available (e.g., Ollama not running)
    #[error("Provider '{provider}' is not available: {message}")]
    ProviderUnavailable {
        /// The provider name
        provider: String,
        /// Error message
        message: String,
    },

    /// Streaming error
    #[error("Streaming error: {0}")]
    StreamError(String),

    /// Timeout during request
    #[error("Request timed out after {seconds} seconds")]
    Timeout {
        /// Timeout duration in seconds
        seconds: u64,
    },

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Tool calling error
    #[error("Tool calling error: {0}")]
    ToolError(String),
}

impl Error {
    /// Create a new API key missing error
    pub fn api_key_missing(provider: impl Into<String>) -> Self {
        let provider = provider.into();
        let env_var = format!("{}_API_KEY", provider.to_uppercase());
        Self::ApiKeyMissing { provider, env_var }
    }

    /// Create a new API error
    pub fn api_error(provider: impl Into<String>, status: u16, message: impl Into<String>) -> Self {
        Self::ApiError {
            provider: provider.into(),
            status,
            message: message.into(),
        }
    }

    /// Create a new model not found error
    pub fn model_not_found(model: impl Into<String>, provider: impl Into<String>) -> Self {
        Self::ModelNotFound {
            model: model.into(),
            provider: provider.into(),
        }
    }

    /// Create a new rate limit error
    pub fn rate_limit(provider: impl Into<String>, retry_after: Option<u64>) -> Self {
        Self::RateLimitExceeded {
            provider: provider.into(),
            retry_after,
        }
    }

    /// Create a new provider unavailable error
    pub fn provider_unavailable(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ProviderUnavailable {
            provider: provider.into(),
            message: message.into(),
        }
    }

    /// Check if the error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::HttpError(_)
                | Self::RateLimitExceeded { .. }
                | Self::Timeout { .. }
                | Self::ProviderUnavailable { .. }
        )
    }

    /// Check if the error is due to rate limiting
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, Self::RateLimitExceeded { .. })
    }

    /// Get the retry-after duration if this is a rate limit error
    pub fn retry_after(&self) -> Option<u64> {
        if let Self::RateLimitExceeded { retry_after, .. } = self {
            *retry_after
        } else {
            None
        }
    }

    /// Get a user-friendly error message with suggestions
    pub fn user_message(&self) -> String {
        match self {
            Self::ApiKeyMissing { provider, env_var } => {
                format!(
                    "API key for {} not found.\n\n\
                    Set it via environment variable:\n  \
                    export {}=your-key-here\n\n\
                    Or add to ~/.ember/config.toml:\n  \
                    [llm.{}]\n  \
                    api_key = \"your-key-here\"",
                    provider,
                    env_var,
                    provider.to_lowercase()
                )
            }
            Self::ProviderUnavailable { provider, message } => {
                if provider == "ollama" {
                    format!(
                        "Ollama is not running.\n\n\
                        Start Ollama with:\n  \
                        ollama serve\n\n\
                        Or install it from: https://ollama.com\n\n\
                        Original error: {}",
                        message
                    )
                } else {
                    format!(
                        "{} is not available: {}\n\n\
                        Please check your network connection and try again.",
                        provider, message
                    )
                }
            }
            Self::RateLimitExceeded {
                provider,
                retry_after,
            } => {
                let retry_msg = retry_after
                    .map(|s| format!(" Try again in {} seconds.", s))
                    .unwrap_or_default();
                format!(
                    "Rate limit exceeded for {}.{}\n\n\
                    Consider using a different provider or waiting before retrying.",
                    provider, retry_msg
                )
            }
            _ => self.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_missing_error() {
        let err = Error::api_key_missing("openai");
        assert!(err.to_string().contains("OPENAI_API_KEY"));
    }

    #[test]
    fn test_is_retryable() {
        assert!(Error::rate_limit("openai", Some(60)).is_retryable());
        assert!(Error::Timeout { seconds: 30 }.is_retryable());
        assert!(!Error::api_key_missing("openai").is_retryable());
    }

    #[test]
    fn test_user_message() {
        let err = Error::api_key_missing("openai");
        let msg = err.user_message();
        assert!(msg.contains("OPENAI_API_KEY"));
        assert!(msg.contains("~/.ember/config.toml"));
    }
}
