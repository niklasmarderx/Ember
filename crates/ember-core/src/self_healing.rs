//! Self-Healing and Auto-Recovery System
//!
//! This module implements intelligent error recovery that OpenClaw doesn't have!
//!
//! # Features
//! - **Automatic Error Detection**: Detects errors in tool execution, LLM responses, etc.
//! - **Smart Recovery Strategies**: Multiple recovery strategies based on error type
//! - **Learning from Failures**: Remembers what worked and what didn't
//! - **Graceful Degradation**: Falls back to simpler approaches when complex ones fail
//! - **Circuit Breaker Pattern**: Prevents cascading failures

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Error category for recovery strategy selection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorCategory {
    /// Network-related errors (timeouts, connection refused, etc.)
    Network,
    /// API errors (rate limits, auth failures, etc.)
    Api,
    /// Tool execution errors (command failed, file not found, etc.)
    ToolExecution,
    /// LLM response errors (invalid JSON, hallucination detected, etc.)
    LlmResponse,
    /// Resource errors (out of memory, disk full, etc.)
    Resource,
    /// Configuration errors (missing config, invalid settings, etc.)
    Configuration,
    /// Unknown/unexpected errors
    Unknown,
}

impl ErrorCategory {
    /// Categorize an error message.
    pub fn from_error_message(message: &str) -> Self {
        let lower = message.to_lowercase();

        if lower.contains("timeout") || lower.contains("connection") || lower.contains("network") {
            Self::Network
        } else if lower.contains("rate limit")
            || lower.contains("401")
            || lower.contains("403")
            || lower.contains("api")
        {
            Self::Api
        } else if lower.contains("command")
            || lower.contains("exit code")
            || lower.contains("not found")
        {
            Self::ToolExecution
        } else if lower.contains("json")
            || lower.contains("parse")
            || lower.contains("invalid response")
        {
            Self::LlmResponse
        } else if lower.contains("memory") || lower.contains("disk") || lower.contains("resource") {
            Self::Resource
        } else if lower.contains("config") || lower.contains("missing") || lower.contains("invalid")
        {
            Self::Configuration
        } else {
            Self::Unknown
        }
    }
}

/// A recovery strategy to try when an error occurs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStrategy {
    /// Strategy name for logging.
    pub name: String,
    /// Description of what this strategy does.
    pub description: String,
    /// Priority (higher = try first).
    pub priority: u8,
    /// Maximum attempts for this strategy.
    pub max_attempts: u32,
    /// Delay between attempts.
    pub retry_delay: Duration,
    /// Whether to use exponential backoff.
    pub exponential_backoff: bool,
    /// Success rate from past attempts (0.0 - 1.0).
    pub success_rate: f32,
    /// Number of times this strategy was tried.
    pub total_attempts: u32,
    /// Number of successful recoveries.
    pub successful_recoveries: u32,
}

impl RecoveryStrategy {
    /// Create a simple retry strategy.
    pub fn retry(max_attempts: u32, delay: Duration) -> Self {
        Self {
            name: "Retry".to_string(),
            description: "Simply retry the operation".to_string(),
            priority: 10,
            max_attempts,
            retry_delay: delay,
            exponential_backoff: true,
            success_rate: 0.5,
            total_attempts: 0,
            successful_recoveries: 0,
        }
    }

    /// Create a fallback model strategy.
    pub fn fallback_model() -> Self {
        Self {
            name: "FallbackModel".to_string(),
            description: "Try a different/simpler LLM model".to_string(),
            priority: 8,
            max_attempts: 3,
            retry_delay: Duration::from_secs(1),
            exponential_backoff: false,
            success_rate: 0.7,
            total_attempts: 0,
            successful_recoveries: 0,
        }
    }

    /// Create a simplify request strategy.
    pub fn simplify_request() -> Self {
        Self {
            name: "SimplifyRequest".to_string(),
            description: "Reduce complexity of the request".to_string(),
            priority: 7,
            max_attempts: 2,
            retry_delay: Duration::from_millis(500),
            exponential_backoff: false,
            success_rate: 0.6,
            total_attempts: 0,
            successful_recoveries: 0,
        }
    }

