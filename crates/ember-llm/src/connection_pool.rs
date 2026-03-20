//! Connection pooling for LLM providers
//!
//! This module provides efficient connection management including:
//! - Per-provider connection pools
//! - Health checks and automatic recovery
//! - Connection reuse metrics
//! - Adaptive pool sizing

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock, Semaphore};

use crate::Result;

/// Configuration for connection pools
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Minimum connections to maintain
    pub min_connections: usize,
    /// Maximum connections allowed
    pub max_connections: usize,
    /// Connection idle timeout
    pub idle_timeout: Duration,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Connection acquire timeout
    pub acquire_timeout: Duration,
    /// Enable adaptive sizing
    pub adaptive_sizing: bool,
    /// Target utilization for adaptive sizing (0.0 - 1.0)
    pub target_utilization: f64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_connections: 2,
            max_connections: 20,
            idle_timeout: Duration::from_secs(300), // 5 minutes
            health_check_interval: Duration::from_secs(30),
            acquire_timeout: Duration::from_secs(10),
            adaptive_sizing: true,
            target_utilization: 0.7,
        }
    }
}

/// Metrics for connection pool operations
#[derive(Debug, Default)]
pub struct PoolMetrics {
    /// Total connections created
    pub connections_created: AtomicU64,
    /// Total connections closed
    pub connections_closed: AtomicU64,
    /// Total connection acquisitions
    pub acquisitions: AtomicU64,
    /// Failed acquisitions (timeouts)
    pub acquisition_failures: AtomicU64,
    /// Health check failures
    pub health_check_failures: AtomicU64,
    /// Current active connections
    pub active_connections: AtomicUsize,
    /// Current idle connections
    pub idle_connections: AtomicUsize,
    /// Total wait time for acquisitions (ms)
    pub total_wait_time_ms: AtomicU64,
    /// Connection reuse count
    pub connection_reuses: AtomicU64,
}

impl PoolMetrics {
    /// Create new metrics instance
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Record a new connection
    pub fn connection_created(&self) {
        self.connections_created.fetch_add(1, Ordering::Relaxed);
    }

    /// Record connection closed
    pub fn connection_closed(&self) {
        self.connections_closed.fetch_add(1, Ordering::Relaxed);
    }

    /// Record successful acquisition
    pub fn acquired(&self, wait_time: Duration) {
        self.acquisitions.fetch_add(1, Ordering::Relaxed);
        self.total_wait_time_ms
            .fetch_add(wait_time.as_millis() as u64, Ordering::Relaxed);
    }

