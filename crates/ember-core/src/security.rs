//! # Security Module
//!
//! Comprehensive security features for the Ember framework including:
//! - Input validation and sanitization
//! - Rate limiting with token bucket algorithm
//! - Audit logging with structured events
//! - Security policies and rules engine

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use regex::Regex;

// ============================================================================
// Input Validation
// ============================================================================

/// Input validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    /// Maximum input length in characters
    pub max_input_length: usize,
    /// Maximum number of lines
    pub max_lines: usize,
    /// Allow code blocks in input
    pub allow_code_blocks: bool,
    /// Allow URLs in input
    pub allow_urls: bool,
    /// Allow file paths in input
    pub allow_file_paths: bool,
    /// Custom blocked patterns (regex)
    pub blocked_patterns: Vec<String>,
    /// Custom allowed patterns (regex)
    pub allowed_patterns: Vec<String>,
    /// Sanitize HTML entities
    pub sanitize_html: bool,
    /// Maximum nesting depth for structured input
    pub max_nesting_depth: usize,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_input_length: 100_000,
            max_lines: 5_000,
            allow_code_blocks: true,
            allow_urls: true,
            allow_file_paths: true,
            blocked_patterns: vec![],
            allowed_patterns: vec![],
            sanitize_html: true,
            max_nesting_depth: 10,
        }
    }
}

/// Validation error types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationError {
    /// Input exceeds maximum length
    InputTooLong { length: usize, max: usize },
    /// Input has too many lines
    TooManyLines { lines: usize, max: usize },
    /// Blocked pattern found
    BlockedPattern { pattern: String, position: usize },
    /// URL not allowed
    UrlNotAllowed { url: String },
    /// File path not allowed
    FilePathNotAllowed { path: String },
    /// Invalid encoding
    InvalidEncoding { details: String },
    /// Potential injection detected
    PotentialInjection { injection_type: String, content: String },
    /// Nesting depth exceeded
    NestingTooDeep { depth: usize, max: usize },
    /// Custom validation failed
    CustomValidation { message: String },
}

/// Result of input validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether validation passed
    pub is_valid: bool,
    /// List of validation errors
    pub errors: Vec<ValidationError>,
    /// Sanitized input (if sanitization was performed)
    pub sanitized_input: Option<String>,
    /// Warnings (non-blocking issues)
    pub warnings: Vec<String>,
    /// Metadata about the validation
    pub metadata: ValidationMetadata,
}

/// Metadata about validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationMetadata {
    /// Original input length
    pub original_length: usize,
    /// Sanitized input length
    pub sanitized_length: usize,
    /// Number of patterns checked
    pub patterns_checked: usize,
    /// Validation duration in microseconds
    pub duration_us: u64,
}

/// Input validator
pub struct InputValidator {
    config: ValidationConfig,
    blocked_regexes: Vec<Regex>,
    allowed_regexes: Vec<Regex>,
    url_regex: Regex,
    path_regex: Regex,
    injection_patterns: Vec<(String, Regex)>,
}