    /// Create a wait and retry strategy (for rate limits).
    pub fn wait_and_retry(wait_time: Duration) -> Self {
        Self {
            name: "WaitAndRetry".to_string(),
            description: "Wait for a period then retry".to_string(),
            priority: 9,
            max_attempts: 3,
            retry_delay: wait_time,
            exponential_backoff: false,
            success_rate: 0.8,
            total_attempts: 0,
            successful_recoveries: 0,
        }
    }

    /// Create a use cache strategy.
    pub fn use_cached() -> Self {
        Self {
            name: "UseCached".to_string(),
            description: "Use a cached response if available".to_string(),
            priority: 6,
            max_attempts: 1,
            retry_delay: Duration::ZERO,
            exponential_backoff: false,
            success_rate: 0.9,
            total_attempts: 0,
            successful_recoveries: 0,
        }
    }

    /// Create a human intervention strategy.
    pub fn ask_human() -> Self {
        Self {
            name: "AskHuman".to_string(),
            description: "Ask the user for help".to_string(),
            priority: 1,
            max_attempts: 1,
            retry_delay: Duration::ZERO,
            exponential_backoff: false,
            success_rate: 0.95,
            total_attempts: 0,
            successful_recoveries: 0,
        }
    }

    /// Update success rate after an attempt.
    pub fn record_attempt(&mut self, success: bool) {
        self.total_attempts += 1;
        if success {
            self.successful_recoveries += 1;
        }
        // Calculate new success rate with smoothing
        if self.total_attempts > 0 {
            self.success_rate = self.successful_recoveries as f32 / self.total_attempts as f32;
        }
    }
}

/// Circuit breaker state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed, operations proceed normally.
    Closed,
    /// Circuit is open, operations fail fast.
    Open,
    /// Circuit is half-open, testing if recovery is possible.
    HalfOpen,
}

