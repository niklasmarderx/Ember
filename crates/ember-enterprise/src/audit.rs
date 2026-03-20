//! Audit logging module - Comprehensive enterprise audit trail
//!
//! Provides detailed audit logging for compliance and security:
//! - All user actions tracked
//! - Configurable retention
//! - Export capabilities
//! - Real-time streaming

use crate::{AuditConfig, EnterpriseError, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Audit log level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditLevel {
    /// Debug level - verbose logging
    Debug,
    /// Info level - normal operations
    Info,
    /// Warning level - potential issues
    Warning,
    /// Error level - errors and failures
    Error,
    /// Critical level - security events
    Critical,
}

impl Default for AuditLevel {
    fn default() -> Self {
        Self::Info
    }
}

impl std::fmt::Display for AuditLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditLevel::Debug => write!(f, "DEBUG"),
            AuditLevel::Info => write!(f, "INFO"),
            AuditLevel::Warning => write!(f, "WARN"),
            AuditLevel::Error => write!(f, "ERROR"),
            AuditLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Audit entry - a single audit log record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique entry ID
    pub id: Uuid,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Log level
    pub level: AuditLevel,
    /// Category (auth, api, tool, config, etc.)
    pub category: String,
    /// Action performed
    pub action: String,
    /// Description
    pub description: String,
    /// User ID (if applicable)
    pub user_id: Option<Uuid>,
    /// Session ID (if applicable)
    pub session_id: Option<Uuid>,
    /// Resource type (if applicable)
    pub resource_type: Option<String>,
    /// Resource ID (if applicable)
    pub resource_id: Option<String>,
    /// IP address
    pub ip_address: Option<String>,
    /// User agent
    pub user_agent: Option<String>,
    /// Request ID (for correlation)
    pub request_id: Option<String>,
    /// Duration in milliseconds
    pub duration_ms: Option<u64>,
    /// Success/failure status
    pub success: bool,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// Request body (if configured)
    pub request_body: Option<serde_json::Value>,
    /// Response body (if configured)
    pub response_body: Option<serde_json::Value>,
}

