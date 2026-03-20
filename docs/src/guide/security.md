# Security Module

Ember provides comprehensive security features to protect your AI agent applications. The security module includes input validation, rate limiting, audit logging, and a policy engine.

## Input Validation

The `InputValidator` helps sanitize and validate user inputs before processing:

```rust
use ember_core::security::{InputValidator, ValidationConfig};

// Create validator with custom config
let config = ValidationConfig {
    max_input_length: 10000,
    allow_html: false,
    allow_scripts: false,
    blocked_patterns: vec![
        r"(?i)drop\s+table".to_string(),
        r"(?i)<script".to_string(),
    ],
    required_patterns: vec![],
    custom_validators: vec![],
};

let validator = InputValidator::new(config);

// Validate input
match validator.validate("Hello, world!") {
    Ok(result) => {
        println!("Sanitized: {}", result.sanitized);
        if !result.warnings.is_empty() {
            println!("Warnings: {:?}", result.warnings);
        }
    }
    Err(e) => println!("Validation failed: {}", e),
}
```

### Validation Options

- **max_input_length**: Maximum allowed input length
- **allow_html**: Whether to allow HTML tags
- **allow_scripts**: Whether to allow script-like content
- **blocked_patterns**: Regex patterns that cause validation to fail
- **required_patterns**: Regex patterns that must be present
- **custom_validators**: Custom validation functions

## Rate Limiting

Protect your application from abuse with the built-in rate limiter:

```rust
use ember_core::security::{RateLimiter, RateLimitConfig};
use std::time::Duration;

let config = RateLimitConfig {
    requests_per_second: 10.0,
    burst_size: 20,
    enable_adaptive: true,
    min_rate: 1.0,
    max_rate: 100.0,
};

let limiter = RateLimiter::new(config);

// Check if request is allowed
if limiter.check("user_123") {
    // Process request
} else {
    // Rate limited - reject or queue
}

// Get remaining quota
let remaining = limiter.remaining("user_123");
println!("Remaining requests: {}", remaining);
```

### Adaptive Rate Limiting

When `enable_adaptive` is true, the rate limiter automatically adjusts based on system load and error rates.

## Audit Logging

Track all security-relevant events with the audit logger:

```rust
use ember_core::security::{AuditLogger, AuditEvent, AuditSeverity, AuditCategory};

let logger = AuditLogger::new("/var/log/ember/audit.log");

// Log an event
logger.log(AuditEvent {
    timestamp: chrono::Utc::now(),
    severity: AuditSeverity::Warning,
    category: AuditCategory::Authentication,
    action: "login_attempt".to_string(),
    actor: Some("user@example.com".to_string()),
    resource: Some("api/auth".to_string()),
    outcome: "failure".to_string(),
    details: serde_json::json!({
        "ip": "192.168.1.1",
        "reason": "invalid_password"
    }),
});
```

### Event Categories

- **Authentication**: Login, logout, token operations
- **Authorization**: Permission checks, access control
- **DataAccess**: Read/write operations on sensitive data
- **Configuration**: System configuration changes
- **ToolExecution**: AI tool invocations
- **SystemEvent**: System-level events

### Severity Levels

- **Debug**: Detailed debugging information
- **Info**: General operational events
- **Warning**: Potentially harmful situations
- **Error**: Error events
- **Critical**: Severe problems requiring immediate attention

## Security Policy Engine

Define and enforce security policies declaratively:

```rust
use ember_core::security::{
    PolicyEngine, SecurityPolicy, SecurityRule, 
    RuleCondition, ConditionOperator, RuleAction
};

let mut engine = PolicyEngine::new();

// Create a policy
let policy = SecurityPolicy {
    id: "tool_restrictions".to_string(),
    name: "Tool Execution Restrictions".to_string(),
    description: "Limit tool execution capabilities".to_string(),
    enabled: true,
    priority: 100,
    rules: vec![
        SecurityRule {
            id: "block_shell".to_string(),
            conditions: vec![
                RuleCondition {
                    field: "tool_name".to_string(),
                    operator: ConditionOperator::Equals,
                    value: serde_json::json!("shell"),
                },
                RuleCondition {
                    field: "user_role".to_string(),
                    operator: ConditionOperator::NotEquals,
                    value: serde_json::json!("admin"),
                },
            ],
            action: RuleAction::Deny,
            message: Some("Shell access requires admin role".to_string()),
        },
    ],
};

engine.add_policy(policy);

// Evaluate a request
let context = serde_json::json!({
    "tool_name": "shell",
    "user_role": "user",
    "command": "ls -la"
});

match engine.evaluate(&context) {
    Ok(result) => {
        if result.allowed {
            // Proceed with execution
        } else {
            println!("Denied: {:?}", result.denied_reasons);
        }
    }
    Err(e) => println!("Policy evaluation error: {}", e),
}
```

### Condition Operators

- **Equals**: Exact match
- **NotEquals**: Not equal
- **Contains**: String contains
- **StartsWith**: String starts with
- **EndsWith**: String ends with
- **GreaterThan**: Numeric comparison
- **LessThan**: Numeric comparison
- **In**: Value in array
- **NotIn**: Value not in array
- **Matches**: Regex match

### Rule Actions

- **Allow**: Permit the action
- **Deny**: Block the action
- **Audit**: Log the action (continue execution)
- **RateLimit**: Apply rate limiting

## Best Practices

1. **Defense in Depth**: Use multiple security layers together
2. **Least Privilege**: Grant minimal permissions needed
3. **Audit Everything**: Log all security-relevant events
4. **Validate Early**: Validate input as early as possible
5. **Fail Securely**: Default to denial when in doubt

## Configuration

Security settings can be configured in `ember.toml`:

```toml
[security]
enable_input_validation = true
max_input_length = 100000
enable_rate_limiting = true
requests_per_second = 10.0

[security.audit]
enabled = true
log_path = "/var/log/ember/audit.log"
retention_days = 90

[security.policies]
enabled = true
policy_dir = "/etc/ember/policies"
```

## See Also

- [Performance Module](./performance.md)
- [Tool Execution](./tools/overview.md)
- [Configuration](./getting-started/configuration.md)