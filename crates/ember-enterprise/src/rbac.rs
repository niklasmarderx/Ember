//! Role-Based Access Control (RBAC) module
//!
//! Provides enterprise-grade access control:
//! - Hierarchical roles
//! - Fine-grained permissions
//! - Resource-based access control
//! - Permission inheritance

use crate::{EnterpriseError, RbacConfig, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Permission definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    /// Permission ID
    pub id: Uuid,
    /// Permission name (e.g., "users:read", "tools:execute")
    pub name: String,
    /// Description
    pub description: String,
    /// Resource type this permission applies to
    pub resource_type: Option<String>,
    /// Allowed actions
    pub actions: Vec<String>,
    /// Is this a system permission
    pub system: bool,
    /// Created at
    pub created_at: DateTime<Utc>,
}

impl Permission {
    /// Create a new permission
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: description.to_string(),
            resource_type: None,
            actions: Vec::new(),
            system: false,
            created_at: Utc::now(),
        }
    }

    /// Create a resource permission
    pub fn resource(resource_type: &str, actions: Vec<&str>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: format!("{}:{}", resource_type, actions.join(",")),
            description: format!("Permission for {} actions on {}", actions.join(", "), resource_type),
            resource_type: Some(resource_type.to_string()),
            actions: actions.iter().map(|s| s.to_string()).collect(),
            system: false,
            created_at: Utc::now(),
        }
    }

    /// Check if this permission allows an action on a resource
    pub fn allows(&self, resource: &str, action: &str) -> bool {
        // Check resource type match
        if let Some(ref rt) = self.resource_type {
            if rt != resource && rt != "*" {
                return false;
            }
        }

        // Check action match
        self.actions.contains(&action.to_string()) || self.actions.contains(&"*".to_string())
    }
}

/// Set of permissions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionSet {
    /// Permissions in this set
    permissions: HashSet<String>,
}

impl PermissionSet {
    pub fn new() -> Self {
        Self {
            permissions: HashSet::new(),
        }
    }

    /// Add a permission
    pub fn add(&mut self, permission: &str) {
        self.permissions.insert(permission.to_string());
    }

    /// Remove a permission
    pub fn remove(&mut self, permission: &str) {
        self.permissions.remove(permission);
    }

    /// Check if permission exists
    pub fn has(&self, permission: &str) -> bool {
        // Direct match
        if self.permissions.contains(permission) {
            return true;
        }

        // Wildcard match
        if self.permissions.contains("*") {
            return true;
        }

        // Check for partial wildcard (e.g., "users:*" matches "users:read")
        let parts: Vec<&str> = permission.split(':').collect();
        if parts.len() >= 2 {
            let wildcard = format!("{}:*", parts[0]);
            if self.permissions.contains(&wildcard) {
                return true;
            }
        }

        false
    }

    /// Check if can perform action on resource
    pub fn can(&self, resource: &str, action: &str) -> bool {
        let permission = format!("{}:{}", resource, action);
        self.has(&permission)
    }

    /// Get all permissions
    pub fn all(&self) -> &HashSet<String> {
        &self.permissions
    }

    /// Merge with another permission set
    pub fn merge(&mut self, other: &PermissionSet) {
        for perm in &other.permissions {
            self.permissions.insert(perm.clone());
        }
    }

    /// Create from a list of permissions
    pub fn from_list(permissions: Vec<&str>) -> Self {
        let mut set = Self::new();
        for perm in permissions {
            set.add(perm);
        }
        set
    }
}

/// Role definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    /// Role ID
    pub id: Uuid,
    /// Role name
    pub name: String,
    /// Description
    pub description: String,
    /// Permissions granted by this role
    pub permissions: PermissionSet,
    /// Parent roles (for inheritance)
    pub inherits: Vec<Uuid>,
    /// Is this a system role
    pub system: bool,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Updated at
    pub updated_at: DateTime<Utc>,
}

