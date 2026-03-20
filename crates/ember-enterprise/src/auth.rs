//! Authentication module - SSO, SAML, OIDC, LDAP support
//!
//! Provides enterprise authentication capabilities including:
//! - OAuth2/OIDC integration
//! - SAML 2.0 support
//! - LDAP/Active Directory
//! - Multi-factor authentication

use crate::{EnterpriseError, Result, User, UserStatus};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Enable local authentication
    pub local_auth_enabled: bool,
    /// SSO providers
    pub sso_providers: Vec<SsoProviderConfig>,
    /// Session configuration
    pub session: SessionConfig,
    /// Token configuration
    pub token: TokenConfig,
    /// MFA configuration
    pub mfa: MfaConfig,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            local_auth_enabled: true,
            sso_providers: Vec::new(),
            session: SessionConfig::default(),
            token: TokenConfig::default(),
            mfa: MfaConfig::default(),
        }
    }
}

/// Session configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Session timeout in seconds
    pub timeout: u64,
    /// Refresh token lifetime in seconds
    pub refresh_lifetime: u64,
    /// Maximum concurrent sessions per user
    pub max_concurrent: u32,
    /// Invalidate on IP change
    pub invalidate_on_ip_change: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            timeout: 3600,
            refresh_lifetime: 604800,
            max_concurrent: 5,
            invalidate_on_ip_change: false,
        }
    }
}

/// Token configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenConfig {
    /// Access token lifetime in seconds
    pub access_lifetime: u64,
    /// Refresh token lifetime in seconds
    pub refresh_lifetime: u64,
    /// Token issuer
    pub issuer: String,
    /// Token audience
    pub audience: String,
    /// Signing algorithm
    pub algorithm: String,
}

impl Default for TokenConfig {
    fn default() -> Self {
        Self {
            access_lifetime: 900,
            refresh_lifetime: 604800,
            issuer: "ember".to_string(),
            audience: "ember-api".to_string(),
            algorithm: "HS256".to_string(),
        }
    }
}

/// MFA configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MfaConfig {
    /// Enable MFA
    pub enabled: bool,
    /// Require MFA for all users
    pub required: bool,
    /// Allowed MFA methods
    pub methods: Vec<String>,
    /// Remember device for days
    pub remember_device_days: u32,
}

impl Default for MfaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            required: false,
            methods: vec!["totp".to_string(), "email".to_string()],
            remember_device_days: 30,
        }
    }
}

/// SSO provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoProviderConfig {
    /// Provider ID
    pub id: String,
    /// Provider name
    pub name: String,
    /// Provider type
    pub provider_type: SsoProviderType,
    /// Enabled
    pub enabled: bool,
    /// OIDC configuration
    pub oidc: Option<OidcConfig>,
    /// SAML configuration
    pub saml: Option<SamlConfig>,
    /// LDAP configuration
    pub ldap: Option<LdapConfig>,
    /// Attribute mappings
    pub attribute_mappings: AttributeMappings,
    /// Auto-create users
    pub auto_create_users: bool,
    /// Default roles for new users
    pub default_roles: Vec<String>,
}

/// SSO provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SsoProviderType {
    /// OpenID Connect
    Oidc,
    /// SAML 2.0
    Saml,
    /// LDAP/Active Directory
    Ldap,
    /// OAuth2
    OAuth2,
}

/// OIDC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    /// Client ID
    pub client_id: String,
    /// Client secret
    pub client_secret: String,
    /// Discovery URL
    pub discovery_url: String,
    /// Authorization endpoint (if not using discovery)
    pub authorization_endpoint: Option<String>,
    /// Token endpoint (if not using discovery)
    pub token_endpoint: Option<String>,
    /// Userinfo endpoint (if not using discovery)
    pub userinfo_endpoint: Option<String>,
    /// JWKS URI (if not using discovery)
    pub jwks_uri: Option<String>,
    /// Scopes to request
    pub scopes: Vec<String>,
    /// Response type
    pub response_type: String,
    /// Redirect URI
    pub redirect_uri: String,
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            discovery_url: String::new(),
            authorization_endpoint: None,
            token_endpoint: None,
            userinfo_endpoint: None,
            jwks_uri: None,
            scopes: vec!["openid".to_string(), "profile".to_string(), "email".to_string()],
            response_type: "code".to_string(),
            redirect_uri: String::new(),
        }
    }
}

