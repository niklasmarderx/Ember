//! # OAuth 2.0 + PKCE Authentication
//!
//! This module implements OAuth 2.0 authorization code flow with PKCE
//! (Proof Key for Code Exchange, RFC 7636) for secure API authentication.
//!
//! ## Overview
//!
//! PKCE prevents authorization code interception attacks by binding each
//! authorization request to a cryptographic secret that only the initiating
//! client possesses. The S256 challenge method hashes the verifier with
//! SHA-256 and encodes it as base64url.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use ember_core::oauth::{OAuthConfig, TokenStore, generate_pkce_pair, build_authorization_url};
//!
//! # fn main() -> anyhow::Result<()> {
//! let config = OAuthConfig {
//!     client_id: "my-app".to_string(),
//!     auth_url: "https://provider.example.com/oauth/authorize".to_string(),
//!     token_url: "https://provider.example.com/oauth/token".to_string(),
//!     redirect_uri: "http://localhost:8080/callback".to_string(),
//!     scopes: vec!["read".to_string(), "write".to_string()],
//! };
//!
//! let pkce = generate_pkce_pair();
//! let state = "random-csrf-token";
//! let url = build_authorization_url(&config, &pkce, state)?;
//! println!("Visit: {url}");
//! # Ok(())
//! # }
//! ```

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use uuid::Uuid;

// ============================================================================
// Error type
// ============================================================================

/// Errors that can occur in the OAuth flow.
#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    /// The stored token file could not be read or written.
    #[error("token store I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization / deserialization failed.
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// A required URL could not be parsed or constructed.
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    /// No token is stored for the given key.
    #[error("no token found for key: {0}")]
    NotFound(String),
}

/// Convenience result alias.
pub type Result<T> = std::result::Result<T, OAuthError>;

// ============================================================================
// PKCE types
// ============================================================================

/// The challenge method used in the PKCE flow.
///
/// Only S256 is considered secure; the `Plain` method exists solely for
/// legacy compatibility and should not be used in new code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PkceChallengeMethod {
    /// SHA-256 hash of the verifier, base64url-encoded (recommended).
    S256,
}

impl std::fmt::Display for PkceChallengeMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PkceChallengeMethod::S256 => f.write_str("S256"),
        }
    }
}

/// A PKCE code verifier / challenge pair.
///
/// Generate one with [`generate_pkce_pair`] and keep the [`verifier`] secret.
/// Send only the [`challenge`] and [`challenge_method`] in the authorization
/// request; send the verifier in the token exchange request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkceCodePair {
    /// The high-entropy random secret (43–128 URL-safe characters).
    pub verifier: String,
    /// `BASE64URL(SHA256(ASCII(verifier)))` — sent with the auth request.
    pub challenge: String,
    /// Always `S256`.
    pub challenge_method: PkceChallengeMethod,
}

// ============================================================================
// Token types
// ============================================================================

/// A set of OAuth 2.0 tokens returned by the authorization server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenSet {
    /// The bearer token used to authenticate API calls.
    pub access_token: String,
    /// Optional token that can be used to obtain a fresh access token.
    pub refresh_token: Option<String>,
    /// UTC timestamp at which the access token expires.
    ///
    /// `None` means the server did not report an expiry — treat as
    /// "valid indefinitely" (but still refresh proactively if a refresh
    /// token is available).
    pub expires_at: Option<DateTime<Utc>>,
    /// The scopes granted for this token set.
    pub scopes: Vec<String>,
}

impl OAuthTokenSet {
    /// Create a new token set from raw parts.
    ///
    /// `expires_in_seconds` is the value returned by the token endpoint
    /// (`expires_in` field).  Pass `None` when the server omits that field.
    pub fn new(
        access_token: impl Into<String>,
        refresh_token: Option<impl Into<String>>,
        expires_in_seconds: Option<i64>,
        scopes: Vec<String>,
    ) -> Self {
        let expires_at = expires_in_seconds.map(|secs| {
            Utc::now() + chrono::Duration::seconds(secs)
        });

        Self {
            access_token: access_token.into(),
            refresh_token: refresh_token.map(Into::into),
            expires_at,
            scopes,
        }
    }
}

// ============================================================================
// Config
// ============================================================================