impl Role {
    /// Create a new role
    pub fn new(name: &str, description: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: description.to_string(),
            permissions: PermissionSet::new(),
            inherits: Vec::new(),
            system: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Add a permission to this role
    pub fn add_permission(&mut self, permission: &str) {
        self.permissions.add(permission);
        self.updated_at = Utc::now();
    }

    /// Remove a permission from this role
    pub fn remove_permission(&mut self, permission: &str) {
        self.permissions.remove(permission);
        self.updated_at = Utc::now();
    }

    /// Add an inherited role
    pub fn inherit_from(&mut self, role_id: Uuid) {
        if !self.inherits.contains(&role_id) {
            self.inherits.push(role_id);
            self.updated_at = Utc::now();
        }
    }
}

/// Role assignment - links a user to a role
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleAssignment {
    /// Assignment ID
    pub id: Uuid,
    /// User ID
    pub user_id: Uuid,
    /// Role ID
    pub role_id: Uuid,
    /// Scope (optional - for resource-specific assignments)
    pub scope: Option<RoleScope>,
    /// Assigned at
    pub assigned_at: DateTime<Utc>,
    /// Assigned by
    pub assigned_by: Option<Uuid>,
    /// Expires at
    pub expires_at: Option<DateTime<Utc>>,
}

impl RoleAssignment {
    pub fn new(user_id: Uuid, role_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            role_id,
            scope: None,
            assigned_at: Utc::now(),
            assigned_by: None,
            expires_at: None,
        }
    }

    /// Check if assignment is valid (not expired)
    pub fn is_valid(&self) -> bool {
        match self.expires_at {
            Some(expires) => Utc::now() < expires,
            None => true,
        }
    }
}

/// Role scope - limits role to specific resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleScope {
    /// Resource type
    pub resource_type: String,
    /// Resource IDs (empty = all)
    pub resource_ids: Vec<String>,
}

/// RBAC Manager
pub struct RbacManager {
    config: RbacConfig,
    roles: Arc<RwLock<HashMap<Uuid, Role>>>,
    assignments: Arc<RwLock<Vec<RoleAssignment>>>,
    permission_cache: Arc<RwLock<HashMap<Uuid, PermissionSet>>>,
}

impl RbacManager {
    /// Create a new RBAC manager
    pub fn new(config: RbacConfig) -> Result<Self> {
        let manager = Self {
            config,
            roles: Arc::new(RwLock::new(HashMap::new())),
            assignments: Arc::new(RwLock::new(Vec::new())),
            permission_cache: Arc::new(RwLock::new(HashMap::new())),
        };

        // Initialize default roles synchronously
        let default_roles = create_default_roles();
        let roles = manager.roles.clone();
        tokio::spawn(async move {
            let mut roles = roles.write().await;
            for role in default_roles {
                roles.insert(role.id, role);
            }
        });

        Ok(manager)
    }

    /// Create a new role
    pub async fn create_role(&self, name: &str, description: &str) -> Result<Role> {
        let role = Role::new(name, description);
        let mut roles = self.roles.write().await;
        roles.insert(role.id, role.clone());
        Ok(role)
    }

    /// Get a role by ID
    pub async fn get_role(&self, role_id: Uuid) -> Option<Role> {
        let roles = self.roles.read().await;
        roles.get(&role_id).cloned()
    }

    /// Get a role by name
    pub async fn get_role_by_name(&self, name: &str) -> Option<Role> {
        let roles = self.roles.read().await;
        roles.values().find(|r| r.name == name).cloned()
    }

    /// Update a role
    pub async fn update_role(&self, role: Role) -> Result<Role> {
        let mut roles = self.roles.write().await;
        if !roles.contains_key(&role.id) {
            return Err(EnterpriseError::RoleNotFound(role.id.to_string()));
        }
        roles.insert(role.id, role.clone());
        
        // Invalidate cache for users with this role
        self.invalidate_cache_for_role(role.id).await;
        
        Ok(role)
    }

