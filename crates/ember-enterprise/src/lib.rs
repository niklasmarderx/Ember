//! Ember Enterprise - Enterprise-grade features for Ember
//!
//! This crate provides enterprise features including:
//! - SSO/SAML/OIDC authentication
//! - Comprehensive audit logging
//! - Role-Based Access Control (RBAC)
//! - Team management and collaboration

// Allow common clippy lints for this crate during development
#![allow(clippy::derivable_impls)]
#![allow(clippy::field_reassign_with_default)]

pub mod audit;
pub mod auth;
pub mod rbac;
pub mod teams;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

// Re-exports
pub use audit::{AuditEntry, AuditLevel, AuditLog, AuditQuery};
pub use auth::{
    AuthConfig, AuthProvider, AuthSession, AuthToken, OidcConfig, SamlConfig, SsoProvider,
};
pub use rbac::{Permission, PermissionSet, RbacManager, Role, RoleAssignment};
pub use teams::{Team, TeamConfig, TeamInvite, TeamManager, TeamMember, TeamRole};

/// Enterprise error types
#[derive(Debug, Error)]
pub enum EnterpriseError {
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Authorization denied: {0}")]
    AuthorizationDenied(String),

    #[error("Session expired")]
    SessionExpired,

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Team not found: {0}")]
    TeamNotFound(String),

    #[error("Role not found: {0}")]
    RoleNotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Audit log error: {0}")]
    AuditError(String),

    #[error("SSO provider error: {0}")]
    SsoError(String),

    #[error("SAML error: {0}")]
    SamlError(String),

    #[error("LDAP error: {0}")]
    LdapError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Rate limited: retry after {0} seconds")]
    RateLimited(u64),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<reqwest::Error> for EnterpriseError {
    fn from(err: reqwest::Error) -> Self {
        EnterpriseError::NetworkError(err.to_string())
    }
}

impl From<serde_json::Error> for EnterpriseError {
    fn from(err: serde_json::Error) -> Self {
        EnterpriseError::Internal(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, EnterpriseError>;

/// Enterprise user identity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Unique user ID
    pub id: Uuid,
    /// Username/login name
    pub username: String,
    /// Email address
    pub email: String,
    /// Display name
    pub display_name: String,
    /// Profile picture URL
    pub avatar_url: Option<String>,
    /// User status
    pub status: UserStatus,
    /// SSO provider (if authenticated via SSO)
    pub sso_provider: Option<String>,
    /// External ID from SSO provider
    pub external_id: Option<String>,
    /// Teams the user belongs to
    pub teams: Vec<Uuid>,
    /// Direct role assignments
    pub roles: Vec<Uuid>,
    /// User metadata
    pub metadata: serde_json::Value,
    /// Account creation time
    pub created_at: DateTime<Utc>,
    /// Last update time
    pub updated_at: DateTime<Utc>,
    /// Last login time
    pub last_login: Option<DateTime<Utc>>,
}

impl User {
    pub fn new(username: String, email: String, display_name: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            username,
            email,
            display_name,
            avatar_url: None,
            status: UserStatus::Active,
            sso_provider: None,
            external_id: None,
            teams: Vec::new(),
            roles: Vec::new(),
            metadata: serde_json::json!({}),
            created_at: now,
            updated_at: now,
            last_login: None,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, UserStatus::Active)
    }
}

/// User account status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    /// Active user
    Active,
    /// Pending email verification
    Pending,
    /// Suspended by admin
    Suspended,
    /// Deactivated by user
    Deactivated,
    /// Deleted (soft delete)
    Deleted,
}

/// Enterprise configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseConfig {
    /// Organization name
    pub organization: String,
    /// Organization ID
    pub organization_id: Uuid,
    /// Authentication configuration
    pub auth: AuthConfig,
    /// Audit configuration
    pub audit: AuditConfig,
    /// RBAC configuration
    pub rbac: RbacConfig,
    /// Team configuration
    pub teams: TeamConfig,
    /// Security settings
    pub security: SecurityConfig,
}

