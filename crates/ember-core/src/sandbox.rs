//! Security Sandbox Module
//!
//! Enterprise-grade security for tool execution that OpenClaw doesn't have!
//!
//! # Features
//! - **Capability-based Security**: Fine-grained permissions
//! - **Resource Limits**: CPU, memory, time limits
//! - **Path Restrictions**: Jail tools to specific directories
//! - **Network Control**: Block or allow specific domains
//! - **Command Filtering**: Whitelist/blacklist commands
//! - **Audit Logging**: Complete security event trail

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Security capability for tools.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// Read files from filesystem.
    FileRead,
    /// Write files to filesystem.
    FileWrite,
    /// Delete files from filesystem.
    FileDelete,
    /// Execute shell commands.
    ShellExecute,
    /// Access network.
    NetworkAccess,
    /// Access environment variables.
    EnvAccess,
    /// Spawn processes.
    ProcessSpawn,
    /// Access clipboard.
    ClipboardAccess,
    /// Access browser.
    BrowserAccess,
    /// Access database.
    DatabaseAccess,
    /// Execute code.
    CodeExecution,
    /// Custom capability.
    Custom(String),
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileRead => write!(f, "file:read"),
            Self::FileWrite => write!(f, "file:write"),
            Self::FileDelete => write!(f, "file:delete"),
            Self::ShellExecute => write!(f, "shell:execute"),
            Self::NetworkAccess => write!(f, "network:access"),
            Self::EnvAccess => write!(f, "env:access"),
            Self::ProcessSpawn => write!(f, "process:spawn"),
            Self::ClipboardAccess => write!(f, "clipboard:access"),
            Self::BrowserAccess => write!(f, "browser:access"),
            Self::DatabaseAccess => write!(f, "database:access"),
            Self::CodeExecution => write!(f, "code:execute"),
            Self::Custom(name) => write!(f, "custom:{}", name),
        }
    }
}

/// Security level presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityLevel {
    /// No restrictions (not recommended).
    None,
    /// Basic restrictions - block dangerous commands.
    Basic,
    /// Standard - require approval for sensitive operations.
    Standard,
    /// Strict - whitelist-only approach.
    Strict,
    /// Maximum - read-only, no network, no execution.
    Maximum,
}

impl Default for SecurityLevel {
    fn default() -> Self {
        Self::Standard
    }
}

/// Resource limits for tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum execution time.
    pub max_execution_time: Duration,
    /// Maximum memory usage in bytes.
    pub max_memory_bytes: usize,
    /// Maximum output size in bytes.
    pub max_output_bytes: usize,
    /// Maximum file size for operations.
    pub max_file_size_bytes: usize,
    /// Maximum number of files that can be modified.
    pub max_files_modified: usize,
    /// Maximum network requests per minute.
    pub max_network_requests_per_minute: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_execution_time: Duration::from_secs(30),
            max_memory_bytes: 100 * 1024 * 1024,   // 100 MB
            max_output_bytes: 1024 * 1024,         // 1 MB
            max_file_size_bytes: 10 * 1024 * 1024, // 10 MB
            max_files_modified: 10,
            max_network_requests_per_minute: 60,
        }
    }
}

/// Path restriction rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathRules {
    /// Allowed paths for read operations.
    pub read_allowed: Vec<PathBuf>,
    /// Allowed paths for write operations.
    pub write_allowed: Vec<PathBuf>,
    /// Blocked paths (takes precedence).
    pub blocked: Vec<PathBuf>,
}

impl Default for PathRules {
    fn default() -> Self {
        Self {
            read_allowed: vec![],
            write_allowed: vec![],
            blocked: vec![
                PathBuf::from("/etc"),
                PathBuf::from("/root"),
                PathBuf::from("/var"),
                PathBuf::from("/usr"),
                PathBuf::from("/bin"),
                PathBuf::from("/sbin"),
                PathBuf::from("/boot"),
                PathBuf::from("/sys"),
                PathBuf::from("/proc"),
                PathBuf::from("~/.ssh"),
                PathBuf::from("~/.gnupg"),
                PathBuf::from("~/.aws"),
                PathBuf::from("~/.kube"),
            ],
        }
    }
}

