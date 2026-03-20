//! Team management module
//!
//! Provides enterprise team collaboration features:
//! - Team creation and management
//! - Member management
//! - Team roles
//! - Invitations

use crate::{EnterpriseError, Result, User};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Team configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    /// Maximum team size (0 = unlimited)
    pub max_team_size: u32,
    /// Maximum teams per user
    pub max_teams_per_user: u32,
    /// Allow self-service team creation
    pub allow_self_service_creation: bool,
    /// Require admin approval for new teams
    pub require_approval: bool,
    /// Default team role for new members
    pub default_member_role: TeamRole,
    /// Invitation expiration in days
    pub invite_expiration_days: u32,
}

impl Default for TeamConfig {
    fn default() -> Self {
        Self {
            max_team_size: 100,
            max_teams_per_user: 10,
            allow_self_service_creation: true,
            require_approval: false,
            default_member_role: TeamRole::Member,
            invite_expiration_days: 7,
        }
    }
}

/// Team definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    /// Team ID
    pub id: Uuid,
    /// Team name
    pub name: String,
    /// Team description
    pub description: String,
    /// Team avatar URL
    pub avatar_url: Option<String>,
    /// Team status
    pub status: TeamStatus,
    /// Team visibility
    pub visibility: TeamVisibility,
    /// Team settings
    pub settings: TeamSettings,
    /// Team metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// Created by user ID
    pub created_by: Uuid,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Updated at
    pub updated_at: DateTime<Utc>,
}

impl Team {
    /// Create a new team
    pub fn new(name: &str, description: &str, created_by: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: description.to_string(),
            avatar_url: None,
            status: TeamStatus::Active,
            visibility: TeamVisibility::Private,
            settings: TeamSettings::default(),
            metadata: HashMap::new(),
            created_by,
            created_at: now,
            updated_at: now,
        }
    }

    /// Check if team is active
    pub fn is_active(&self) -> bool {
        matches!(self.status, TeamStatus::Active)
    }
}

/// Team status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamStatus {
    /// Active team
    Active,
    /// Inactive/disabled
    Inactive,
    /// Pending approval
    Pending,
    /// Archived
    Archived,
    /// Deleted (soft delete)
    Deleted,
}

/// Team visibility
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamVisibility {
    /// Visible to all organization members
    Public,
    /// Only visible to team members
    Private,
    /// Discoverable but requires invitation
    Internal,
}

/// Team settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamSettings {
    /// Allow members to invite others
    pub members_can_invite: bool,
    /// Allow members to share conversations
    pub allow_conversation_sharing: bool,
    /// Shared model/provider settings
    pub shared_provider: Option<String>,
    /// Shared API keys
    pub shared_api_keys: bool,
    /// Cost tracking enabled
    pub cost_tracking: bool,
    /// Monthly budget limit
    pub budget_limit: Option<f64>,
    /// Notifications enabled
    pub notifications_enabled: bool,
}

impl Default for TeamSettings {
    fn default() -> Self {
        Self {
            members_can_invite: false,
            allow_conversation_sharing: true,
            shared_provider: None,
            shared_api_keys: false,
            cost_tracking: true,
            budget_limit: None,
            notifications_enabled: true,
        }
    }
}

/// Team member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    /// Member ID
    pub id: Uuid,
    /// Team ID
    pub team_id: Uuid,
    /// User ID
    pub user_id: Uuid,
    /// Member role in team
    pub role: TeamRole,
    /// Joined at
    pub joined_at: DateTime<Utc>,
    /// Invited by user ID
    pub invited_by: Option<Uuid>,
    /// Custom permissions (overrides role defaults)
    pub custom_permissions: Option<Vec<String>>,
    /// Member status
    pub status: MemberStatus,
}

impl TeamMember {
    pub fn new(team_id: Uuid, user_id: Uuid, role: TeamRole) -> Self {
        Self {
            id: Uuid::new_v4(),
            team_id,
            user_id,
            role,
            joined_at: Utc::now(),
            invited_by: None,
            custom_permissions: None,
            status: MemberStatus::Active,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, MemberStatus::Active)
    }