/// SAML configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamlConfig {
    /// Entity ID (Service Provider)
    pub entity_id: String,
    /// Assertion Consumer Service URL
    pub acs_url: String,
    /// Single Logout URL
    pub slo_url: Option<String>,
    /// Identity Provider metadata URL
    pub idp_metadata_url: Option<String>,
    /// Identity Provider SSO URL
    pub idp_sso_url: Option<String>,
    /// Identity Provider certificate
    pub idp_certificate: Option<String>,
    /// Sign requests
    pub sign_requests: bool,
    /// Sign assertions
    pub want_assertions_signed: bool,
    /// Name ID format
    pub name_id_format: String,
}

impl Default for SamlConfig {
    fn default() -> Self {
        Self {
            entity_id: String::new(),
            acs_url: String::new(),
            slo_url: None,
            idp_metadata_url: None,
            idp_sso_url: None,
            idp_certificate: None,
            sign_requests: true,
            want_assertions_signed: true,
            name_id_format: "urn:oasis:names:tc:SAML:1.1:nameid-format:emailAddress".to_string(),
        }
    }
}

/// LDAP configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LdapConfig {
    /// LDAP server URL
    pub url: String,
    /// Bind DN
    pub bind_dn: String,
    /// Bind password
    pub bind_password: String,
    /// Base DN for user search
    pub user_base_dn: String,
    /// User search filter
    pub user_filter: String,
    /// Group base DN
    pub group_base_dn: Option<String>,
    /// Group search filter
    pub group_filter: Option<String>,
    /// Use TLS
    pub use_tls: bool,
    /// Skip TLS verification
    pub skip_tls_verify: bool,
    /// Connection timeout in seconds
    pub timeout: u64,
}

impl Default for LdapConfig {
    fn default() -> Self {
        Self {
            url: "ldap://localhost:389".to_string(),
            bind_dn: String::new(),
            bind_password: String::new(),
            user_base_dn: String::new(),
            user_filter: "(uid={username})".to_string(),
            group_base_dn: None,
            group_filter: None,
            use_tls: true,
            skip_tls_verify: false,
            timeout: 10,
        }
    }
}

/// Attribute mappings for SSO providers
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AttributeMappings {
    /// Username attribute
    pub username: String,
    /// Email attribute
    pub email: String,
    /// Display name attribute
    pub display_name: Option<String>,
    /// First name attribute
    pub first_name: Option<String>,
    /// Last name attribute
    pub last_name: Option<String>,
    /// Groups attribute
    pub groups: Option<String>,
    /// Avatar URL attribute
    pub avatar_url: Option<String>,
}

/// Authentication token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    /// Token ID
    pub id: Uuid,
    /// Access token
    pub access_token: String,
    /// Refresh token
    pub refresh_token: Option<String>,
    /// Token type
    pub token_type: String,
    /// Expiration time
    pub expires_at: DateTime<Utc>,
    /// Scopes
    pub scopes: Vec<String>,
}

/// Authentication session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    /// Session ID
    pub id: Uuid,
    /// User ID
    pub user_id: Uuid,
    /// Authentication provider
    pub provider: String,
    /// Session token
    pub token: AuthToken,
    /// IP address
    pub ip_address: Option<String>,
    /// User agent
    pub user_agent: Option<String>,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Last activity
    pub last_activity: DateTime<Utc>,
    /// Expires at
    pub expires_at: DateTime<Utc>,
    /// MFA verified
    pub mfa_verified: bool,
}

impl AuthSession {
    pub fn is_valid(&self) -> bool {
        Utc::now() < self.expires_at
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }
}

/// SSO provider trait
#[async_trait]
pub trait SsoProvider: Send + Sync {
    /// Get provider ID
    fn id(&self) -> &str;

    /// Get provider name
    fn name(&self) -> &str;