impl AuditEntry {
    /// Create a new audit entry
    pub fn new(level: AuditLevel, category: &str, action: &str, description: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            level,
            category: category.to_string(),
            action: action.to_string(),
            description,
            user_id: None,
            session_id: None,
            resource_type: None,
            resource_id: None,
            ip_address: None,
            user_agent: None,
            request_id: None,
            duration_ms: None,
            success: true,
            error: None,
            metadata: HashMap::new(),
            request_body: None,
            response_body: None,
        }
    }

    /// Set user ID
    pub fn with_user_id(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Set session ID
    pub fn with_session_id(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Set resource
    pub fn with_resource(mut self, resource_type: &str, resource_id: &str) -> Self {
        self.resource_type = Some(resource_type.to_string());
        self.resource_id = Some(resource_id.to_string());
        self
    }

    /// Set IP address
    pub fn with_ip(mut self, ip: &str) -> Self {
        self.ip_address = Some(ip.to_string());
        self
    }

    /// Set user agent
    pub fn with_user_agent(mut self, user_agent: &str) -> Self {
        self.user_agent = Some(user_agent.to_string());
        self
    }

    /// Set request ID
    pub fn with_request_id(mut self, request_id: &str) -> Self {
        self.request_id = Some(request_id.to_string());
        self
    }

    /// Set duration
    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    /// Set success status
    pub fn with_success(mut self, success: bool) -> Self {
        self.success = success;
        self
    }

    /// Set error
    pub fn with_error(mut self, error: &str) -> Self {
        self.success = false;
        self.error = Some(error.to_string());
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }

    /// Set request body
    pub fn with_request_body(mut self, body: serde_json::Value) -> Self {
        self.request_body = Some(body);
        self
    }

    /// Set response body
    pub fn with_response_body(mut self, body: serde_json::Value) -> Self {
        self.response_body = Some(body);
        self
    }

    /// Create an info level entry
    pub fn info(category: &str, action: &str, description: &str) -> Self {
        Self::new(AuditLevel::Info, category, action, description.to_string())
    }

    /// Create a warning level entry
    pub fn warning(category: &str, action: &str, description: &str) -> Self {
        Self::new(AuditLevel::Warning, category, action, description.to_string())
    }

    /// Create an error level entry
    pub fn error(category: &str, action: &str, description: &str) -> Self {
        Self::new(AuditLevel::Error, category, action, description.to_string())
            .with_success(false)
    }

    /// Create a critical level entry
    pub fn critical(category: &str, action: &str, description: &str) -> Self {
        Self::new(AuditLevel::Critical, category, action, description.to_string())
    }
}

/// Query parameters for searching audit logs
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditQuery {
    /// Filter by user ID
    pub user_id: Option<Uuid>,
    /// Filter by session ID
    pub session_id: Option<Uuid>,
    /// Filter by category
    pub category: Option<String>,
    /// Filter by action
    pub action: Option<String>,
    /// Filter by minimum level
    pub min_level: Option<AuditLevel>,
    /// Filter by success status
    pub success: Option<bool>,
    /// Filter by start time
    pub start_time: Option<DateTime<Utc>>,
    /// Filter by end time
    pub end_time: Option<DateTime<Utc>>,
    /// Filter by resource type
    pub resource_type: Option<String>,
    /// Filter by resource ID
    pub resource_id: Option<String>,
    /// Filter by IP address
    pub ip_address: Option<String>,
    /// Search in description
    pub search: Option<String>,
    /// Limit results
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
    /// Sort order (ascending if true)
    pub ascending: bool,
}

impl AuditQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn user(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn category(mut self, category: &str) -> Self {
        self.category = Some(category.to_string());
        self
    }

    pub fn action(mut self, action: &str) -> Self {
        self.action = Some(action.to_string());
        self
    }

    pub fn min_level(mut self, level: AuditLevel) -> Self {
        self.min_level = Some(level);
        self
    }

    pub fn time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }

    pub fn last_hours(mut self, hours: i64) -> Self {
        self.start_time = Some(Utc::now() - Duration::hours(hours));
        self.end_time = Some(Utc::now());
        self
    }

    pub fn last_days(mut self, days: i64) -> Self {
        self.start_time = Some(Utc::now() - Duration::days(days));
        self.end_time = Some(Utc::now());
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn search(mut self, text: &str) -> Self {
        self.search = Some(text.to_string());
        self
    }

    /// Check if an entry matches this query
    pub fn matches(&self, entry: &AuditEntry) -> bool {
        // User filter
        if let Some(user_id) = self.user_id {
            if entry.user_id != Some(user_id) {
                return false;
            }
        }

        // Session filter
        if let Some(session_id) = self.session_id {
            if entry.session_id != Some(session_id) {
                return false;
            }
        }

        // Category filter
        if let Some(ref category) = self.category {
            if &entry.category != category {
                return false;
            }
        }

        // Action filter
        if let Some(ref action) = self.action {
            if &entry.action != action {
                return false;
            }
        }

        // Level filter
        if let Some(min_level) = self.min_level {
            if entry.level < min_level {
                return false;
            }
        }

        // Success filter
        if let Some(success) = self.success {
            if entry.success != success {
                return false;
            }
        }

        // Time range filter
        if let Some(start) = self.start_time {
            if entry.timestamp < start {
                return false;
            }
        }
        if let Some(end) = self.end_time {
            if entry.timestamp > end {
                return false;
            }
        }

        // Resource filter
        if let Some(ref resource_type) = self.resource_type {
            if entry.resource_type.as_ref() != Some(resource_type) {
                return false;
            }
        }
        if let Some(ref resource_id) = self.resource_id {
            if entry.resource_id.as_ref() != Some(resource_id) {
                return false;
            }
        }

        // IP filter
        if let Some(ref ip) = self.ip_address {
            if entry.ip_address.as_ref() != Some(ip) {
                return false;
            }
        }

        // Search filter
        if let Some(ref search) = self.search {
            let search_lower = search.to_lowercase();
            if !entry.description.to_lowercase().contains(&search_lower)
                && !entry.action.to_lowercase().contains(&search_lower)
                && !entry.category.to_lowercase().contains(&search_lower)
            {
                return false;
            }
        }

        true
    }
}

/// Audit log manager
pub struct AuditLog {
    config: AuditConfig,
    entries: Arc<RwLock<Vec<AuditEntry>>>,
    file_writer: Option<Arc<RwLock<BufWriter<File>>>>,
}

impl AuditLog {
    /// Create a new audit log
    pub fn new(config: AuditConfig) -> Result<Self> {
        let file_writer = if config.log_to_file {
            let path = config.log_path.as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    dirs::data_local_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join("ember")
                        .join("audit.log")
                });

            // Create parent directories
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| EnterpriseError::AuditError(format!("Failed to create log directory: {}", e)))?;
            }

            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|e| EnterpriseError::AuditError(format!("Failed to open log file: {}", e)))?;

            Some(Arc::new(RwLock::new(BufWriter::new(file))))
        } else {
            None
        };

        Ok(Self {
            config,
            entries: Arc::new(RwLock::new(Vec::new())),
            file_writer,
        })
    }

    /// Log an audit entry
    pub fn log(&mut self, entry: AuditEntry) -> Result<()> {
        // Check if logging is enabled
        if !self.config.enabled {
            return Ok(());
        }

        // Check log level
        if entry.level < self.config.level {
            return Ok(());
        }

        // Mask PII if configured
        let entry = if self.config.mask_pii {
            self.mask_pii(entry)
        } else {
            entry
        };

        // Write to file if configured
        if let Some(ref writer) = self.file_writer {
            let log_line = self.format_log_line(&entry);
            let writer_clone = writer.clone();
            
            // Write synchronously for simplicity
            if let Ok(mut w) = writer_clone.try_write() {
                let _ = writeln!(w, "{}", log_line);
                let _ = w.flush();
            }
        }

        // Store in memory
        let entries = self.entries.clone();
        let entry_clone = entry.clone();
        tokio::spawn(async move {
            let mut entries = entries.write().await;
            entries.push(entry_clone);
            
            // Limit memory usage - keep last 10000 entries
            if entries.len() > 10000 {
                entries.drain(0..1000);
            }
        });

        // Log to tracing
        match entry.level {
            AuditLevel::Debug => tracing::debug!("[AUDIT] {} - {}: {}", entry.category, entry.action, entry.description),
            AuditLevel::Info => tracing::info!("[AUDIT] {} - {}: {}", entry.category, entry.action, entry.description),
            AuditLevel::Warning => tracing::warn!("[AUDIT] {} - {}: {}", entry.category, entry.action, entry.description),
            AuditLevel::Error => tracing::error!("[AUDIT] {} - {}: {}", entry.category, entry.action, entry.description),
            AuditLevel::Critical => tracing::error!("[AUDIT-CRITICAL] {} - {}: {}", entry.category, entry.action, entry.description),
        }

        Ok(())
    }

    /// Query audit logs
    pub async fn query(&self, query: &AuditQuery) -> Result<Vec<AuditEntry>> {
        let entries = self.entries.read().await;
        
        let mut results: Vec<AuditEntry> = entries
            .iter()
            .filter(|e| query.matches(e))
            .cloned()
            .collect();

        // Sort
        if query.ascending {
            results.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        } else {
            results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        }

        // Apply offset and limit
        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(1000);
        
        Ok(results.into_iter().skip(offset).take(limit).collect())
    }

    /// Get entry by ID
    pub async fn get(&self, id: Uuid) -> Option<AuditEntry> {
        let entries = self.entries.read().await;
        entries.iter().find(|e| e.id == id).cloned()
    }

    /// Count entries matching query
    pub async fn count(&self, query: &AuditQuery) -> usize {
        let entries = self.entries.read().await;
        entries.iter().filter(|e| query.matches(e)).count()
    }

    /// Export audit logs to JSON
    pub async fn export_json(&self, query: &AuditQuery) -> Result<String> {
        let entries = self.query(query).await?;
        serde_json::to_string_pretty(&entries)
            .map_err(|e| EnterpriseError::AuditError(format!("Failed to serialize: {}", e)))
    }

    /// Export audit logs to CSV
    pub async fn export_csv(&self, query: &AuditQuery) -> Result<String> {
        let entries = self.query(query).await?;
        
        let mut csv = String::new();
        csv.push_str("id,timestamp,level,category,action,description,user_id,session_id,success,error,ip_address\n");
        
        for entry in entries {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{}\n",
                entry.id,
                entry.timestamp.to_rfc3339(),
                entry.level,
                escape_csv(&entry.category),
                escape_csv(&entry.action),
                escape_csv(&entry.description),
                entry.user_id.map(|u| u.to_string()).unwrap_or_default(),
                entry.session_id.map(|s| s.to_string()).unwrap_or_default(),
                entry.success,
                entry.error.as_deref().map(escape_csv).unwrap_or_default(),
                entry.ip_address.as_deref().unwrap_or(""),
            ));
        }
        
        Ok(csv)
    }

    /// Get statistics
    pub async fn stats(&self, query: &AuditQuery) -> AuditStats {
        let entries = self.entries.read().await;
        let filtered: Vec<_> = entries.iter().filter(|e| query.matches(e)).collect();
        
        let mut stats = AuditStats::default();
        stats.total = filtered.len();
        
        for entry in &filtered {
            // Count by level
            match entry.level {
                AuditLevel::Debug => stats.by_level.debug += 1,
                AuditLevel::Info => stats.by_level.info += 1,
                AuditLevel::Warning => stats.by_level.warning += 1,
                AuditLevel::Error => stats.by_level.error += 1,
                AuditLevel::Critical => stats.by_level.critical += 1,
            }
            
            // Count by category
            *stats.by_category.entry(entry.category.clone()).or_insert(0) += 1;
            
            // Count successes/failures
            if entry.success {
                stats.successes += 1;
            } else {
                stats.failures += 1;
            }
        }
        
        stats
    }

    /// Purge old entries
    pub async fn purge_old(&mut self, older_than: DateTime<Utc>) -> usize {
        let mut entries = self.entries.write().await;
        let initial_len = entries.len();
        entries.retain(|e| e.timestamp >= older_than);
        initial_len - entries.len()
    }

    /// Apply retention policy
    pub async fn apply_retention(&mut self) -> usize {
        let cutoff = Utc::now() - Duration::days(self.config.retention_days as i64);
        self.purge_old(cutoff).await
    }

    /// Clear all entries
    pub async fn clear(&mut self) {
        let mut entries = self.entries.write().await;
        entries.clear();
    }

    // Internal methods

    fn mask_pii(&self, mut entry: AuditEntry) -> AuditEntry {
        // Mask email addresses in description
        let email_regex = regex::Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}")
            .unwrap();
        entry.description = email_regex.replace_all(&entry.description, "[EMAIL]").to_string();
        
        // Mask IP addresses (last octet)
        if let Some(ref ip) = entry.ip_address {
            if let Some(last_dot) = ip.rfind('.') {
                entry.ip_address = Some(format!("{}.xxx", &ip[..last_dot]));
            }
        }
        
        // Remove sensitive metadata keys
        let sensitive_keys = ["password", "token", "secret", "api_key", "credit_card"];
        for key in sensitive_keys {
            if entry.metadata.contains_key(key) {
                entry.metadata.insert(key.to_string(), serde_json::json!("[REDACTED]"));
            }
        }
        
        entry
    }

    fn format_log_line(&self, entry: &AuditEntry) -> String {
        format!(
            "{} [{}] {} - {}: {} | user={} success={} ip={}",
            entry.timestamp.to_rfc3339(),
            entry.level,
            entry.category,
            entry.action,
            entry.description,
            entry.user_id.map(|u| u.to_string()).unwrap_or_else(|| "-".to_string()),
            entry.success,
            entry.ip_address.as_deref().unwrap_or("-"),
        )
    }
}

