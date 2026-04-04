//! Advanced Tool Execution System
//!
//! Provides robust tool execution with:
//! - Async tool execution
//! - Parallel execution support
//! - Timeout handling
//! - Retry logic
//! - Sandboxing options
//! - Execution metrics

use crate::{Error, Result};
use async_trait::async_trait;
use ember_llm::{ToolCall, ToolResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use tracing::{error, info, warn};

// =============================================================================
// Async Tool Trait
// =============================================================================

/// Async tool executor trait
#[async_trait]
pub trait AsyncTool: Send + Sync {
    /// Get the tool name
    fn name(&self) -> &str;

    /// Get the tool description
    fn description(&self) -> &str;

    /// Execute the tool asynchronously
    async fn execute(&self, call: &ToolCall) -> Result<ToolResult>;

    /// Check if the tool requires confirmation before execution
    fn requires_confirmation(&self) -> bool {
        false
    }

    /// Get the estimated execution time
    fn estimated_duration(&self) -> Duration {
        Duration::from_secs(5)
    }

    /// Check if the tool can be retried on failure
    fn is_retryable(&self) -> bool {
        true
    }
}

/// Tool execution result with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    /// Tool name
    pub tool_name: String,
    /// Call ID
    pub call_id: String,
    /// The actual result
    pub result: ToolResult,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
    /// Number of retries attempted
    pub retries: u32,
    /// Whether execution was successful
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Timestamp of execution
    pub executed_at: chrono::DateTime<chrono::Utc>,
}

// =============================================================================
// Tool Registry
// =============================================================================

/// Registry for managing async tools
pub struct ToolRegistry {
    /// Registered tools
    tools: RwLock<HashMap<String, Arc<dyn AsyncTool>>>,
    /// Tool categories
    categories: RwLock<HashMap<String, Vec<String>>>,
}