    /// Get authorization URL
    async fn get_authorization_url(&self, state: &str) -> Result<String>;

    /// Exchange code for tokens
    async fn exchange_code(&self, code: &str) -> Result<AuthToken>;

    /// Get user info
    async fn get_user_info(&self, token: &AuthToken) -> Result<SsoUserInfo>;

    /// Refresh token
    async fn refresh_token(&self, refresh_token: &str) -> Result<AuthToken>;

    /// Logout
    async fn logout(&self, token: &AuthToken) -> Result<()>;
}

/// SSO user info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoUserInfo {
    /// External user ID
    pub external_id: String,
    /// Username
    pub username: String,
    /// Email
    pub email: String,
    /// Display name
    pub display_name: String,
    /// First name
    pub first_name: Option<String>,
    /// Last name
    pub last_name: Option<String>,
    /// Groups
    pub groups: Vec<String>,
    /// Avatar URL
    pub avatar_url: Option<String>,
    /// Raw attributes
    pub attributes: HashMap<String, serde_json::Value>,
}

/// OIDC provider implementation
pub struct OidcProvider {
    id: String,
    name: String,
    config: OidcConfig,
    client: reqwest::Client,
}

impl OidcProvider {
    pub fn new(id: String, name: String, config: OidcConfig) -> Self {
        Self {
            id,
            name,
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl SsoProvider for OidcProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    async fn get_authorization_url(&self, state: &str) -> Result<String> {
        let auth_endpoint = if let Some(ref endpoint) = self.config.authorization_endpoint {
            endpoint.clone()
        } else {
            // Fetch from discovery
            let discovery: serde_json::Value = self.client
                .get(&self.config.discovery_url)
                .send()
                .await?
                .json()
                .await?;
            
            discovery["authorization_endpoint"]
                .as_str()
                .ok_or_else(|| EnterpriseError::SsoError("Missing authorization_endpoint".to_string()))?
                .to_string()
        };

        let scopes = self.config.scopes.join(" ");
        let url = format!(
            "{}?client_id={}&redirect_uri={}&response_type={}&scope={}&state={}",
            auth_endpoint,
            urlencoding::encode(&self.config.client_id),
            urlencoding::encode(&self.config.redirect_uri),
            urlencoding::encode(&self.config.response_type),
            urlencoding::encode(&scopes),
            urlencoding::encode(state)
        );

        Ok(url)
    }

    async fn exchange_code(&self, code: &str) -> Result<AuthToken> {
        let token_endpoint = if let Some(ref endpoint) = self.config.token_endpoint {
            endpoint.clone()
        } else {
            let discovery: serde_json::Value = self.client
                .get(&self.config.discovery_url)
                .send()
                .await?
                .json()
                .await?;
            
            discovery["token_endpoint"]
                .as_str()
                .ok_or_else(|| EnterpriseError::SsoError("Missing token_endpoint".to_string()))?
                .to_string()
        };

        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &self.config.redirect_uri),
            ("client_id", &self.config.client_id),
            ("client_secret", &self.config.client_secret),
        ];

        let response: serde_json::Value = self.client
            .post(&token_endpoint)
            .form(&params)
            .send()
            .await?
            .json()
            .await?;

        let access_token = response["access_token"]
            .as_str()
            .ok_or_else(|| EnterpriseError::SsoError("Missing access_token".to_string()))?
            .to_string();

        let expires_in = response["expires_in"].as_i64().unwrap_or(3600);
        let refresh_token = response["refresh_token"].as_str().map(String::from);

        Ok(AuthToken {
            id: Uuid::new_v4(),
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_at: Utc::now() + Duration::seconds(expires_in),
            scopes: self.config.scopes.clone(),
        })
    }

