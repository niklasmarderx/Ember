//! Error Handling Patterns in Ember
//!
//! This example demonstrates best practices for error handling when using
//! the Ember AI agent framework, including:
//! - Handling provider errors
//! - Dealing with rate limits
//! - Graceful degradation with fallbacks
//! - Retry patterns
//!
//! Run with: `cargo run --example error_handling`

use ember_core::{Agent, AgentBuilder, Error as CoreError};
use ember_llm::{
    Error as LlmError, Message, OllamaProvider, OpenAIProvider, Provider, ProviderError,
    ProviderRouter,
};

/// Custom application error type that wraps Ember errors
#[derive(Debug)]
enum AppError {
    /// Provider is unavailable
    ProviderUnavailable(String),
    /// Rate limit exceeded
    RateLimited { retry_after: Option<u64> },
    /// Invalid API key
    AuthenticationFailed,
    /// Network or connection error
    NetworkError(String),
    /// Unexpected error
    Other(String),
}

impl From<LlmError> for AppError {
    fn from(err: LlmError) -> Self {
        match err {
            LlmError::ProviderError(ProviderError::RateLimited { retry_after }) => {
                AppError::RateLimited { retry_after }
            }
            LlmError::ProviderError(ProviderError::AuthenticationFailed) => {
                AppError::AuthenticationFailed
            }
            LlmError::ProviderError(ProviderError::NetworkError(msg)) => {
                AppError::NetworkError(msg)
            }
            LlmError::ProviderError(ProviderError::Unavailable(name)) => {
                AppError::ProviderUnavailable(name)
            }
            other => AppError::Other(other.to_string()),
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::ProviderUnavailable(name) => {
                write!(f, "Provider '{}' is currently unavailable", name)
            }
            AppError::RateLimited { retry_after } => {
                if let Some(secs) = retry_after {
                    write!(f, "Rate limited. Please retry after {} seconds", secs)
                } else {
                    write!(f, "Rate limited. Please try again later")
                }
            }
            AppError::AuthenticationFailed => {
                write!(f, "Authentication failed. Please check your API key")
            }
            AppError::NetworkError(msg) => {
                write!(f, "Network error: {}", msg)
            }
            AppError::Other(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    println!("=== Ember Error Handling Examples ===\n");

    // Example 1: Basic error handling
    println!("1. Basic Error Handling");
    basic_error_handling().await;

    // Example 2: Retry with exponential backoff
    println!("\n2. Retry with Exponential Backoff");
    retry_with_backoff().await;

    // Example 3: Fallback providers
    println!("\n3. Fallback Providers");
    fallback_providers().await;

    // Example 4: Graceful degradation
    println!("\n4. Graceful Degradation");
    graceful_degradation().await;

    Ok(())
}

/// Example 1: Basic error handling with pattern matching
async fn basic_error_handling() {
    // Attempt to create a provider with potentially invalid config
    let result = create_provider_with_validation("invalid-key");

    match result {
        Ok(provider) => {
            println!("   Provider created successfully");
            // Use the provider...
        }
        Err(AppError::AuthenticationFailed) => {
            println!("   [!] Authentication failed - check your API key");
            println!("       Hint: Set OPENAI_API_KEY environment variable");
        }
        Err(AppError::ProviderUnavailable(name)) => {
            println!("   [!] Provider '{}' is unavailable", name);
            println!("       Consider using a fallback provider");
        }
        Err(e) => {
            println!("   [!] Unexpected error: {}", e);
        }
    }
}

/// Example 2: Implementing retry logic with exponential backoff
async fn retry_with_backoff() {
    let max_retries = 3;
    let base_delay_ms = 100;

    for attempt in 1..=max_retries {
        println!("   Attempt {} of {}", attempt, max_retries);

        match simulate_api_call().await {
            Ok(response) => {
                println!("   Success: {}", response);
                return;
            }
            Err(AppError::RateLimited { retry_after }) => {
                let delay = retry_after.unwrap_or_else(|| {
                    // Exponential backoff: 100ms, 200ms, 400ms, ...
                    (base_delay_ms * 2_u64.pow(attempt as u32 - 1)) / 1000
                });

                if attempt < max_retries {
                    println!("   Rate limited, waiting {} seconds...", delay);
                    tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;
                } else {
                    println!("   Max retries exceeded");
                }
            }
            Err(AppError::NetworkError(msg)) => {
                if attempt < max_retries {
                    let delay = base_delay_ms * 2_u64.pow(attempt as u32 - 1);
                    println!(
                        "   Network error: {}. Retrying in {}ms...",
                        msg, delay
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                } else {
                    println!("   Network error persists after {} attempts", max_retries);
                }
            }
            Err(e) => {
                // Non-retryable errors
                println!("   Non-retryable error: {}", e);
                return;
            }
        }
    }
}

/// Example 3: Using fallback providers
async fn fallback_providers() {
    // Define a chain of providers to try
    let providers = vec!["openai", "anthropic", "ollama"];

    for provider_name in providers {
        println!("   Trying provider: {}", provider_name);

        match try_provider(provider_name).await {
            Ok(response) => {
                println!("   Got response from {}: {}", provider_name, response);
                return;
            }
            Err(AppError::ProviderUnavailable(_)) => {
                println!("   {} unavailable, trying next...", provider_name);
                continue;
            }
            Err(AppError::AuthenticationFailed) => {
                println!("   {} auth failed, trying next...", provider_name);
                continue;
            }
            Err(e) => {
                println!("   Error with {}: {}", provider_name, e);
                continue;
            }
        }
    }

    println!("   All providers failed!");
}

/// Example 4: Graceful degradation
async fn graceful_degradation() {
    // Try full AI response first
    match get_ai_response("What is 2+2?").await {
        Ok(response) => {
            println!("   AI Response: {}", response);
        }
        Err(_) => {
            // Fallback to cached/static response
            println!("   AI unavailable, using cached response");
            let cached = get_cached_response("What is 2+2?");
            println!("   Cached Response: {}", cached);
        }
    }

    // Show feature availability
    println!("\n   Feature Availability:");
    println!(
        "   - AI Chat: {}",
        if check_ai_available().await {
            "Available"
        } else {
            "Degraded (using cache)"
        }
    );
    println!(
        "   - Code Execution: {}",
        if check_sandbox_available().await {
            "Available"
        } else {
            "Disabled"
        }
    );
}

// Helper functions for the examples

fn create_provider_with_validation(api_key: &str) -> Result<OpenAIProvider, AppError> {
    // Validate API key format
    if api_key.is_empty() || api_key == "invalid-key" {
        return Err(AppError::AuthenticationFailed);
    }

    // In a real application, this would create the provider
    Ok(OpenAIProvider::new(api_key))
}

async fn simulate_api_call() -> Result<String, AppError> {
    // Simulate a successful call for demonstration
    // In real code, this would make actual API calls
    static CALL_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let count = CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    // Succeed on third attempt for demonstration
    if count < 2 {
        Err(AppError::RateLimited {
            retry_after: Some(1),
        })
    } else {
        Ok("Response received successfully!".to_string())
    }
}

async fn try_provider(name: &str) -> Result<String, AppError> {
    // Simulate provider availability
    match name {
        "openai" => Err(AppError::AuthenticationFailed),
        "anthropic" => Err(AppError::ProviderUnavailable("anthropic".to_string())),
        "ollama" => Ok("Ollama response: Hello!".to_string()),
        _ => Err(AppError::ProviderUnavailable(name.to_string())),
    }
}

async fn get_ai_response(prompt: &str) -> Result<String, AppError> {
    // Simulate AI response
    // In real code, this would call the provider
    Ok(format!("AI says: The answer to '{}' is 4", prompt))
}

fn get_cached_response(prompt: &str) -> String {
    // Return cached/static responses for common queries
    match prompt {
        "What is 2+2?" => "4 (cached)".to_string(),
        _ => "I don't have a cached response for that.".to_string(),
    }
}

async fn check_ai_available() -> bool {
    // Check if AI service is available
    true
}

async fn check_sandbox_available() -> bool {
    // Check if code execution sandbox is available
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_error_display() {
        let err = AppError::RateLimited {
            retry_after: Some(30),
        };
        assert!(err.to_string().contains("30 seconds"));

        let err = AppError::AuthenticationFailed;
        assert!(err.to_string().contains("API key"));
    }

    #[test]
    fn test_error_conversion() {
        // Test that LlmError converts to AppError correctly
        // This would require actual error types from ember-llm
    }

    #[tokio::test]
    async fn test_fallback_reaches_ollama() {
        // The fallback chain should eventually reach ollama
        let result = try_provider("ollama").await;
        assert!(result.is_ok());
    }
}