    /// Record failed acquisition
    pub fn acquisition_failed(&self) {
        self.acquisition_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Record health check failure
    pub fn health_check_failed(&self) {
        self.health_check_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Update active count
    pub fn set_active(&self, count: usize) {
        self.active_connections.store(count, Ordering::Relaxed);
    }

    /// Update idle count
    pub fn set_idle(&self, count: usize) {
        self.idle_connections.store(count, Ordering::Relaxed);
    }

    /// Record connection reuse
    pub fn connection_reused(&self) {
        self.connection_reuses.fetch_add(1, Ordering::Relaxed);
    }

    /// Get metrics snapshot
    pub fn snapshot(&self) -> PoolMetricsSnapshot {
        PoolMetricsSnapshot {
            connections_created: self.connections_created.load(Ordering::Relaxed),
            connections_closed: self.connections_closed.load(Ordering::Relaxed),
            acquisitions: self.acquisitions.load(Ordering::Relaxed),
            acquisition_failures: self.acquisition_failures.load(Ordering::Relaxed),
            health_check_failures: self.health_check_failures.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            idle_connections: self.idle_connections.load(Ordering::Relaxed),
            total_wait_time_ms: self.total_wait_time_ms.load(Ordering::Relaxed),
            connection_reuses: self.connection_reuses.load(Ordering::Relaxed),
        }
    }

    /// Calculate average wait time
    pub fn average_wait_time(&self) -> Duration {
        let total = self.total_wait_time_ms.load(Ordering::Relaxed);
        let count = self.acquisitions.load(Ordering::Relaxed);
        if count == 0 {
            Duration::ZERO
        } else {
            Duration::from_millis(total / count)
        }
    }

    /// Calculate current utilization
    pub fn utilization(&self) -> f64 {
        let active = self.active_connections.load(Ordering::Relaxed);
        let idle = self.idle_connections.load(Ordering::Relaxed);
        let total = active + idle;
        if total == 0 {
            0.0
        } else {
            active as f64 / total as f64
        }
    }
}

/// Snapshot of pool metrics
#[derive(Debug, Clone)]
pub struct PoolMetricsSnapshot {
    /// Total connections created
    pub connections_created: u64,
    /// Total connections closed
    pub connections_closed: u64,
    /// Total acquisitions
    pub acquisitions: u64,
    /// Failed acquisitions
    pub acquisition_failures: u64,
    /// Health check failures
    pub health_check_failures: u64,
    /// Current active connections
    pub active_connections: usize,
    /// Current idle connections
    pub idle_connections: usize,
    /// Total wait time (ms)
    pub total_wait_time_ms: u64,
    /// Connection reuses
    pub connection_reuses: u64,
}

/// Connection state tracking
#[derive(Debug)]
pub struct ConnectionState {
    /// When the connection was created
    pub created_at: Instant,
    /// Last time the connection was used
    pub last_used: Instant,
    /// Number of times this connection was used
    pub use_count: u64,
    /// Last health check time
    pub last_health_check: Option<Instant>,
    /// Is the connection healthy
    pub healthy: bool,
}

impl ConnectionState {
    /// Create new connection state
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            created_at: now,
            last_used: now,
            use_count: 0,
            last_health_check: None,
            healthy: true,
        }
    }

    /// Mark connection as used
    pub fn mark_used(&mut self) {
        self.last_used = Instant::now();
        self.use_count += 1;
    }

    /// Check if connection is idle too long
    pub fn is_idle_expired(&self, timeout: Duration) -> bool {
        self.last_used.elapsed() > timeout
    }

    /// Check if health check is needed
    pub fn needs_health_check(&self, interval: Duration) -> bool {
        match self.last_health_check {
            Some(last) => last.elapsed() > interval,
            None => true,
        }
    }

    /// Record health check result
    pub fn record_health_check(&mut self, healthy: bool) {
        self.last_health_check = Some(Instant::now());
        self.healthy = healthy;
    }
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self::new()
    }
}

/// Generic connection wrapper
pub struct PooledConnection<T> {
    /// The actual connection
    pub connection: T,
    /// Connection state
    pub state: ConnectionState,
    /// Provider name
    pub provider: String,
}

impl<T> PooledConnection<T> {
    /// Create a new pooled connection
    pub fn new(connection: T, provider: String) -> Self {
        Self {
            connection,
            state: ConnectionState::new(),
            provider,
        }
    }

    /// Mark as used
    pub fn mark_used(&mut self) {
        self.state.mark_used();
    }
}

/// Connection pool for a specific provider
pub struct ProviderPool<T> {
    /// Pool configuration
    config: PoolConfig,
    /// Available connections
    connections: Mutex<Vec<PooledConnection<T>>>,
    /// Semaphore for limiting connections
    semaphore: Arc<Semaphore>,
    /// Pool metrics
    metrics: Arc<PoolMetrics>,
    /// Provider name
    provider: String,
}