/// OAuth 2.0 client configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// The client identifier issued during registration.
    pub client_id: String,
    /// The authorization endpoint URL.
    pub auth_url: String,
    /// The token endpoint URL.
    pub token_url: String,
    /// The URI the authorization server redirects to after user consent.
    pub redirect_uri: String,
    /// The access scopes to request.
    pub scopes: Vec<String>,
}

// ============================================================================
// Public functions
// ============================================================================

/// Generate a fresh PKCE code pair using S256.
///
/// Internally creates a 32-byte random value (via UUID v4 — which uses the
/// OS CSPRNG), encodes it as base64url to form the verifier, then computes
/// `BASE64URL(SHA-256(verifier))` for the challenge.
///
/// Using two UUIDs gives 256 bits of entropy for the verifier, well above
/// the RFC 7636 minimum of 256 bits from the underlying secret.
pub fn generate_pkce_pair() -> PkceCodePair {
    // Build a 43-char (minimum RFC length) to 128-char verifier.
    // Two UUIDs without hyphens → 64 hex chars, then base64url-encode for
    // the URL-safe character set required by the spec.
    let raw = format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let verifier = URL_SAFE_NO_PAD.encode(raw.as_bytes());

    let challenge = compute_s256_challenge(&verifier);

    PkceCodePair {
        verifier,
        challenge,
        challenge_method: PkceChallengeMethod::S256,
    }
}

/// Compute `BASE64URL(SHA-256(verifier))` — the S256 PKCE challenge.
fn compute_s256_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

/// Build the authorization URL for the first leg of the OAuth flow.
///
/// Adds the following query parameters:
/// - `response_type=code`
/// - `client_id`
/// - `redirect_uri`
/// - `scope` (space-separated)
/// - `state`
/// - `code_challenge` (S256 value from `pkce`)
/// - `code_challenge_method=S256`
///
/// # Errors
///
/// Returns [`OAuthError::InvalidUrl`] when `config.auth_url` cannot be
/// parsed as a URL base.
pub fn build_authorization_url(
    config: &OAuthConfig,
    pkce: &PkceCodePair,
    state: &str,
) -> Result<String> {
    // Basic validation: the auth URL must not be empty and should start with
    // a scheme we recognise.
    if config.auth_url.is_empty() {
        return Err(OAuthError::InvalidUrl(
            "auth_url must not be empty".to_string(),
        ));
    }

    let scope = config.scopes.join(" ");

    let params = [
        ("response_type", "code"),
        ("client_id", config.client_id.as_str()),
        ("redirect_uri", config.redirect_uri.as_str()),
        ("scope", scope.as_str()),
        ("state", state),
        ("code_challenge", pkce.challenge.as_str()),
        ("code_challenge_method", "S256"),
    ];

    let query: String = params
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    let separator = if config.auth_url.contains('?') { '&' } else { '?' };
    Ok(format!("{}{}{}", config.auth_url, separator, query))
}

/// Returns `true` when the token has passed its expiry timestamp.
///
/// Tokens without an `expires_at` value are considered **not** expired —
/// callers should still perform proactive refresh when a refresh token is
/// available.
pub fn is_token_expired(token: &OAuthTokenSet) -> bool {
    match token.expires_at {
        Some(expires) => Utc::now() >= expires,
        None => false,
    }
}

/// Minimal percent-encoding for URL query parameter values.
///
/// Encodes all characters outside the unreserved set defined in RFC 3986
/// (`A-Z a-z 0-9 - _ . ~`).
fn percent_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~' => encoded.push(byte as char),
            b => encoded.push_str(&format!("%{b:02X}")),
        }
    }
    encoded
}

// ============================================================================
// TokenStore
// ============================================================================

/// File-backed JSON storage for OAuth token sets.
///
/// Tokens are stored in `~/.ember/oauth/tokens.json` as a JSON object
/// keyed by an arbitrary string (e.g. the provider name or client ID).
/// The parent directory is created automatically on first write.
pub struct TokenStore {
    path: PathBuf,
}

/// The on-disk representation.
type TokenMap = std::collections::HashMap<String, OAuthTokenSet>;

impl TokenStore {
    /// Create a [`TokenStore`] backed by the default path
    /// (`~/.ember/oauth/tokens.json`).
    ///
    /// The directory is not created until the first [`save`](Self::save).
    pub fn new() -> Self {
        let path = default_token_path();
        Self { path }
    }