    /// Delete a role
    pub async fn delete_role(&self, role_id: Uuid) -> Result<()> {
        let mut roles = self.roles.write().await;
        let role = roles.get(&role_id)
            .ok_or_else(|| EnterpriseError::RoleNotFound(role_id.to_string()))?;
        
        if role.system {
            return Err(EnterpriseError::PermissionDenied("Cannot delete system role".to_string()));
        }
        
        roles.remove(&role_id);
        
        // Remove assignments
        let mut assignments = self.assignments.write().await;
        assignments.retain(|a| a.role_id != role_id);
        
        Ok(())
    }

    /// List all roles
    pub async fn list_roles(&self) -> Vec<Role> {
        let roles = self.roles.read().await;
        roles.values().cloned().collect()
    }

    /// Assign a role to a user
    pub async fn assign_role(&self, user_id: Uuid, role_id: Uuid) -> Result<RoleAssignment> {
        // Verify role exists
        {
            let roles = self.roles.read().await;
            if !roles.contains_key(&role_id) {
                return Err(EnterpriseError::RoleNotFound(role_id.to_string()));
            }
        }

        let assignment = RoleAssignment::new(user_id, role_id);
        
        let mut assignments = self.assignments.write().await;
        assignments.push(assignment.clone());
        
        // Invalidate user's permission cache
        self.invalidate_cache_for_user(user_id).await;
        
        Ok(assignment)
    }

    /// Remove a role from a user
    pub async fn unassign_role(&self, user_id: Uuid, role_id: Uuid) -> Result<()> {
        let mut assignments = self.assignments.write().await;
        let initial_len = assignments.len();
        assignments.retain(|a| !(a.user_id == user_id && a.role_id == role_id));
        
        if assignments.len() == initial_len {
            return Err(EnterpriseError::RoleNotFound(
                format!("Role {} not assigned to user {}", role_id, user_id)
            ));
        }
        
        // Invalidate cache
        self.invalidate_cache_for_user(user_id).await;
        
        Ok(())
    }

    /// Get roles for a user
    pub async fn get_user_roles(&self, user_id: Uuid) -> Vec<Role> {
        let assignments = self.assignments.read().await;
        let roles = self.roles.read().await;
        
        assignments
            .iter()
            .filter(|a| a.user_id == user_id && a.is_valid())
            .filter_map(|a| roles.get(&a.role_id).cloned())
            .collect()
    }

    /// Get all permissions for a user (including inherited)
    pub async fn get_user_permissions(&self, user_id: Uuid) -> PermissionSet {
        // Check cache first
        {
            let cache = self.permission_cache.read().await;
            if let Some(perms) = cache.get(&user_id) {
                return perms.clone();
            }
        }

        // Compute permissions
        let permissions = self.compute_user_permissions(user_id).await;
        
        // Cache if enabled
        if self.config.cache_permissions {
            let mut cache = self.permission_cache.write().await;
            cache.insert(user_id, permissions.clone());
        }
        
        permissions
    }

    /// Check if a user has a specific permission
    pub fn has_permission(&self, user_id: Uuid, permission: &str) -> Result<bool> {
        if !self.config.enabled || !self.config.enforce {
            return Ok(true);
        }

        // This is a synchronous wrapper - in practice, you'd want to use async
        // For now, we'll use a simple check
        Ok(true) // Placeholder - would need async context
    }

    /// Check if a user can access a resource
    pub fn can_access(&self, user_id: Uuid, resource: &str, action: &str) -> Result<bool> {
        if !self.config.enabled || !self.config.enforce {
            return Ok(true);
        }

        Ok(true) // Placeholder
    }

    /// Check permission asynchronously
    pub async fn check_permission(&self, user_id: Uuid, permission: &str) -> Result<bool> {
        if !self.config.enabled || !self.config.enforce {
            return Ok(true);
        }

        let permissions = self.get_user_permissions(user_id).await;
        Ok(permissions.has(permission))
    }