impl InputValidator {
    /// Create a new input validator
    pub fn new(config: ValidationConfig) -> Self {
        let blocked_regexes: Vec<Regex> = config
            .blocked_patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        let allowed_regexes: Vec<Regex> = config
            .allowed_patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        // URL detection regex
        let url_regex = Regex::new(
            r"https?://[^\s<>\[\]{}|\\^`]+"
        ).unwrap();

        // File path detection regex (Unix and Windows)
        let path_regex = Regex::new(
            r#"(?:^|[\s"'])(?:/[\w.-]+)+|(?:[A-Za-z]:)?\\[\w.-\\]+"#
        ).unwrap();

        // Common injection patterns
        let injection_patterns = vec![
            ("sql_injection".to_string(), Regex::new(
                r"(?i)(?:--|;|'|\bOR\b|\bAND\b|\bUNION\b|\bSELECT\b|\bDROP\b|\bINSERT\b|\bDELETE\b|\bUPDATE\b).*(?:--|;|')"
            ).unwrap()),
            ("command_injection".to_string(), Regex::new(
                r"(?:;|\||&&|\$\(|`)[^;|&`]*(?:rm|cat|wget|curl|bash|sh|python|perl|ruby|nc|netcat)"
            ).unwrap()),
            ("path_traversal".to_string(), Regex::new(
                r"(?:\.\.[\\/]){2,}|(?:\.\.[\\/]).*(?:etc|passwd|shadow|hosts)"
            ).unwrap()),
            ("xss".to_string(), Regex::new(
                r"<script[^>]*>|javascript:|on\w+\s*=|<iframe|<object|<embed"
            ).unwrap()),
        ];

        Self {
            config,
            blocked_regexes,
            allowed_regexes,
            url_regex,
            path_regex,
            injection_patterns,
        }
    }

    /// Validate input
    pub fn validate(&self, input: &str) -> ValidationResult {
        let start = Instant::now();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let original_length = input.len();

        // Check length
        if input.len() > self.config.max_input_length {
            errors.push(ValidationError::InputTooLong {
                length: input.len(),
                max: self.config.max_input_length,
            });
        }

        // Check line count
        let line_count = input.lines().count();
        if line_count > self.config.max_lines {
            errors.push(ValidationError::TooManyLines {
                lines: line_count,
                max: self.config.max_lines,
            });
        }

        // Check for blocked patterns
        for (i, regex) in self.blocked_regexes.iter().enumerate() {
            if let Some(m) = regex.find(input) {
                errors.push(ValidationError::BlockedPattern {
                    pattern: self.config.blocked_patterns.get(i)
                        .cloned()
                        .unwrap_or_default(),
                    position: m.start(),
                });
            }
        }

        // Check for URLs if not allowed
        if !self.config.allow_urls {
            for m in self.url_regex.find_iter(input) {
                errors.push(ValidationError::UrlNotAllowed {
                    url: m.as_str().to_string(),
                });
            }
        }

        // Check for file paths if not allowed
        if !self.config.allow_file_paths {
            for m in self.path_regex.find_iter(input) {
                errors.push(ValidationError::FilePathNotAllowed {
                    path: m.as_str().to_string(),
                });
            }
        }

        // Check for injection patterns
        for (injection_type, regex) in &self.injection_patterns {
            if let Some(m) = regex.find(input) {
                warnings.push(format!(
                    "Potential {} detected at position {}",
                    injection_type, m.start()
                ));
            }
        }

        // Sanitize if configured
        let sanitized_input = if self.config.sanitize_html {
            Some(self.sanitize_html(input))
        } else {
            None
        };

        let duration = start.elapsed();
        let sanitized_length = sanitized_input.as_ref()
            .map(|s| s.len())
            .unwrap_or(original_length);

        ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            sanitized_input,
            warnings,
            metadata: ValidationMetadata {
                original_length,
                sanitized_length,
                patterns_checked: self.blocked_regexes.len() + self.injection_patterns.len(),
                duration_us: duration.as_micros() as u64,
            },
        }
    }

    /// Sanitize HTML entities
    fn sanitize_html(&self, input: &str) -> String {
        input
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#x27;")
    }

    /// Validate and sanitize, returning sanitized input or error
    pub fn validate_and_sanitize(&self, input: &str) -> Result<String, Vec<ValidationError>> {
        let result = self.validate(input);
        if result.is_valid {
            Ok(result.sanitized_input.unwrap_or_else(|| input.to_string()))
        } else {
            Err(result.errors)
        }
    }
}

// ============================================================================
// Rate Limiting
// ============================================================================

/// Rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum requests per window
    pub max_requests: u32,
    /// Window duration in seconds
    pub window_seconds: u64,
    /// Burst allowance (extra requests allowed in short bursts)
    pub burst_allowance: u32,
    /// Whether to use sliding window
    pub sliding_window: bool,
    /// Penalty duration when limit exceeded (seconds)
    pub penalty_seconds: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window_seconds: 60,
            burst_allowance: 10,
            sliding_window: true,
            penalty_seconds: 60,
        }
    }
}

/// Token bucket for rate limiting
#[derive(Debug)]
struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
    penalty_until: Option<Instant>,
}

impl TokenBucket {
    fn new(max_tokens: f64, refill_rate: f64) -> Self {
        Self {
            tokens: max_tokens,
            max_tokens,
            refill_rate,
            last_refill: Instant::now(),
            penalty_until: None,
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;
    }

    fn try_consume(&mut self, tokens: f64) -> bool {
        // Check penalty
        if let Some(until) = self.penalty_until {
            if Instant::now() < until {
                return false;
            }
            self.penalty_until = None;
        }

        self.refill();
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    fn apply_penalty(&mut self, duration: Duration) {
        self.penalty_until = Some(Instant::now() + duration);
    }
}

/// Rate limit result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitResult {
    /// Whether the request is allowed
    pub allowed: bool,
    /// Remaining requests in current window
    pub remaining: u32,
    /// Seconds until rate limit resets
    pub reset_in_seconds: u64,
    /// Whether currently in penalty period
    pub in_penalty: bool,
    /// Retry-After header value (if rate limited)
    pub retry_after: Option<u64>,
}

/// Rate limiter with multiple strategies
pub struct RateLimiter {
    config: RateLimitConfig,
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            buckets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a request is allowed for a given key
    pub async fn check(&self, key: &str) -> RateLimitResult {
        let mut buckets = self.buckets.write().await;
        
        let refill_rate = self.config.max_requests as f64 / self.config.window_seconds as f64;
        let max_tokens = (self.config.max_requests + self.config.burst_allowance) as f64;
        
        let bucket = buckets.entry(key.to_string()).or_insert_with(|| {
            TokenBucket::new(max_tokens, refill_rate)
        });

        // Check penalty
        let in_penalty = bucket.penalty_until
            .map(|until| Instant::now() < until)
            .unwrap_or(false);

        if in_penalty {
            let retry_after = bucket.penalty_until
                .map(|until| until.duration_since(Instant::now()).as_secs())
                .unwrap_or(0);
            
            return RateLimitResult {
                allowed: false,
                remaining: 0,
                reset_in_seconds: retry_after,
                in_penalty: true,
                retry_after: Some(retry_after),
            };
        }

        let allowed = bucket.try_consume(1.0);
        let remaining = bucket.tokens.floor() as u32;
        let reset_in_seconds = if remaining == 0 {
            (1.0 / refill_rate).ceil() as u64
        } else {
            0
        };

        RateLimitResult {
            allowed,
            remaining,
            reset_in_seconds,
            in_penalty: false,
            retry_after: if allowed { None } else { Some(reset_in_seconds) },
        }
    }

    /// Record a request and apply penalty if limit exceeded
    pub async fn record(&self, key: &str) -> RateLimitResult {
        let result = self.check(key).await;
        
        if !result.allowed && !result.in_penalty {
            let mut buckets = self.buckets.write().await;
            if let Some(bucket) = buckets.get_mut(key) {
                bucket.apply_penalty(Duration::from_secs(self.config.penalty_seconds));
            }
        }
        
        result
    }

    /// Reset rate limit for a key
    pub async fn reset(&self, key: &str) {
        let mut buckets = self.buckets.write().await;
        buckets.remove(key);
    }

    /// Get current status for a key without consuming
    pub async fn status(&self, key: &str) -> Option<RateLimitResult> {
        let buckets = self.buckets.read().await;
        
        buckets.get(key).map(|bucket| {
            let in_penalty = bucket.penalty_until
                .map(|until| Instant::now() < until)
                .unwrap_or(false);
            
            let remaining = bucket.tokens.floor() as u32;
            
            RateLimitResult {
                allowed: !in_penalty && remaining > 0,
                remaining,
                reset_in_seconds: 0,
                in_penalty,
                retry_after: None,
            }
        })
    }