impl ToolRegistry {
    /// Create a new tool registry
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            categories: RwLock::new(HashMap::new()),
        }
    }

    /// Register a tool
    pub async fn register(&self, tool: Arc<dyn AsyncTool>) {
        let name = tool.name().to_string();
        self.tools.write().await.insert(name.clone(), tool);
        info!(tool = %name, "Tool registered");
    }

    /// Register a tool in a category
    pub async fn register_in_category(&self, tool: Arc<dyn AsyncTool>, category: &str) {
        let name = tool.name().to_string();
        self.tools.write().await.insert(name.clone(), tool);

        self.categories
            .write()
            .await
            .entry(category.to_string())
            .or_default()
            .push(name.clone());

        info!(tool = %name, category = %category, "Tool registered in category");
    }

    /// Get a tool by name
    pub async fn get(&self, name: &str) -> Option<Arc<dyn AsyncTool>> {
        self.tools.read().await.get(name).cloned()
    }

    /// Get all tools in a category
    pub async fn get_category(&self, category: &str) -> Vec<Arc<dyn AsyncTool>> {
        let categories = self.categories.read().await;
        let tools = self.tools.read().await;

        categories
            .get(category)
            .map(|names| {
                names
                    .iter()
                    .filter_map(|name| tools.get(name).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// List all tool names
    pub async fn list_names(&self) -> Vec<String> {
        self.tools.read().await.keys().cloned().collect()
    }

    /// List all categories
    pub async fn list_categories(&self) -> Vec<String> {
        self.categories.read().await.keys().cloned().collect()
    }

    /// Unregister a tool
    pub async fn unregister(&self, name: &str) {
        self.tools.write().await.remove(name);

        // Remove from all categories
        for tools in self.categories.write().await.values_mut() {
            tools.retain(|n| n != name);
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tool Executor
// =============================================================================

/// Configuration for the tool executor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorConfig {
    /// Default timeout for tool execution
    pub default_timeout: Duration,
    /// Maximum concurrent executions
    pub max_concurrent: usize,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Base delay for exponential backoff (milliseconds)
    pub retry_base_delay_ms: u64,
    /// Whether to enable execution metrics
    pub enable_metrics: bool,
    /// Whether to sandbox tool execution
    pub sandbox_enabled: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(30),
            max_concurrent: 5,
            max_retries: 3,
            retry_base_delay_ms: 100,
            enable_metrics: true,
            sandbox_enabled: false,
        }
    }
}

/// Execution metrics
#[derive(Debug, Default)]
pub struct ExecutionMetrics {
    /// Total executions
    pub total_executions: AtomicU64,
    /// Successful executions
    pub successful_executions: AtomicU64,
    /// Failed executions
    pub failed_executions: AtomicU64,
    /// Timed out executions
    pub timed_out_executions: AtomicU64,
    /// Total execution time in milliseconds
    pub total_execution_time_ms: AtomicU64,
    /// Total retries
    pub total_retries: AtomicU64,
}

impl ExecutionMetrics {
    /// Get metrics summary
    pub fn summary(&self) -> MetricsSummary {
        let total = self.total_executions.load(Ordering::SeqCst);
        let successful = self.successful_executions.load(Ordering::SeqCst);
        let failed = self.failed_executions.load(Ordering::SeqCst);
        let timed_out = self.timed_out_executions.load(Ordering::SeqCst);
        let total_time = self.total_execution_time_ms.load(Ordering::SeqCst);
        let retries = self.total_retries.load(Ordering::SeqCst);

        MetricsSummary {
            total_executions: total,
            successful_executions: successful,
            failed_executions: failed,
            timed_out_executions: timed_out,
            success_rate: if total > 0 {
                successful as f64 / total as f64
            } else {
                0.0
            },
            average_execution_time_ms: if total > 0 { total_time / total } else { 0 },
            total_retries: retries,
        }
    }
}

/// Summary of execution metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    /// Total executions
    pub total_executions: u64,
    /// Successful executions
    pub successful_executions: u64,
    /// Failed executions
    pub failed_executions: u64,
    /// Timed out executions
    pub timed_out_executions: u64,
    /// Success rate (0.0 - 1.0)
    pub success_rate: f64,
    /// Average execution time in milliseconds
    pub average_execution_time_ms: u64,
    /// Total retries
    pub total_retries: u64,
}

/// Advanced tool executor with parallel execution and retry support
pub struct ToolExecutor {
    /// Tool registry
    registry: Arc<ToolRegistry>,
    /// Configuration
    config: ExecutorConfig,
    /// Concurrency limiter
    semaphore: Semaphore,
    /// Execution metrics
    metrics: ExecutionMetrics,
    /// Execution history
    history: RwLock<Vec<ToolExecutionResult>>,
}

impl ToolExecutor {
    /// Create a new tool executor
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self::with_config(registry, ExecutorConfig::default())
    }

    /// Create a new tool executor with config
    pub fn with_config(registry: Arc<ToolRegistry>, config: ExecutorConfig) -> Self {
        let semaphore = Semaphore::new(config.max_concurrent);
        Self {
            registry,
            config,
            semaphore,
            metrics: ExecutionMetrics::default(),
            history: RwLock::new(Vec::new()),
        }
    }

    /// Execute a single tool call
    pub async fn execute(&self, call: &ToolCall) -> Result<ToolExecutionResult> {
        let start = Instant::now();
        let tool_name = call.name.clone();
        let call_id = call.id.clone();

        // Acquire semaphore permit
        let _permit =
            self.semaphore.acquire().await.map_err(|_| {
                Error::tool_execution(&tool_name, "Failed to acquire execution permit")
            })?;

        // Get the tool
        let tool = self.registry.get(&tool_name).await.ok_or_else(|| {
            Error::tool_execution(&tool_name, format!("Tool '{}' not found", tool_name))
        })?;

        // Execute with retry
        let (result, retries) = self.execute_with_retry(&tool, call).await;

        let duration = start.elapsed();
        let duration_ms = duration.as_millis() as u64;

        // Update metrics
        self.metrics.total_executions.fetch_add(1, Ordering::SeqCst);
        self.metrics
            .total_execution_time_ms
            .fetch_add(duration_ms, Ordering::SeqCst);
        self.metrics
            .total_retries
            .fetch_add(retries as u64, Ordering::SeqCst);

        let execution_result = match result {
            Ok(result) => {
                self.metrics
                    .successful_executions
                    .fetch_add(1, Ordering::SeqCst);

                ToolExecutionResult {
                    tool_name,
                    call_id,
                    result,
                    duration_ms,
                    retries,
                    success: true,
                    error: None,
                    executed_at: chrono::Utc::now(),
                }
            }
            Err(e) => {
                self.metrics
                    .failed_executions
                    .fetch_add(1, Ordering::SeqCst);

                ToolExecutionResult {
                    tool_name: tool_name.clone(),
                    call_id: call_id.clone(),
                    result: ToolResult::failure(&call_id, e.to_string()),
                    duration_ms,
                    retries,
                    success: false,
                    error: Some(e.to_string()),
                    executed_at: chrono::Utc::now(),
                }
            }
        };

        // Store in history
        if self.config.enable_metrics {
            self.history.write().await.push(execution_result.clone());
        }

        Ok(execution_result)
    }

    /// Execute multiple tool calls in parallel
    pub async fn execute_parallel(&self, calls: Vec<ToolCall>) -> Vec<ToolExecutionResult> {
        let futures: Vec<_> = calls.iter().map(|call| self.execute(call)).collect();

        let results = futures::future::join_all(futures).await;

        results
            .into_iter()
            .map(|r| {
                r.unwrap_or_else(|e| ToolExecutionResult {
                    tool_name: "unknown".to_string(),
                    call_id: "unknown".to_string(),
                    result: ToolResult::failure("unknown", format!("Execution failed: {}", e)),
                    duration_ms: 0,
                    retries: 0,
                    success: false,
                    error: Some(e.to_string()),
                    executed_at: chrono::Utc::now(),
                })
            })
            .collect()
    }

    /// Execute with retry logic
    async fn execute_with_retry(
        &self,
        tool: &Arc<dyn AsyncTool>,
        call: &ToolCall,
    ) -> (Result<ToolResult>, u32) {
        let mut retries = 0;
        let max_retries = if tool.is_retryable() {
            self.config.max_retries
        } else {
            0
        };

        loop {
            // Execute with timeout
            let timeout = self.config.default_timeout;
            let result = tokio::time::timeout(timeout, tool.execute(call)).await;

            match result {
                Ok(Ok(tool_result)) => {
                    return (Ok(tool_result), retries);
                }
                Ok(Err(e)) => {
                    if retries >= max_retries {
                        error!(
                            tool = %tool.name(),
                            error = %e,
                            retries = retries,
                            "Tool execution failed after retries"
                        );
                        return (Err(e), retries);
                    }

                    retries += 1;
                    let delay = Duration::from_millis(
                        self.config.retry_base_delay_ms * (2_u64.pow(retries - 1)),
                    );

                    warn!(
                        tool = %tool.name(),
                        error = %e,
                        retry = retries,
                        delay_ms = delay.as_millis(),
                        "Retrying tool execution"
                    );

                    tokio::time::sleep(delay).await;
                }
                Err(_) => {
                    self.metrics
                        .timed_out_executions
                        .fetch_add(1, Ordering::SeqCst);

                    if retries >= max_retries {
                        return (
                            Err(Error::tool_execution(tool.name(), "Execution timed out")),
                            retries,
                        );
                    }

                    retries += 1;
                    warn!(
                        tool = %tool.name(),
                        timeout_secs = timeout.as_secs(),
                        retry = retries,
                        "Tool execution timed out, retrying"
                    );
                }
            }
        }
    }

    /// Get execution metrics
    pub fn metrics(&self) -> MetricsSummary {
        self.metrics.summary()
    }

    /// Get execution history
    pub async fn history(&self) -> Vec<ToolExecutionResult> {
        self.history.read().await.clone()
    }

    /// Clear execution history
    pub async fn clear_history(&self) {
        self.history.write().await.clear();
    }

    /// Get tool registry
    pub fn registry(&self) -> &Arc<ToolRegistry> {
        &self.registry
    }
}

// =============================================================================
// Built-in Tool Implementations
// =============================================================================

/// A simple function-based tool
pub struct FunctionTool<F>
where
    F: Fn(&ToolCall) -> Result<ToolResult> + Send + Sync,
{
    name: String,
    description: String,
    func: F,
}

impl<F> FunctionTool<F>
where
    F: Fn(&ToolCall) -> Result<ToolResult> + Send + Sync,
{
    /// Create a new function tool
    pub fn new(name: impl Into<String>, description: impl Into<String>, func: F) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            func,
        }
    }
}

