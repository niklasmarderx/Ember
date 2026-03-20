# Performance Module

Ember includes a comprehensive performance optimization module to help build efficient, scalable AI applications. This module provides connection pooling, object pooling, throttling, circuit breakers, and string interning.

## Connection Pooling

Efficiently manage database and HTTP connections:

```rust
use ember_core::performance::{ConnectionPool, PoolConfig};
use std::time::Duration;

let config = PoolConfig {
    min_connections: 5,
    max_connections: 50,
    connection_timeout: Duration::from_secs(30),
    idle_timeout: Duration::from_secs(300),
    max_lifetime: Duration::from_secs(3600),
    health_check_interval: Duration::from_secs(60),
};

let pool = ConnectionPool::new(config, connection_factory).await?;

// Get a connection from the pool
let conn = pool.get().await?;

// Use the connection
conn.execute("SELECT * FROM users").await?;

// Connection is automatically returned when dropped

// Check pool stats
let stats = pool.stats();
println!("Active: {}, Idle: {}", stats.active, stats.idle);
```

### Pool Configuration Options

- **min_connections**: Minimum connections to maintain
- **max_connections**: Maximum connections allowed
- **connection_timeout**: Time to wait for a connection
- **idle_timeout**: How long idle connections survive
- **max_lifetime**: Maximum connection age
- **health_check_interval**: How often to check connection health

## Object Pooling

Reuse expensive objects to reduce allocation overhead:

```rust
use ember_core::performance::{ObjectPool, PooledObject};

// Create a pool with a factory function
let pool: ObjectPool<ExpensiveObject> = ObjectPool::new(
    || ExpensiveObject::new(),  // Factory
    |obj| obj.reset(),          // Reset function
    100,                        // Max pool size
);

// Get an object from the pool
let obj = pool.get();

// Use the object
obj.do_work();

// Object is automatically returned to pool when dropped

// Get pool statistics
let stats = pool.stats();
println!("Pool size: {}, In use: {}", stats.pool_size, stats.in_use);
```

## Lazy Initialization

Defer expensive initialization until first use:

```rust
use ember_core::performance::{Lazy, AsyncLazy};

// Synchronous lazy initialization
let config: Lazy<Config> = Lazy::new(|| {
    // This only runs on first access
    load_config_from_file()
});

// Access triggers initialization
let value = config.get();

// Async lazy initialization
let client: AsyncLazy<ApiClient> = AsyncLazy::new(|| async {
    ApiClient::connect("api.example.com").await
});

let client = client.get().await;
```

## Batch Processing

Process items in efficient batches:

```rust
use ember_core::performance::{BatchProcessor, BatchConfig};
use std::time::Duration;

let config = BatchConfig {
    max_batch_size: 100,
    max_wait_time: Duration::from_millis(50),
    max_concurrent_batches: 4,
};

let processor = BatchProcessor::new(config, |batch: Vec<Request>| async move {
    // Process entire batch at once
    process_requests(batch).await
});

// Add items - they're automatically batched
processor.add(request1).await;
processor.add(request2).await;
processor.add(request3).await;

// Force flush remaining items
processor.flush().await;
```

## Task Scheduling

Schedule and prioritize background tasks:

```rust
use ember_core::performance::{TaskScheduler, SchedulerConfig, TaskPriority};

let config = SchedulerConfig {
    max_concurrent_tasks: 10,
    queue_size: 1000,
    enable_priority: true,
};

let scheduler = TaskScheduler::new(config);

// Schedule tasks with different priorities
scheduler.schedule(TaskPriority::High, async {
    critical_task().await
}).await;

scheduler.schedule(TaskPriority::Low, async {
    background_cleanup().await
}).await;

// Schedule delayed task
scheduler.schedule_delayed(
    Duration::from_secs(60),
    TaskPriority::Normal,
    async { periodic_task().await }
).await;
```

## Throttling

Control request rates to external services:

```rust
use ember_core::performance::Throttler;
use std::time::Duration;

// Create throttler: 100 requests per second
let throttler = Throttler::new(100, Duration::from_secs(1));

// Wait for permission before making request
throttler.acquire().await;
make_api_call().await;

// Or check without blocking
if throttler.try_acquire() {
    make_api_call().await;
} else {
    // Rate limited, handle accordingly
}
```

## Circuit Breaker

Prevent cascade failures with the circuit breaker pattern:

