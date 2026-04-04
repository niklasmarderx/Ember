//! # Performance Module
//!
//! Performance optimization utilities for the Ember framework including:
//! - Connection pooling for efficient resource management
//! - Lazy loading and initialization patterns
//! - Batch processing for bulk operations
//! - Object pooling for memory efficiency
//! - Async task scheduling and throttling

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, Mutex, RwLock, Semaphore};

// ============================================================================
// Connection Pool
// ============================================================================

/// Configuration for connection pools
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Minimum number of connections to maintain
    pub min_size: usize,
    /// Maximum number of connections
    pub max_size: usize,
    /// Connection timeout in milliseconds
    pub connection_timeout_ms: u64,
    /// Idle timeout before connection is closed (milliseconds)
    pub idle_timeout_ms: u64,
    /// Maximum connection lifetime (milliseconds)
    pub max_lifetime_ms: u64,
    /// Validation interval (milliseconds)
    pub validation_interval_ms: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_size: 2,
            max_size: 10,
            connection_timeout_ms: 5_000,
            idle_timeout_ms: 300_000,   // 5 minutes
            max_lifetime_ms: 1_800_000, // 30 minutes
            validation_interval_ms: 30_000,
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PoolStats {
    /// Total connections created
    pub connections_created: u64,
    /// Total connections closed
    pub connections_closed: u64,
    /// Current active connections
    pub active_connections: usize,
    /// Current idle connections
    pub idle_connections: usize,
    /// Total acquisitions
    pub acquisitions: u64,
    /// Failed acquisitions (timeout)
    pub acquisition_timeouts: u64,
    /// Average acquisition time in microseconds
    pub avg_acquisition_time_us: u64,
}

/// Generic pooled connection wrapper
pub struct PooledConnection<T: Send + 'static> {
    connection: Option<T>,
    pool: Arc<ConnectionPoolInner<T>>,
    #[allow(dead_code)]
    _acquired_at: Instant,
    created_at: Instant,
}

impl<T: Send + 'static> PooledConnection<T> {
    /// Get reference to the connection. Returns None if connection was already taken.
    pub fn get(&self) -> Option<&T> {
        self.connection.as_ref()
    }

    /// Get mutable reference to the connection. Returns None if connection was already taken.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.connection.as_mut()
    }
}

impl<T: Send + 'static> Drop for PooledConnection<T> {
    fn drop(&mut self) {
        if let Some(conn) = self.connection.take() {
            let pool = self.pool.clone();
            let created_at = self.created_at;
            tokio::spawn(async move {
                pool.return_connection(conn, created_at).await;
            });
        }
    }
}

struct ConnectionPoolInner<T> {
    config: PoolConfig,
    idle_connections: Mutex<VecDeque<(T, Instant)>>,
    active_count: AtomicUsize,
    total_created: AtomicU64,
    total_closed: AtomicU64,
    acquisitions: AtomicU64,
    acquisition_timeouts: AtomicU64,
    total_acquisition_time_us: AtomicU64,
    semaphore: Semaphore,
    closed: AtomicBool,
}

impl<T: Send + 'static> ConnectionPoolInner<T> {
    async fn return_connection(&self, conn: T, created_at: Instant) {
        self.active_count.fetch_sub(1, Ordering::SeqCst);

        // Check if pool is closed or connection is too old
        let max_lifetime = Duration::from_millis(self.config.max_lifetime_ms);
        if self.closed.load(Ordering::SeqCst) || created_at.elapsed() > max_lifetime {
            self.total_closed.fetch_add(1, Ordering::SeqCst);
            self.semaphore.add_permits(1);
            return;
        }

        // Return to idle pool
        let mut idle = self.idle_connections.lock().await;
        idle.push_back((conn, Instant::now()));
        self.semaphore.add_permits(1);
    }
}

/// Generic connection pool
pub struct ConnectionPool<T, F>
where
    F: Fn() -> Pin<Box<dyn Future<Output = Result<T, String>> + Send>> + Send + Sync,
{
    inner: Arc<ConnectionPoolInner<T>>,
    factory: F,
}