/// Audit statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditStats {
    /// Total entries
    pub total: usize,
    /// Successes
    pub successes: usize,
    /// Failures
    pub failures: usize,
    /// Counts by level
    pub by_level: LevelCounts,
    /// Counts by category
    pub by_category: HashMap<String, usize>,
}

/// Counts by log level
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LevelCounts {
    pub debug: usize,
    pub info: usize,
    pub warning: usize,
    pub error: usize,
    pub critical: usize,
}

// Helper function to escape CSV values
fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Predefined audit categories
pub mod categories {
    pub const AUTH: &str = "auth";
    pub const API: &str = "api";
    pub const TOOL: &str = "tool";
    pub const CONFIG: &str = "config";
    pub const USER: &str = "user";
    pub const TEAM: &str = "team";
    pub const RBAC: &str = "rbac";
    pub const SYSTEM: &str = "system";
    pub const DATA: &str = "data";
    pub const SECURITY: &str = "security";
}

/// Predefined audit actions
pub mod actions {
    // Auth actions
    pub const LOGIN: &str = "login";
    pub const LOGOUT: &str = "logout";
    pub const LOGIN_FAILED: &str = "login_failed";
    pub const PASSWORD_CHANGE: &str = "password_change";
    pub const MFA_ENABLED: &str = "mfa_enabled";
    pub const MFA_DISABLED: &str = "mfa_disabled";
    pub const SESSION_CREATED: &str = "session_created";
    pub const SESSION_EXPIRED: &str = "session_expired";
    
