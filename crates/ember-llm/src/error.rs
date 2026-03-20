//! Error types for ember-llm
//!
//! Each error type has a unique error code for easy troubleshooting:
//! - E001-E099: Authentication & API Key errors
//! - E100-E199: Network & Connection errors
//! - E200-E299: API Response errors
//! - E300-E399: Model & Provider errors
//! - E400-E499: Request & Input errors
//! - E500-E599: Configuration errors

use thiserror::Error;

/// Result type alias for ember-llm operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error code for troubleshooting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    // Authentication & API Key errors (E001-E099)
    /// API key is missing
    E001,
    /// API key is invalid
    E002,
    /// API key has expired
    E003,

    // Network & Connection errors (E100-E199)
    /// Network request failed
    E100,
    /// Connection refused
    E101,
    /// Connection timeout
    E102,
    /// DNS resolution failed
    E103,
    /// SSL/TLS error
    E104,

    // API Response errors (E200-E299)
    /// API returned an error
    E200,
    /// Failed to parse response
    E201,
    /// Unexpected response format
    E202,
    /// Streaming error
    E203,

    // Model & Provider errors (E300-E399)
    /// Model not found
    E300,
    /// Provider unavailable
    E301,
    /// Rate limit exceeded
    E302,
    /// Context length exceeded
    E303,
    /// Tool calling not supported
    E304,

    // Request & Input errors (E400-E499)
    /// Invalid request
    E400,
    /// Invalid parameters
    E401,
    /// Request too large
    E402,

    // Configuration errors (E500-E599)
    /// Configuration error
    E500,
    /// Invalid configuration
    E501,
}