impl<T, F> ConnectionPool<T, F>
where
    T: Send + 'static,
    F: Fn() -> Pin<Box<dyn Future<Output = Result<T, String>> + Send>> + Send + Sync,
{
    /// Create a new connection pool
    pub fn new(config: PoolConfig, factory: F) -> Self {
        let semaphore = Semaphore::new(config.max_size);

        Self {
            inner: Arc::new(ConnectionPoolInner {
                config,
                idle_connections: Mutex::new(VecDeque::new()),
                active_count: AtomicUsize::new(0),
                total_created: AtomicU64::new(0),
                total_closed: AtomicU64::new(0),
                acquisitions: AtomicU64::new(0),
                acquisition_timeouts: AtomicU64::new(0),
                total_acquisition_time_us: AtomicU64::new(0),
                semaphore,
                closed: AtomicBool::new(false),
            }),
            factory,
        }
    }

    /// Acquire a connection from the pool
    pub async fn acquire(&self) -> Result<PooledConnection<T>, String> {
        let start = Instant::now();

        // Try to acquire permit with timeout
        let timeout = Duration::from_millis(self.inner.config.connection_timeout_ms);
        let permit = tokio::time::timeout(timeout, self.inner.semaphore.acquire())
            .await
            .map_err(|_| {
                self.inner
                    .acquisition_timeouts
                    .fetch_add(1, Ordering::SeqCst);
                "Connection pool acquisition timeout".to_string()
            })?
            .map_err(|_| "Pool closed".to_string())?;

        // Don't drop permit - we manage it manually
        permit.forget();

        self.inner.acquisitions.fetch_add(1, Ordering::SeqCst);

        // Try to get an idle connection
        let idle_timeout = Duration::from_millis(self.inner.config.idle_timeout_ms);
        let mut idle = self.inner.idle_connections.lock().await;

        while let Some((conn, last_used)) = idle.pop_front() {
            if last_used.elapsed() < idle_timeout {
                drop(idle);

                let duration = start.elapsed();
                self.inner
                    .total_acquisition_time_us
                    .fetch_add(duration.as_micros() as u64, Ordering::SeqCst);
                self.inner.active_count.fetch_add(1, Ordering::SeqCst);

                return Ok(PooledConnection {
                    connection: Some(conn),
                    pool: self.inner.clone(),
                    _acquired_at: Instant::now(),
                    created_at: Instant::now()
                        .checked_sub(Duration::from_millis(self.inner.config.max_lifetime_ms / 2))
                        .unwrap(),
                });
            }
            // Connection too old, discard
            self.inner.total_closed.fetch_add(1, Ordering::SeqCst);
        }
        drop(idle);

        // Create new connection
        let conn = (self.factory)().await?;
        self.inner.total_created.fetch_add(1, Ordering::SeqCst);

        let duration = start.elapsed();
        self.inner
            .total_acquisition_time_us
            .fetch_add(duration.as_micros() as u64, Ordering::SeqCst);
        self.inner.active_count.fetch_add(1, Ordering::SeqCst);

        Ok(PooledConnection {
            connection: Some(conn),
            pool: self.inner.clone(),
            _acquired_at: Instant::now(),
            created_at: Instant::now(),
        })
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        let acquisitions = self.inner.acquisitions.load(Ordering::SeqCst);
        let total_time = self.inner.total_acquisition_time_us.load(Ordering::SeqCst);

        PoolStats {
            connections_created: self.inner.total_created.load(Ordering::SeqCst),
            connections_closed: self.inner.total_closed.load(Ordering::SeqCst),
            active_connections: self.inner.active_count.load(Ordering::SeqCst),
            idle_connections: 0, // Would need async to get this
            acquisitions,
            acquisition_timeouts: self.inner.acquisition_timeouts.load(Ordering::SeqCst),
            avg_acquisition_time_us: if acquisitions > 0 {
                total_time / acquisitions
            } else {
                0
            },
        }
    }

    /// Close the pool
    pub fn close(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
    }
}

// ============================================================================
// Lazy Loading
// ============================================================================

/// Lazy-initialized value
pub struct Lazy<T, F = fn() -> T> {
    cell: tokio::sync::OnceCell<T>,
    init: F,
}