    // User actions
    pub const USER_CREATED: &str = "user_created";
    pub const USER_UPDATED: &str = "user_updated";
    pub const USER_DELETED: &str = "user_deleted";
    pub const USER_SUSPENDED: &str = "user_suspended";
    pub const USER_ACTIVATED: &str = "user_activated";
    
    // Team actions
    pub const TEAM_CREATED: &str = "team_created";
    pub const TEAM_UPDATED: &str = "team_updated";
    pub const TEAM_DELETED: &str = "team_deleted";
    pub const MEMBER_ADDED: &str = "member_added";
    pub const MEMBER_REMOVED: &str = "member_removed";
    pub const ROLE_CHANGED: &str = "role_changed";
    
    // RBAC actions
    pub const PERMISSION_GRANTED: &str = "permission_granted";
    pub const PERMISSION_REVOKED: &str = "permission_revoked";
    pub const ROLE_ASSIGNED: &str = "role_assigned";
    pub const ROLE_UNASSIGNED: &str = "role_unassigned";
    pub const ACCESS_DENIED: &str = "access_denied";
    
    // API actions
    pub const API_REQUEST: &str = "api_request";
    pub const API_ERROR: &str = "api_error";
    pub const RATE_LIMITED: &str = "rate_limited";
    
    // Tool actions
    pub const TOOL_EXECUTED: &str = "tool_executed";
    pub const TOOL_FAILED: &str = "tool_failed";
    