impl<T: Send + Sync + 'static> ProviderPool<T> {
    /// Create a new provider pool
    pub fn new(provider: String, config: PoolConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_connections));
        Self {
            config,
            connections: Mutex::new(Vec::new()),
            semaphore,
            metrics: PoolMetrics::new(),
            provider,
        }
    }

    /// Get pool metrics
    pub fn metrics(&self) -> Arc<PoolMetrics> {
        Arc::clone(&self.metrics)
    }

    /// Try to acquire a connection from the pool
    pub async fn try_acquire(&self) -> Option<PooledConnection<T>> {
        let mut connections = self.connections.lock().await;
        
        // Find a healthy, non-expired connection
        let idx = connections.iter().position(|c| {
            c.state.healthy && !c.state.is_idle_expired(self.config.idle_timeout)
        });

        if let Some(idx) = idx {
            let mut conn = connections.remove(idx);
            conn.mark_used();
            self.metrics.connection_reused();
            self.update_metrics(&connections).await;
            Some(conn)
        } else {
            None
        }
    }

    /// Return a connection to the pool
    pub async fn release(&self, mut conn: PooledConnection<T>) {
        let mut connections = self.connections.lock().await;
        
        // Only keep if healthy and not expired
        if conn.state.healthy && !conn.state.is_idle_expired(self.config.idle_timeout) {
            conn.state.last_used = Instant::now();
            connections.push(conn);
        } else {
            self.metrics.connection_closed();
        }
        
        self.update_metrics(&connections).await;
    }

    /// Update metrics based on current pool state
    async fn update_metrics(&self, connections: &[PooledConnection<T>]) {
        let idle = connections.len();
        let total = self.config.max_connections - self.semaphore.available_permits();
        let active = total.saturating_sub(idle);
        
        self.metrics.set_idle(idle);
        self.metrics.set_active(active);
    }

    /// Clean up expired connections
    pub async fn cleanup(&self) {
        let mut connections = self.connections.lock().await;
        let before = connections.len();
        
        connections.retain(|c| {
            !c.state.is_idle_expired(self.config.idle_timeout)
        });
        
        let removed = before - connections.len();
        for _ in 0..removed {
            self.metrics.connection_closed();
        }
        
        self.update_metrics(&connections).await;
    }

    /// Get current pool size
    pub async fn size(&self) -> usize {
        let connections = self.connections.lock().await;
        connections.len()
    }

    /// Get pool statistics
    pub async fn stats(&self) -> PoolStats {
        let connections = self.connections.lock().await;
        let snapshot = self.metrics.snapshot();
        
        PoolStats {
            provider: self.provider.clone(),
            idle_connections: connections.len(),
            active_connections: snapshot.active_connections,
            total_acquisitions: snapshot.acquisitions,
            total_reuses: snapshot.connection_reuses,
            average_wait_time: self.metrics.average_wait_time(),
            utilization: self.metrics.utilization(),
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Provider name
    pub provider: String,
    /// Idle connections
    pub idle_connections: usize,
    /// Active connections
    pub active_connections: usize,
    /// Total acquisitions
    pub total_acquisitions: u64,
    /// Total reuses
    pub total_reuses: u64,
    /// Average wait time
    pub average_wait_time: Duration,
    /// Current utilization
    pub utilization: f64,
}

/// Multi-provider connection pool manager
pub struct ConnectionPoolManager<T> {
    /// Per-provider pools
    pools: RwLock<HashMap<String, Arc<ProviderPool<T>>>>,
    /// Default configuration
    default_config: PoolConfig,
    /// Global metrics
    global_metrics: Arc<PoolMetrics>,
}

impl<T: Send + Sync + 'static> ConnectionPoolManager<T> {
    /// Create a new pool manager
    pub fn new(default_config: PoolConfig) -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
            default_config,
            global_metrics: PoolMetrics::new(),
        }
    }

    /// Get or create a pool for a provider
    pub async fn get_pool(&self, provider: &str) -> Arc<ProviderPool<T>> {
        // Check if pool exists
        {
            let pools = self.pools.read().await;
            if let Some(pool) = pools.get(provider) {
                return Arc::clone(pool);
            }
        }

        // Create new pool
        let mut pools = self.pools.write().await;
        
        // Double-check after acquiring write lock
        if let Some(pool) = pools.get(provider) {
            return Arc::clone(pool);
        }

        let pool = Arc::new(ProviderPool::new(
            provider.to_string(),
            self.default_config.clone(),
        ));
        pools.insert(provider.to_string(), Arc::clone(&pool));
        pool
    }

    /// Get global metrics
    pub fn global_metrics(&self) -> Arc<PoolMetrics> {
        Arc::clone(&self.global_metrics)
    }

    /// Get all pool statistics
    pub async fn all_stats(&self) -> Vec<PoolStats> {
        let pools = self.pools.read().await;
        let mut stats = Vec::new();
        
        for pool in pools.values() {
            stats.push(pool.stats().await);
        }
        
        stats
    }

    /// Cleanup all pools
    pub async fn cleanup_all(&self) {
        let pools = self.pools.read().await;
        for pool in pools.values() {
            pool.cleanup().await;
        }
    }

    /// Get list of all providers with pools
    pub async fn providers(&self) -> Vec<String> {
        let pools = self.pools.read().await;
        pools.keys().cloned().collect()
    }
}

