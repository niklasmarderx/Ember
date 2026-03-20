//! Marketplace Registry Client
//!
//! HTTP client for interacting with the Ember plugin marketplace API.

use super::types::*;
use reqwest::{Client, StatusCode};
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Default marketplace registry URL
pub const DEFAULT_REGISTRY_URL: &str = "https://plugins.ember.ai/api/v1";

/// Marketplace client error
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Plugin not found: {0}")]
    NotFound(String),

    #[error("Version not found: {0}@{1}")]
    VersionNotFound(String, String),

    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("Rate limited, retry after {retry_after} seconds")]
    RateLimited { retry_after: u64 },

    #[error("Authentication required")]
    AuthRequired,

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Parse error: {0}")]
    Parse(String),
}

/// Configuration for the registry client
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Base URL for the registry API
    pub base_url: String,
    /// Request timeout
    pub timeout: Duration,
    /// API token for authenticated requests
    pub api_token: Option<String>,
    /// User agent string
    pub user_agent: String,
    /// Maximum retries for failed requests
    pub max_retries: u32,
    /// Cache directory for downloaded plugins
    pub cache_dir: PathBuf,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_REGISTRY_URL.to_string(),
            timeout: Duration::from_secs(30),
            api_token: None,
            user_agent: format!("ember-cli/{}", env!("CARGO_PKG_VERSION")),
            max_retries: 3,
            cache_dir: dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("ember")
                .join("plugins"),
        }
    }
}

/// Client for the Ember plugin marketplace
#[derive(Debug, Clone)]
pub struct RegistryClient {
    client: Client,
    config: RegistryConfig,
}

impl RegistryClient {
    /// Create a new registry client with default configuration
    pub fn new() -> Result<Self, RegistryError> {
        Self::with_config(RegistryConfig::default())
    }