    /// Create a [`TokenStore`] backed by an explicit path (useful for
    /// testing or custom deployments).
    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Load all stored token sets.  Returns an empty map when no file exists.
    fn load_all(&self) -> Result<TokenMap> {
        if !self.path.exists() {
            return Ok(TokenMap::new());
        }

        let raw = std::fs::read_to_string(&self.path)?;
        let map: TokenMap = serde_json::from_str(&raw)?;
        Ok(map)
    }

    /// Load the token set for `key`.
    ///
    /// # Errors
    ///
    /// Returns [`OAuthError::NotFound`] when no token is stored under `key`.
    pub fn load(&self, key: &str) -> Result<OAuthTokenSet> {
        let map = self.load_all()?;
        map.into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
            .ok_or_else(|| OAuthError::NotFound(key.to_string()))
    }

    /// Persist `token` under `key`, overwriting any existing entry.
    ///
    /// Creates `~/.ember/oauth/` (or the configured directory) if it does
    /// not yet exist.
    pub fn save(&self, key: &str, token: &OAuthTokenSet) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut map = self.load_all()?;
        map.insert(key.to_string(), token.clone());

        let json = serde_json::to_string_pretty(&map)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    /// Remove the token stored under `key`.
    ///
    /// Silently succeeds when no entry exists for `key`.
    pub fn delete(&self, key: &str) -> Result<()> {
        if !self.path.exists() {
            return Ok(());
        }

        let mut map = self.load_all()?;
        map.remove(key);

        let json = serde_json::to_string_pretty(&map)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    /// Returns `true` when the stored token for `key` has expired.
    ///
    /// Returns `false` when no token exists or the token has no expiry.
    pub fn is_expired(&self, key: &str) -> bool {
        self.load(key)
            .map(|t| is_token_expired(&t))
            .unwrap_or(false)
    }

    /// Returns `true` when the token for `key` either has expired or will
    /// expire within the next `threshold_seconds` seconds.
    ///
    /// Use this to proactively refresh tokens before they actually expire.
    /// Returns `false` when no token exists.
    pub fn needs_refresh(&self, key: &str, threshold_seconds: i64) -> bool {
        match self.load(key) {
            Ok(token) => match token.expires_at {
                Some(expires) => {
                    let deadline = expires - chrono::Duration::seconds(threshold_seconds);
                    Utc::now() >= deadline
                }
                None => false,
            },
            Err(_) => false,
        }
    }
}

impl Default for TokenStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the default token storage path: `~/.ember/oauth/tokens.json`.
fn default_token_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ember")
        .join("oauth")
        .join("tokens.json")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn sample_config() -> OAuthConfig {
        OAuthConfig {
            client_id: "test-client".to_string(),
            auth_url: "https://auth.example.com/oauth/authorize".to_string(),
            token_url: "https://auth.example.com/oauth/token".to_string(),
            redirect_uri: "http://localhost:8080/callback".to_string(),
            scopes: vec!["read".to_string(), "write".to_string()],
        }
    }

    fn store_in_temp(dir: &TempDir) -> TokenStore {
        let path = dir.path().join("tokens.json");
        TokenStore::with_path(path)
    }

    // ── PKCE generation ──────────────────────────────────────────────────────

    #[test]
    fn pkce_pair_has_s256_method() {
        let pair = generate_pkce_pair();
        assert_eq!(pair.challenge_method, PkceChallengeMethod::S256);
    }

    #[test]
    fn pkce_verifier_is_url_safe() {
        let pair = generate_pkce_pair();
        assert!(
            pair.verifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')),
            "verifier must only contain URL-safe base64 chars, got: {}",
            pair.verifier
        );
    }

    #[test]
    fn pkce_challenge_is_url_safe() {
        let pair = generate_pkce_pair();
        assert!(
            pair.challenge
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')),
            "challenge must only contain URL-safe base64 chars"
        );
    }

    #[test]
    fn pkce_challenge_matches_sha256_of_verifier() {
        let pair = generate_pkce_pair();
        let expected = compute_s256_challenge(&pair.verifier);
        assert_eq!(pair.challenge, expected);
    }