    // Config actions
    pub const CONFIG_CHANGED: &str = "config_changed";
    pub const SETTINGS_UPDATED: &str = "settings_updated";
    
    // Security actions
    pub const SECURITY_ALERT: &str = "security_alert";
    pub const SUSPICIOUS_ACTIVITY: &str = "suspicious_activity";
    pub const IP_BLOCKED: &str = "ip_blocked";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_entry_creation() {
        let entry = AuditEntry::info("auth", "login", "User logged in");
        assert_eq!(entry.category, "auth");
        assert_eq!(entry.action, "login");
        assert!(entry.success);
    }

    #[test]
    fn test_audit_entry_builder() {
        let user_id = Uuid::new_v4();
        let entry = AuditEntry::info("auth", "login", "User logged in")
            .with_user_id(user_id)
            .with_ip("192.168.1.1")
            .with_duration(100);
        
        assert_eq!(entry.user_id, Some(user_id));
        assert_eq!(entry.ip_address, Some("192.168.1.1".to_string()));
        assert_eq!(entry.duration_ms, Some(100));
    }

    #[test]
    fn test_audit_query_matches() {
        let entry = AuditEntry::info("auth", "login", "User logged in");
        
        let query = AuditQuery::new().category("auth");
        assert!(query.matches(&entry));
        
        let query = AuditQuery::new().category("api");
        assert!(!query.matches(&entry));
    }

    #[test]
    fn test_audit_level_ordering() {
        assert!(AuditLevel::Debug < AuditLevel::Info);
        assert!(AuditLevel::Info < AuditLevel::Warning);
        assert!(AuditLevel::Warning < AuditLevel::Error);
        assert!(AuditLevel::Error < AuditLevel::Critical);
    }

    #[tokio::test]
    async fn test_audit_log_creation() {
        let config = AuditConfig {
            enabled: true,
            log_to_file: false,
            ..Default::default()
        };
        let log = AuditLog::new(config);
        assert!(log.is_ok());
    }

    #[test]
    fn test_escape_csv() {
        assert_eq!(escape_csv("simple"), "simple");
        assert_eq!(escape_csv("with,comma"), "\"with,comma\"");
        assert_eq!(escape_csv("with\"quote"), "\"with\"\"quote\"");
    }
}