impl<T, F> Lazy<T, F>
where
    F: Fn() -> T,
{
    /// Create a new lazy value
    pub const fn new(init: F) -> Self {
        Self {
            cell: tokio::sync::OnceCell::const_new(),
            init,
        }
    }

    /// Get or initialize the value
    pub async fn get(&self) -> &T {
        self.cell.get_or_init(|| async { (self.init)() }).await
    }

    /// Check if initialized
    pub fn is_initialized(&self) -> bool {
        self.cell.initialized()
    }
}

/// Async lazy-initialized value
pub struct AsyncLazy<T, F, Fut>
where
    F: Fn() -> Fut,
    Fut: Future<Output = T>,
{
    cell: tokio::sync::OnceCell<T>,
    init: F,
}

impl<T, F, Fut> AsyncLazy<T, F, Fut>
where
    F: Fn() -> Fut,
    Fut: Future<Output = T>,
{
    /// Create a new async lazy value
    pub const fn new(init: F) -> Self {
        Self {
            cell: tokio::sync::OnceCell::const_new(),
            init,
        }
    }

    /// Get or initialize the value
    pub async fn get(&self) -> &T {
        self.cell.get_or_init(|| (self.init)()).await
    }

    /// Check if initialized
    pub fn is_initialized(&self) -> bool {
        self.cell.initialized()
    }
}

// ============================================================================
// Batch Processing
// ============================================================================

/// Batch processor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    /// Maximum batch size
    pub max_batch_size: usize,
    /// Maximum wait time before processing batch (milliseconds)
    pub max_wait_ms: u64,
    /// Number of concurrent batches
    pub concurrency: usize,
    /// Retry failed items
    pub retry_failed: bool,
    /// Maximum retries per item
    pub max_retries: u32,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            max_wait_ms: 100,
            concurrency: 4,
            retry_failed: true,
            max_retries: 3,
        }
    }
}

/// Result of batch processing
#[derive(Debug, Clone)]
pub struct BatchResult<T, E> {
    /// Successfully processed items
    pub successes: Vec<T>,
    /// Failed items with their errors
    pub failures: Vec<(T, E)>,
    /// Processing duration
    pub duration: Duration,
    /// Number of retries performed
    pub retries: u32,
}

/// Batch processor for bulk operations
#[allow(clippy::type_complexity)]
pub struct BatchProcessor<T, R, E, F>
where
    F: Fn(Vec<T>) -> Pin<Box<dyn Future<Output = Vec<Result<R, E>>> + Send>>,
{
    config: BatchConfig,
    processor: F,
    pending: Arc<Mutex<Vec<(T, oneshot::Sender<Result<R, E>>)>>>,
    semaphore: Arc<Semaphore>,
    _phantom: std::marker::PhantomData<(R, E)>,
}

impl<T, R, E, F> BatchProcessor<T, R, E, F>
where
    T: Send + Clone + 'static,
    R: Send + 'static,
    E: Send + Clone + 'static,
    F: Fn(Vec<T>) -> Pin<Box<dyn Future<Output = Vec<Result<R, E>>> + Send>>
        + Send
        + Sync
        + 'static,
{
    /// Create a new batch processor
    pub fn new(config: BatchConfig, processor: F) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(config.concurrency)),
            config,
            processor,
            pending: Arc::new(Mutex::new(Vec::new())),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Submit an item for processing
    pub async fn submit(&self, item: T) -> oneshot::Receiver<Result<R, E>> {
        let (tx, rx) = oneshot::channel();

        let mut pending = self.pending.lock().await;
        pending.push((item, tx));

        if pending.len() >= self.config.max_batch_size {
            self.flush_internal(&mut pending).await;
        }

        rx
    }

    /// Flush pending items
    pub async fn flush(&self) {
        let mut pending = self.pending.lock().await;
        self.flush_internal(&mut pending).await;
    }

    async fn flush_internal(&self, pending: &mut Vec<(T, oneshot::Sender<Result<R, E>>)>) {
        if pending.is_empty() {
            return;
        }

        let batch: Vec<_> = std::mem::take(pending);
        let items: Vec<T> = batch.iter().map(|(item, _)| item.clone()).collect();
        let senders: Vec<_> = batch.into_iter().map(|(_, tx)| tx).collect();

        // Process batch
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| {
                tracing::error!("Batch processor semaphore closed unexpectedly");
            })
            .ok();
        let results = (self.processor)(items).await;

        // Send results
        for (result, tx) in results.into_iter().zip(senders) {
            let _ = tx.send(result);
        }
    }
}