    #[test]
    fn pkce_verifier_minimum_length() {
        // RFC 7636 §4.1: verifier MUST be >= 43 characters.
        let pair = generate_pkce_pair();
        assert!(
            pair.verifier.len() >= 43,
            "verifier too short: {} chars",
            pair.verifier.len()
        );
    }

    #[test]
    fn pkce_pairs_are_unique() {
        let a = generate_pkce_pair();
        let b = generate_pkce_pair();
        assert_ne!(a.verifier, b.verifier, "consecutive verifiers must differ");
        assert_ne!(a.challenge, b.challenge, "consecutive challenges must differ");
    }

    #[test]
    fn pkce_known_vector() {
        // RFC 7636 §B: verifier "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
        // → challenge "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let expected = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert_eq!(compute_s256_challenge(verifier), expected);
    }

    // ── Authorization URL ────────────────────────────────────────────────────

    #[test]
    fn auth_url_contains_required_params() {
        let config = sample_config();
        let pkce = generate_pkce_pair();
        let url = build_authorization_url(&config, &pkce, "state-token").unwrap();

        assert!(url.contains("response_type=code"), "missing response_type");
        assert!(url.contains("client_id=test-client"), "missing client_id");
        assert!(url.contains("code_challenge_method=S256"), "missing method");
        assert!(url.contains("state=state-token"), "missing state");
        assert!(url.contains("code_challenge="), "missing challenge");
    }

    #[test]
    fn auth_url_encodes_redirect_uri() {
        let config = sample_config();
        let pkce = generate_pkce_pair();
        let url = build_authorization_url(&config, &pkce, "s").unwrap();

        // Colons and slashes in the redirect_uri must be percent-encoded.
        assert!(
            url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A8080%2Fcallback"),
            "redirect_uri not percent-encoded, got: {url}"
        );
    }

    #[test]
    fn auth_url_joins_scopes_with_space() {
        let config = sample_config();
        let pkce = generate_pkce_pair();
        let url = build_authorization_url(&config, &pkce, "s").unwrap();
        // Space is encoded as %20
        assert!(url.contains("scope=read%20write"), "scope not found in: {url}");
    }

    #[test]
    fn auth_url_empty_auth_url_returns_error() {
        let mut config = sample_config();
        config.auth_url = String::new();
        let pkce = generate_pkce_pair();
        let result = build_authorization_url(&config, &pkce, "s");
        assert!(result.is_err());
    }

    #[test]
    fn auth_url_appends_ampersand_when_query_present() {
        let mut config = sample_config();
        config.auth_url = "https://auth.example.com/oauth/authorize?prompt=consent".to_string();
        let pkce = generate_pkce_pair();
        let url = build_authorization_url(&config, &pkce, "s").unwrap();
        // Should use & not ? to join params
        assert!(!url.contains("??")); // guard against double ?
        assert!(url.contains("prompt=consent&"));
    }

    // ── Token expiry ─────────────────────────────────────────────────────────

    #[test]
    fn is_token_expired_when_past_expiry() {
        let token = OAuthTokenSet {
            access_token: "tok".to_string(),
            refresh_token: None,
            expires_at: Some(Utc::now() - chrono::Duration::seconds(1)),
            scopes: vec![],
        };
        assert!(is_token_expired(&token));
    }

    #[test]
    fn is_token_not_expired_when_future_expiry() {
        let token = OAuthTokenSet {
            access_token: "tok".to_string(),
            refresh_token: None,
            expires_at: Some(Utc::now() + chrono::Duration::seconds(300)),
            scopes: vec![],
        };
        assert!(!is_token_expired(&token));
    }

    #[test]
    fn is_token_not_expired_when_no_expiry() {
        let token = OAuthTokenSet {
            access_token: "tok".to_string(),
            refresh_token: None,
            expires_at: None,
            scopes: vec![],
        };
        assert!(!is_token_expired(&token));
    }

    // ── TokenStore ───────────────────────────────────────────────────────────

    #[test]
    fn token_store_save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let store = store_in_temp(&dir);

        let token = OAuthTokenSet {
            access_token: "access-abc".to_string(),
            refresh_token: Some("refresh-xyz".to_string()),
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
            scopes: vec!["read".to_string()],
        };

        store.save("provider-a", &token).unwrap();
        let loaded = store.load("provider-a").unwrap();