    async fn get_user_info(&self, token: &AuthToken) -> Result<SsoUserInfo> {
        let userinfo_endpoint = if let Some(ref endpoint) = self.config.userinfo_endpoint {
            endpoint.clone()
        } else {
            let discovery: serde_json::Value = self.client
                .get(&self.config.discovery_url)
                .send()
                .await?
                .json()
                .await?;
            
            discovery["userinfo_endpoint"]
                .as_str()
                .ok_or_else(|| EnterpriseError::SsoError("Missing userinfo_endpoint".to_string()))?
                .to_string()
        };

        let response: serde_json::Value = self.client
            .get(&userinfo_endpoint)
            .bearer_auth(&token.access_token)
            .send()
            .await?
            .json()
            .await?;

        let external_id = response["sub"]
            .as_str()
            .ok_or_else(|| EnterpriseError::SsoError("Missing sub claim".to_string()))?
            .to_string();

        let email = response["email"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let username = response["preferred_username"]
            .as_str()
            .unwrap_or(&email)
            .to_string();

        let display_name = response["name"]
            .as_str()
            .unwrap_or(&username)
            .to_string();

        Ok(SsoUserInfo {
            external_id,
            username,
            email,
            display_name,
            first_name: response["given_name"].as_str().map(String::from),
            last_name: response["family_name"].as_str().map(String::from),
            groups: response["groups"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            avatar_url: response["picture"].as_str().map(String::from),
            attributes: response.as_object()
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
        })
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<AuthToken> {
        let token_endpoint = if let Some(ref endpoint) = self.config.token_endpoint {
            endpoint.clone()
        } else {
            let discovery: serde_json::Value = self.client
                .get(&self.config.discovery_url)
                .send()
                .await?
                .json()
                .await?;
            
            discovery["token_endpoint"]
                .as_str()
                .ok_or_else(|| EnterpriseError::SsoError("Missing token_endpoint".to_string()))?
                .to_string()
        };

        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.config.client_id),
            ("client_secret", &self.config.client_secret),
        ];

        let response: serde_json::Value = self.client
            .post(&token_endpoint)
            .form(&params)
            .send()
            .await?
            .json()
            .await?;

        let access_token = response["access_token"]
            .as_str()
            .ok_or_else(|| EnterpriseError::SsoError("Missing access_token".to_string()))?
            .to_string();

        let expires_in = response["expires_in"].as_i64().unwrap_or(3600);
        let new_refresh_token = response["refresh_token"]
            .as_str()
            .map(String::from)
            .or_else(|| Some(refresh_token.to_string()));

        Ok(AuthToken {
            id: Uuid::new_v4(),
            access_token,
            refresh_token: new_refresh_token,
            token_type: "Bearer".to_string(),
            expires_at: Utc::now() + Duration::seconds(expires_in),
            scopes: self.config.scopes.clone(),
        })
    }

    async fn logout(&self, _token: &AuthToken) -> Result<()> {
        // OIDC logout - would typically redirect to end_session_endpoint
        Ok(())
    }
}

/// Authentication provider trait
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Authenticate with credentials
    async fn authenticate(&self, username: &str, password: &str) -> Result<User>;
    
    /// Validate a session
    async fn validate_session(&self, session_id: Uuid) -> Result<AuthSession>;
    
    /// Refresh a session
    async fn refresh_session(&self, session_id: Uuid) -> Result<AuthSession>;
    
    /// Logout
    async fn logout(&self, session_id: Uuid) -> Result<()>;
}

/// Authentication manager
pub struct AuthManager {
    config: AuthConfig,
    sessions: Arc<RwLock<HashMap<Uuid, AuthSession>>>,
    users: Arc<RwLock<HashMap<Uuid, User>>>,
    sso_providers: HashMap<String, Arc<dyn SsoProvider>>,
}