// ============================================================================
// Object Pool
// ============================================================================

/// Object pool for reusable objects
pub struct ObjectPool<T> {
    objects: Arc<Mutex<Vec<T>>>,
    factory: Box<dyn Fn() -> T + Send + Sync>,
    max_size: usize,
    created: AtomicUsize,
    reused: AtomicU64,
}

impl<T: Send + 'static> ObjectPool<T> {
    /// Create a new object pool
    pub fn new<F>(factory: F, max_size: usize) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            objects: Arc::new(Mutex::new(Vec::new())),
            factory: Box::new(factory),
            max_size,
            created: AtomicUsize::new(0),
            reused: AtomicU64::new(0),
        }
    }

    /// Acquire an object from the pool
    pub async fn acquire(&self) -> PooledObject<T> {
        let mut objects = self.objects.lock().await;

        let object = if let Some(obj) = objects.pop() {
            self.reused.fetch_add(1, Ordering::SeqCst);
            obj
        } else {
            self.created.fetch_add(1, Ordering::SeqCst);
            (self.factory)()
        };

        PooledObject {
            object: Some(object),
            pool: self.objects.clone(),
            max_size: self.max_size,
        }
    }

    /// Get pool statistics
    pub fn stats(&self) -> ObjectPoolStats {
        ObjectPoolStats {
            created: self.created.load(Ordering::SeqCst),
            reused: self.reused.load(Ordering::SeqCst),
            max_size: self.max_size,
        }
    }
}

/// Pooled object wrapper
pub struct PooledObject<T: Send + 'static> {
    object: Option<T>,
    pool: Arc<Mutex<Vec<T>>>,
    max_size: usize,
}

impl<T: Send + 'static> PooledObject<T> {
    /// Get reference to the object
    pub fn get(&self) -> &T {
        self.object.as_ref().expect("pooled object already taken")
    }

    /// Get mutable reference to the object
    pub fn get_mut(&mut self) -> &mut T {
        self.object.as_mut().expect("pooled object already taken")
    }
}

impl<T: Send + 'static> Drop for PooledObject<T> {
    fn drop(&mut self) {
        if let Some(obj) = self.object.take() {
            let pool = self.pool.clone();
            let max_size = self.max_size;
            tokio::spawn(async move {
                let mut objects = pool.lock().await;
                if objects.len() < max_size {
                    objects.push(obj);
                }
            });
        }
    }
}

/// Object pool statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectPoolStats {
    /// Total objects created
    pub created: usize,
    /// Total objects reused
    pub reused: u64,
    /// Maximum pool size
    pub max_size: usize,
}

// ============================================================================
// Task Scheduler
// ============================================================================

/// Task scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// Maximum concurrent tasks
    pub max_concurrent: usize,
    /// Task timeout in milliseconds
    pub task_timeout_ms: u64,
    /// Queue size
    pub queue_size: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 10,
            task_timeout_ms: 30_000,
            queue_size: 1000,
        }
    }
}

/// Task priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum TaskPriority {
    /// Low priority
    Low = 0,
    /// Normal priority
    #[default]
    Normal = 1,
    /// High priority
    High = 2,
    /// Critical priority
    Critical = 3,
}

/// Scheduled task
#[allow(dead_code)]
struct ScheduledTask {
    id: String,
    priority: TaskPriority,
    task: Pin<Box<dyn Future<Output = ()> + Send>>,
    created_at: Instant,
}

/// Task scheduler for rate-limited execution
pub struct TaskScheduler {
    config: SchedulerConfig,
    semaphore: Arc<Semaphore>,
    queue: Arc<Mutex<Vec<ScheduledTask>>>,
    completed: Arc<AtomicU64>,
    failed: Arc<AtomicU64>,
    active: Arc<AtomicUsize>,
}