impl Default for EnterpriseConfig {
    fn default() -> Self {
        Self {
            organization: "My Organization".to_string(),
            organization_id: Uuid::new_v4(),
            auth: AuthConfig::default(),
            audit: AuditConfig::default(),
            rbac: RbacConfig::default(),
            teams: TeamConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

/// Audit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Enable audit logging
    pub enabled: bool,
    /// Minimum log level
    pub level: AuditLevel,
    /// Retention period in days
    pub retention_days: u32,
    /// Log to file
    pub log_to_file: bool,
    /// Log file path
    pub log_path: Option<String>,
    /// Log to external service
    pub external_service: Option<String>,
    /// Include request body in logs
    pub include_request_body: bool,
    /// Include response body in logs
    pub include_response_body: bool,
    /// PII masking enabled
    pub mask_pii: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            level: AuditLevel::Info,
            retention_days: 90,
            log_to_file: true,
            log_path: None,
            external_service: None,
            include_request_body: false,
            include_response_body: false,
            mask_pii: true,
        }
    }
}

/// RBAC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RbacConfig {
    /// Enable RBAC
    pub enabled: bool,
    /// Default role for new users
    pub default_role: Option<String>,
    /// Enforce permissions
    pub enforce: bool,
    /// Allow super admin bypass
    pub super_admin_bypass: bool,
    /// Cache permissions
    pub cache_permissions: bool,
    /// Cache TTL in seconds
    pub cache_ttl: u64,
}

impl Default for RbacConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_role: Some("user".to_string()),
            enforce: true,
            super_admin_bypass: true,
            cache_permissions: true,
            cache_ttl: 300,
        }
    }
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Session timeout in seconds
    pub session_timeout: u64,
    /// Maximum concurrent sessions
    pub max_concurrent_sessions: u32,
    /// Require MFA
    pub require_mfa: bool,
    /// MFA methods allowed
    pub mfa_methods: Vec<MfaMethod>,
    /// IP whitelist
    pub ip_whitelist: Vec<String>,
    /// IP blacklist
    pub ip_blacklist: Vec<String>,
    /// Password policy
    pub password_policy: PasswordPolicy,
    /// Account lockout settings
    pub account_lockout: AccountLockout,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            session_timeout: 3600,
            max_concurrent_sessions: 5,
            require_mfa: false,
            mfa_methods: vec![MfaMethod::Totp, MfaMethod::Email],
            ip_whitelist: Vec::new(),
            ip_blacklist: Vec::new(),
            password_policy: PasswordPolicy::default(),
            account_lockout: AccountLockout::default(),
        }
    }
}

/// MFA methods
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MfaMethod {
    /// Time-based OTP
    Totp,
    /// SMS
    Sms,
    /// Email
    Email,
    /// Hardware key (FIDO2)
    HardwareKey,
    /// Push notification
    Push,
}

/// Password policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordPolicy {
    /// Minimum length
    pub min_length: u8,
    /// Require uppercase
    pub require_uppercase: bool,
    /// Require lowercase
    pub require_lowercase: bool,
    /// Require number
    pub require_number: bool,
    /// Require special character
    pub require_special: bool,
    /// Password history count
    pub history_count: u8,
    /// Maximum age in days
    pub max_age_days: u32,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            min_length: 12,
            require_uppercase: true,
            require_lowercase: true,
            require_number: true,
            require_special: true,
            history_count: 5,
            max_age_days: 90,
        }
    }
}

/// Account lockout settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountLockout {
    /// Enable lockout
    pub enabled: bool,
    /// Max failed attempts
    pub max_attempts: u8,
    /// Lockout duration in seconds
    pub lockout_duration: u64,
    /// Reset counter after seconds
    pub reset_after: u64,
}

impl Default for AccountLockout {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 5,
            lockout_duration: 900,
            reset_after: 3600,
        }
    }
}

/// Enterprise manager - main entry point
pub struct Enterprise {
    config: EnterpriseConfig,
    auth_manager: auth::AuthManager,
    audit_log: audit::AuditLog,
    rbac_manager: rbac::RbacManager,
    team_manager: teams::TeamManager,
}

impl Enterprise {
    /// Create a new enterprise instance
    pub fn new(config: EnterpriseConfig) -> Result<Self> {
        let auth_manager = auth::AuthManager::new(config.auth.clone())?;
        let audit_log = audit::AuditLog::new(config.audit.clone())?;
        let rbac_manager = rbac::RbacManager::new(config.rbac.clone())?;
        let team_manager = teams::TeamManager::new(config.teams.clone())?;

        Ok(Self {
            config,
            auth_manager,
            audit_log,
            rbac_manager,
            team_manager,
        })
    }

    /// Get the configuration
    pub fn config(&self) -> &EnterpriseConfig {
        &self.config
    }