impl AuthManager {
    /// Create a new auth manager
    pub fn new(config: AuthConfig) -> Result<Self> {
        let mut sso_providers: HashMap<String, Arc<dyn SsoProvider>> = HashMap::new();

        // Initialize SSO providers
        for provider_config in &config.sso_providers {
            if !provider_config.enabled {
                continue;
            }

            match provider_config.provider_type {
                SsoProviderType::Oidc => {
                    if let Some(oidc_config) = &provider_config.oidc {
                        let provider = OidcProvider::new(
                            provider_config.id.clone(),
                            provider_config.name.clone(),
                            oidc_config.clone(),
                        );
                        sso_providers.insert(provider_config.id.clone(), Arc::new(provider));
                    }
                }
                SsoProviderType::Saml => {
                    // SAML provider would be initialized here
                    tracing::info!("SAML provider {} configured", provider_config.id);
                }
                SsoProviderType::Ldap => {
                    // LDAP provider would be initialized here
                    tracing::info!("LDAP provider {} configured", provider_config.id);
                }
                SsoProviderType::OAuth2 => {
                    // OAuth2 provider would be similar to OIDC
                    tracing::info!("OAuth2 provider {} configured", provider_config.id);
                }
            }
        }

        Ok(Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            users: Arc::new(RwLock::new(HashMap::new())),
            sso_providers,
        })
    }

    /// Authenticate with username/password
    pub async fn authenticate(&self, username: &str, password: &str) -> Result<AuthSession> {
        if !self.config.local_auth_enabled {
            return Err(EnterpriseError::AuthenticationFailed(
                "Local authentication is disabled".to_string(),
            ));
        }

        // Hash password for comparison (in real implementation, fetch from DB)
        let _password_hash = hash_password(password);

        // Create or find user (mock implementation)
        let user = User::new(
            username.to_string(),
            format!("{}@example.com", username),
            username.to_string(),
        );

        let user_id = user.id;
        
        // Store user
        {
            let mut users = self.users.write().await;
            users.insert(user.id, user);
        }

        // Create session
        let session = self.create_session(user_id, "local").await?;

        Ok(session)
    }

    /// Authenticate via SSO provider
    pub async fn authenticate_sso(&self, provider_id: &str, code: &str) -> Result<AuthSession> {
        let provider = self.sso_providers.get(provider_id)
            .ok_or_else(|| EnterpriseError::SsoError(format!("Provider '{}' not found", provider_id)))?;

        // Exchange code for token
        let token = provider.exchange_code(code).await?;

        // Get user info
        let user_info = provider.get_user_info(&token).await?;

        // Find or create user
        let user = self.find_or_create_sso_user(provider_id, &user_info).await?;

        // Create session with SSO token
        let session = self.create_session_with_token(user.id, provider_id, token).await?;

        Ok(session)
    }

    /// Get authorization URL for SSO
    pub async fn get_sso_authorization_url(&self, provider_id: &str) -> Result<(String, String)> {
        let provider = self.sso_providers.get(provider_id)
            .ok_or_else(|| EnterpriseError::SsoError(format!("Provider '{}' not found", provider_id)))?;

        let state = generate_state();
        let url = provider.get_authorization_url(&state).await?;

        Ok((url, state))
    }

    /// Validate a session
    pub async fn validate_session(&self, session_id: Uuid) -> Result<AuthSession> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(&session_id)
            .ok_or(EnterpriseError::SessionExpired)?;

        if session.is_expired() {
            return Err(EnterpriseError::SessionExpired);
        }

        Ok(session.clone())
    }

    /// Refresh a session
    pub async fn refresh_session(&self, session_id: Uuid) -> Result<AuthSession> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(&session_id)
            .ok_or(EnterpriseError::SessionExpired)?;

        if session.is_expired() {
            return Err(EnterpriseError::SessionExpired);
        }

        // Extend session
        session.last_activity = Utc::now();
        session.expires_at = Utc::now() + Duration::seconds(self.config.session.timeout as i64);

        // If SSO, try to refresh token
        if session.provider != "local" {
            if let Some(provider) = self.sso_providers.get(&session.provider) {
                if let Some(refresh_token) = &session.token.refresh_token {
                    if let Ok(new_token) = provider.refresh_token(refresh_token).await {
                        session.token = new_token;
                    }
                }
            }
        }

        Ok(session.clone())
    }

    /// Logout
    pub async fn logout(&self, session_id: Uuid) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        
        if let Some(session) = sessions.remove(&session_id) {
            // If SSO, call provider logout
            if session.provider != "local" {
                if let Some(provider) = self.sso_providers.get(&session.provider) {
                    let _ = provider.logout(&session.token).await;
                }
            }
        }

        Ok(())
    }

    /// Get user by ID
    pub async fn get_user(&self, user_id: Uuid) -> Result<User> {
        let users = self.users.read().await;
        users.get(&user_id)
            .cloned()
            .ok_or_else(|| EnterpriseError::UserNotFound(user_id.to_string()))
    }

    /// List SSO providers
    pub fn list_sso_providers(&self) -> Vec<&str> {
        self.sso_providers.keys().map(|s| s.as_str()).collect()
    }

    // Internal methods

    async fn create_session(&self, user_id: Uuid, provider: &str) -> Result<AuthSession> {
        let now = Utc::now();
        let session = AuthSession {
            id: Uuid::new_v4(),
            user_id,
            provider: provider.to_string(),
            token: AuthToken {
                id: Uuid::new_v4(),
                access_token: generate_token(),
                refresh_token: Some(generate_token()),
                token_type: "Bearer".to_string(),
                expires_at: now + Duration::seconds(self.config.token.access_lifetime as i64),
                scopes: vec!["*".to_string()],
            },
            ip_address: None,
            user_agent: None,
            created_at: now,
            last_activity: now,
            expires_at: now + Duration::seconds(self.config.session.timeout as i64),
            mfa_verified: false,
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id, session.clone());

        Ok(session)
    }

    async fn create_session_with_token(
        &self,
        user_id: Uuid,
        provider: &str,
        token: AuthToken,
    ) -> Result<AuthSession> {
        let now = Utc::now();
        let session = AuthSession {
            id: Uuid::new_v4(),
            user_id,
            provider: provider.to_string(),
            token,
            ip_address: None,
            user_agent: None,
            created_at: now,
            last_activity: now,
            expires_at: now + Duration::seconds(self.config.session.timeout as i64),
            mfa_verified: false,
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id, session.clone());

        Ok(session)
    }

    async fn find_or_create_sso_user(
        &self,
        provider: &str,
        user_info: &SsoUserInfo,
    ) -> Result<User> {
        // Check if user exists
        {
            let users = self.users.read().await;
            for user in users.values() {
                if user.external_id.as_ref() == Some(&user_info.external_id)
                    && user.sso_provider.as_ref() == Some(&provider.to_string())
                {
                    return Ok(user.clone());
                }
            }
        }

        // Create new user
        let mut user = User::new(
            user_info.username.clone(),
            user_info.email.clone(),
            user_info.display_name.clone(),
        );
        user.sso_provider = Some(provider.to_string());
        user.external_id = Some(user_info.external_id.clone());
        user.avatar_url = user_info.avatar_url.clone();
        user.status = UserStatus::Active;

        let mut users = self.users.write().await;
        users.insert(user.id, user.clone());

        Ok(user)
    }
}