#[async_trait]
impl<F> AsyncTool for FunctionTool<F>
where
    F: Fn(&ToolCall) -> Result<ToolResult> + Send + Sync,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    async fn execute(&self, call: &ToolCall) -> Result<ToolResult> {
        (self.func)(call)
    }
}

/// An async function-based tool
pub struct AsyncFunctionTool<F>
where
    F: Fn(
            ToolCall,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ToolResult>> + Send>>
        + Send
        + Sync,
{
    name: String,
    description: String,
    func: F,
}

impl<F> AsyncFunctionTool<F>
where
    F: Fn(
            ToolCall,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ToolResult>> + Send>>
        + Send
        + Sync,
{
    /// Create a new async function tool
    pub fn new(name: impl Into<String>, description: impl Into<String>, func: F) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            func,
        }
    }
}

#[async_trait]
impl<F> AsyncTool for AsyncFunctionTool<F>
where
    F: Fn(
            ToolCall,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ToolResult>> + Send>>
        + Send
        + Sync,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    async fn execute(&self, call: &ToolCall) -> Result<ToolResult> {
        (self.func)(call.clone()).await
    }
}

// =============================================================================
// Tool Execution Context
// =============================================================================

/// Context provided to tool execution
#[derive(Debug, Clone, Default)]
pub struct ToolContext {
    /// Current conversation ID
    pub conversation_id: Option<String>,
    /// User who initiated the tool call
    pub user_id: Option<String>,
    /// Session metadata
    pub metadata: HashMap<String, String>,
    /// Working directory for file operations
    pub working_dir: Option<String>,
    /// Environment variables
    pub env_vars: HashMap<String, String>,
}