    /// Clean up old buckets
    pub async fn cleanup(&self, max_age: Duration) {
        let mut buckets = self.buckets.write().await;
        let now = Instant::now();
        
        buckets.retain(|_, bucket| {
            now.duration_since(bucket.last_refill) < max_age
        });
    }
}

// ============================================================================
// Audit Logging
// ============================================================================

/// Audit event severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditSeverity {
    /// Informational event
    Info,
    /// Warning event
    Warning,
    /// Error event
    Error,
    /// Critical security event
    Critical,
}

/// Audit event categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditCategory {
    /// Authentication events
    Authentication,
    /// Authorization events
    Authorization,
    /// Data access events
    DataAccess,
    /// Configuration changes
    Configuration,
    /// Tool execution
    ToolExecution,
    /// API calls
    ApiCall,
    /// Security violations
    SecurityViolation,
    /// Rate limiting
    RateLimiting,
    /// System events
    System,
}

/// Audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unique event ID
    pub id: String,
    /// Timestamp (ISO 8601)
    pub timestamp: String,
    /// Event severity
    pub severity: AuditSeverity,
    /// Event category
    pub category: AuditCategory,
    /// Event action/type
    pub action: String,
    /// Actor (user, agent, system)
    pub actor: String,
    /// Resource being accessed/modified
    pub resource: Option<String>,
    /// Event outcome
    pub outcome: AuditOutcome,
    /// Additional details
    pub details: HashMap<String, serde_json::Value>,
    /// Source IP address
    pub source_ip: Option<String>,
    /// User agent
    pub user_agent: Option<String>,
    /// Session ID
    pub session_id: Option<String>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

/// Audit event outcome
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditOutcome {
    /// Successful operation
    Success,
    /// Failed operation
    Failure { reason: String },
    /// Denied operation
    Denied { reason: String },
    /// Unknown/pending outcome
    Unknown,
}

/// Audit logger configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Enable audit logging
    pub enabled: bool,
    /// Minimum severity to log
    pub min_severity: AuditSeverity,
    /// Categories to log
    pub categories: Vec<AuditCategory>,
    /// Maximum events to keep in memory
    pub max_events: usize,
    /// Whether to log to file
    pub log_to_file: bool,
    /// Log file path
    pub log_file_path: Option<String>,
    /// Whether to include PII in logs
    pub include_pii: bool,
    /// Event retention days
    pub retention_days: u32,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_severity: AuditSeverity::Info,
            categories: vec![
                AuditCategory::Authentication,
                AuditCategory::Authorization,
                AuditCategory::SecurityViolation,
                AuditCategory::ToolExecution,
            ],
            max_events: 10_000,
            log_to_file: false,
            log_file_path: None,
            include_pii: false,
            retention_days: 90,
        }
    }
}

/// Audit logger
pub struct AuditLogger {
    config: AuditConfig,
    events: Arc<RwLock<Vec<AuditEvent>>>,
}