    pub fn can_manage(&self) -> bool {
        matches!(self.role, TeamRole::Owner | TeamRole::Admin)
    }

    pub fn can_invite(&self) -> bool {
        matches!(self.role, TeamRole::Owner | TeamRole::Admin | TeamRole::Moderator)
    }
}

/// Team role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamRole {
    /// Team owner - full control
    Owner,
    /// Team admin - can manage members and settings
    Admin,
    /// Moderator - can manage content and invites
    Moderator,
    /// Regular member
    Member,
    /// Read-only member
    Viewer,
    /// Guest with limited access
    Guest,
}

impl TeamRole {
    /// Get role hierarchy level (higher = more permissions)
    pub fn level(&self) -> u8 {
        match self {
            TeamRole::Owner => 100,
            TeamRole::Admin => 80,
            TeamRole::Moderator => 60,
            TeamRole::Member => 40,
            TeamRole::Viewer => 20,
            TeamRole::Guest => 10,
        }
    }

    /// Check if this role can manage another role
    pub fn can_manage(&self, other: &TeamRole) -> bool {
        self.level() > other.level()
    }
}

/// Member status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberStatus {
    /// Active member
    Active,
    /// Suspended
    Suspended,
    /// Left the team
    Left,
    /// Removed by admin
    Removed,
}

/// Team invitation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamInvite {
    /// Invite ID
    pub id: Uuid,
    /// Team ID
    pub team_id: Uuid,
    /// Invitee email
    pub email: String,
    /// Invitee user ID (if existing user)
    pub user_id: Option<Uuid>,
    /// Role to assign
    pub role: TeamRole,
    /// Invite token
    pub token: String,
    /// Invited by user ID
    pub invited_by: Uuid,
    /// Invite status
    pub status: InviteStatus,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Expires at
    pub expires_at: DateTime<Utc>,
    /// Accepted at
    pub accepted_at: Option<DateTime<Utc>>,
    /// Message to invitee
    pub message: Option<String>,
}

impl TeamInvite {
    pub fn new(team_id: Uuid, email: &str, role: TeamRole, invited_by: Uuid, expiration_days: u32) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            team_id,
            email: email.to_string(),
            user_id: None,
            role,
            token: generate_invite_token(),
            invited_by,
            status: InviteStatus::Pending,
            created_at: now,
            expires_at: now + Duration::days(expiration_days as i64),
            accepted_at: None,
            message: None,
        }
    }

    pub fn is_valid(&self) -> bool {
        matches!(self.status, InviteStatus::Pending) && Utc::now() < self.expires_at
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }
}

/// Invite status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InviteStatus {
    /// Pending acceptance
    Pending,
    /// Accepted
    Accepted,
    /// Declined
    Declined,
    /// Expired
    Expired,
    /// Revoked by admin
    Revoked,
}

/// Team manager
pub struct TeamManager {
    config: TeamConfig,
    teams: Arc<RwLock<HashMap<Uuid, Team>>>,
    members: Arc<RwLock<Vec<TeamMember>>>,
    invites: Arc<RwLock<Vec<TeamInvite>>>,
}