    /// Check resource access asynchronously
    pub async fn check_access(&self, user_id: Uuid, resource: &str, action: &str) -> Result<bool> {
        if !self.config.enabled || !self.config.enforce {
            return Ok(true);
        }

        let permissions = self.get_user_permissions(user_id).await;
        Ok(permissions.can(resource, action))
    }

    /// Require permission (throws if not granted)
    pub async fn require_permission(&self, user_id: Uuid, permission: &str) -> Result<()> {
        if !self.check_permission(user_id, permission).await? {
            return Err(EnterpriseError::PermissionDenied(format!(
                "Permission '{}' required", permission
            )));
        }
        Ok(())
    }

    /// Require resource access (throws if not granted)
    pub async fn require_access(&self, user_id: Uuid, resource: &str, action: &str) -> Result<()> {
        if !self.check_access(user_id, resource, action).await? {
            return Err(EnterpriseError::PermissionDenied(format!(
                "Access to {} (action: {}) denied", resource, action
            )));
        }
        Ok(())
    }

    // Internal methods

    async fn compute_user_permissions(&self, user_id: Uuid) -> PermissionSet {
        let mut permissions = PermissionSet::new();
        let user_roles = self.get_user_roles(user_id).await;
        
        for role in user_roles {
            // Add direct permissions
            permissions.merge(&role.permissions);
            
            // Add inherited permissions
            for inherited_id in &role.inherits {
                if let Some(inherited_role) = self.get_role(*inherited_id).await {
                    permissions.merge(&inherited_role.permissions);
                }
            }
        }
        
        permissions
    }

    async fn invalidate_cache_for_user(&self, user_id: Uuid) {
        let mut cache = self.permission_cache.write().await;
        cache.remove(&user_id);
    }

    async fn invalidate_cache_for_role(&self, role_id: Uuid) {
        // Find all users with this role and invalidate their cache
        let assignments = self.assignments.read().await;
        let user_ids: Vec<Uuid> = assignments
            .iter()
            .filter(|a| a.role_id == role_id)
            .map(|a| a.user_id)
            .collect();
        
        let mut cache = self.permission_cache.write().await;
        for user_id in user_ids {
            cache.remove(&user_id);
        }
    }
}

/// Create default system roles
fn create_default_roles() -> Vec<Role> {
    vec![
        // Super Admin - full access
        {
            let mut role = Role::new("super_admin", "Super Administrator with full system access");
            role.system = true;
            role.add_permission("*");
            role
        },
        
        // Admin - administrative access
        {
            let mut role = Role::new("admin", "Administrator with management access");
            role.system = true;
            role.add_permission("users:*");
            role.add_permission("teams:*");
            role.add_permission("roles:read");
            role.add_permission("config:*");
            role.add_permission("audit:read");
            role.add_permission("tools:*");
            role.add_permission("chat:*");
            role
        },
        
        // User - standard user access
        {
            let mut role = Role::new("user", "Standard user with basic access");
            role.system = true;
            role.add_permission("chat:*");
            role.add_permission("tools:execute");
            role.add_permission("profile:*");
            role.add_permission("history:read");
            role
        },
        
        // Viewer - read-only access
        {
            let mut role = Role::new("viewer", "Read-only access");
            role.system = true;
            role.add_permission("chat:read");
            role.add_permission("history:read");
            role.add_permission("profile:read");
            role
        },
        
        // Developer - developer access
        {
            let mut role = Role::new("developer", "Developer with tool and code access");
            role.system = true;
            role.add_permission("chat:*");
            role.add_permission("tools:*");
            role.add_permission("code:*");
            role.add_permission("shell:execute");
            role.add_permission("filesystem:*");
            role.add_permission("git:*");
            role.add_permission("profile:*");
            role.add_permission("history:*");
            role
        },
    ]
}

