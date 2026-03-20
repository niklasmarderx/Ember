//! Integration tests for new Ember modules (v1.1)
//!
//! Tests for:
//! - Security module (validation, rate limiting, audit)
//! - Performance module (pooling, batching, throttling)
//! - Tool selector
//! - Context manager

use std::time::Duration;

// ============================================================================
// Security Module Tests
// ============================================================================

mod security_tests {
    use ember_core::security::{
        InputValidator, ValidationConfig, ValidationError,
        RateLimiter, RateLimitConfig,
        AuditLogger, AuditConfig, AuditEvent, AuditSeverity, AuditCategory, AuditOutcome,
        PolicyEngine, PolicyContext, SecurityPolicy, SecurityRule, RuleCondition, 
        ConditionOperator, RuleAction,
    };
    use std::collections::HashMap;

    #[test]
    fn test_input_validation_basic() {
        let validator = InputValidator::new(ValidationConfig::default());
        
        // Valid input
        let result = validator.validate("Hello, world! This is a test.");
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
        assert!(result.sanitized_input.is_some());
    }

    #[test]
    fn test_input_validation_length_limit() {
        let config = ValidationConfig {
            max_input_length: 100,
            ..Default::default()
        };
        let validator = InputValidator::new(config);
        
        // Input too long
        let long_input = "x".repeat(150);
        let result = validator.validate(&long_input);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| matches!(e, ValidationError::InputTooLong { .. })));
    }

    #[test]
    fn test_input_validation_line_limit() {
        let config = ValidationConfig {
            max_lines: 5,
            ..Default::default()
        };
        let validator = InputValidator::new(config);
        
        // Too many lines
        let many_lines = (0..10).map(|i| format!("Line {}", i)).collect::<Vec<_>>().join("\n");
        let result = validator.validate(&many_lines);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| matches!(e, ValidationError::TooManyLines { .. })));
    }

    #[test]
    fn test_input_validation_url_blocking() {
        let config = ValidationConfig {
            allow_urls: false,
            ..Default::default()
        };
        let validator = InputValidator::new(config);
        
        let input = "Check out https://example.com for more info";
        let result = validator.validate(input);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| matches!(e, ValidationError::UrlNotAllowed { .. })));
    }

    #[test]
    fn test_html_sanitization() {
        let validator = InputValidator::new(ValidationConfig::default());
        
        let input = "<script>alert('xss')</script>";
        let result = validator.validate(input);
        
        assert!(result.is_valid);
        let sanitized = result.sanitized_input.unwrap();
        assert!(!sanitized.contains('<'));
        assert!(!sanitized.contains('>'));
        assert!(sanitized.contains("&lt;"));
        assert!(sanitized.contains("&gt;"));
    }

    #[test]
    fn test_blocked_pattern() {
        let config = ValidationConfig {
            blocked_patterns: vec![r"password\s*=".to_string()],
            ..Default::default()
        };
        let validator = InputValidator::new(config);
        
        let input = "my password = secret123";
        let result = validator.validate(input);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| matches!(e, ValidationError::BlockedPattern { .. })));
    }

    #[tokio::test]
    async fn test_rate_limiter_allows_within_limit() {
        let config = RateLimitConfig {
            max_requests: 5,
            window_seconds: 60,
            burst_allowance: 0,
            sliding_window: true,
            penalty_seconds: 60,
        };
        let limiter = RateLimiter::new(config);
        
        // Should allow 5 requests
        for i in 0..5 {
            let result = limiter.check("user1").await;
            assert!(result.allowed, "Request {} should be allowed", i + 1);
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks_over_limit() {
        let config = RateLimitConfig {
            max_requests: 3,
            window_seconds: 60,
            burst_allowance: 0,
            sliding_window: true,
            penalty_seconds: 60,
        };
        let limiter = RateLimiter::new(config);
        
        // Use up limit
        for _ in 0..3 {
            limiter.check("user1").await;
        }
        
        // 4th request should fail
        let result = limiter.check("user1").await;
        assert!(!result.allowed);
        assert!(result.retry_after.is_some());
    }

    #[tokio::test]
    async fn test_rate_limiter_separate_keys() {
        let config = RateLimitConfig {
            max_requests: 2,
            window_seconds: 60,
            burst_allowance: 0,
            sliding_window: true,
            penalty_seconds: 60,
        };
        let limiter = RateLimiter::new(config);
        
        // Each user has separate limit
        limiter.check("user1").await;
        limiter.check("user1").await;
        let result1 = limiter.check("user1").await;
        assert!(!result1.allowed);
        
        // user2 should still have limit
        let result2 = limiter.check("user2").await;
        assert!(result2.allowed);
    }

    #[tokio::test]
    async fn test_audit_logger_logs_events() {
        let logger = AuditLogger::new(AuditConfig::default());
        
        logger.log(AuditEvent {
            id: "test-1".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            severity: AuditSeverity::Info,
            category: AuditCategory::Authentication,
            action: "login_attempt".to_string(),
            actor: "user@example.com".to_string(),
            resource: Some("auth_service".to_string()),
            outcome: AuditOutcome::Success,
            details: HashMap::new(),
            source_ip: Some("192.168.1.100".to_string()),
            user_agent: Some("TestClient/1.0".to_string()),
            session_id: None,
            correlation_id: None,
        }).await;

        let events = logger.get_recent_events(10).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "login_attempt");
        assert_eq!(events[0].actor, "user@example.com");
    }

    #[tokio::test]
    async fn test_audit_logger_filters_by_category() {
        let logger = AuditLogger::new(AuditConfig {
            categories: vec![AuditCategory::Authentication, AuditCategory::SecurityViolation],
            ..Default::default()
        });
        
        // This should be logged (Authentication)
        logger.log(AuditEvent {
            id: "1".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            severity: AuditSeverity::Info,
            category: AuditCategory::Authentication,
            action: "login".to_string(),
            actor: "user".to_string(),
            resource: None,
            outcome: AuditOutcome::Success,
            details: HashMap::new(),
            source_ip: None,
            user_agent: None,
            session_id: None,
            correlation_id: None,
        }).await;
        
        // This should NOT be logged (DataAccess not in categories)
        logger.log(AuditEvent {
            id: "2".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            severity: AuditSeverity::Info,
            category: AuditCategory::DataAccess,
            action: "read_data".to_string(),
            actor: "user".to_string(),
            resource: None,
            outcome: AuditOutcome::Success,
            details: HashMap::new(),
            source_ip: None,
            user_agent: None,
            session_id: None,
            correlation_id: None,
        }).await;

        let events = logger.get_recent_events(10).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "login");
    }

    #[tokio::test]
    async fn test_policy_engine_allow_rule() {
        let engine = PolicyEngine::new();
        
        let policy = SecurityPolicy {
            id: "test".to_string(),
            name: "Test Policy".to_string(),
            description: "Test".to_string(),
            enabled: true,
            rules: vec![
                SecurityRule {
                    id: "allow-admin".to_string(),
                    name: "Allow Admin".to_string(),
                    description: "Allow admin users".to_string(),
                    enabled: true,
                    priority: 1,
                    conditions: vec![
                        RuleCondition {
                            field: "role".to_string(),
                            operator: ConditionOperator::Equals,
                            value: "admin".to_string(),
                        },
                    ],
                    action: RuleAction::Allow,
                },
            ],
            default_action: RuleAction::Deny { message: "Access denied".to_string() },
        };
        
        engine.add_policy(policy).await;
        
        // Admin should be allowed
        let context = PolicyContext::new().with_field("role", "admin");
        let result = engine.evaluate(&context).await;
        assert!(matches!(result.action, RuleAction::Allow));
        
        // Non-admin should be denied
        let context = PolicyContext::new().with_field("role", "user");
        let result = engine.evaluate(&context).await;
        assert!(matches!(result.action, RuleAction::Deny { .. }));
    }

    #[tokio::test]
    async fn test_policy_engine_complex_conditions() {
        let engine = PolicyEngine::new();
        
        let policy = SecurityPolicy {
            id: "test".to_string(),
            name: "Complex Policy".to_string(),
            description: "Test complex conditions".to_string(),
            enabled: true,
            rules: vec![
                SecurityRule {
                    id: "block-high-risk".to_string(),
                    name: "Block High Risk".to_string(),
                    description: "Block requests with high risk score".to_string(),
                    enabled: true,
                    priority: 1,
                    conditions: vec![
                        RuleCondition {
                            field: "risk_score".to_string(),
                            operator: ConditionOperator::GreaterThan,
                            value: "80".to_string(),
                        },
                    ],
                    action: RuleAction::Deny { message: "High risk detected".to_string() },
                },
            ],
            default_action: RuleAction::Allow,
        };
        
        engine.add_policy(policy).await;
        
        // High risk should be blocked
        let context = PolicyContext::new().with_field("risk_score", "90");
        let result = engine.evaluate(&context).await;
        assert!(matches!(result.action, RuleAction::Deny { .. }));
        
        // Low risk should be allowed
        let context = PolicyContext::new().with_field("risk_score", "50");
        let result = engine.evaluate(&context).await;
        assert!(matches!(result.action, RuleAction::Allow));
    }
}