impl AuditLogger {
    /// Create a new audit logger
    pub fn new(config: AuditConfig) -> Self {
        Self {
            config,
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Log an audit event
    pub async fn log(&self, event: AuditEvent) {
        if !self.config.enabled {
            return;
        }

        // Check severity
        if !self.should_log_severity(event.severity) {
            return;
        }

        // Check category
        if !self.config.categories.contains(&event.category) {
            return;
        }

        // Redact PII if configured
        let event = if !self.config.include_pii {
            self.redact_pii(event)
        } else {
            event
        };

        // Add to in-memory store
        let mut events = self.events.write().await;
        events.push(event.clone());

        // Trim if over limit
        if events.len() > self.config.max_events {
            let excess = events.len() - self.config.max_events;
            events.drain(0..excess);
        }

        // Log to console for debugging
        tracing::info!(
            category = ?event.category,
            action = %event.action,
            actor = %event.actor,
            outcome = ?event.outcome,
            "Audit event"
        );
    }

    /// Create and log an event with builder pattern
    pub async fn log_event(
        &self,
        severity: AuditSeverity,
        category: AuditCategory,
        action: &str,
        actor: &str,
    ) -> AuditEventBuilder {
        AuditEventBuilder::new(self, severity, category, action.to_string(), actor.to_string())
    }

    /// Get events by category
    pub async fn get_events_by_category(&self, category: AuditCategory) -> Vec<AuditEvent> {
        let events = self.events.read().await;
        events.iter()
            .filter(|e| e.category == category)
            .cloned()
            .collect()
    }

    /// Get events by severity
    pub async fn get_events_by_severity(&self, severity: AuditSeverity) -> Vec<AuditEvent> {
        let events = self.events.read().await;
        events.iter()
            .filter(|e| e.severity == severity)
            .cloned()
            .collect()
    }

    /// Get recent events
    pub async fn get_recent_events(&self, count: usize) -> Vec<AuditEvent> {
        let events = self.events.read().await;
        events.iter()
            .rev()
            .take(count)
            .cloned()
            .collect()
    }

    /// Search events
    pub async fn search(
        &self,
        query: &AuditQuery,
    ) -> Vec<AuditEvent> {
        let events = self.events.read().await;
        events.iter()
            .filter(|e| self.matches_query(e, query))
            .cloned()
            .collect()
    }

    fn should_log_severity(&self, severity: AuditSeverity) -> bool {
        match (self.config.min_severity, severity) {
            (AuditSeverity::Critical, AuditSeverity::Critical) => true,
            (AuditSeverity::Error, AuditSeverity::Critical | AuditSeverity::Error) => true,
            (AuditSeverity::Warning, AuditSeverity::Critical | AuditSeverity::Error | AuditSeverity::Warning) => true,
            (AuditSeverity::Info, _) => true,
            _ => false,
        }
    }

    fn redact_pii(&self, mut event: AuditEvent) -> AuditEvent {
        // Redact common PII fields
        if let Some(ip) = &event.source_ip {
            // Redact last octet of IP
            let parts: Vec<&str> = ip.split('.').collect();
            if parts.len() == 4 {
                event.source_ip = Some(format!("{}.{}.{}.xxx", parts[0], parts[1], parts[2]));
            }
        }
        
        event
    }

    fn matches_query(&self, event: &AuditEvent, query: &AuditQuery) -> bool {
        if let Some(severity) = query.severity {
            if event.severity != severity {
                return false;
            }
        }
        if let Some(category) = query.category {
            if event.category != category {
                return false;
            }
        }
        if let Some(ref actor) = query.actor {
            if !event.actor.contains(actor) {
                return false;
            }
        }
        if let Some(ref action) = query.action {
            if !event.action.contains(action) {
                return false;
            }
        }
        true
    }

    /// Export events to JSON
    pub async fn export_json(&self) -> String {
        let events = self.events.read().await;
        serde_json::to_string_pretty(&*events).unwrap_or_default()
    }

    /// Clear all events
    pub async fn clear(&self) {
        let mut events = self.events.write().await;
        events.clear();
    }
}

/// Query parameters for audit search
#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    /// Filter by severity
    pub severity: Option<AuditSeverity>,
    /// Filter by category
    pub category: Option<AuditCategory>,
    /// Filter by actor (partial match)
    pub actor: Option<String>,
    /// Filter by action (partial match)
    pub action: Option<String>,
    /// Filter by time range start
    pub from_time: Option<String>,
    /// Filter by time range end
    pub to_time: Option<String>,
}

/// Builder for audit events
pub struct AuditEventBuilder<'a> {
    logger: &'a AuditLogger,
    event: AuditEvent,
}