```rust
use ember_core::performance::{CircuitBreakerV2, BreakerConfig, BreakerState};
use std::time::Duration;

let config = BreakerConfig {
    failure_threshold: 5,
    success_threshold: 3,
    timeout: Duration::from_secs(30),
    half_open_max_calls: 3,
};

let breaker = CircuitBreakerV2::new(config);

// Execute with circuit breaker protection
match breaker.call(|| async {
    external_service_call().await
}).await {
    Ok(result) => println!("Success: {:?}", result),
    Err(e) => {
        if matches!(e, CircuitBreakerError::Open) {
            println!("Circuit is open, service unavailable");
        } else {
            println!("Call failed: {:?}", e);
        }
    }
}

// Check circuit state
match breaker.state() {
    BreakerState::Closed => println!("Circuit closed, normal operation"),
    BreakerState::Open => println!("Circuit open, rejecting calls"),
    BreakerState::HalfOpen => println!("Circuit testing, limited calls"),
}
```

### Circuit Breaker States

1. **Closed**: Normal operation, all calls pass through
2. **Open**: Failure threshold exceeded, calls rejected immediately
3. **Half-Open**: Testing recovery, limited calls allowed

## String Interning

Reduce memory usage for frequently repeated strings:

```rust
use ember_core::performance::{StringInterner, InternerStats};

let interner = StringInterner::new();

// Intern strings
let s1 = interner.intern("frequently_used_string");
let s2 = interner.intern("frequently_used_string");

// s1 and s2 point to the same memory
assert!(std::ptr::eq(s1.as_ptr(), s2.as_ptr()));

// Get statistics
let stats = interner.stats();
println!("Unique strings: {}", stats.unique_count);
println!("Total interned: {}", stats.total_interned);
println!("Memory saved: {} bytes", stats.memory_saved);

// Resolve interned string
if let Some(s) = interner.resolve(&s1) {
    println!("Resolved: {}", s);
}
```

## Performance Best Practices

### 1. Use Connection Pools

```rust
// Bad: Creating new connections
for request in requests {
    let conn = Database::connect().await?;  // Expensive!
    conn.execute(request).await?;
}

// Good: Use a pool
let pool = ConnectionPool::new(config, factory).await?;
for request in requests {
    let conn = pool.get().await?;  // Fast!
    conn.execute(request).await?;
}
```

### 2. Batch Operations

```rust
// Bad: Individual requests
for item in items {
    api.send(item).await?;  // N network calls
}

// Good: Batch requests
let processor = BatchProcessor::new(config, |batch| async {
    api.send_batch(batch).await  // 1 network call
});
for item in items {
    processor.add(item).await;
}
```

### 3. Protect External Calls

```rust
// Bad: No protection
let result = external_api.call().await?;  // Can cascade failures

// Good: Use circuit breaker
let result = breaker.call(|| async {
    external_api.call().await
}).await?;  // Fails fast when service is down
```

### 4. Intern Repeated Strings

```rust
// Bad: Many duplicate strings
let messages: Vec<String> = events
    .iter()
    .map(|e| e.event_type.clone())  // Duplicates "click", "view", etc.
    .collect();

// Good: Intern repeated strings
let messages: Vec<&str> = events
    .iter()
    .map(|e| interner.intern(&e.event_type))
    .collect();
```

## Configuration

Performance settings in `ember.toml`:

```toml
[performance]
enable_connection_pooling = true
enable_object_pooling = true

[performance.connection_pool]
min_connections = 5
max_connections = 50
connection_timeout_secs = 30
idle_timeout_secs = 300

[performance.circuit_breaker]
failure_threshold = 5
success_threshold = 3
timeout_secs = 30

[performance.batch]
max_batch_size = 100
max_wait_ms = 50
```

## Monitoring

All performance components expose metrics:

```rust
// Connection pool metrics
let pool_stats = pool.stats();
metrics.gauge("pool.active", pool_stats.active);
metrics.gauge("pool.idle", pool_stats.idle);
metrics.counter("pool.timeouts", pool_stats.timeouts);

// Circuit breaker metrics
let breaker_stats = breaker.stats();
metrics.gauge("breaker.state", breaker_stats.state as i64);
metrics.counter("breaker.failures", breaker_stats.failures);
metrics.counter("breaker.successes", breaker_stats.successes);
```

## See Also

- [Security Module](./security.md)
- [Benchmarks](../benchmarks/README.md)
- [Configuration](./getting-started/configuration.md)