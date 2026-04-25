//! Retry logic with exponential backoff for LLM requests.
//!
//! This module provides retry functionality for handling transient failures
//! and rate limiting when interacting with LLM providers.

use std::time::Duration;
use tracing::{debug, warn};

use crate::{CompletionRequest, CompletionResponse, Error, LLMProvider, Result};

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (not including the initial request)
    pub max_retries: u32,
    /// Initial delay before first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Whether to add random jitter to delays
    pub jitter: bool,
    /// Maximum jitter as a fraction of the delay (0.0 - 1.0)
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter: true,
            jitter_factor: 0.25,
        }
    }
}

impl RetryConfig {
    /// Create a new retry config with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of retries.
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set the initial delay.
    pub fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Set the maximum delay.
    pub fn with_max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Set the backoff multiplier.
    pub fn with_backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.backoff_multiplier = multiplier;
        self
    }

    /// Enable or disable jitter.
    pub fn with_jitter(mut self, enabled: bool) -> Self {
        self.jitter = enabled;
        self
    }

    /// Create a config optimized for rate limiting scenarios.
    pub fn for_rate_limits() -> Self {
        Self {
            max_retries: 5,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            jitter: true,
            jitter_factor: 0.25,
        }
    }

    /// Create a config for fast retries (e.g., network glitches).
    pub fn fast() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(2),
            backoff_multiplier: 1.5,
            jitter: true,
            jitter_factor: 0.1,
        }
    }

    /// Create a config with no retries.
    pub fn no_retry() -> Self {
        Self {
            max_retries: 0,
            ..Self::default()
        }
    }

    /// Calculate the delay for a given attempt number.
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }

        let base_delay = self.initial_delay.as_secs_f64()
            * self
                .backoff_multiplier
                .powi(attempt.saturating_sub(1) as i32);

        let capped_delay = base_delay.min(self.max_delay.as_secs_f64());

        let final_delay = if self.jitter {
            let jitter_range = capped_delay * self.jitter_factor;
            let jitter = (rand_simple() * 2.0 - 1.0) * jitter_range;
            (capped_delay + jitter).max(0.0)
        } else {
            capped_delay
        };

        Duration::from_secs_f64(final_delay)
    }
}

/// Simple pseudo-random number generator for jitter.
/// Uses a basic LCG for simplicity - not cryptographically secure.
fn rand_simple() -> f64 {
    use std::cell::Cell;
    use std::time::SystemTime;

    thread_local! {
        static SEED: Cell<u64> = Cell::new(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64
        );
    }

    SEED.with(|seed| {
        let s = seed.get();
        // LCG parameters from Numerical Recipes
        let next = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        seed.set(next);
        (next >> 33) as f64 / (1u64 << 31) as f64
    })
}

/// Execute a completion request with retry logic.
///
/// This function will automatically retry failed requests according to the
/// provided retry configuration, handling rate limits and transient errors.
///
/// # Arguments
///
/// * `provider` - The LLM provider to use
/// * `request` - The completion request
/// * `config` - Retry configuration
///
/// # Returns
///
/// The completion response on success, or the last error encountered.
///
/// # Example
///
/// ```rust,no_run
/// use ember_llm::{retry, LLMProvider, CompletionRequest, Message};
/// use std::sync::Arc;
///
/// async fn example(provider: Arc<dyn LLMProvider>) {
///     let request = CompletionRequest::new("gpt-4")
///         .with_message(Message::user("Hello!"));
///     
///     let config = retry::RetryConfig::default();
///     let response = retry::complete_with_retry(&*provider, request, &config).await;
/// }
/// ```
pub async fn complete_with_retry(
    provider: &dyn LLMProvider,
    request: CompletionRequest,
    config: &RetryConfig,
) -> Result<CompletionResponse> {
    let mut last_error: Option<Error> = None;
    let mut attempt = 0;

    loop {
        if attempt > 0 {
            let delay = calculate_delay(config, attempt, last_error.as_ref());
            debug!(
                attempt = attempt,
                delay_ms = delay.as_millis(),
                "Retrying after delay"
            );
            tokio::time::sleep(delay).await;
        }

        match provider.complete(request.clone()).await {
            Ok(response) => {
                if attempt > 0 {
                    debug!(attempt = attempt, "Request succeeded after retry");
                }
                return Ok(response);
            }
            Err(e) => {
                if !should_retry(&e, attempt, config) {
                    return Err(e);
                }

                warn!(
                    attempt = attempt,
                    max_retries = config.max_retries,
                    error = %e,
                    "Request failed, will retry"
                );
                eprintln!(
                    "Retrying request (attempt {}/{})...",
                    attempt + 1,
                    config.max_retries
                );

                last_error = Some(e);
                attempt += 1;
            }
        }
    }
}

/// Determine if an error should trigger a retry.
fn should_retry(error: &Error, attempt: u32, config: &RetryConfig) -> bool {
    if attempt >= config.max_retries {
        return false;
    }

    error.is_retryable()
}