impl<'a> AuditEventBuilder<'a> {
    fn new(
        logger: &'a AuditLogger,
        severity: AuditSeverity,
        category: AuditCategory,
        action: String,
        actor: String,
    ) -> Self {
        Self {
            logger,
            event: AuditEvent {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                severity,
                category,
                action,
                actor,
                resource: None,
                outcome: AuditOutcome::Unknown,
                details: HashMap::new(),
                source_ip: None,
                user_agent: None,
                session_id: None,
                correlation_id: None,
            },
        }
    }

    /// Set resource
    pub fn resource(mut self, resource: &str) -> Self {
        self.event.resource = Some(resource.to_string());
        self
    }

    /// Set outcome
    pub fn outcome(mut self, outcome: AuditOutcome) -> Self {
        self.event.outcome = outcome;
        self
    }

    /// Set success outcome
    pub fn success(mut self) -> Self {
        self.event.outcome = AuditOutcome::Success;
        self
    }

    /// Set failure outcome
    pub fn failure(mut self, reason: &str) -> Self {
        self.event.outcome = AuditOutcome::Failure { reason: reason.to_string() };
        self
    }

    /// Set denied outcome
    pub fn denied(mut self, reason: &str) -> Self {
        self.event.outcome = AuditOutcome::Denied { reason: reason.to_string() };
        self
    }

    /// Add detail
    pub fn detail(mut self, key: &str, value: impl Serialize) -> Self {
        self.event.details.insert(
            key.to_string(),
            serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        );
        self
    }

    /// Set source IP
    pub fn source_ip(mut self, ip: &str) -> Self {
        self.event.source_ip = Some(ip.to_string());
        self
    }

    /// Set user agent
    pub fn user_agent(mut self, ua: &str) -> Self {
        self.event.user_agent = Some(ua.to_string());
        self
    }

    /// Set session ID
    pub fn session_id(mut self, id: &str) -> Self {
        self.event.session_id = Some(id.to_string());
        self
    }

    /// Set correlation ID
    pub fn correlation_id(mut self, id: &str) -> Self {
        self.event.correlation_id = Some(id.to_string());
        self
    }

    /// Log the event
    pub async fn log(self) {
        self.logger.log(self.event).await;
    }
}

// ============================================================================
// Security Policy Engine
// ============================================================================

/// Security policy rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityRule {
    /// Rule ID
    pub id: String,
    /// Rule name
    pub name: String,
    /// Rule description
    pub description: String,
    /// Whether rule is enabled
    pub enabled: bool,
    /// Rule priority (lower = higher priority)
    pub priority: u32,
    /// Conditions for rule to match
    pub conditions: Vec<RuleCondition>,
    /// Action to take when rule matches
    pub action: RuleAction,
}

/// Rule condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleCondition {
    /// Field to check
    pub field: String,
    /// Operator
    pub operator: ConditionOperator,
    /// Value to compare
    pub value: String,
}

/// Condition operators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConditionOperator {
    /// Equals
    Equals,
    /// Not equals
    NotEquals,
    /// Contains
    Contains,
    /// Not contains
    NotContains,
    /// Matches regex
    Matches,
    /// Greater than (for numbers)
    GreaterThan,
    /// Less than (for numbers)
    LessThan,
    /// In list
    In,
    /// Not in list
    NotIn,
}

/// Rule action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleAction {
    /// Allow the request
    Allow,
    /// Deny the request
    Deny { message: String },
    /// Log and allow
    LogAndAllow { level: AuditSeverity },
    /// Rate limit
    RateLimit { config: RateLimitConfig },
    /// Require additional validation
    RequireValidation { validation_type: String },
    /// Transform the request
    Transform { transformations: Vec<String> },
}

/// Security policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// Policy ID
    pub id: String,
    /// Policy name
    pub name: String,
    /// Policy description
    pub description: String,
    /// Whether policy is enabled
    pub enabled: bool,
    /// Rules in this policy
    pub rules: Vec<SecurityRule>,
    /// Default action if no rules match
    pub default_action: RuleAction,
}