impl TaskScheduler {
    /// Create a new task scheduler
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(config.max_concurrent)),
            config,
            queue: Arc::new(Mutex::new(Vec::new())),
            completed: Arc::new(AtomicU64::new(0)),
            failed: Arc::new(AtomicU64::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Schedule a task
    pub async fn schedule<F>(&self, id: &str, priority: TaskPriority, task: F) -> Result<(), String>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let mut queue = self.queue.lock().await;

        if queue.len() >= self.config.queue_size {
            return Err("Queue full".to_string());
        }

        queue.push(ScheduledTask {
            id: id.to_string(),
            priority,
            task: Box::pin(task),
            created_at: Instant::now(),
        });

        // Sort by priority (highest first)
        queue.sort_by(|a, b| b.priority.cmp(&a.priority));

        Ok(())
    }

    /// Run the scheduler
    pub async fn run(&self) {
        loop {
            // Get next task
            let task = {
                let mut queue = self.queue.lock().await;
                queue.pop()
            };

            let task = match task {
                Some(t) => t,
                None => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    continue;
                }
            };

            // Acquire semaphore
            let permit = match self.semaphore.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => continue,
            };

            self.active.fetch_add(1, Ordering::SeqCst);

            let completed = self.completed.clone();
            let failed = self.failed.clone();
            let active = self.active.clone();
            let timeout = Duration::from_millis(self.config.task_timeout_ms);

            tokio::spawn(async move {
                let result = tokio::time::timeout(timeout, task.task).await;

                match result {
                    Ok(_) => {
                        completed.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(_) => {
                        failed.fetch_add(1, Ordering::SeqCst);
                    }
                }

                active.fetch_sub(1, Ordering::SeqCst);
                drop(permit);
            });
        }
    }

    /// Get scheduler statistics
    pub fn stats(&self) -> SchedulerStats {
        SchedulerStats {
            completed: self.completed.load(Ordering::SeqCst),
            failed: self.failed.load(Ordering::SeqCst),
            active: self.active.load(Ordering::SeqCst),
            max_concurrent: self.config.max_concurrent,
        }
    }
}

/// Scheduler statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerStats {
    /// Completed tasks
    pub completed: u64,
    /// Failed tasks
    pub failed: u64,
    /// Active tasks
    pub active: usize,
    /// Maximum concurrent tasks
    pub max_concurrent: usize,
}

// ============================================================================
// Throttler
// ============================================================================

/// Throttler for rate-limiting operations
pub struct Throttler {
    /// Interval between operations
    interval: Duration,
    /// Last operation time
    last_operation: Arc<Mutex<Instant>>,
}

impl Throttler {
    /// Create a new throttler
    pub fn new(operations_per_second: f64) -> Self {
        Self {
            interval: Duration::from_secs_f64(1.0 / operations_per_second),
            last_operation: Arc::new(Mutex::new(
                Instant::now().checked_sub(Duration::from_secs(1)).unwrap(),
            )),
        }
    }

    /// Wait until the next operation is allowed
    pub async fn wait(&self) {
        let mut last = self.last_operation.lock().await;
        let elapsed = last.elapsed();

        if elapsed < self.interval {
            tokio::time::sleep(self.interval.checked_sub(elapsed).unwrap()).await;
        }

        *last = Instant::now();
    }

    /// Try to perform an operation without waiting
    pub async fn try_acquire(&self) -> bool {
        let mut last = self.last_operation.lock().await;
        let elapsed = last.elapsed();

        if elapsed >= self.interval {
            *last = Instant::now();
            true
        } else {
            false
        }
    }
}

// ============================================================================
// Circuit Breaker Pattern
// ============================================================================

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakerState {
    /// Circuit is closed (normal operation)
    Closed,
    /// Circuit is open (failing fast)
    Open,
    /// Circuit is half-open (testing)
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakerConfig {
    /// Number of failures before opening
    pub failure_threshold: u32,
    /// Duration to stay open before half-open (milliseconds)
    pub open_duration_ms: u64,
    /// Number of successes in half-open to close
    pub half_open_successes: u32,
    /// Timeout for each call (milliseconds)
    pub call_timeout_ms: u64,
}

impl Default for BreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            open_duration_ms: 30_000,
            half_open_successes: 3,
            call_timeout_ms: 5_000,
        }
    }
}