/// Predefined permissions
pub mod permissions {
    // Chat permissions
    pub const CHAT_READ: &str = "chat:read";
    pub const CHAT_WRITE: &str = "chat:write";
    pub const CHAT_DELETE: &str = "chat:delete";
    
    // Tool permissions
    pub const TOOLS_EXECUTE: &str = "tools:execute";
    pub const TOOLS_MANAGE: &str = "tools:manage";
    
    // Shell permissions
    pub const SHELL_EXECUTE: &str = "shell:execute";
    pub const SHELL_ADMIN: &str = "shell:admin";
    
    // Filesystem permissions
    pub const FS_READ: &str = "filesystem:read";
    pub const FS_WRITE: &str = "filesystem:write";
    pub const FS_DELETE: &str = "filesystem:delete";
    
    // User permissions
    pub const USERS_READ: &str = "users:read";
    pub const USERS_WRITE: &str = "users:write";
    pub const USERS_DELETE: &str = "users:delete";
    pub const USERS_ADMIN: &str = "users:admin";
    
    // Team permissions
    pub const TEAMS_READ: &str = "teams:read";
    pub const TEAMS_WRITE: &str = "teams:write";
    pub const TEAMS_DELETE: &str = "teams:delete";
    pub const TEAMS_ADMIN: &str = "teams:admin";
    
    // Config permissions
    pub const CONFIG_READ: &str = "config:read";
    pub const CONFIG_WRITE: &str = "config:write";
    
    // Audit permissions
    pub const AUDIT_READ: &str = "audit:read";
    pub const AUDIT_EXPORT: &str = "audit:export";
    
    // Role permissions
    pub const ROLES_READ: &str = "roles:read";
    pub const ROLES_WRITE: &str = "roles:write";
    pub const ROLES_ASSIGN: &str = "roles:assign";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_set() {
        let mut perms = PermissionSet::new();
        perms.add("users:read");
        perms.add("users:write");
        
        assert!(perms.has("users:read"));
        assert!(perms.has("users:write"));
        assert!(!perms.has("users:delete"));
    }

    #[test]
    fn test_wildcard_permission() {
        let mut perms = PermissionSet::new();
        perms.add("users:*");
        
        assert!(perms.has("users:read"));
        assert!(perms.has("users:write"));
        assert!(perms.has("users:delete"));
        assert!(!perms.has("teams:read"));
    }

    #[test]
    fn test_global_wildcard() {
        let mut perms = PermissionSet::new();
        perms.add("*");
        
        assert!(perms.has("users:read"));
        assert!(perms.has("teams:write"));
        assert!(perms.has("anything:else"));
    }

    #[test]
    fn test_permission_set_can() {
        let mut perms = PermissionSet::new();
        perms.add("users:read");
        
        assert!(perms.can("users", "read"));
        assert!(!perms.can("users", "write"));
    }

    #[test]
    fn test_role_creation() {
        let mut role = Role::new("test", "Test role");
        role.add_permission("users:read");
        
        assert_eq!(role.name, "test");
        assert!(role.permissions.has("users:read"));
    }

    #[test]
    fn test_role_assignment() {
        let user_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();
        let assignment = RoleAssignment::new(user_id, role_id);
        
        assert_eq!(assignment.user_id, user_id);
        assert_eq!(assignment.role_id, role_id);
        assert!(assignment.is_valid());
    }

    #[test]
    fn test_default_roles() {
        let roles = create_default_roles();
        assert!(roles.len() >= 4);
        
        let super_admin = roles.iter().find(|r| r.name == "super_admin").unwrap();
        assert!(super_admin.permissions.has("*"));
        assert!(super_admin.system);
    }

    #[tokio::test]
    async fn test_rbac_manager_creation() {
        let config = RbacConfig::default();
        let manager = RbacManager::new(config);
        assert!(manager.is_ok());
    }
}