impl ErrorCode {
    /// Get the string representation of the error code
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::E001 => "E001",
            Self::E002 => "E002",
            Self::E003 => "E003",
            Self::E100 => "E100",
            Self::E101 => "E101",
            Self::E102 => "E102",
            Self::E103 => "E103",
            Self::E104 => "E104",
            Self::E200 => "E200",
            Self::E201 => "E201",
            Self::E202 => "E202",
            Self::E203 => "E203",
            Self::E300 => "E300",
            Self::E301 => "E301",
            Self::E302 => "E302",
            Self::E303 => "E303",
            Self::E304 => "E304",
            Self::E400 => "E400",
            Self::E401 => "E401",
            Self::E402 => "E402",
            Self::E500 => "E500",
            Self::E501 => "E501",
        }
    }

    /// Get the documentation URL for this error code
    pub fn doc_url(&self) -> String {
        format!(
            "https://docs.ember.dev/errors/{}",
            self.as_str().to_lowercase()
        )
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

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
                    "🔑 API key for {} not found.\n\n\
                    Set it via environment variable:\n  \
                    export {}=your-key-here\n\n\
                    Or add to ~/.ember/config.toml:\n  \
                    [llm.{}]\n  \
                    api_key = \"your-key-here\"\n\n\
                    📖 See: https://docs.ember.dev/providers/{}",
                    provider,
                    env_var,
                    provider.to_lowercase(),
                    provider.to_lowercase()
                )
            }
            Self::ProviderUnavailable { provider, message } => {
                if provider == "ollama" {
                    format!(
                        "🔌 Ollama is not running.\n\n\
                        Start Ollama with:\n  \
                        ollama serve\n\n\
                        Or install it from: https://ollama.com\n\n\
                        📖 See: https://docs.ember.dev/providers/ollama\n\n\
                        Original error: {}",
                        message
                    )
                } else {
                    format!(
                        "🔌 {} is not available: {}\n\n\
                        Troubleshooting steps:\n\
                        1. Check your internet connection\n\
                        2. Verify the provider's status page\n\
                        3. Try again in a few moments\n\n\
                        📖 See: https://docs.ember.dev/providers/{}",
                        provider,
                        message,
                        provider.to_lowercase()
                    )
                }
            }
            Self::RateLimitExceeded {
                provider,
                retry_after,
            } => {
                let retry_msg = retry_after
                    .map(|s| format!("\n\n⏱️  Wait {} seconds before retrying.", s))
                    .unwrap_or_default();
                format!(
                    "⚠️  Rate limit exceeded for {}.{}\n\n\
                    Options:\n\
                    1. Wait a moment and try again\n\
                    2. Use a different provider: ember chat --provider ollama\n\
                    3. Upgrade your API plan for higher limits\n\n\
                    💡 Tip: Use --provider to switch providers temporarily.",
                    provider, retry_msg
                )
            }
            Self::ModelNotFound { model, provider } => {
                format!(
                    "❌ Model '{}' not found on {}.\n\n\
                    Suggestions:\n\
                    1. Check the model name spelling\n\
                    2. List available models: ember models --provider {}\n\
                    3. Try a default model\n\n\
                    Popular models for {}:\n{}",
                    model,
                    provider,
                    provider.to_lowercase(),
                    provider,
                    Self::suggest_models(provider)
                )
            }
            Self::ContextLengthExceeded { message } => {
                format!(
                    "📏 Context length exceeded: {}\n\n\
                    Solutions:\n\
                    1. Shorten your message or conversation\n\
                    2. Clear conversation history: /clear\n\
                    3. Use a model with larger context window\n\n\
                    💡 Tip: GPT-4 Turbo supports 128K tokens, Claude 3 supports 200K tokens.",
                    message
                )
            }
            Self::HttpError(e) => {
                let hint = if e.is_timeout() {
                    "The request timed out. The server might be overloaded."
                } else if e.is_connect() {
                    "Could not connect to the server. Check your internet connection."
                } else if e.is_request() {
                    "There was a problem with the request."
                } else {
                    "A network error occurred."
                };
                format!(
                    "🌐 Network error: {}\n\n\
                    💡 {}\n\n\
                    Suggestions:\n\
                    1. Check your internet connection\n\
                    2. Try again in a few moments\n\
                    3. Use a local model: ember chat --provider ollama",
                    e, hint
                )
            }
            Self::ApiError {
                provider,
                status,
                message,
            } => {
                let status_hint = match *status {
                    400 => "Bad Request - The request was malformed.",
                    401 => "Unauthorized - Check your API key.",
                    403 => "Forbidden - You don't have access to this resource.",
                    404 => "Not Found - The endpoint or model doesn't exist.",
                    422 => "Unprocessable - The request parameters are invalid.",
                    429 => "Too Many Requests - Rate limit exceeded.",
                    500..=599 => "Server Error - The provider is having issues.",
                    _ => "An error occurred with the API request.",
                };
                format!(
                    "⚠️  API error from {} (HTTP {}):\n{}\n\n\
                    💡 {}\n\n\
                    If this persists, try:\n\
                    1. Using a different provider\n\
                    2. Checking the provider's status page\n\
                    3. Updating your API key",
                    provider, status, message, status_hint
                )
            }
            Self::Timeout { seconds } => {
                format!(
                    "⏱️  Request timed out after {} seconds.\n\n\
                    The server is taking too long to respond.\n\n\
                    Suggestions:\n\
                    1. Try a simpler request\n\
                    2. Use a faster provider like Groq\n\
                    3. Check the provider's status page",
                    seconds
                )
            }
            Self::InvalidRequest(msg) => {
                format!(
                    "❌ Invalid request: {}\n\n\
                    Please check your input and try again.\n\n\
                    💡 Tip: Use --help to see available options.",
                    msg
                )
            }
            Self::ConfigError(msg) => {
                format!(
                    "⚙️  Configuration error: {}\n\n\
                    Run 'ember config show' to see your current configuration.\n\
                    Run 'ember config init' to create a new configuration file.",
                    msg
                )
            }
            Self::StreamError(msg) => {
                format!(
                    "📡 Streaming error: {}\n\n\
                    Try using --no-stream to disable streaming.",
                    msg
                )
            }
            Self::ToolError(msg) => {
                format!(
                    "🔧 Tool error: {}\n\n\
                    The tool execution failed. Check the tool configuration.",
                    msg
                )
            }
            Self::ParseError(e) => {
                format!(
                    "📄 Failed to parse response: {}\n\n\
                    The API returned an unexpected format.\n\
                    This might be a temporary issue. Please try again.",
                    e
                )
            }
        }
    }

    /// Suggest popular models for a provider
    fn suggest_models(provider: &str) -> String {
        match provider.to_lowercase().as_str() {
            "openai" => "  • gpt-4o (recommended)\n  • gpt-4-turbo\n  • gpt-3.5-turbo".to_string(),
            "anthropic" => "  • claude-3-5-sonnet-20241022 (recommended)\n  • claude-3-opus-20240229\n  • claude-3-haiku-20240307".to_string(),
            "ollama" => "  • llama3.2 (recommended)\n  • mistral\n  • codellama".to_string(),
            "groq" => "  • llama-3.3-70b-versatile (recommended)\n  • mixtral-8x7b-32768".to_string(),
            "gemini" => "  • gemini-1.5-pro (recommended)\n  • gemini-1.5-flash".to_string(),
            "deepseek" => "  • deepseek-chat (recommended)\n  • deepseek-coder".to_string(),
            "mistral" => "  • mistral-large-latest (recommended)\n  • mistral-medium".to_string(),
            "openrouter" => "  • anthropic/claude-3.5-sonnet\n  • openai/gpt-4o\n  • google/gemini-pro".to_string(),
            "xai" => "  • grok-beta (recommended)".to_string(),
            _ => "  Check the provider documentation for available models.".to_string(),
        }
    }

    /// Get recovery suggestions for the error
    pub fn recovery_suggestions(&self) -> Vec<String> {
        match self {
            Self::ApiKeyMissing { .. } => vec![
                "Set the API key via environment variable".to_string(),
                "Add the API key to ~/.ember/config.toml".to_string(),
                "Use a different provider that's already configured".to_string(),
            ],
            Self::ProviderUnavailable { provider, .. } => {
                if provider == "ollama" {
                    vec![
                        "Start Ollama: ollama serve".to_string(),
                        "Install Ollama from https://ollama.com".to_string(),
                        "Use a cloud provider instead".to_string(),
                    ]
                } else {
                    vec![
                        "Check your internet connection".to_string(),
                        "Try again in a few moments".to_string(),
                        "Use a different provider".to_string(),
                    ]
                }
            }
            Self::RateLimitExceeded { retry_after, .. } => {
                let mut suggestions = vec![];
                if let Some(seconds) = retry_after {
                    suggestions.push(format!("Wait {} seconds and try again", seconds));
                }
                suggestions.push("Use a different provider".to_string());
                suggestions.push("Reduce request frequency".to_string());
                suggestions.push("Upgrade your API plan".to_string());
                suggestions
            }
            Self::ModelNotFound { .. } => vec![
                "Check the model name spelling".to_string(),
                "List available models with 'ember models'".to_string(),
                "Use a default model for the provider".to_string(),
            ],
            Self::HttpError(_) => vec![
                "Check your internet connection".to_string(),
                "Try again in a few moments".to_string(),
                "Use a local model with Ollama".to_string(),
            ],
            Self::Timeout { .. } => vec![
                "Try a simpler request".to_string(),
                "Use a faster provider".to_string(),
                "Increase the timeout setting".to_string(),
            ],
            _ => vec![
                "Try again".to_string(),
                "Check the error message for details".to_string(),
            ],
        }
    }

    /// Check if this error might be transient
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::HttpError(_)
                | Self::RateLimitExceeded { .. }
                | Self::Timeout { .. }
                | Self::ProviderUnavailable { .. }
                | Self::StreamError(_)
        )
    }

    /// Get the recommended wait time before retry (in seconds)
    pub fn recommended_retry_delay(&self) -> Option<u64> {
        match self {
            Self::RateLimitExceeded { retry_after, .. } => Some(retry_after.unwrap_or(60)),
            Self::Timeout { .. } => Some(5),
            Self::HttpError(_) => Some(2),
            Self::ProviderUnavailable { .. } => Some(10),
            _ => None,
        }
    }

    /// Get the error code for this error
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::ApiKeyMissing { .. } => ErrorCode::E001,
            Self::HttpError(e) => {
                if e.is_timeout() {
                    ErrorCode::E102
                } else if e.is_connect() {
                    ErrorCode::E101
                } else {
                    ErrorCode::E100
                }
            }
            Self::ApiError { status, .. } => match *status {
                401 => ErrorCode::E002,
                429 => ErrorCode::E302,
                _ => ErrorCode::E200,
            },
            Self::ParseError(_) => ErrorCode::E201,
            Self::ModelNotFound { .. } => ErrorCode::E300,
            Self::RateLimitExceeded { .. } => ErrorCode::E302,
            Self::ContextLengthExceeded { .. } => ErrorCode::E303,
            Self::InvalidRequest(_) => ErrorCode::E400,
            Self::ProviderUnavailable { .. } => ErrorCode::E301,
            Self::StreamError(_) => ErrorCode::E203,
            Self::Timeout { .. } => ErrorCode::E102,
            Self::ConfigError(_) => ErrorCode::E500,
            Self::ToolError(_) => ErrorCode::E304,
        }
    }

    /// Get a short error title for display
    pub fn title(&self) -> &'static str {
        match self {
            Self::ApiKeyMissing { .. } => "API Key Missing",
            Self::HttpError(_) => "Network Error",
            Self::ApiError { .. } => "API Error",
            Self::ParseError(_) => "Parse Error",
            Self::ModelNotFound { .. } => "Model Not Found",
            Self::RateLimitExceeded { .. } => "Rate Limit Exceeded",
            Self::ContextLengthExceeded { .. } => "Context Length Exceeded",
            Self::InvalidRequest(_) => "Invalid Request",
            Self::ProviderUnavailable { .. } => "Provider Unavailable",
            Self::StreamError(_) => "Streaming Error",
            Self::Timeout { .. } => "Request Timeout",
            Self::ConfigError(_) => "Configuration Error",
            Self::ToolError(_) => "Tool Error",
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