    /// Create a new registry client with custom configuration
    pub fn with_config(config: RegistryConfig) -> Result<Self, RegistryError> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            config.user_agent.parse().unwrap(),
        );

        if let Some(ref token) = config.api_token {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", token).parse().unwrap(),
            );
        }

        let client = Client::builder()
            .timeout(config.timeout)
            .default_headers(headers)
            .build()?;

        Ok(Self { client, config })
    }

    /// Search for plugins in the marketplace
    pub async fn search(&self, query: &SearchQuery) -> Result<SearchResults, RegistryError> {
        let url = format!("{}/plugins/search", self.config.base_url);

        let response = self.client.post(&url).json(query).send().await?;

        self.handle_response(response).await
    }

    /// Get plugin metadata by ID
    pub async fn get_plugin(&self, plugin_id: &str) -> Result<PluginMetadata, RegistryError> {
        let url = format!("{}/plugins/{}", self.config.base_url, plugin_id);

        let response = self.client.get(&url).send().await?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(RegistryError::NotFound(plugin_id.to_string()));
        }

        self.handle_response(response).await
    }

    /// Get a specific version of a plugin
    pub async fn get_plugin_version(
        &self,
        plugin_id: &str,
        version: &str,
    ) -> Result<PluginVersion, RegistryError> {
        let url = format!(
            "{}/plugins/{}/versions/{}",
            self.config.base_url, plugin_id, version
        );

        let response = self.client.get(&url).send().await?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(RegistryError::VersionNotFound(
                plugin_id.to_string(),
                version.to_string(),
            ));
        }

        self.handle_response(response).await
    }

    /// Get all versions of a plugin
    pub async fn get_plugin_versions(
        &self,
        plugin_id: &str,
    ) -> Result<Vec<PluginVersion>, RegistryError> {
        let url = format!("{}/plugins/{}/versions", self.config.base_url, plugin_id);

        let response = self.client.get(&url).send().await?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(RegistryError::NotFound(plugin_id.to_string()));
        }

        self.handle_response(response).await
    }

    /// Get plugin reviews
    pub async fn get_reviews(
        &self,
        plugin_id: &str,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<PluginReview>, RegistryError> {
        let url = format!(
            "{}/plugins/{}/reviews?page={}&page_size={}",
            self.config.base_url, plugin_id, page, page_size
        );

        let response = self.client.get(&url).send().await?;

        self.handle_response(response).await
    }

    /// Download a plugin to the cache directory
    pub async fn download_plugin(
        &self,
        plugin_id: &str,
        version: &PluginVersion,
    ) -> Result<PathBuf, RegistryError> {
        // Create cache directory
        let plugin_dir = self.config.cache_dir.join(plugin_id);
        fs::create_dir_all(&plugin_dir).await?;

        let file_name = format!("{}-{}.wasm", plugin_id, version.version);
        let file_path = plugin_dir.join(&file_name);

        // Check if already cached
        if file_path.exists() {
            // Verify checksum
            let content = fs::read(&file_path).await?;
            let checksum = compute_sha256(&content);
            if checksum == version.checksum {
                return Ok(file_path);
            }
            // Checksum mismatch, re-download
            fs::remove_file(&file_path).await?;
        }

        // Download file
        let response = self.client.get(&version.download_url).send().await?;

        if !response.status().is_success() {
            return Err(RegistryError::ApiError {
                status: response.status().as_u16(),
                message: "Failed to download plugin".to_string(),
            });
        }

        let bytes = response.bytes().await?;

        // Verify checksum
        let checksum = compute_sha256(&bytes);
        if checksum != version.checksum {
            return Err(RegistryError::ChecksumMismatch {
                expected: version.checksum.clone(),
                actual: checksum,
            });
        }

        // Write to file
        let mut file = fs::File::create(&file_path).await?;
        file.write_all(&bytes).await?;

        Ok(file_path)
    }

    /// Check for updates for installed plugins
    pub async fn check_updates(
        &self,
        installed: &[InstalledPlugin],
    ) -> Result<Vec<PluginUpdate>, RegistryError> {
        let mut updates = Vec::new();

        for plugin in installed {
            if let Ok(metadata) = self.get_plugin(&plugin.metadata.id).await {
                if let Some(latest) = metadata.latest_version() {
                    if latest.version > plugin.installed_version {
                        updates.push(PluginUpdate {
                            plugin_id: plugin.metadata.id.clone(),
                            name: plugin.metadata.name.clone(),
                            current_version: plugin.installed_version.clone(),
                            latest_version: latest.version.clone(),
                            changelog: latest.changelog.clone(),
                            security_update: false, // Would need security advisory data
                            breaking_change: latest.version.major > plugin.installed_version.major,
                        });
                    }
                }
            }
        }

        Ok(updates)
    }

    /// Get featured plugins
    pub async fn get_featured(&self) -> Result<Vec<PluginMetadata>, RegistryError> {
        let query = SearchQuery {
            featured_only: true,
            page_size: Some(20),
            ..Default::default()
        };

        let results = self.search(&query).await?;
        Ok(results.plugins)
    }

    /// Get trending plugins
    pub async fn get_trending(&self) -> Result<Vec<PluginMetadata>, RegistryError> {
        let query = SearchQuery {
            sort_by: Some(SearchSort::Trending),
            page_size: Some(20),
            ..Default::default()
        };

        let results = self.search(&query).await?;
        Ok(results.plugins)
    }

    /// Get recently updated plugins
    pub async fn get_recent(&self) -> Result<Vec<PluginMetadata>, RegistryError> {
        let query = SearchQuery {
            sort_by: Some(SearchSort::RecentlyUpdated),
            page_size: Some(20),
            ..Default::default()
        };

        let results = self.search(&query).await?;
        Ok(results.plugins)
    }

    /// Get plugins by category
    pub async fn get_by_category(
        &self,
        category: PluginCategory,
    ) -> Result<SearchResults, RegistryError> {
        let query = SearchQuery {
            category: Some(category),
            page_size: Some(50),
            ..Default::default()
        };

        self.search(&query).await
    }

    /// Publish a plugin (requires authentication)
    pub async fn publish(&self, manifest_path: &PathBuf) -> Result<PluginMetadata, RegistryError> {
        if self.config.api_token.is_none() {
            return Err(RegistryError::AuthRequired);
        }

        let content = fs::read(manifest_path).await?;
        let url = format!("{}/plugins/publish", self.config.base_url);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/octet-stream")
            .body(content)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Submit a review (requires authentication)
    pub async fn submit_review(
        &self,
        plugin_id: &str,
        rating: u8,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<PluginReview, RegistryError> {
        if self.config.api_token.is_none() {
            return Err(RegistryError::AuthRequired);
        }

        let url = format!("{}/plugins/{}/reviews", self.config.base_url, plugin_id);

        #[derive(serde::Serialize)]
        struct ReviewSubmission<'a> {
            rating: u8,
            title: Option<&'a str>,
            body: Option<&'a str>,
        }

        let response = self
            .client
            .post(&url)
            .json(&ReviewSubmission {
                rating,
                title,
                body,
            })
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Handle API response
    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, RegistryError> {
        let status = response.status();

        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .unwrap_or(60);

            return Err(RegistryError::RateLimited { retry_after });
        }

        if status == StatusCode::UNAUTHORIZED {
            return Err(RegistryError::Unauthorized(
                "Invalid or expired token".to_string(),
            ));
        }

        if status == StatusCode::FORBIDDEN {
            return Err(RegistryError::Unauthorized(
                "Insufficient permissions".to_string(),
            ));
        }

        if !status.is_success() {
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            return Err(RegistryError::ApiError {
                status: status.as_u16(),
                message,
            });
        }

        response
            .json()
            .await
            .map_err(|e| RegistryError::Parse(e.to_string()))
    }
}

impl Default for RegistryClient {
    fn default() -> Self {
        Self::new().expect("Failed to create registry client")
    }
}

/// Compute SHA256 hash of data
fn compute_sha256(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RegistryConfig::default();
        assert_eq!(config.base_url, DEFAULT_REGISTRY_URL);
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert!(config.api_token.is_none());
    }

    #[test]
    fn test_compute_sha256() {
        let data = b"hello world";
        let hash = compute_sha256(data);
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }
}