/// Circuit breaker for preventing cascading failures.
#[derive(Debug)]
pub struct CircuitBreaker {
    /// Current state.
    state: CircuitState,
    /// Number of consecutive failures.
    failure_count: u32,
    /// Threshold to open the circuit.
    failure_threshold: u32,
    /// Time when the circuit was opened.
    opened_at: Option<Instant>,
    /// Duration to wait before trying again.
    reset_timeout: Duration,
    /// Last success time.
    last_success: Option<Instant>,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(5, Duration::from_secs(30))
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker.
    pub fn new(failure_threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            failure_threshold,
            opened_at: None,
            reset_timeout,
            last_success: None,
        }
    }

    /// Check if the circuit allows the operation.
    pub fn allow_request(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if we should transition to half-open
                if let Some(opened_at) = self.opened_at {
                    if opened_at.elapsed() >= self.reset_timeout {
                        self.state = CircuitState::HalfOpen;
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record a successful operation.
    pub fn record_success(&mut self) {
        self.failure_count = 0;
        self.state = CircuitState::Closed;
        self.opened_at = None;
        self.last_success = Some(Instant::now());
    }

    /// Record a failed operation.
    pub fn record_failure(&mut self) {
        self.failure_count += 1;

        if self.failure_count >= self.failure_threshold {
            self.state = CircuitState::Open;
            self.opened_at = Some(Instant::now());
        }
    }

    /// Get the current state.
    pub fn state(&self) -> &CircuitState {
        &self.state
    }

    /// Reset the circuit breaker.
    pub fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.opened_at = None;
    }
}

/// Record of a recovery attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryRecord {
    /// Error category.
    pub error_category: ErrorCategory,
    /// Error message.
    pub error_message: String,
    /// Strategy used.
    pub strategy_name: String,
    /// Whether recovery succeeded.
    pub success: bool,
    /// Time taken for recovery.
    pub recovery_time_ms: u64,
    /// Timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// The Self-Healing System.
///
/// Manages error recovery across the entire agent framework.
pub struct SelfHealingSystem {
    /// Recovery strategies by error category.
    strategies: Arc<RwLock<HashMap<ErrorCategory, Vec<RecoveryStrategy>>>>,
    /// Circuit breakers by operation name.
    circuit_breakers: Arc<RwLock<HashMap<String, CircuitBreaker>>>,
    /// Recovery history for learning.
    history: Arc<RwLock<Vec<RecoveryRecord>>>,
    /// Maximum history size.
    max_history: usize,
    /// Whether learning is enabled.
    learning_enabled: bool,
}

impl Default for SelfHealingSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl SelfHealingSystem {
    /// Create a new self-healing system with default strategies.
    pub fn new() -> Self {
        let mut strategies = HashMap::new();

        // Network errors
        strategies.insert(
            ErrorCategory::Network,
            vec![
                RecoveryStrategy::retry(3, Duration::from_secs(2)),
                RecoveryStrategy::wait_and_retry(Duration::from_secs(10)),
                RecoveryStrategy::use_cached(),
            ],
        );

        // API errors
        strategies.insert(
            ErrorCategory::Api,
            vec![
                RecoveryStrategy::wait_and_retry(Duration::from_secs(60)),
                RecoveryStrategy::fallback_model(),
                RecoveryStrategy::ask_human(),
            ],
        );

        // Tool execution errors
        strategies.insert(
            ErrorCategory::ToolExecution,
            vec![
                RecoveryStrategy::retry(2, Duration::from_secs(1)),
                RecoveryStrategy::simplify_request(),
                RecoveryStrategy::ask_human(),
            ],
        );

        // LLM response errors
        strategies.insert(
            ErrorCategory::LlmResponse,
            vec![
                RecoveryStrategy::retry(2, Duration::from_millis(500)),
                RecoveryStrategy::simplify_request(),
                RecoveryStrategy::fallback_model(),
            ],
        );

        // Resource errors
        strategies.insert(
            ErrorCategory::Resource,
            vec![
                RecoveryStrategy::wait_and_retry(Duration::from_secs(30)),
                RecoveryStrategy::simplify_request(),
                RecoveryStrategy::ask_human(),
            ],
        );

        // Configuration errors
        strategies.insert(
            ErrorCategory::Configuration,
            vec![RecoveryStrategy::ask_human()],
        );

        // Unknown errors
        strategies.insert(
            ErrorCategory::Unknown,
            vec![
                RecoveryStrategy::retry(1, Duration::from_secs(1)),
                RecoveryStrategy::ask_human(),
            ],
        );

        Self {
            strategies: Arc::new(RwLock::new(strategies)),
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            max_history: 1000,
            learning_enabled: true,
        }
    }

    /// Get recovery strategies for an error.
    pub async fn get_strategies(&self, category: &ErrorCategory) -> Vec<RecoveryStrategy> {
        let strategies = self.strategies.read().await;
        let mut result = strategies.get(category).cloned().unwrap_or_default();

        // Sort by success rate and priority
        result.sort_by(|a, b| {
            // First by success rate (descending), then by priority (descending)
            let score_a = a.success_rate * 10.0 + a.priority as f32;
            let score_b = b.success_rate * 10.0 + b.priority as f32;
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        result
    }

    /// Check if an operation is allowed (circuit breaker).
    pub async fn allow_operation(&self, operation: &str) -> bool {
        let mut breakers = self.circuit_breakers.write().await;
        let breaker = breakers
            .entry(operation.to_string())
            .or_insert_with(CircuitBreaker::default);
        breaker.allow_request()
    }

    /// Record operation success.
    pub async fn record_success(&self, operation: &str) {
        let mut breakers = self.circuit_breakers.write().await;
        if let Some(breaker) = breakers.get_mut(operation) {
            breaker.record_success();
        }
    }

    /// Record operation failure.
    pub async fn record_failure(&self, operation: &str) {
        let mut breakers = self.circuit_breakers.write().await;
        let breaker = breakers
            .entry(operation.to_string())
            .or_insert_with(CircuitBreaker::default);
        breaker.record_failure();
    }

    /// Record a recovery attempt for learning.
    pub async fn record_recovery(
        &self,
        category: ErrorCategory,
        error_message: &str,
        strategy_name: &str,
        success: bool,
        recovery_time_ms: u64,
    ) {
        // Record in history
        let record = RecoveryRecord {
            error_category: category.clone(),
            error_message: error_message.to_string(),
            strategy_name: strategy_name.to_string(),
            success,
            recovery_time_ms,
            timestamp: chrono::Utc::now(),
        };

        let mut history = self.history.write().await;
        history.push(record);

        // Trim history if too large
        let current_len = history.len();
        if current_len > self.max_history {
            history.drain(0..current_len - self.max_history);
        }

        // Update strategy success rates if learning is enabled
        if self.learning_enabled {
            let mut strategies = self.strategies.write().await;
            if let Some(category_strategies) = strategies.get_mut(&category) {
                for strategy in category_strategies.iter_mut() {
                    if strategy.name == strategy_name {
                        strategy.record_attempt(success);
                        break;
                    }
                }
            }
        }
    }

    /// Get recovery statistics.
    pub async fn get_stats(&self) -> RecoveryStats {
        let history = self.history.read().await;

        let total_attempts = history.len();
        let successful = history.iter().filter(|r| r.success).count();

        let mut by_category: HashMap<ErrorCategory, (usize, usize)> = HashMap::new();
        for record in history.iter() {
            let entry = by_category
                .entry(record.error_category.clone())
                .or_default();
            entry.0 += 1;
            if record.success {
                entry.1 += 1;
            }
        }

        let avg_recovery_time = if total_attempts > 0 {
            history.iter().map(|r| r.recovery_time_ms).sum::<u64>() / total_attempts as u64
        } else {
            0
        };

        RecoveryStats {
            total_attempts,
            successful_recoveries: successful,
            success_rate: if total_attempts > 0 {
                successful as f32 / total_attempts as f32
            } else {
                0.0
            },
            average_recovery_time_ms: avg_recovery_time,
            by_category,
        }
    }

    /// Get circuit breaker status.
    pub async fn get_circuit_status(&self, operation: &str) -> Option<CircuitState> {
        let breakers = self.circuit_breakers.read().await;
        breakers.get(operation).map(|b| b.state().clone())
    }

    /// Reset a circuit breaker.
    pub async fn reset_circuit(&self, operation: &str) {
        let mut breakers = self.circuit_breakers.write().await;
        if let Some(breaker) = breakers.get_mut(operation) {
            breaker.reset();
        }
    }

    /// Add a custom recovery strategy.
    pub async fn add_strategy(&self, category: ErrorCategory, strategy: RecoveryStrategy) {
        let mut strategies = self.strategies.write().await;
        strategies.entry(category).or_default().push(strategy);
    }
}

/// Recovery statistics.
#[derive(Debug, Clone, Serialize)]
pub struct RecoveryStats {
    /// Total recovery attempts.
    pub total_attempts: usize,
    /// Successful recoveries.
    pub successful_recoveries: usize,
    /// Overall success rate.
    pub success_rate: f32,
    /// Average recovery time in milliseconds.
    pub average_recovery_time_ms: u64,
    /// Stats by error category (total, successful).
    pub by_category: HashMap<ErrorCategory, (usize, usize)>,
}

/// Helper function to attempt recovery with the self-healing system.
#[allow(dead_code)]
pub async fn attempt_recovery<F, Fut, T, E>(
    system: &SelfHealingSystem,
    operation: &str,
    _error: &E,
    retry_fn: F,
) -> Result<T, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let error_message = _error.to_string();
    let category = ErrorCategory::from_error_message(&error_message);

    // Check circuit breaker
    if !system.allow_operation(operation).await {
        tracing::warn!(operation, "Circuit breaker open, returning original error");
        return Err(format!("Circuit breaker open for operation: {}", operation));
    }

    let strategies = system.get_strategies(&category).await;

    for strategy in strategies {
        let start = Instant::now();

        for attempt in 0..strategy.max_attempts {
            // Calculate delay with optional exponential backoff
            let delay = if strategy.exponential_backoff {
                strategy.retry_delay * (2_u32.pow(attempt))
            } else {
                strategy.retry_delay
            };

            if attempt > 0 {
                tokio::time::sleep(delay).await;
            }

            match retry_fn().await {
                Ok(result) => {
                    system.record_success(operation).await;
                    system
                        .record_recovery(
                            category,
                            &error_message,
                            &strategy.name,
                            true,
                            start.elapsed().as_millis() as u64,
                        )
                        .await;
                    return Ok(result);
                }
                Err(_) => continue,
            }
        }
    }

    // All strategies failed
    system.record_failure(operation).await;
    Err(format!(
        "All recovery strategies failed for operation: {}",
        operation
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_categorization() {
        assert_eq!(
            ErrorCategory::from_error_message("Connection timeout"),
            ErrorCategory::Network
        );
        assert_eq!(
            ErrorCategory::from_error_message("Rate limit exceeded"),
            ErrorCategory::Api
        );
        assert_eq!(
            ErrorCategory::from_error_message("Command failed with exit code 1"),
            ErrorCategory::ToolExecution
        );
        assert_eq!(
            ErrorCategory::from_error_message("Invalid JSON response"),
            ErrorCategory::LlmResponse
        );
    }

    #[test]
    fn test_circuit_breaker() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(30));

        assert!(cb.allow_request());
        assert_eq!(cb.state(), &CircuitState::Closed);

        // Record failures
        cb.record_failure();
        cb.record_failure();
        assert!(cb.allow_request()); // Still closed

        cb.record_failure(); // This should open the circuit
        assert_eq!(cb.state(), &CircuitState::Open);
        assert!(!cb.allow_request());

        // Record success
        cb.record_success();
        assert_eq!(cb.state(), &CircuitState::Closed);
        assert!(cb.allow_request());
    }

    #[test]
    fn test_strategy_success_rate() {
        let mut strategy = RecoveryStrategy::retry(3, Duration::from_secs(1));

        strategy.record_attempt(true);
        strategy.record_attempt(true);
        strategy.record_attempt(false);

        assert_eq!(strategy.total_attempts, 3);
        assert_eq!(strategy.successful_recoveries, 2);
        assert!((strategy.success_rate - 0.666).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_self_healing_system() {
        let system = SelfHealingSystem::new();

        // Get strategies for network error
        let strategies = system.get_strategies(&ErrorCategory::Network).await;
        assert!(!strategies.is_empty());

        // Test circuit breaker
        assert!(system.allow_operation("test_op").await);
        system.record_failure("test_op").await;
        system.record_failure("test_op").await;
        system.record_failure("test_op").await;
        system.record_failure("test_op").await;
        system.record_failure("test_op").await;

        // Circuit should be open now
        let status = system.get_circuit_status("test_op").await;
        assert_eq!(status, Some(CircuitState::Open));

        // Reset and verify
        system.reset_circuit("test_op").await;
        let status = system.get_circuit_status("test_op").await;
        assert_eq!(status, Some(CircuitState::Closed));
    }

    #[tokio::test]
    async fn test_recovery_recording() {
        let system = SelfHealingSystem::new();

        system
            .record_recovery(
                ErrorCategory::Network,
                "Connection timeout",
                "Retry",
                true,
                500,
            )
            .await;

        let stats = system.get_stats().await;
        assert_eq!(stats.total_attempts, 1);
        assert_eq!(stats.successful_recoveries, 1);
    }
}