/// Network rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRules {
    /// Allowed domains.
    pub allowed_domains: Vec<String>,
    /// Blocked domains.
    pub blocked_domains: Vec<String>,
    /// Whether to allow localhost.
    pub allow_localhost: bool,
    /// Whether to allow any domain (if false, only allowed_domains work).
    pub allow_any: bool,
}

impl Default for NetworkRules {
    fn default() -> Self {
        Self {
            allowed_domains: vec![
                "api.openai.com".to_string(),
                "api.anthropic.com".to_string(),
                "api.github.com".to_string(),
            ],
            blocked_domains: vec!["*.onion".to_string(), "*.local".to_string()],
            allow_localhost: true,
            allow_any: false,
        }
    }
}

/// Command rules for shell execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRules {
    /// Allowed commands (whitelist).
    pub allowed: Vec<String>,
    /// Blocked commands (blacklist).
    pub blocked: Vec<String>,
    /// Blocked command arguments.
    pub blocked_args: Vec<String>,
    /// Whether to use whitelist mode (only allowed commands).
    pub whitelist_mode: bool,
}

impl Default for CommandRules {
    fn default() -> Self {
        Self {
            allowed: vec![
                "ls".to_string(),
                "cat".to_string(),
                "head".to_string(),
                "tail".to_string(),
                "grep".to_string(),
                "find".to_string(),
                "wc".to_string(),
                "echo".to_string(),
                "pwd".to_string(),
                "date".to_string(),
                "whoami".to_string(),
                "git".to_string(),
                "cargo".to_string(),
                "npm".to_string(),
                "node".to_string(),
                "python".to_string(),
                "python3".to_string(),
                "pip".to_string(),
                "pip3".to_string(),
            ],
            blocked: vec![
                "rm".to_string(),
                "sudo".to_string(),
                "su".to_string(),
                "chmod".to_string(),
                "chown".to_string(),
                "dd".to_string(),
                "mkfs".to_string(),
                "fdisk".to_string(),
                "mount".to_string(),
                "umount".to_string(),
                "kill".to_string(),
                "killall".to_string(),
                "reboot".to_string(),
                "shutdown".to_string(),
                "init".to_string(),
                "systemctl".to_string(),
                "service".to_string(),
                "curl".to_string(),
                "wget".to_string(),
                "nc".to_string(),
                "netcat".to_string(),
                "ssh".to_string(),
                "scp".to_string(),
                "rsync".to_string(),
                "eval".to_string(),
                "exec".to_string(),
            ],
            blocked_args: vec![
                "-rf".to_string(),
                "--force".to_string(),
                "--no-preserve-root".to_string(),
                "|".to_string(),
                ";".to_string(),
                "&&".to_string(),
                "$(".to_string(),
                "`".to_string(),
                ">".to_string(),
                ">>".to_string(),
                "<".to_string(),
            ],
            whitelist_mode: false,
        }
    }
}

/// Security configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Security level.
    pub level: SecurityLevel,
    /// Granted capabilities.
    pub capabilities: HashSet<Capability>,
    /// Resource limits.
    pub resource_limits: ResourceLimits,
    /// Path rules.
    pub path_rules: PathRules,
    /// Network rules.
    pub network_rules: NetworkRules,
    /// Command rules.
    pub command_rules: CommandRules,
    /// Whether to require user approval for sensitive operations.
    pub require_approval: bool,
    /// Whether to log all operations.
    pub audit_logging: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self::standard()
    }
}