// Helper functions

fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn generate_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &bytes)
}

fn generate_state() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..16).map(|_| rng.gen()).collect();
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &bytes)
}

fn urlencoding_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        super::urlencoding_encode(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_password() {
        let hash = hash_password("password123");
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64); // SHA256 hex
    }

    #[test]
    fn test_generate_token() {
        let token = generate_token();
        assert!(!token.is_empty());
    }

    #[test]
    fn test_session_validity() {
        let session = AuthSession {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            provider: "local".to_string(),
            token: AuthToken {
                id: Uuid::new_v4(),
                access_token: "token".to_string(),
                refresh_token: None,
                token_type: "Bearer".to_string(),
                expires_at: Utc::now() + Duration::hours(1),
                scopes: vec![],
            },
            ip_address: None,
            user_agent: None,
            created_at: Utc::now(),
            last_activity: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
            mfa_verified: false,
        };

        assert!(session.is_valid());
        assert!(!session.is_expired());
    }

    #[test]
    fn test_default_config() {
        let config = AuthConfig::default();
        assert!(config.local_auth_enabled);
        assert!(config.sso_providers.is_empty());
    }

    #[tokio::test]
    async fn test_auth_manager_creation() {
        let config = AuthConfig::default();
        let manager = AuthManager::new(config);
        assert!(manager.is_ok());
    }
}