impl ToolContext {
    /// Create a new tool context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set conversation ID
    pub fn with_conversation(mut self, id: impl Into<String>) -> Self {
        self.conversation_id = Some(id.into());
        self
    }

    /// Set user ID
    pub fn with_user(mut self, id: impl Into<String>) -> Self {
        self.user_id = Some(id.into());
        self
    }

    /// Set working directory
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Add environment variable
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_registry() {
        let registry = ToolRegistry::new();

        let tool = Arc::new(FunctionTool::new(
            "test_tool",
            "A test tool",
            |call: &ToolCall| Ok(ToolResult::success(&call.id, "Test output")),
        ));

        registry.register(tool).await;

        assert!(registry.get("test_tool").await.is_some());
        assert!(registry.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_tool_executor() {
        let registry = Arc::new(ToolRegistry::new());

        let tool = Arc::new(FunctionTool::new(
            "echo",
            "Echoes input",
            |call: &ToolCall| Ok(ToolResult::success(&call.id, call.arguments.to_string())),
        ));

        registry.register(tool).await;

        let executor = ToolExecutor::new(registry);

        let call = ToolCall {
            id: "call-1".to_string(),
            name: "echo".to_string(),
            arguments: serde_json::json!({"message": "Hello!"}),
        };

        let result = executor.execute(&call).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_parallel_execution() {
        let registry = Arc::new(ToolRegistry::new());

        let tool = Arc::new(FunctionTool::new(
            "delay",
            "Delays and returns",
            |call: &ToolCall| Ok(ToolResult::success(&call.id, "Done")),
        ));

        registry.register(tool).await;

        let executor = ToolExecutor::new(registry);

        let calls: Vec<ToolCall> = (0..3)
            .map(|i| ToolCall {
                id: format!("call-{}", i),
                name: "delay".to_string(),
                arguments: serde_json::json!({}),
            })
            .collect();

        let results = executor.execute_parallel(calls).await;
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.success));
    }

    #[tokio::test]
    async fn test_metrics() {
        let registry = Arc::new(ToolRegistry::new());

        let tool = Arc::new(FunctionTool::new("counter", "Counts", |call: &ToolCall| {
            Ok(ToolResult::success(&call.id, "1"))
        }));

        registry.register(tool).await;

        let executor = ToolExecutor::new(registry);

        for i in 0..5 {
            let call = ToolCall {
                id: format!("call-{}", i),
                name: "counter".to_string(),
                arguments: serde_json::json!({}),
            };
            let _ = executor.execute(&call).await;
        }

        let metrics = executor.metrics();
        assert_eq!(metrics.total_executions, 5);
        assert_eq!(metrics.successful_executions, 5);
        assert_eq!(metrics.failed_executions, 0);
    }
}