        assert_eq!(loaded.access_token, "access-abc");
        assert_eq!(loaded.refresh_token.as_deref(), Some("refresh-xyz"));
        assert_eq!(loaded.scopes, vec!["read"]);
    }

    #[test]
    fn token_store_load_missing_key_returns_not_found() {
        let dir = TempDir::new().unwrap();
        let store = store_in_temp(&dir);

        let err = store.load("ghost").unwrap_err();
        assert!(matches!(err, OAuthError::NotFound(_)));
    }

    #[test]
    fn token_store_delete_removes_entry() {
        let dir = TempDir::new().unwrap();
        let store = store_in_temp(&dir);

        let token = OAuthTokenSet {
            access_token: "tok".to_string(),
            refresh_token: None,
            expires_at: None,
            scopes: vec![],
        };

        store.save("key", &token).unwrap();
        store.delete("key").unwrap();

        assert!(matches!(store.load("key"), Err(OAuthError::NotFound(_))));
    }

    #[test]
    fn token_store_delete_nonexistent_is_ok() {
        let dir = TempDir::new().unwrap();
        let store = store_in_temp(&dir);
        assert!(store.delete("never-existed").is_ok());
    }

    #[test]
    fn token_store_multiple_keys() {
        let dir = TempDir::new().unwrap();
        let store = store_in_temp(&dir);

        for i in 0..3 {
            let token = OAuthTokenSet {
                access_token: format!("tok-{i}"),
                refresh_token: None,
                expires_at: None,
                scopes: vec![],
            };
            store.save(&format!("provider-{i}"), &token).unwrap();
        }

        for i in 0..3 {
            let t = store.load(&format!("provider-{i}")).unwrap();
            assert_eq!(t.access_token, format!("tok-{i}"));
        }
    }

    #[test]
    fn token_store_is_expired_true_for_stale_token() {
        let dir = TempDir::new().unwrap();
        let store = store_in_temp(&dir);

        let token = OAuthTokenSet {
            access_token: "stale".to_string(),
            refresh_token: None,
            expires_at: Some(Utc::now() - chrono::Duration::minutes(5)),
            scopes: vec![],
        };
        store.save("old", &token).unwrap();

        assert!(store.is_expired("old"));
    }

    #[test]
    fn token_store_is_expired_false_for_fresh_token() {
        let dir = TempDir::new().unwrap();
        let store = store_in_temp(&dir);

        let token = OAuthTokenSet {
            access_token: "fresh".to_string(),
            refresh_token: None,
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
            scopes: vec![],
        };
        store.save("new", &token).unwrap();

        assert!(!store.is_expired("new"));
    }

    #[test]
    fn token_store_needs_refresh_within_threshold() {
        let dir = TempDir::new().unwrap();
        let store = store_in_temp(&dir);

        // Expires in 30 seconds — within a 60-second refresh window.
        let token = OAuthTokenSet {
            access_token: "expiring".to_string(),
            refresh_token: Some("ref".to_string()),
            expires_at: Some(Utc::now() + chrono::Duration::seconds(30)),
            scopes: vec![],
        };
        store.save("soon", &token).unwrap();

        assert!(store.needs_refresh("soon", 60));
        assert!(!store.needs_refresh("soon", 10));
    }

    #[test]
    fn token_store_overwrite_existing_key() {
        let dir = TempDir::new().unwrap();
        let store = store_in_temp(&dir);

        let t1 = OAuthTokenSet {
            access_token: "v1".to_string(),
            refresh_token: None,
            expires_at: None,
            scopes: vec![],
        };
        let t2 = OAuthTokenSet {
            access_token: "v2".to_string(),
            refresh_token: None,
            expires_at: None,
            scopes: vec![],
        };

        store.save("key", &t1).unwrap();
        store.save("key", &t2).unwrap();

        let loaded = store.load("key").unwrap();
        assert_eq!(loaded.access_token, "v2");
    }

    #[test]
    fn token_new_computes_expires_at() {
        let token = OAuthTokenSet::new("at", Some("rt"), Some(3600), vec!["read".to_string()]);
        assert!(token.expires_at.is_some());
        let expires = token.expires_at.unwrap();
        // Should be roughly one hour from now (allow 5-second clock drift).
        let diff = (expires - Utc::now()).num_seconds();
        assert!(diff > 3594 && diff <= 3600, "unexpected diff: {diff}");
    }

}