    /// Get the auth manager
    pub fn auth(&self) -> &auth::AuthManager {
        &self.auth_manager
    }

    /// Get mutable auth manager
    pub fn auth_mut(&mut self) -> &mut auth::AuthManager {
        &mut self.auth_manager
    }

    /// Get the audit log
    pub fn audit(&self) -> &audit::AuditLog {
        &self.audit_log
    }

    /// Get mutable audit log
    pub fn audit_mut(&mut self) -> &mut audit::AuditLog {
        &mut self.audit_log
    }

    /// Get the RBAC manager
    pub fn rbac(&self) -> &rbac::RbacManager {
        &self.rbac_manager
    }

    /// Get mutable RBAC manager
    pub fn rbac_mut(&mut self) -> &mut rbac::RbacManager {
        &mut self.rbac_manager
    }

    /// Get the team manager
    pub fn teams(&self) -> &teams::TeamManager {
        &self.team_manager
    }

    /// Get mutable team manager
    pub fn teams_mut(&mut self) -> &mut teams::TeamManager {
        &mut self.team_manager
    }

    /// Authenticate a user and create a session
    pub async fn authenticate(&mut self, username: &str, password: &str) -> Result<AuthSession> {
        // Authenticate
        let session = self.auth_manager.authenticate(username, password).await?;

        // Log the authentication
        self.audit_log.log(
            AuditEntry::new(
                AuditLevel::Info,
                "auth",
                "user_login",
                format!("User '{}' logged in", username),
            )
            .with_user_id(session.user_id),
        )?;

        Ok(session)
    }

    /// Authenticate via SSO
    pub async fn authenticate_sso(&mut self, provider: &str, token: &str) -> Result<AuthSession> {
        let session = self.auth_manager.authenticate_sso(provider, token).await?;

        self.audit_log.log(
            AuditEntry::new(
                AuditLevel::Info,
                "auth",
                "sso_login",
                format!("User logged in via SSO provider '{}'", provider),
            )
            .with_user_id(session.user_id),
        )?;

        Ok(session)
    }

    /// Check if a user has a permission
    pub fn has_permission(&self, user_id: Uuid, permission: &str) -> Result<bool> {
        self.rbac_manager.has_permission(user_id, permission)
    }

    /// Check if a user can perform an action on a resource
    pub fn can_access(&self, user_id: Uuid, resource: &str, action: &str) -> Result<bool> {
        self.rbac_manager.can_access(user_id, resource, action)
    }

    /// Log an audit event
    pub fn log_event(&mut self, entry: AuditEntry) -> Result<()> {
        self.audit_log.log(entry)
    }
}

/// Builder for Enterprise
pub struct EnterpriseBuilder {
    config: EnterpriseConfig,
}

impl EnterpriseBuilder {
    pub fn new() -> Self {
        Self {
            config: EnterpriseConfig::default(),
        }
    }

    pub fn organization(mut self, name: &str) -> Self {
        self.config.organization = name.to_string();
        self
    }

    pub fn auth_config(mut self, config: AuthConfig) -> Self {
        self.config.auth = config;
        self
    }

    pub fn audit_config(mut self, config: AuditConfig) -> Self {
        self.config.audit = config;
        self
    }

    pub fn rbac_config(mut self, config: RbacConfig) -> Self {
        self.config.rbac = config;
        self
    }

    pub fn team_config(mut self, config: TeamConfig) -> Self {
        self.config.teams = config;
        self
    }

    pub fn security_config(mut self, config: SecurityConfig) -> Self {
        self.config.security = config;
        self
    }

    pub fn build(self) -> Result<Enterprise> {
        Enterprise::new(self.config)
    }
}

impl Default for EnterpriseBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation() {
        let user = User::new(
            "testuser".to_string(),
            "test@example.com".to_string(),
            "Test User".to_string(),
        );

        assert_eq!(user.username, "testuser");
        assert_eq!(user.email, "test@example.com");
        assert!(user.is_active());
    }

    #[test]
    fn test_default_config() {
        let config = EnterpriseConfig::default();
        assert!(config.audit.enabled);
        assert!(config.rbac.enabled);
    }

    #[test]
    fn test_password_policy() {
        let policy = PasswordPolicy::default();
        assert_eq!(policy.min_length, 12);
        assert!(policy.require_uppercase);
    }

    #[test]
    fn test_builder() {
        let builder = EnterpriseBuilder::new().organization("Test Org");

        assert_eq!(builder.config.organization, "Test Org");
    }
}