impl TeamManager {
    /// Create a new team manager
    pub fn new(config: TeamConfig) -> Result<Self> {
        Ok(Self {
            config,
            teams: Arc::new(RwLock::new(HashMap::new())),
            members: Arc::new(RwLock::new(Vec::new())),
            invites: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// Create a new team
    pub async fn create_team(&self, name: &str, description: &str, creator_id: Uuid) -> Result<Team> {
        // Check if user can create teams
        if !self.config.allow_self_service_creation {
            return Err(EnterpriseError::PermissionDenied(
                "Team creation requires admin approval".to_string()
            ));
        }

        // Check user's team limit
        let user_team_count = self.get_user_teams(creator_id).await.len();
        if self.config.max_teams_per_user > 0 && user_team_count >= self.config.max_teams_per_user as usize {
            return Err(EnterpriseError::PermissionDenied(format!(
                "Maximum teams per user ({}) reached", self.config.max_teams_per_user
            )));
        }

        // Create team
        let mut team = Team::new(name, description, creator_id);
        if self.config.require_approval {
            team.status = TeamStatus::Pending;
        }

        let team_id = team.id;

        // Store team
        {
            let mut teams = self.teams.write().await;
            teams.insert(team.id, team.clone());
        }

        // Add creator as owner
        let owner = TeamMember::new(team_id, creator_id, TeamRole::Owner);
        {
            let mut members = self.members.write().await;
            members.push(owner);
        }

        Ok(team)
    }

    /// Get a team by ID
    pub async fn get_team(&self, team_id: Uuid) -> Option<Team> {
        let teams = self.teams.read().await;
        teams.get(&team_id).cloned()
    }

    /// Update a team
    pub async fn update_team(&self, team: Team) -> Result<Team> {
        let mut teams = self.teams.write().await;
        if !teams.contains_key(&team.id) {
            return Err(EnterpriseError::TeamNotFound(team.id.to_string()));
        }

        let mut updated = team.clone();
        updated.updated_at = Utc::now();
        teams.insert(team.id, updated.clone());

        Ok(updated)
    }

    /// Delete a team
    pub async fn delete_team(&self, team_id: Uuid, requester_id: Uuid) -> Result<()> {
        // Check if requester is owner
        let member = self.get_member(team_id, requester_id).await
            .ok_or_else(|| EnterpriseError::PermissionDenied("Not a team member".to_string()))?;

        if !matches!(member.role, TeamRole::Owner) {
            return Err(EnterpriseError::PermissionDenied(
                "Only team owner can delete the team".to_string()
            ));
        }

        // Remove team
        let mut teams = self.teams.write().await;
        teams.remove(&team_id);

        // Remove members
        let mut members = self.members.write().await;
        members.retain(|m| m.team_id != team_id);

        // Remove invites
        let mut invites = self.invites.write().await;
        invites.retain(|i| i.team_id != team_id);

        Ok(())
    }

    /// List all teams
    pub async fn list_teams(&self) -> Vec<Team> {
        let teams = self.teams.read().await;
        teams.values().cloned().collect()
    }

    /// Get teams for a user
    pub async fn get_user_teams(&self, user_id: Uuid) -> Vec<Team> {
        let members = self.members.read().await;
        let teams = self.teams.read().await;

        members
            .iter()
            .filter(|m| m.user_id == user_id && m.is_active())
            .filter_map(|m| teams.get(&m.team_id).cloned())
            .collect()
    }

    /// Add a member to a team
    pub async fn add_member(
        &self,
        team_id: Uuid,
        user_id: Uuid,
        role: TeamRole,
        added_by: Uuid,
    ) -> Result<TeamMember> {
        // Check team exists
        {
            let teams = self.teams.read().await;
            if !teams.contains_key(&team_id) {
                return Err(EnterpriseError::TeamNotFound(team_id.to_string()));
            }
        }

        // Check requester has permission
        let requester = self.get_member(team_id, added_by).await
            .ok_or_else(|| EnterpriseError::PermissionDenied("Not a team member".to_string()))?;

        if !requester.can_invite() {
            return Err(EnterpriseError::PermissionDenied(
                "You don't have permission to add members".to_string()
            ));
        }

        // Can't add role higher than own
        if !requester.role.can_manage(&role) && requester.role != role {
            return Err(EnterpriseError::PermissionDenied(
                "Cannot add member with higher role".to_string()
            ));
        }

        // Check team size limit
        let team_size = self.get_team_members(team_id).await.len();
        if self.config.max_team_size > 0 && team_size >= self.config.max_team_size as usize {
            return Err(EnterpriseError::PermissionDenied(format!(
                "Team size limit ({}) reached", self.config.max_team_size
            )));
        }

        // Check if already a member
        if self.get_member(team_id, user_id).await.is_some() {
            return Err(EnterpriseError::PermissionDenied(
                "User is already a team member".to_string()
            ));
        }

        // Create member
        let mut member = TeamMember::new(team_id, user_id, role);
        member.invited_by = Some(added_by);

        let mut members = self.members.write().await;
        members.push(member.clone());

        Ok(member)
    }

    /// Remove a member from a team
    pub async fn remove_member(&self, team_id: Uuid, user_id: Uuid, removed_by: Uuid) -> Result<()> {
        // Check requester has permission
        let requester = self.get_member(team_id, removed_by).await
            .ok_or_else(|| EnterpriseError::PermissionDenied("Not a team member".to_string()))?;

        // Check target member exists
        let target = self.get_member(team_id, user_id).await
            .ok_or_else(|| EnterpriseError::UserNotFound(user_id.to_string()))?;

        // Can't remove owner unless self-leaving
        if target.role == TeamRole::Owner && user_id != removed_by {
            return Err(EnterpriseError::PermissionDenied(
                "Cannot remove team owner".to_string()
            ));
        }

        // Check if requester can remove target
        if user_id != removed_by && !requester.role.can_manage(&target.role) {
            return Err(EnterpriseError::PermissionDenied(
                "Cannot remove member with same or higher role".to_string()
            ));
        }

        // Update member status
        let mut members = self.members.write().await;
        if let Some(member) = members.iter_mut().find(|m| m.team_id == team_id && m.user_id == user_id) {
            member.status = if user_id == removed_by {
                MemberStatus::Left
            } else {
                MemberStatus::Removed
            };
        }

        Ok(())
    }

    /// Update a member's role
    pub async fn update_member_role(
        &self,
        team_id: Uuid,
        user_id: Uuid,
        new_role: TeamRole,
        updated_by: Uuid,
    ) -> Result<TeamMember> {
        // Check requester has permission
        let requester = self.get_member(team_id, updated_by).await
            .ok_or_else(|| EnterpriseError::PermissionDenied("Not a team member".to_string()))?;

        if !requester.can_manage() {
            return Err(EnterpriseError::PermissionDenied(
                "You don't have permission to change roles".to_string()
            ));
        }

        // Check target member exists
        let target = self.get_member(team_id, user_id).await
            .ok_or_else(|| EnterpriseError::UserNotFound(user_id.to_string()))?;

        // Can't change owner role (must transfer ownership)
        if target.role == TeamRole::Owner && new_role != TeamRole::Owner {
            return Err(EnterpriseError::PermissionDenied(
                "Cannot demote owner. Use transfer ownership instead.".to_string()
            ));
        }

        // Can't promote to owner
        if new_role == TeamRole::Owner && target.role != TeamRole::Owner {
            return Err(EnterpriseError::PermissionDenied(
                "Cannot promote to owner. Use transfer ownership instead.".to_string()
            ));
        }

        // Can't manage higher/equal role
        if !requester.role.can_manage(&target.role) && requester.user_id != user_id {
            return Err(EnterpriseError::PermissionDenied(
                "Cannot change role of member with same or higher role".to_string()
            ));
        }

        // Update role
        let mut members = self.members.write().await;
        let member = members
            .iter_mut()
            .find(|m| m.team_id == team_id && m.user_id == user_id)
            .ok_or_else(|| EnterpriseError::UserNotFound(user_id.to_string()))?;

        member.role = new_role;
        Ok(member.clone())
    }

    /// Get team members
    pub async fn get_team_members(&self, team_id: Uuid) -> Vec<TeamMember> {
        let members = self.members.read().await;
        members
            .iter()
            .filter(|m| m.team_id == team_id && m.is_active())
            .cloned()
            .collect()
    }

    /// Get a specific member
    pub async fn get_member(&self, team_id: Uuid, user_id: Uuid) -> Option<TeamMember> {
        let members = self.members.read().await;
        members
            .iter()
            .find(|m| m.team_id == team_id && m.user_id == user_id && m.is_active())
            .cloned()
    }

    /// Create an invitation
    pub async fn create_invite(
        &self,
        team_id: Uuid,
        email: &str,
        role: TeamRole,
        invited_by: Uuid,
    ) -> Result<TeamInvite> {
        // Check team exists
        {
            let teams = self.teams.read().await;
            if !teams.contains_key(&team_id) {
                return Err(EnterpriseError::TeamNotFound(team_id.to_string()));
            }
        }

        // Check requester has permission
        let requester = self.get_member(team_id, invited_by).await
            .ok_or_else(|| EnterpriseError::PermissionDenied("Not a team member".to_string()))?;

        if !requester.can_invite() {
            return Err(EnterpriseError::PermissionDenied(
                "You don't have permission to invite members".to_string()
            ));
        }

        // Can't invite with role higher than own
        if !requester.role.can_manage(&role) && requester.role != role {
            return Err(EnterpriseError::PermissionDenied(
                "Cannot invite with higher role".to_string()
            ));
        }

        // Check for existing pending invite
        {
            let invites = self.invites.read().await;
            if invites.iter().any(|i| i.team_id == team_id && i.email == email && i.is_valid()) {
                return Err(EnterpriseError::PermissionDenied(
                    "Pending invitation already exists for this email".to_string()
                ));
            }
        }

        // Create invite
        let invite = TeamInvite::new(team_id, email, role, invited_by, self.config.invite_expiration_days);

        let mut invites = self.invites.write().await;
        invites.push(invite.clone());

        Ok(invite)
    }

    /// Accept an invitation
    pub async fn accept_invite(&self, token: &str, user_id: Uuid) -> Result<TeamMember> {
        let mut invites = self.invites.write().await;
        let invite = invites
            .iter_mut()
            .find(|i| i.token == token)
            .ok_or_else(|| EnterpriseError::InvalidToken("Invalid invitation token".to_string()))?;

        if !invite.is_valid() {
            if invite.is_expired() {
                invite.status = InviteStatus::Expired;
            }
            return Err(EnterpriseError::InvalidToken("Invitation is no longer valid".to_string()));
        }

        // Mark invite as accepted
        invite.status = InviteStatus::Accepted;
        invite.accepted_at = Some(Utc::now());
        invite.user_id = Some(user_id);

        let team_id = invite.team_id;
        let role = invite.role;
        let invited_by = invite.invited_by;

        drop(invites);

        // Add as member
        let mut member = TeamMember::new(team_id, user_id, role);
        member.invited_by = Some(invited_by);

        let mut members = self.members.write().await;
        members.push(member.clone());

        Ok(member)
    }

    /// Get pending invites for a team
    pub async fn get_team_invites(&self, team_id: Uuid) -> Vec<TeamInvite> {
        let invites = self.invites.read().await;
        invites
            .iter()
            .filter(|i| i.team_id == team_id && i.is_valid())
            .cloned()
            .collect()
    }

    /// Revoke an invitation
    pub async fn revoke_invite(&self, invite_id: Uuid, revoked_by: Uuid) -> Result<()> {
        let mut invites = self.invites.write().await;
        let invite = invites
            .iter_mut()
            .find(|i| i.id == invite_id)
            .ok_or_else(|| EnterpriseError::InvalidToken("Invitation not found".to_string()))?;

        // Check permission
        let requester = self.get_member(invite.team_id, revoked_by).await
            .ok_or_else(|| EnterpriseError::PermissionDenied("Not a team member".to_string()))?;

        if !requester.can_manage() {
            return Err(EnterpriseError::PermissionDenied(
                "You don't have permission to revoke invitations".to_string()
            ));
        }

        invite.status = InviteStatus::Revoked;
        Ok(())
    }

    /// Transfer team ownership
    pub async fn transfer_ownership(
        &self,
        team_id: Uuid,
        new_owner_id: Uuid,
        current_owner_id: Uuid,
    ) -> Result<()> {
        // Check current owner
        let current_owner = self.get_member(team_id, current_owner_id).await
            .ok_or_else(|| EnterpriseError::PermissionDenied("Not a team member".to_string()))?;

        if current_owner.role != TeamRole::Owner {
            return Err(EnterpriseError::PermissionDenied(
                "Only the owner can transfer ownership".to_string()
            ));
        }

        // Check new owner is a member
        let new_owner = self.get_member(team_id, new_owner_id).await
            .ok_or_else(|| EnterpriseError::UserNotFound(new_owner_id.to_string()))?;

        if !new_owner.is_active() {
            return Err(EnterpriseError::PermissionDenied(
                "New owner must be an active member".to_string()
            ));
        }

        // Update roles
        let mut members = self.members.write().await;
        
        // Demote current owner to admin
        if let Some(member) = members.iter_mut().find(|m| m.team_id == team_id && m.user_id == current_owner_id) {
            member.role = TeamRole::Admin;
        }

        // Promote new owner
        if let Some(member) = members.iter_mut().find(|m| m.team_id == team_id && m.user_id == new_owner_id) {
            member.role = TeamRole::Owner;
        }

        Ok(())
    }
}

/// Generate a secure invite token
fn generate_invite_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_team_creation() {
        let creator_id = Uuid::new_v4();
        let team = Team::new("Test Team", "A test team", creator_id);
        
        assert_eq!(team.name, "Test Team");
        assert!(team.is_active());
        assert_eq!(team.created_by, creator_id);
    }

    #[test]
    fn test_team_role_hierarchy() {
        assert!(TeamRole::Owner.level() > TeamRole::Admin.level());
        assert!(TeamRole::Admin.level() > TeamRole::Moderator.level());
        assert!(TeamRole::Moderator.level() > TeamRole::Member.level());
        assert!(TeamRole::Member.level() > TeamRole::Viewer.level());
        assert!(TeamRole::Viewer.level() > TeamRole::Guest.level());
    }

    #[test]
    fn test_role_can_manage() {
        assert!(TeamRole::Owner.can_manage(&TeamRole::Admin));
        assert!(TeamRole::Admin.can_manage(&TeamRole::Member));
        assert!(!TeamRole::Member.can_manage(&TeamRole::Admin));
        assert!(!TeamRole::Member.can_manage(&TeamRole::Member));
    }

    #[test]
    fn test_team_member() {
        let team_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let member = TeamMember::new(team_id, user_id, TeamRole::Member);
        
        assert_eq!(member.team_id, team_id);
        assert_eq!(member.user_id, user_id);
        assert!(member.is_active());
        assert!(!member.can_manage());
    }

    #[test]
    fn test_team_invite() {
        let team_id = Uuid::new_v4();
        let invited_by = Uuid::new_v4();
        let invite = TeamInvite::new(team_id, "test@example.com", TeamRole::Member, invited_by, 7);
        
        assert_eq!(invite.email, "test@example.com");
        assert!(invite.is_valid());
        assert!(!invite.is_expired());
    }

    #[tokio::test]
    async fn test_team_manager_creation() {
        let config = TeamConfig::default();
        let manager = TeamManager::new(config);
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_create_and_get_team() {
        let config = TeamConfig::default();
        let manager = TeamManager::new(config).unwrap();
        
        let creator_id = Uuid::new_v4();
        let team = manager.create_team("Test", "Test team", creator_id).await.unwrap();
        
        let retrieved = manager.get_team(team.id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test");
        
        // Creator should be owner
        let member = manager.get_member(team.id, creator_id).await;
        assert!(member.is_some());
        assert_eq!(member.unwrap().role, TeamRole::Owner);
    }
}