/// Health checker for connections
pub struct HealthChecker {
    /// Check interval
    interval: Duration,
    /// Timeout for health checks
    timeout: Duration,
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new(interval: Duration, timeout: Duration) -> Self {
        Self { interval, timeout }
    }

    /// Check interval
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Check timeout
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pool_config_default() {
        let config = PoolConfig::default();
        assert_eq!(config.min_connections, 2);
        assert_eq!(config.max_connections, 20);
        assert!(config.adaptive_sizing);
    }

    #[tokio::test]
    async fn test_pool_metrics() {
        let metrics = PoolMetrics::new();
        
        metrics.connection_created();
        metrics.acquired(Duration::from_millis(50));
        metrics.connection_reused();
        
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.connections_created, 1);
        assert_eq!(snapshot.acquisitions, 1);
        assert_eq!(snapshot.connection_reuses, 1);
    }

    #[tokio::test]
    async fn test_connection_state() {
        let mut state = ConnectionState::new();
        assert!(state.healthy);
        assert_eq!(state.use_count, 0);
        
        state.mark_used();
        assert_eq!(state.use_count, 1);
        
        state.record_health_check(true);
        assert!(state.healthy);
        assert!(state.last_health_check.is_some());
    }

    #[tokio::test]
    async fn test_provider_pool() {
        let config = PoolConfig::default();
        let pool: ProviderPool<String> = ProviderPool::new("test".to_string(), config);
        
        assert_eq!(pool.size().await, 0);
        
        let stats = pool.stats().await;
        assert_eq!(stats.provider, "test");
        assert_eq!(stats.idle_connections, 0);
    }

    #[tokio::test]
    async fn test_connection_pool_manager() {
        let config = PoolConfig::default();
        let manager: ConnectionPoolManager<String> = ConnectionPoolManager::new(config);
        
        let pool1 = manager.get_pool("openai").await;
        let pool2 = manager.get_pool("anthropic").await;
        let pool3 = manager.get_pool("openai").await;
        
        // Same pool returned for same provider
        assert!(Arc::ptr_eq(&pool1, &pool3));
        assert!(!Arc::ptr_eq(&pool1, &pool2));
        
        let providers = manager.providers().await;
        assert_eq!(providers.len(), 2);
    }

    #[tokio::test]
    async fn test_metrics_utilization() {
        let metrics = PoolMetrics::new();
        
        metrics.set_active(3);
        metrics.set_idle(7);
        
        let utilization = metrics.utilization();
        assert!((utilization - 0.3).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_connection_idle_expiry() {
        let state = ConnectionState::new();
        
        // Not expired with long timeout
        assert!(!state.is_idle_expired(Duration::from_secs(60)));
        
        // Would be expired with zero timeout
        assert!(state.is_idle_expired(Duration::ZERO));
    }
}