impl SecurityConfig {
    /// Create a configuration with no restrictions.
    pub fn none() -> Self {
        let mut capabilities = HashSet::new();
        capabilities.insert(Capability::FileRead);
        capabilities.insert(Capability::FileWrite);
        capabilities.insert(Capability::FileDelete);
        capabilities.insert(Capability::ShellExecute);
        capabilities.insert(Capability::NetworkAccess);
        capabilities.insert(Capability::EnvAccess);
        capabilities.insert(Capability::ProcessSpawn);
        capabilities.insert(Capability::ClipboardAccess);
        capabilities.insert(Capability::BrowserAccess);
        capabilities.insert(Capability::DatabaseAccess);
        capabilities.insert(Capability::CodeExecution);

        Self {
            level: SecurityLevel::None,
            capabilities,
            resource_limits: ResourceLimits::default(),
            path_rules: PathRules::default(),
            network_rules: NetworkRules {
                allow_any: true,
                ..Default::default()
            },
            command_rules: CommandRules {
                whitelist_mode: false,
                blocked: vec![],
                ..Default::default()
            },
            require_approval: false,
            audit_logging: false,
        }
    }

    /// Create a basic security configuration.
    pub fn basic() -> Self {
        let mut capabilities = HashSet::new();
        capabilities.insert(Capability::FileRead);
        capabilities.insert(Capability::FileWrite);
        capabilities.insert(Capability::ShellExecute);
        capabilities.insert(Capability::NetworkAccess);
        capabilities.insert(Capability::BrowserAccess);

        Self {
            level: SecurityLevel::Basic,
            capabilities,
            resource_limits: ResourceLimits::default(),
            path_rules: PathRules::default(),
            network_rules: NetworkRules::default(),
            command_rules: CommandRules::default(),
            require_approval: false,
            audit_logging: true,
        }
    }

    /// Create a standard security configuration.
    pub fn standard() -> Self {
        let mut capabilities = HashSet::new();
        capabilities.insert(Capability::FileRead);
        capabilities.insert(Capability::FileWrite);
        capabilities.insert(Capability::ShellExecute);
        capabilities.insert(Capability::NetworkAccess);

        Self {
            level: SecurityLevel::Standard,
            capabilities,
            resource_limits: ResourceLimits::default(),
            path_rules: PathRules::default(),
            network_rules: NetworkRules::default(),
            command_rules: CommandRules::default(),
            require_approval: true,
            audit_logging: true,
        }
    }

    /// Create a strict security configuration.
    pub fn strict() -> Self {
        let mut capabilities = HashSet::new();
        capabilities.insert(Capability::FileRead);
        capabilities.insert(Capability::FileWrite);
        capabilities.insert(Capability::ShellExecute);

        Self {
            level: SecurityLevel::Strict,
            capabilities,
            resource_limits: ResourceLimits {
                max_execution_time: Duration::from_secs(10),
                max_files_modified: 5,
                ..Default::default()
            },
            path_rules: PathRules::default(),
            network_rules: NetworkRules {
                allow_any: false,
                ..Default::default()
            },
            command_rules: CommandRules {
                whitelist_mode: true,
                ..Default::default()
            },
            require_approval: true,
            audit_logging: true,
        }
    }

    /// Create a maximum security configuration (read-only).
    pub fn maximum() -> Self {
        let mut capabilities = HashSet::new();
        capabilities.insert(Capability::FileRead);

        Self {
            level: SecurityLevel::Maximum,
            capabilities,
            resource_limits: ResourceLimits {
                max_execution_time: Duration::from_secs(5),
                max_files_modified: 0,
                max_network_requests_per_minute: 0,
                ..Default::default()
            },
            path_rules: PathRules::default(),
            network_rules: NetworkRules {
                allow_any: false,
                allowed_domains: vec![],
                allow_localhost: false,
                ..Default::default()
            },
            command_rules: CommandRules {
                whitelist_mode: true,
                allowed: vec![
                    "ls".to_string(),
                    "cat".to_string(),
                    "head".to_string(),
                    "tail".to_string(),
                    "grep".to_string(),
                    "wc".to_string(),
                    "pwd".to_string(),
                ],
                ..Default::default()
            },
            require_approval: true,
            audit_logging: true,
        }
    }
}