/// Security policy engine
pub struct PolicyEngine {
    policies: Arc<RwLock<Vec<SecurityPolicy>>>,
}

impl PolicyEngine {
    /// Create a new policy engine
    pub fn new() -> Self {
        Self {
            policies: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add a policy
    pub async fn add_policy(&self, policy: SecurityPolicy) {
        let mut policies = self.policies.write().await;
        policies.push(policy);
        // Sort by priority
        policies.sort_by(|a, b| {
            let a_priority = a.rules.first().map(|r| r.priority).unwrap_or(u32::MAX);
            let b_priority = b.rules.first().map(|r| r.priority).unwrap_or(u32::MAX);
            a_priority.cmp(&b_priority)
        });
    }

    /// Evaluate request against policies
    pub async fn evaluate(&self, context: &PolicyContext) -> PolicyResult {
        let policies = self.policies.read().await;
        
        for policy in policies.iter().filter(|p| p.enabled) {
            for rule in policy.rules.iter().filter(|r| r.enabled) {
                if self.matches_conditions(&rule.conditions, context) {
                    return PolicyResult {
                        policy_id: policy.id.clone(),
                        rule_id: Some(rule.id.clone()),
                        action: rule.action.clone(),
                        matched: true,
                    };
                }
            }
            
            // If no rules matched, use default action
            return PolicyResult {
                policy_id: policy.id.clone(),
                rule_id: None,
                action: policy.default_action.clone(),
                matched: false,
            };
        }

        // No policies - allow by default
        PolicyResult {
            policy_id: String::new(),
            rule_id: None,
            action: RuleAction::Allow,
            matched: false,
        }
    }

    fn matches_conditions(&self, conditions: &[RuleCondition], context: &PolicyContext) -> bool {
        conditions.iter().all(|cond| self.matches_condition(cond, context))
    }

    fn matches_condition(&self, condition: &RuleCondition, context: &PolicyContext) -> bool {
        let value = context.get_field(&condition.field);
        
        match &condition.operator {
            ConditionOperator::Equals => value == Some(&condition.value),
            ConditionOperator::NotEquals => value != Some(&condition.value),
            ConditionOperator::Contains => {
                value.map(|v| v.contains(&condition.value)).unwrap_or(false)
            }
            ConditionOperator::NotContains => {
                value.map(|v| !v.contains(&condition.value)).unwrap_or(true)
            }
            ConditionOperator::Matches => {
                if let (Some(v), Ok(re)) = (value, Regex::new(&condition.value)) {
                    re.is_match(v)
                } else {
                    false
                }
            }
            ConditionOperator::GreaterThan => {
                if let (Some(v), Ok(cond_num)) = (value.and_then(|v| v.parse::<f64>().ok()), condition.value.parse::<f64>()) {
                    v > cond_num
                } else {
                    false
                }
            }
            ConditionOperator::LessThan => {
                if let (Some(v), Ok(cond_num)) = (value.and_then(|v| v.parse::<f64>().ok()), condition.value.parse::<f64>()) {
                    v < cond_num
                } else {
                    false
                }
            }
            ConditionOperator::In => {
                let list: Vec<&str> = condition.value.split(',').collect();
                value.map(|v| list.contains(&v.as_str())).unwrap_or(false)
            }
            ConditionOperator::NotIn => {
                let list: Vec<&str> = condition.value.split(',').collect();
                value.map(|v| !list.contains(&v.as_str())).unwrap_or(true)
            }
        }
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Context for policy evaluation
#[derive(Debug, Clone, Default)]
pub struct PolicyContext {
    /// Request fields
    pub fields: HashMap<String, String>,
}

impl PolicyContext {
    /// Create a new policy context
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a field
    pub fn with_field(mut self, key: &str, value: &str) -> Self {
        self.fields.insert(key.to_string(), value.to_string());
        self
    }

    /// Get a field value
    pub fn get_field(&self, key: &str) -> Option<&String> {
        self.fields.get(key)
    }
}

/// Result of policy evaluation
#[derive(Debug, Clone)]
pub struct PolicyResult {
    /// Policy ID that matched
    pub policy_id: String,
    /// Rule ID that matched (if any)
    pub rule_id: Option<String>,
    /// Action to take
    pub action: RuleAction,
    /// Whether a rule explicitly matched
    pub matched: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_validation() {
        let validator = InputValidator::new(ValidationConfig::default());
        
        // Valid input
        let result = validator.validate("Hello, world!");
        assert!(result.is_valid);
        
        // Too long input
        let long_input = "x".repeat(200_000);
        let result = validator.validate(&long_input);
        assert!(!result.is_valid);
        assert!(matches!(&result.errors[0], ValidationError::InputTooLong { .. }));
    }

    #[test]
    fn test_html_sanitization() {
        let validator = InputValidator::new(ValidationConfig::default());
        let result = validator.validate("<script>alert('xss')</script>");
        assert!(result.is_valid);
        assert!(result.sanitized_input.is_some());
        assert!(!result.sanitized_input.unwrap().contains('<'));
    }

    #[tokio::test]
    async fn test_rate_limiting() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 3,
            window_seconds: 60,
            burst_allowance: 0,
            sliding_window: true,
            penalty_seconds: 60,
        });

        let key = "test_user";
        
        // First 3 requests should succeed
        for _ in 0..3 {
            let result = limiter.check(key).await;
            assert!(result.allowed);
        }
        
        // 4th request should fail
        let result = limiter.check(key).await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_audit_logging() {
        let logger = AuditLogger::new(AuditConfig::default());
        
        logger.log(AuditEvent {
            id: "test-1".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            severity: AuditSeverity::Info,
            category: AuditCategory::Authentication,
            action: "login".to_string(),
            actor: "user@example.com".to_string(),
            resource: None,
            outcome: AuditOutcome::Success,
            details: HashMap::new(),
            source_ip: Some("192.168.1.100".to_string()),
            user_agent: None,
            session_id: None,
            correlation_id: None,
        }).await;

        let events = logger.get_recent_events(10).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "login");
    }

    #[tokio::test]
    async fn test_policy_engine() {
        let engine = PolicyEngine::new();
        
        let policy = SecurityPolicy {
            id: "test-policy".to_string(),
            name: "Test Policy".to_string(),
            description: "Test security policy".to_string(),
            enabled: true,
            rules: vec![
                SecurityRule {
                    id: "block-admin".to_string(),
                    name: "Block Admin Access".to_string(),
                    description: "Block admin access from unknown IPs".to_string(),
                    enabled: true,
                    priority: 1,
                    conditions: vec![
                        RuleCondition {
                            field: "role".to_string(),
                            operator: ConditionOperator::Equals,
                            value: "admin".to_string(),
                        },
                        RuleCondition {
                            field: "ip".to_string(),
                            operator: ConditionOperator::NotIn,
                            value: "10.0.0.1,10.0.0.2".to_string(),
                        },
                    ],
                    action: RuleAction::Deny { message: "Admin access denied".to_string() },
                },
            ],
            default_action: RuleAction::Allow,
        };
        
        engine.add_policy(policy).await;
        
        // Test matching rule
        let context = PolicyContext::new()
            .with_field("role", "admin")
            .with_field("ip", "192.168.1.1");
        
        let result = engine.evaluate(&context).await;
        assert!(matches!(result.action, RuleAction::Deny { .. }));
        
        // Test non-matching (allowed)
        let context = PolicyContext::new()
            .with_field("role", "admin")
            .with_field("ip", "10.0.0.1");
        
        let result = engine.evaluate(&context).await;
        assert!(matches!(result.action, RuleAction::Allow));
    }
}