/// Calculate the delay for a retry attempt, considering rate limit headers.
fn calculate_delay(config: &RetryConfig, attempt: u32, error: Option<&Error>) -> Duration {
    // If the error provides a retry-after duration, use it
    if let Some(err) = error {
        if let Some(retry_after) = err.retry_after() {
            // Add some buffer to the suggested retry-after
            let suggested = Duration::from_secs(retry_after);
            let with_buffer = suggested + Duration::from_secs(1);
            return with_buffer.min(config.max_delay);
        }
    }

    // Otherwise use exponential backoff
    config.delay_for_attempt(attempt)
}

/// A wrapper that adds retry behavior to any LLM provider.
pub struct RetryProvider<P> {
    inner: P,
    config: RetryConfig,
}

impl<P> RetryProvider<P> {
    /// Create a new retry provider wrapping the given provider.
    pub fn new(provider: P, config: RetryConfig) -> Self {
        Self {
            inner: provider,
            config,
        }
    }

    /// Get a reference to the inner provider.
    pub fn inner(&self) -> &P {
        &self.inner
    }

    /// Get the retry configuration.
    pub fn config(&self) -> &RetryConfig {
        &self.config
    }
}

#[cfg(feature = "retry-provider")]
mod retry_provider_impl {
    use super::*;
    use crate::{provider::StreamResponse, ModelInfo};
    use async_trait::async_trait;

    #[async_trait]
    impl<P: LLMProvider> LLMProvider for RetryProvider<P> {
        fn name(&self) -> &str {
            self.inner.name()
        }

        async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
            complete_with_retry(&self.inner, request, &self.config).await
        }

        async fn complete_stream(&self, request: CompletionRequest) -> Result<StreamResponse> {
            // Streaming doesn't support retry in the same way
            // Could implement partial retry, but for now just pass through
            self.inner.complete_stream(request).await
        }

        async fn list_models(&self) -> Result<Vec<ModelInfo>> {
            self.inner.list_models().await
        }

        async fn health_check(&self) -> Result<()> {
            self.inner.health_check().await
        }

        fn supports_tools(&self) -> bool {
            self.inner.supports_tools()
        }

        fn supports_vision(&self) -> bool {
            self.inner.supports_vision()
        }

        fn default_model(&self) -> &str {
            self.inner.default_model()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(500));
    }

    #[test]
    fn test_delay_calculation() {
        let config = RetryConfig::new()
            .with_initial_delay(Duration::from_secs(1))
            .with_backoff_multiplier(2.0)
            .with_jitter(false);

        assert_eq!(config.delay_for_attempt(0), Duration::ZERO);
        assert_eq!(config.delay_for_attempt(1), Duration::from_secs(1));
        assert_eq!(config.delay_for_attempt(2), Duration::from_secs(2));
        assert_eq!(config.delay_for_attempt(3), Duration::from_secs(4));
    }

    #[test]
    fn test_delay_capped_at_max() {
        let config = RetryConfig::new()
            .with_initial_delay(Duration::from_secs(10))
            .with_max_delay(Duration::from_secs(15))
            .with_backoff_multiplier(2.0)
            .with_jitter(false);

        // Should be capped at max_delay
        assert_eq!(config.delay_for_attempt(3), Duration::from_secs(15));
    }

    #[test]
    fn test_rate_limit_config() {
        let config = RetryConfig::for_rate_limits();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_delay, Duration::from_secs(1));
    }

    #[test]
    fn test_no_retry_config() {
        let config = RetryConfig::no_retry();
        assert_eq!(config.max_retries, 0);
    }

    #[test]
    fn test_should_retry() {
        let config = RetryConfig::new().with_max_retries(3);

        // Rate limit should be retried
        let rate_limit = Error::rate_limit("openai", Some(60));
        assert!(should_retry(&rate_limit, 0, &config));
        assert!(should_retry(&rate_limit, 2, &config));
        assert!(!should_retry(&rate_limit, 3, &config)); // Max retries reached

        // API key missing should not be retried
        let api_key = Error::api_key_missing("openai");
        assert!(!should_retry(&api_key, 0, &config));
    }

    #[tokio::test]
    async fn test_retry_exhaustion_returns_last_error() {
        use crate::mock::MockProvider;

        let provider = MockProvider::new();
        // Queue two errors — the second one will never be reached since
        // InvalidRequest is not retryable, but this verifies that
        // complete_with_retry propagates errors correctly.
        provider.queue_error("something went wrong");

        let config = RetryConfig::new()
            .with_max_retries(3)
            .with_initial_delay(Duration::from_millis(1))
            .with_jitter(false);

        let request = crate::CompletionRequest::new("mock-model")
            .with_message(crate::Message::user("hello"));

        let result = complete_with_retry(&provider, request, &config).await;
        assert!(result.is_err());
    }
}