/// Security event for audit logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEvent {
    /// Event timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Event type.
    pub event_type: SecurityEventType,
    /// Operation attempted.
    pub operation: String,
    /// Whether it was allowed.
    pub allowed: bool,
    /// Reason for decision.
    pub reason: String,
    /// Additional details.
    pub details: HashMap<String, String>,
}

/// Types of security events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityEventType {
    /// Capability check.
    CapabilityCheck,
    /// Path access.
    PathAccess,
    /// Network access.
    NetworkAccess,
    /// Command execution.
    CommandExecution,
    /// Resource limit.
    ResourceLimit,
    /// User approval.
    UserApproval,
}

/// Result of a security check.
#[derive(Debug, Clone)]
pub struct SecurityCheckResult {
    /// Whether the operation is allowed.
    pub allowed: bool,
    /// Whether user approval is required.
    pub requires_approval: bool,
    /// Reason for the decision.
    pub reason: String,
    /// Warnings (non-blocking issues).
    pub warnings: Vec<String>,
}

impl SecurityCheckResult {
    /// Create an allowed result.
    pub fn allowed() -> Self {
        Self {
            allowed: true,
            requires_approval: false,
            reason: "Operation allowed".to_string(),
            warnings: vec![],
        }
    }

    /// Create an allowed result with approval requirement.
    pub fn allowed_with_approval(reason: &str) -> Self {
        Self {
            allowed: true,
            requires_approval: true,
            reason: reason.to_string(),
            warnings: vec![],
        }
    }

    /// Create a denied result.
    pub fn denied(reason: &str) -> Self {
        Self {
            allowed: false,
            requires_approval: false,
            reason: reason.to_string(),
            warnings: vec![],
        }
    }

    /// Add a warning.
    pub fn with_warning(mut self, warning: &str) -> Self {
        self.warnings.push(warning.to_string());
        self
    }
}

/// The Security Sandbox.
///
/// Enforces security policies for tool execution.
pub struct SecuritySandbox {
    /// Configuration.
    config: SecurityConfig,
    /// Audit log.
    audit_log: Arc<RwLock<Vec<SecurityEvent>>>,
    /// Network request counter (for rate limiting).
    network_requests: Arc<RwLock<Vec<std::time::Instant>>>,
    /// Files modified counter.
    files_modified: Arc<RwLock<HashSet<PathBuf>>>,
}

impl SecuritySandbox {
    /// Create a new sandbox with default configuration.
    pub fn new() -> Self {
        Self::with_config(SecurityConfig::default())
    }