/// Circuit breaker for fault tolerance
pub struct CircuitBreakerV2 {
    config: BreakerConfig,
    state: Arc<RwLock<BreakerState>>,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    last_failure: Arc<Mutex<Option<Instant>>>,
    total_calls: AtomicU64,
    total_failures: AtomicU64,
}

impl CircuitBreakerV2 {
    /// Create a new circuit breaker
    pub fn new(config: BreakerConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(BreakerState::Closed)),
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            last_failure: Arc::new(Mutex::new(None)),
            total_calls: AtomicU64::new(0),
            total_failures: AtomicU64::new(0),
        }
    }

    /// Execute a function with circuit breaker protection
    pub async fn call<F, T, E>(&self, f: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: Future<Output = Result<T, E>>,
    {
        self.total_calls.fetch_add(1, Ordering::SeqCst);

        // Check state
        let state = *self.state.read().await;

        match state {
            BreakerState::Open => {
                // Check if we should transition to half-open
                let last_failure = self.last_failure.lock().await;
                if let Some(last) = *last_failure {
                    let open_duration = Duration::from_millis(self.config.open_duration_ms);
                    if last.elapsed() >= open_duration {
                        drop(last_failure);
                        *self.state.write().await = BreakerState::HalfOpen;
                        self.success_count.store(0, Ordering::SeqCst);
                    } else {
                        return Err(CircuitBreakerError::Open);
                    }
                } else {
                    return Err(CircuitBreakerError::Open);
                }
            }
            BreakerState::Closed | BreakerState::HalfOpen => {}
        }

        // Execute with timeout
        let timeout = Duration::from_millis(self.config.call_timeout_ms);
        let result = tokio::time::timeout(timeout, f).await;

        match result {
            Ok(Ok(value)) => {
                self.on_success().await;
                Ok(value)
            }
            Ok(Err(e)) => {
                self.on_failure().await;
                Err(CircuitBreakerError::Inner(e))
            }
            Err(_) => {
                self.on_failure().await;
                Err(CircuitBreakerError::Timeout)
            }
        }
    }

    async fn on_success(&self) {
        let state = *self.state.read().await;

        match state {
            BreakerState::HalfOpen => {
                let count = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
                if count >= self.config.half_open_successes {
                    *self.state.write().await = BreakerState::Closed;
                    self.failure_count.store(0, Ordering::SeqCst);
                }
            }
            BreakerState::Closed => {
                self.failure_count.store(0, Ordering::SeqCst);
            }
            BreakerState::Open => {}
        }
    }

    async fn on_failure(&self) {
        self.total_failures.fetch_add(1, Ordering::SeqCst);

        let state = *self.state.read().await;

        match state {
            BreakerState::Closed => {
                let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
                if count >= self.config.failure_threshold {
                    *self.state.write().await = BreakerState::Open;
                    *self.last_failure.lock().await = Some(Instant::now());
                }
            }
            BreakerState::HalfOpen => {
                *self.state.write().await = BreakerState::Open;
                *self.last_failure.lock().await = Some(Instant::now());
            }
            BreakerState::Open => {}
        }
    }

    /// Get current state
    pub async fn state(&self) -> BreakerState {
        *self.state.read().await
    }

    /// Get statistics
    pub fn stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            total_calls: self.total_calls.load(Ordering::SeqCst),
            total_failures: self.total_failures.load(Ordering::SeqCst),
            current_failures: self.failure_count.load(Ordering::SeqCst),
        }
    }

    /// Reset the circuit breaker
    pub async fn reset(&self) {
        *self.state.write().await = BreakerState::Closed;
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);
        *self.last_failure.lock().await = None;
    }
}

/// Circuit breaker error
#[derive(Debug)]
pub enum CircuitBreakerError<E> {
    /// Circuit is open
    Open,
    /// Call timed out
    Timeout,
    /// Inner error
    Inner(E),
}

/// Circuit breaker statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStats {
    /// Total calls
    pub total_calls: u64,
    /// Total failures
    pub total_failures: u64,
    /// Current failure count
    pub current_failures: u32,
}

// ============================================================================
// Memory-efficient String Interning
// ============================================================================