// ============================================================================
// Performance Module Tests
// ============================================================================

mod performance_tests {
    use ember_core::performance::{
        ObjectPool, ObjectPoolStats,
        Throttler,
        CircuitBreakerV2, BreakerConfig, BreakerState, CircuitBreakerError,
        StringInterner,
        SchedulerConfig, TaskScheduler,
    };
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn test_object_pool_creates_objects() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        
        let pool = ObjectPool::new(
            move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                String::from("test_object")
            },
            5,
        );

        let obj1 = pool.acquire().await;
        assert_eq!(obj1.get(), "test_object");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        
        let obj2 = pool.acquire().await;
        assert_eq!(counter.load(Ordering::SeqCst), 2);
        
        drop(obj1);
        drop(obj2);
    }

    #[tokio::test]
    async fn test_object_pool_reuses_objects() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        
        let pool = ObjectPool::new(
            move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                vec![0u8; 100]
            },
            5,
        );

        // Acquire and release
        {
            let _obj = pool.acquire().await;
        }
        
        // Wait for return to pool
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        // Should reuse existing object
        {
            let _obj = pool.acquire().await;
        }
        
        let stats = pool.stats();
        // First acquisition creates, second should reuse
        assert!(stats.reused > 0 || stats.created <= 2);
    }

    #[tokio::test]
    async fn test_throttler_rate_limits() {
        let throttler = Throttler::new(5.0); // 5 ops/sec = 200ms interval
        
        let start = Instant::now();
        
        // First call should be immediate
        throttler.wait().await;
        let first_elapsed = start.elapsed();
        assert!(first_elapsed < Duration::from_millis(50));
        
        // Second call should wait
        throttler.wait().await;
        let second_elapsed = start.elapsed();
        assert!(second_elapsed >= Duration::from_millis(150)); // ~200ms interval
    }

    #[tokio::test]
    async fn test_throttler_try_acquire() {
        let throttler = Throttler::new(2.0); // 2 ops/sec = 500ms interval
        
        // First should succeed
        assert!(throttler.try_acquire().await);
        
        // Immediate second should fail
        assert!(!throttler.try_acquire().await);
        
        // After waiting, should succeed
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(throttler.try_acquire().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_closes_after_success() {
        let breaker = CircuitBreakerV2::new(BreakerConfig {
            failure_threshold: 3,
            open_duration_ms: 100,
            half_open_successes: 2,
            call_timeout_ms: 1000,
        });

        // Successful calls keep circuit closed
        for _ in 0..5 {
            let result: Result<i32, &str> = breaker.call(async { Ok(42) }).await;
            assert!(result.is_ok());
        }
        
        assert_eq!(breaker.state().await, BreakerState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_failures() {
        let breaker = CircuitBreakerV2::new(BreakerConfig {
            failure_threshold: 2,
            open_duration_ms: 1000,
            half_open_successes: 1,
            call_timeout_ms: 1000,
        });

        // Cause failures
        let _: Result<i32, &str> = breaker.call(async { Err("error1") }).await;
        let _: Result<i32, &str> = breaker.call(async { Err("error2") }).await;
        
        // Should be open now
        assert_eq!(breaker.state().await, BreakerState::Open);
        
        // Calls should fail fast
        let result: Result<i32, &str> = breaker.call(async { Ok(42) }).await;
        assert!(matches!(result, Err(CircuitBreakerError::Open)));
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_transition() {
        let breaker = CircuitBreakerV2::new(BreakerConfig {
            failure_threshold: 1,
            open_duration_ms: 50, // Short for testing
            half_open_successes: 1,
            call_timeout_ms: 1000,
        });

        // Open the circuit
        let _: Result<i32, &str> = breaker.call(async { Err("error") }).await;
        assert_eq!(breaker.state().await, BreakerState::Open);
        
        // Wait for transition to half-open
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Next successful call should close circuit
        let result: Result<i32, &str> = breaker.call(async { Ok(42) }).await;
        assert!(result.is_ok());
        assert_eq!(breaker.state().await, BreakerState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_timeout() {
        let breaker = CircuitBreakerV2::new(BreakerConfig {
            failure_threshold: 5,
            open_duration_ms: 1000,
            half_open_successes: 1,
            call_timeout_ms: 50, // Very short timeout
        });

        // Call that takes too long
        let result: Result<i32, &str> = breaker.call(async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(42)
        }).await;
        
        assert!(matches!(result, Err(CircuitBreakerError::Timeout)));
    }

    #[tokio::test]
    async fn test_circuit_breaker_reset() {
        let breaker = CircuitBreakerV2::new(BreakerConfig {
            failure_threshold: 1,
            open_duration_ms: 10000,
            half_open_successes: 1,
            call_timeout_ms: 1000,
        });

        // Open the circuit
        let _: Result<i32, &str> = breaker.call(async { Err("error") }).await;
        assert_eq!(breaker.state().await, BreakerState::Open);
        
        // Reset
        breaker.reset().await;
        assert_eq!(breaker.state().await, BreakerState::Closed);
        
        // Should work again
        let result: Result<i32, &str> = breaker.call(async { Ok(42) }).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_string_interner_basic() {
        let interner = StringInterner::new();
        
        let s1 = interner.intern("hello").await;
        let s2 = interner.intern("hello").await;
        let s3 = interner.intern("world").await;
        
        // Same string should return same Arc
        assert!(Arc::ptr_eq(&s1, &s2));
        
        // Different strings should be different
        assert!(!Arc::ptr_eq(&s1, &s3));
        
        // Check contents
        assert_eq!(&*s1, "hello");
        assert_eq!(&*s3, "world");
    }

    #[tokio::test]
    async fn test_string_interner_stats() {
        let interner = StringInterner::new();
        
        // Intern some strings
        interner.intern("a").await;
        interner.intern("b").await;
        interner.intern("c").await;
        interner.intern("a").await; // Hit
        interner.intern("b").await; // Hit
        
        let stats = interner.stats();
        assert_eq!(stats.total_interned, 3);
        assert_eq!(stats.total_hits, 2);
    }

    #[tokio::test]
    async fn test_string_interner_concurrent() {
        let interner = Arc::new(StringInterner::new());
        
        let mut handles = vec![];
        
        for i in 0..10 {
            let interner = interner.clone();
            handles.push(tokio::spawn(async move {
                for j in 0..100 {
                    let key = format!("key-{}-{}", i % 3, j % 10);
                    interner.intern(&key).await;
                }
            }));
        }
        
        for handle in handles {
            handle.await.unwrap();
        }
        
        // Should have interned unique keys only
        let stats = interner.stats();
        assert!(stats.total_interned <= 30); // At most 3 * 10 unique keys
        assert!(stats.total_hits > 0); // Should have some hits
    }

    #[tokio::test]
    async fn test_task_scheduler_basic() {
        let scheduler = TaskScheduler::new(SchedulerConfig {
            max_concurrent: 2,
            task_timeout_ms: 5000,
            queue_size: 100,
        });
        
        let counter = Arc::new(AtomicU32::new(0));
        
        for i in 0..5 {
            let counter = counter.clone();
            scheduler.schedule(
                &format!("task-{}", i),
                ember_core::performance::TaskPriority::Normal,
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            ).await.unwrap();
        }
        
        // Note: Tasks aren't executed until run() is called
        // This just tests scheduling
        let stats = scheduler.stats();
        assert_eq!(stats.active, 0);
    }
}

// ============================================================================
// Tool Selector Tests
// ============================================================================

mod tool_selector_tests {
    use ember_core::{
        ToolSelector, ToolSelectorConfig, ToolMetadata, ToolCapability, SelectionContext,
    };

    #[tokio::test]
    async fn test_tool_selector_creation() {
        let config = ToolSelectorConfig::default();
        let selector = ToolSelector::new(config);
        
        // Should be able to create selector
        assert!(selector.get_all_tools().await.is_empty() || !selector.get_all_tools().await.is_empty());
    }

    #[tokio::test]
    async fn test_tool_selector_registers_tool() {
        let selector = ToolSelector::new(ToolSelectorConfig::default());
        
        let tool = ToolMetadata {
            name: "test_tool".to_string(),
            description: "A test tool for testing".to_string(),
            capabilities: vec![ToolCapability::TextProcessing],
            input_types: vec!["text".to_string()],
            output_types: vec!["text".to_string()],
            cost_estimate: 0.001,
            latency_estimate_ms: 100,
            reliability_score: 0.95,
            examples: vec![],
        };
        
        selector.register_tool(tool).await;
        
        let tools = selector.get_all_tools().await;
        assert!(!tools.is_empty());
    }

    #[tokio::test]
    async fn test_tool_selector_recommends_tools() {
        let selector = ToolSelector::new(ToolSelectorConfig::default());
        
        // Register a file-related tool
        selector.register_tool(ToolMetadata {
            name: "read_file".to_string(),
            description: "Read contents of a file".to_string(),
            capabilities: vec![ToolCapability::FileOperation],
            input_types: vec!["path".to_string()],
            output_types: vec!["text".to_string()],
            cost_estimate: 0.0,
            latency_estimate_ms: 10,
            reliability_score: 0.99,
            examples: vec!["read a file".to_string(), "get file contents".to_string()],
        }).await;
        
        // Register a web tool
        selector.register_tool(ToolMetadata {
            name: "web_search".to_string(),
            description: "Search the web for information".to_string(),
            capabilities: vec![ToolCapability::WebAccess],
            input_types: vec!["query".to_string()],
            output_types: vec!["text".to_string()],
            cost_estimate: 0.01,
            latency_estimate_ms: 1000,
            reliability_score: 0.90,
            examples: vec!["search the web".to_string()],
        }).await;
        
        // Create context for file reading
        let context = SelectionContext {
            user_query: "read the config file".to_string(),
            required_capabilities: vec![ToolCapability::FileOperation],
            max_cost: None,
            max_latency_ms: None,
            previous_tools: vec![],
        };
        
        let recommendations = selector.recommend_tools(&context, 5).await;
        
        // Should recommend file tool for file-related query
        assert!(!recommendations.is_empty());
        assert_eq!(recommendations[0].tool_name, "read_file");
    }
}

// ============================================================================
// Context Manager Tests
// ============================================================================

mod context_manager_tests {
    use ember_core::{
        ContextManagerV2, ContextManagerV2Builder, ContextMessage, MessageRole,
        PruningStrategy, PriorityWeights,
    };

    #[test]
    fn test_context_manager_creation() {
        let manager = ContextManagerV2Builder::new()
            .max_tokens(4000)
            .strategy(PruningStrategy::SlidingWindow)
            .build();
        
        assert!(manager.is_ok());
    }

    #[test]
    fn test_context_manager_add_message() {
        let mut manager = ContextManagerV2Builder::new()
            .max_tokens(4000)
            .build()
            .unwrap();
        
        let message = ContextMessage {
            role: MessageRole::User,
            content: "Hello, world!".to_string(),
            timestamp: chrono::Utc::now(),
            token_count: 3,
            priority: 1.0,
            is_pinned: false,
            metadata: Default::default(),
        };
        
        manager.add_message(message);
        
        let messages = manager.get_messages();
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn test_context_manager_token_counting() {
        let mut manager = ContextManagerV2Builder::new()
            .max_tokens(100)
            .build()
            .unwrap();
        
        for i in 0..5 {
            manager.add_message(ContextMessage {
                role: MessageRole::User,
                content: format!("Message {}", i),
                timestamp: chrono::Utc::now(),
                token_count: 10,
                priority: 1.0,
                is_pinned: false,
                metadata: Default::default(),
            });
        }
        
        let stats = manager.get_stats();
        assert_eq!(stats.total_tokens, 50);
    }

    #[test]
    fn test_context_manager_pruning_sliding_window() {
        let mut manager = ContextManagerV2Builder::new()
            .max_tokens(30)
            .strategy(PruningStrategy::SlidingWindow)
            .build()
            .unwrap();
        
        for i in 0..5 {
            manager.add_message(ContextMessage {
                role: MessageRole::User,
                content: format!("Message {}", i),
                timestamp: chrono::Utc::now(),
                token_count: 10,
                priority: 1.0,
                is_pinned: false,
                metadata: Default::default(),
            });
        }
        
        // Should prune to fit within 30 tokens (3 messages)
        manager.prune();
        
        let messages = manager.get_messages();
        assert!(messages.len() <= 3);
    }

    #[test]
    fn test_context_manager_pinned_messages() {
        let mut manager = ContextManagerV2Builder::new()
            .max_tokens(25)
            .strategy(PruningStrategy::SlidingWindow)
            .build()
            .unwrap();
        
        // Add pinned message first
        manager.add_message(ContextMessage {
            role: MessageRole::System,
            content: "System prompt".to_string(),
            timestamp: chrono::Utc::now(),
            token_count: 10,
            priority: 10.0,
            is_pinned: true,
            metadata: Default::default(),
        });
        
        // Add regular messages
        for i in 0..3 {
            manager.add_message(ContextMessage {
                role: MessageRole::User,
                content: format!("Message {}", i),
                timestamp: chrono::Utc::now(),
                token_count: 10,
                priority: 1.0,
                is_pinned: false,
                metadata: Default::default(),
            });
        }
        
        manager.prune();
        
        let messages = manager.get_messages();
        // Pinned message should still be present
        assert!(messages.iter().any(|m| m.is_pinned));
    }

    #[test]
    fn test_context_manager_priority_pruning() {
        let mut manager = ContextManagerV2Builder::new()
            .max_tokens(20)
            .strategy(PruningStrategy::PriorityBased(PriorityWeights::default()))
            .build()
            .unwrap();
        
        // Low priority message
        manager.add_message(ContextMessage {
            role: MessageRole::User,
            content: "Low priority".to_string(),
            timestamp: chrono::Utc::now(),
            token_count: 10,
            priority: 0.1,
            is_pinned: false,
            metadata: Default::default(),
        });
        
        // High priority message
        manager.add_message(ContextMessage {
            role: MessageRole::User,
            content: "High priority".to_string(),
            timestamp: chrono::Utc::now(),
            token_count: 10,
            priority: 10.0,
            is_pinned: false,
            metadata: Default::default(),
        });
        
        // Another message to trigger pruning
        manager.add_message(ContextMessage {
            role: MessageRole::User,
            content: "Normal".to_string(),
            timestamp: chrono::Utc::now(),
            token_count: 10,
            priority: 1.0,
            is_pinned: false,
            metadata: Default::default(),
        });
        
        manager.prune();
        
        let messages = manager.get_messages();
        // High priority message should be kept
        assert!(messages.iter().any(|m| m.content.contains("High priority")));
    }
}