    /// Create a sandbox with custom configuration.
    pub fn with_config(config: SecurityConfig) -> Self {
        Self {
            config,
            audit_log: Arc::new(RwLock::new(Vec::new())),
            network_requests: Arc::new(RwLock::new(Vec::new())),
            files_modified: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Create a strict sandbox.
    pub fn strict() -> Self {
        Self::with_config(SecurityConfig::strict())
    }

    /// Create a maximum security sandbox.
    pub fn maximum() -> Self {
        Self::with_config(SecurityConfig::maximum())
    }

    /// Check if a capability is granted.
    pub async fn check_capability(&self, capability: &Capability) -> SecurityCheckResult {
        let allowed = self.config.capabilities.contains(capability);

        let result = if allowed {
            if self.config.require_approval && self.is_sensitive_capability(capability) {
                SecurityCheckResult::allowed_with_approval(&format!(
                    "Capability {} requires approval",
                    capability
                ))
            } else {
                SecurityCheckResult::allowed()
            }
        } else {
            SecurityCheckResult::denied(&format!("Capability {} not granted", capability))
        };

        // Log the check
        self.log_event(SecurityEvent {
            timestamp: chrono::Utc::now(),
            event_type: SecurityEventType::CapabilityCheck,
            operation: capability.to_string(),
            allowed: result.allowed,
            reason: result.reason.clone(),
            details: HashMap::new(),
        })
        .await;

        result
    }

    /// Check if a path access is allowed.
    pub async fn check_path(&self, path: &std::path::Path, write: bool) -> SecurityCheckResult {
        let path_str = path.to_string_lossy().to_string();

        // Check blocked paths first
        for blocked in &self.config.path_rules.blocked {
            let blocked_str = blocked.to_string_lossy();
            if path_str.starts_with(blocked_str.as_ref()) || path_str.contains(blocked_str.as_ref())
            {
                let result = SecurityCheckResult::denied(&format!(
                    "Path {} is blocked by security policy",
                    path_str
                ));

                self.log_event(SecurityEvent {
                    timestamp: chrono::Utc::now(),
                    event_type: SecurityEventType::PathAccess,
                    operation: format!("{}:{}", if write { "write" } else { "read" }, path_str),
                    allowed: false,
                    reason: result.reason.clone(),
                    details: HashMap::new(),
                })
                .await;

                return result;
            }
        }

        // Check allowed paths
        let allowed_paths = if write {
            &self.config.path_rules.write_allowed
        } else {
            &self.config.path_rules.read_allowed
        };

        if !allowed_paths.is_empty() {
            let in_allowed = allowed_paths.iter().any(|allowed| {
                let allowed_str = allowed.to_string_lossy();
                path_str.starts_with(allowed_str.as_ref())
            });

            if !in_allowed {
                return SecurityCheckResult::denied(&format!(
                    "Path {} not in allowed list",
                    path_str
                ));
            }
        }

        // Check file modification limit
        if write {
            let files = self.files_modified.read().await;
            if files.len() >= self.config.resource_limits.max_files_modified
                && !files.contains(path)
            {
                return SecurityCheckResult::denied(&format!(
                    "Maximum files modified limit ({}) reached",
                    self.config.resource_limits.max_files_modified
                ));
            }
        }

        let result = if self.config.require_approval && write {
            SecurityCheckResult::allowed_with_approval("File write requires approval")
        } else {
            SecurityCheckResult::allowed()
        };

        self.log_event(SecurityEvent {
            timestamp: chrono::Utc::now(),
            event_type: SecurityEventType::PathAccess,
            operation: format!("{}:{}", if write { "write" } else { "read" }, path_str),
            allowed: result.allowed,
            reason: result.reason.clone(),
            details: HashMap::new(),
        })
        .await;

        result
    }

    /// Check if a network request is allowed.
    pub async fn check_network(&self, domain: &str) -> SecurityCheckResult {
        // Check blocked domains
        for blocked in &self.config.network_rules.blocked_domains {
            if self.domain_matches(domain, blocked) {
                let result = SecurityCheckResult::denied(&format!("Domain {} is blocked", domain));

                self.log_event(SecurityEvent {
                    timestamp: chrono::Utc::now(),
                    event_type: SecurityEventType::NetworkAccess,
                    operation: domain.to_string(),
                    allowed: false,
                    reason: result.reason.clone(),
                    details: HashMap::new(),
                })
                .await;

                return result;
            }
        }

        // Check localhost
        if domain == "localhost" || domain == "127.0.0.1" || domain == "::1" {
            if !self.config.network_rules.allow_localhost {
                return SecurityCheckResult::denied("Localhost access is blocked");
            }
            return SecurityCheckResult::allowed();
        }

        // Check rate limit
        {
            let mut requests = self.network_requests.write().await;
            let now = std::time::Instant::now();
            let minute_ago = now - Duration::from_secs(60);

            // Remove old requests
            requests.retain(|t| *t > minute_ago);

            if requests.len() as u32 >= self.config.resource_limits.max_network_requests_per_minute
            {
                return SecurityCheckResult::denied("Network rate limit exceeded");
            }

            requests.push(now);
        }

        // Check allowed domains
        if !self.config.network_rules.allow_any {
            let in_allowed = self
                .config
                .network_rules
                .allowed_domains
                .iter()
                .any(|allowed| self.domain_matches(domain, allowed));

            if !in_allowed {
                return SecurityCheckResult::denied(&format!(
                    "Domain {} not in allowed list",
                    domain
                ));
            }
        }

        let result = SecurityCheckResult::allowed();

        self.log_event(SecurityEvent {
            timestamp: chrono::Utc::now(),
            event_type: SecurityEventType::NetworkAccess,
            operation: domain.to_string(),
            allowed: true,
            reason: result.reason.clone(),
            details: HashMap::new(),
        })
        .await;

        result
    }

    /// Check if a command is allowed.
    pub async fn check_command(&self, command: &str) -> SecurityCheckResult {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let cmd = parts.first().unwrap_or(&"");

        // Check blocked arguments
        for blocked_arg in &self.config.command_rules.blocked_args {
            if command.contains(blocked_arg) {
                let result = SecurityCheckResult::denied(&format!(
                    "Command contains blocked argument: {}",
                    blocked_arg
                ));

                self.log_event(SecurityEvent {
                    timestamp: chrono::Utc::now(),
                    event_type: SecurityEventType::CommandExecution,
                    operation: command.to_string(),
                    allowed: false,
                    reason: result.reason.clone(),
                    details: HashMap::new(),
                })
                .await;

                return result;
            }
        }

        // Check blocked commands
        for blocked in &self.config.command_rules.blocked {
            if cmd == blocked || cmd.ends_with(&format!("/{}", blocked)) {
                let result = SecurityCheckResult::denied(&format!("Command {} is blocked", cmd));

                self.log_event(SecurityEvent {
                    timestamp: chrono::Utc::now(),
                    event_type: SecurityEventType::CommandExecution,
                    operation: command.to_string(),
                    allowed: false,
                    reason: result.reason.clone(),
                    details: HashMap::new(),
                })
                .await;

                return result;
            }
        }

        // Check whitelist mode
        if self.config.command_rules.whitelist_mode {
            let in_allowed = self
                .config
                .command_rules
                .allowed
                .iter()
                .any(|allowed| cmd == allowed || cmd.ends_with(&format!("/{}", allowed)));

            if !in_allowed {
                return SecurityCheckResult::denied(&format!("Command {} not in whitelist", cmd));
            }
        }

        let result = if self.config.require_approval {
            SecurityCheckResult::allowed_with_approval("Command execution requires approval")
        } else {
            SecurityCheckResult::allowed()
        };

        self.log_event(SecurityEvent {
            timestamp: chrono::Utc::now(),
            event_type: SecurityEventType::CommandExecution,
            operation: command.to_string(),
            allowed: result.allowed,
            reason: result.reason.clone(),
            details: HashMap::new(),
        })
        .await;

        result
    }

    /// Get resource limits.
    pub fn resource_limits(&self) -> &ResourceLimits {
        &self.config.resource_limits
    }

    /// Get the security level.
    pub fn security_level(&self) -> SecurityLevel {
        self.config.level
    }

    /// Get audit log entries.
    pub async fn get_audit_log(&self, limit: usize) -> Vec<SecurityEvent> {
        let log = self.audit_log.read().await;
        log.iter().rev().take(limit).cloned().collect()
    }

    /// Record a file modification.
    pub async fn record_file_modified(&self, path: &std::path::Path) {
        let mut files = self.files_modified.write().await;
        files.insert(path.to_path_buf());
    }

    /// Reset file modification counter.
    pub async fn reset_file_counter(&self) {
        let mut files = self.files_modified.write().await;
        files.clear();
    }

    /// Check if a capability is sensitive.
    fn is_sensitive_capability(&self, capability: &Capability) -> bool {
        matches!(
            capability,
            Capability::FileWrite
                | Capability::FileDelete
                | Capability::ShellExecute
                | Capability::NetworkAccess
                | Capability::ProcessSpawn
                | Capability::CodeExecution
        )
    }

    /// Check if a domain matches a pattern.
    fn domain_matches(&self, domain: &str, pattern: &str) -> bool {
        if pattern.starts_with("*.") {
            let suffix = &pattern[2..];
            domain.ends_with(suffix) || domain == &suffix[1..]
        } else {
            domain == pattern || domain.ends_with(&format!(".{}", pattern))
        }
    }

    /// Log a security event.
    async fn log_event(&self, event: SecurityEvent) {
        if self.config.audit_logging {
            let mut log = self.audit_log.write().await;
            log.push(event);

            // Keep only last 10000 events
            if log.len() > 10000 {
                let excess = log.len() - 10000;
                log.drain(0..excess);
            }
        }
    }
}

impl Default for SecuritySandbox {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_capability_check() {
        let sandbox = SecuritySandbox::new();

        // FileRead should be allowed by default
        let result = sandbox.check_capability(&Capability::FileRead).await;
        assert!(result.allowed);

        // CodeExecution should be denied by default
        let result = sandbox.check_capability(&Capability::CodeExecution).await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_path_check() {
        let sandbox = SecuritySandbox::new();

        // /etc should be blocked
        let result = sandbox
            .check_path(std::path::Path::new("/etc/passwd"), false)
            .await;
        assert!(!result.allowed);

        // Normal path should be allowed
        let result = sandbox
            .check_path(std::path::Path::new("/tmp/test.txt"), false)
            .await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_network_check() {
        let sandbox = SecuritySandbox::new();

        // Allowed domain
        let result = sandbox.check_network("api.openai.com").await;
        assert!(result.allowed);

        // Not in allowed list (default is whitelist)
        let result = sandbox.check_network("evil.com").await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_command_check() {
        let sandbox = SecuritySandbox::new();

        // Blocked command
        let result = sandbox.check_command("rm -rf /").await;
        assert!(!result.allowed);

        // Allowed command
        let result = sandbox.check_command("ls -la").await;
        assert!(result.allowed);

        // Command with blocked argument
        let result = sandbox.check_command("git push; rm -rf /").await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_strict_mode() {
        let sandbox = SecuritySandbox::strict();

        // Only whitelisted commands allowed
        let result = sandbox.check_command("ls").await;
        assert!(result.allowed);

        // Non-whitelisted command denied
        let result = sandbox.check_command("make").await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_maximum_mode() {
        let sandbox = SecuritySandbox::maximum();

        // No network access
        let result = sandbox.check_network("api.openai.com").await;
        assert!(!result.allowed);

        // No file write
        let result = sandbox.check_capability(&Capability::FileWrite).await;
        assert!(!result.allowed);

        // File read still allowed
        let result = sandbox.check_capability(&Capability::FileRead).await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_audit_log() {
        let sandbox = SecuritySandbox::new();

        sandbox.check_command("ls").await;
        sandbox.check_command("rm").await;

        let log = sandbox.get_audit_log(10).await;
        assert_eq!(log.len(), 2);
    }

    #[tokio::test]
    async fn test_file_modification_limit() {
        let config = SecurityConfig {
            resource_limits: ResourceLimits {
                max_files_modified: 2,
                ..Default::default()
            },
            ..SecurityConfig::standard()
        };
        let sandbox = SecuritySandbox::with_config(config);

        // First two files should be allowed
        sandbox
            .record_file_modified(std::path::Path::new("/tmp/file1.txt"))
            .await;
        sandbox
            .record_file_modified(std::path::Path::new("/tmp/file2.txt"))
            .await;

        // Third file should be denied
        let result = sandbox
            .check_path(std::path::Path::new("/tmp/file3.txt"), true)
            .await;
        assert!(!result.allowed);

        // Reset counter
        sandbox.reset_file_counter().await;

        // Now it should be allowed again
        let result = sandbox
            .check_path(std::path::Path::new("/tmp/file3.txt"), true)
            .await;
        assert!(result.allowed || result.requires_approval);
    }
}