/// String interner for memory-efficient string storage
pub struct StringInterner {
    strings: Arc<RwLock<HashMap<String, Arc<str>>>>,
    total_interned: AtomicU64,
    total_hits: AtomicU64,
}

impl StringInterner {
    /// Create a new string interner
    pub fn new() -> Self {
        Self {
            strings: Arc::new(RwLock::new(HashMap::new())),
            total_interned: AtomicU64::new(0),
            total_hits: AtomicU64::new(0),
        }
    }

    /// Intern a string
    pub async fn intern(&self, s: &str) -> Arc<str> {
        // Check if already interned
        {
            let strings = self.strings.read().await;
            if let Some(interned) = strings.get(s) {
                self.total_hits.fetch_add(1, Ordering::SeqCst);
                return interned.clone();
            }
        }

        // Intern new string
        let mut strings = self.strings.write().await;

        // Double-check after acquiring write lock
        if let Some(interned) = strings.get(s) {
            self.total_hits.fetch_add(1, Ordering::SeqCst);
            return interned.clone();
        }

        let interned: Arc<str> = Arc::from(s);
        strings.insert(s.to_string(), interned.clone());
        self.total_interned.fetch_add(1, Ordering::SeqCst);

        interned
    }

    /// Get statistics
    pub fn stats(&self) -> InternerStats {
        InternerStats {
            total_interned: self.total_interned.load(Ordering::SeqCst),
            total_hits: self.total_hits.load(Ordering::SeqCst),
        }
    }
}

impl Default for StringInterner {
    fn default() -> Self {
        Self::new()
    }
}

/// String interner statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternerStats {
    /// Total strings interned
    pub total_interned: u64,
    /// Total cache hits
    pub total_hits: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[tokio::test]
    async fn test_object_pool() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let pool = ObjectPool::new(
            move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                vec![0u8; 1024]
            },
            5,
        );

        // Acquire and release
        {
            let obj = pool.acquire().await;
            assert_eq!(obj.get().len(), 1024);
        }

        // Should reuse
        tokio::time::sleep(Duration::from_millis(10)).await;
        {
            let _obj = pool.acquire().await;
        }

        let stats = pool.stats();
        assert_eq!(stats.created, 1);
    }

    #[tokio::test]
    async fn test_throttler() {
        let throttler = Throttler::new(10.0); // 10 ops/sec

        let start = Instant::now();
        for _ in 0..3 {
            throttler.wait().await;
        }
        let elapsed = start.elapsed();

        // Should take at least 200ms (2 intervals of 100ms)
        assert!(elapsed >= Duration::from_millis(180));
    }

    #[tokio::test]
    async fn test_circuit_breaker() {
        let breaker = CircuitBreakerV2::new(BreakerConfig {
            failure_threshold: 2,
            open_duration_ms: 100,
            half_open_successes: 1,
            call_timeout_ms: 1000,
        });

        // Success
        let result = breaker.call(async { Ok::<i32, &str>(42) }).await;
        assert!(result.is_ok());

        // Failures to open circuit
        let _ = breaker.call(async { Err::<i32, &str>("error") }).await;
        let _ = breaker.call(async { Err::<i32, &str>("error") }).await;

        assert_eq!(breaker.state().await, BreakerState::Open);

        // Should fail fast
        let result = breaker.call(async { Ok::<i32, &str>(42) }).await;
        assert!(matches!(result, Err(CircuitBreakerError::Open)));

        // Wait for half-open
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should allow one request
        let result = breaker.call(async { Ok::<i32, &str>(42) }).await;
        assert!(result.is_ok());

        assert_eq!(breaker.state().await, BreakerState::Closed);
    }

    #[tokio::test]
    async fn test_string_interner() {
        let interner = StringInterner::new();

        let s1 = interner.intern("hello").await;
        let s2 = interner.intern("hello").await;
        let s3 = interner.intern("world").await;

        // Same pointer for same string
        assert!(Arc::ptr_eq(&s1, &s2));
        assert!(!Arc::ptr_eq(&s1, &s3));

        let stats = interner.stats();
        assert_eq!(stats.total_interned, 2);
        assert_eq!(stats.total_hits, 1);
    }
}
