//! Plugin Marketplace for discovering, downloading, and managing plugins.
//!
//! This module provides functionality for:
//! - Discovering plugins from remote registries
//! - Downloading and installing plugins
//! - Managing plugin versions and updates
//! - Local plugin caching

use crate::error::{PluginError, Result};
use crate::manifest::PluginManifest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, info, warn};

/// Plugin registry entry with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRegistryEntry {
    /// Plugin manifest.
    pub manifest: PluginManifest,
    /// Download URL for the WASM file.
    pub download_url: String,
    /// SHA256 checksum of the WASM file.
    pub checksum: String,
    /// Download count.
    pub downloads: u64,
    /// Average rating (0.0 - 5.0).
    pub rating: f32,
    /// Number of ratings.
    pub rating_count: u32,
    /// Publication timestamp.
    pub published_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Author information.
    pub author: PluginAuthor,
    /// Repository URL.
    pub repository: Option<String>,
    /// License identifier (SPDX).
    pub license: String,
    /// Tags for categorization.
    pub tags: Vec<String>,
}

/// Plugin author information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAuthor {
    /// Author name.
    pub name: String,
    /// Author email.
    pub email: Option<String>,
    /// Author website.
    pub url: Option<String>,
}

/// Search query for plugins.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginSearchQuery {
    /// Text search query.
    pub query: Option<String>,
    /// Filter by tags.
    pub tags: Vec<String>,
    /// Minimum rating filter.
    pub min_rating: Option<f32>,
    /// Sort field.
    pub sort_by: PluginSortField,
    /// Sort order.
    pub sort_order: SortOrder,
    /// Page number (0-indexed).
    pub page: u32,
    /// Results per page.
    pub per_page: u32,
}

/// Sort field for plugin search.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginSortField {
    /// Sort by name.
    Name,
    /// Sort by download count.
    #[default]
    Downloads,
    /// Sort by rating.
    Rating,
    /// Sort by publication date.
    Published,
    /// Sort by last update.
    Updated,
}

/// Sort order.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum SortOrder {
    /// Ascending order.
    Ascending,
    /// Descending order.
    #[default]
    Descending,
}

/// Search results from the marketplace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSearchResults {
    /// Matching plugins.
    pub plugins: Vec<PluginRegistryEntry>,
    /// Total count of matching plugins.
    pub total: u64,
    /// Current page.
    pub page: u32,
    /// Results per page.
    pub per_page: u32,
}

/// Local plugin cache for offline access.
pub struct PluginCache {
    /// Cache directory.
    cache_dir: PathBuf,
    /// Index of cached plugins.
    index: HashMap<String, CachedPluginInfo>,
}

/// Information about a cached plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPluginInfo {
    /// Plugin name.
    pub name: String,
    /// Installed version.
    pub version: String,
    /// Path to the WASM file.
    pub wasm_path: PathBuf,
    /// Path to the manifest file.
    pub manifest_path: PathBuf,
    /// Installation timestamp.
    pub installed_at: chrono::DateTime<chrono::Utc>,
    /// SHA256 checksum.
    pub checksum: String,
}

impl PluginCache {
    /// Create a new plugin cache.
    pub async fn new(cache_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&cache_dir).await?;
        
        let index_path = cache_dir.join("index.json");
        let index = if index_path.exists() {
            let content = fs::read_to_string(&index_path).await?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        };
        
        Ok(Self { cache_dir, index })
    }
    
    /// Get the cache directory.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
    
    /// Check if a plugin is cached.
    pub fn is_cached(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }
    
    /// Get cached plugin info.
    pub fn get_cached(&self, name: &str) -> Option<&CachedPluginInfo> {
        self.index.get(name)
    }
    
    /// List all cached plugins.
    pub fn list_cached(&self) -> Vec<&CachedPluginInfo> {
        self.index.values().collect()
    }
    
    /// Add a plugin to the cache.
    pub async fn cache_plugin(
        &mut self,
        name: &str,
        version: &str,
        wasm_bytes: &[u8],
        manifest: &PluginManifest,
    ) -> Result<PathBuf> {
        let plugin_dir = self.cache_dir.join(name);
        fs::create_dir_all(&plugin_dir).await?;
        
        let wasm_path = plugin_dir.join(format!("{}-{}.wasm", name, version));
        let manifest_path = plugin_dir.join("manifest.json");
        
        // Write WASM file
        fs::write(&wasm_path, wasm_bytes).await?;
        
        // Write manifest
        let manifest_json = serde_json::to_string_pretty(manifest)
            .map_err(|e| PluginError::Internal(e.to_string()))?;
        fs::write(&manifest_path, manifest_json).await?;
        
        // Calculate checksum
        let checksum = format!("{:x}", md5::compute(wasm_bytes));
        
        // Update index
        let info = CachedPluginInfo {
            name: name.to_string(),
            version: version.to_string(),
            wasm_path: wasm_path.clone(),
            manifest_path,
            installed_at: chrono::Utc::now(),
            checksum,
        };
        self.index.insert(name.to_string(), info);
        
        // Save index
        self.save_index().await?;
        
        info!(name = %name, version = %version, "Plugin cached");
        
        Ok(wasm_path)
    }
    
    /// Remove a plugin from the cache.
    pub async fn remove_plugin(&mut self, name: &str) -> Result<()> {
        if let Some(info) = self.index.remove(name) {
            // Remove plugin directory
            let plugin_dir = info.wasm_path.parent().unwrap();
            if plugin_dir.exists() {
                fs::remove_dir_all(plugin_dir).await?;
            }
            
            // Save index
            self.save_index().await?;
            
            info!(name = %name, "Plugin removed from cache");
        }
        
        Ok(())
    }
    
    /// Clear the entire cache.
    pub async fn clear(&mut self) -> Result<()> {
        // Remove all plugin directories
        let mut entries = fs::read_dir(&self.cache_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all(&path).await?;
            }
        }
        
        self.index.clear();
        self.save_index().await?;
        
        info!("Plugin cache cleared");
        
        Ok(())
    }
    
    /// Get the total cache size in bytes.
    pub async fn cache_size(&self) -> Result<u64> {
        let mut total = 0u64;
        
        for info in self.index.values() {
            if info.wasm_path.exists() {
                let metadata = fs::metadata(&info.wasm_path).await?;
                total += metadata.len();
            }
        }
        
        Ok(total)
    }
    
    /// Save the index to disk.
    async fn save_index(&self) -> Result<()> {
        let index_path = self.cache_dir.join("index.json");
        let content = serde_json::to_string_pretty(&self.index)
            .map_err(|e| PluginError::Internal(e.to_string()))?;
        fs::write(index_path, content).await?;
        Ok(())
    }
}

/// Plugin marketplace client.
pub struct MarketplaceClient {
    /// Base URL for the marketplace API.
    base_url: String,
    /// HTTP client.
    client: reqwest::Client,
    /// Local plugin cache.
    cache: PluginCache,
}

impl MarketplaceClient {
    /// Default marketplace URL.
    pub const DEFAULT_MARKETPLACE_URL: &'static str = "https://plugins.ember.dev/api/v1";
    
    /// Create a new marketplace client.
    pub async fn new(cache_dir: PathBuf) -> Result<Self> {
        Self::with_url(Self::DEFAULT_MARKETPLACE_URL, cache_dir).await
    }
    
    /// Create a marketplace client with a custom URL.
    pub async fn with_url(base_url: &str, cache_dir: PathBuf) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(format!("ember-plugins/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| PluginError::Internal(e.to_string()))?;
        
        let cache = PluginCache::new(cache_dir).await?;
        
        Ok(Self {
            base_url: base_url.to_string(),
            client,
            cache,
        })
    }
    
    /// Search for plugins.
    pub async fn search(&self, query: PluginSearchQuery) -> Result<PluginSearchResults> {
        let url = format!("{}/plugins/search", self.base_url);
        
        let response = self.client
            .post(&url)
            .json(&query)
            .send()
            .await
            .map_err(|e| PluginError::Internal(format!("Search request failed: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(PluginError::Internal(format!(
                "Search failed with status: {}",
                response.status()
            )));
        }
        
        let results: PluginSearchResults = response
            .json()
            .await
            .map_err(|e| PluginError::Internal(format!("Failed to parse search results: {}", e)))?;
        
        Ok(results)
    }
    
    /// Get plugin details by name.
    pub async fn get_plugin(&self, name: &str) -> Result<PluginRegistryEntry> {
        let url = format!("{}/plugins/{}", self.base_url, name);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| PluginError::Internal(format!("Get plugin request failed: {}", e)))?;
        
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(PluginError::NotFound(name.to_string()));
        }
        
        if !response.status().is_success() {
            return Err(PluginError::Internal(format!(
                "Get plugin failed with status: {}",
                response.status()
            )));
        }
        
        let entry: PluginRegistryEntry = response
            .json()
            .await
            .map_err(|e| PluginError::Internal(format!("Failed to parse plugin details: {}", e)))?;
        
        Ok(entry)
    }
    
    /// Download and install a plugin.
    pub async fn install(&mut self, name: &str, version: Option<&str>) -> Result<PathBuf> {
        // Get plugin info
        let entry = self.get_plugin(name).await?;
        
        let target_version = version.unwrap_or(&entry.manifest.version);
        
        // Check if already cached
        if let Some(cached) = self.cache.get_cached(name) {
            if cached.version == target_version {
                info!(name = %name, version = %target_version, "Plugin already installed");
                return Ok(cached.wasm_path.clone());
            }
        }
        
        // Download WASM file
        info!(name = %name, version = %target_version, "Downloading plugin");
        
        let response = self.client
            .get(&entry.download_url)
            .send()
            .await
            .map_err(|e| PluginError::Internal(format!("Download failed: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(PluginError::Internal(format!(
                "Download failed with status: {}",
                response.status()
            )));
        }
        
        let wasm_bytes = response
            .bytes()
            .await
            .map_err(|e| PluginError::Internal(format!("Failed to read download: {}", e)))?;
        
        // Verify checksum
        let computed_checksum = format!("{:x}", md5::compute(&wasm_bytes));
        if computed_checksum != entry.checksum {
            return Err(PluginError::Internal(format!(
                "Checksum mismatch: expected {}, got {}",
                entry.checksum, computed_checksum
            )));
        }
        
        // Cache the plugin
        let path = self.cache.cache_plugin(
            name,
            target_version,
            &wasm_bytes,
            &entry.manifest,
        ).await?;
        
        info!(name = %name, version = %target_version, "Plugin installed");
        
        Ok(path)
    }
    
    /// Uninstall a plugin.
    pub async fn uninstall(&mut self, name: &str) -> Result<()> {
        self.cache.remove_plugin(name).await
    }
    
    /// List installed plugins.
    pub fn list_installed(&self) -> Vec<&CachedPluginInfo> {
        self.cache.list_cached()
    }
    
    /// Check for updates.
    pub async fn check_updates(&self) -> Result<Vec<(String, String, String)>> {
        let mut updates = Vec::new();
        
        for info in self.cache.list_cached() {
            match self.get_plugin(&info.name).await {
                Ok(entry) => {
                    if entry.manifest.version != info.version {
                        updates.push((
                            info.name.clone(),
                            info.version.clone(),
                            entry.manifest.version.clone(),
                        ));
                    }
                }
                Err(e) => {
                    warn!(name = %info.name, error = %e, "Failed to check for updates");
                }
            }
        }
        
        Ok(updates)
    }
    
    /// Update a plugin to the latest version.
    pub async fn update(&mut self, name: &str) -> Result<PathBuf> {
        // Remove old version and install new
        self.cache.remove_plugin(name).await?;
        self.install(name, None).await
    }
    
    /// Get cache statistics.
    pub async fn cache_stats(&self) -> Result<CacheStats> {
        Ok(CacheStats {
            plugin_count: self.cache.index.len(),
            total_size: self.cache.cache_size().await?,
            cache_dir: self.cache.cache_dir().to_path_buf(),
        })
    }
}

/// Cache statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Number of cached plugins.
    pub plugin_count: usize,
    /// Total cache size in bytes.
    pub total_size: u64,
    /// Cache directory path.
    pub cache_dir: PathBuf,
}

/// Featured and popular plugins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturedPlugins {
    /// Editor's picks.
    pub editors_picks: Vec<PluginRegistryEntry>,
    /// Most downloaded.
    pub popular: Vec<PluginRegistryEntry>,
    /// Recently updated.
    pub recent: Vec<PluginRegistryEntry>,
    /// Trending (gaining downloads quickly).
    pub trending: Vec<PluginRegistryEntry>,
}

impl MarketplaceClient {
    /// Get featured plugins.
    pub async fn get_featured(&self) -> Result<FeaturedPlugins> {
        let url = format!("{}/plugins/featured", self.base_url);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| PluginError::Internal(format!("Featured request failed: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(PluginError::Internal(format!(
                "Featured request failed with status: {}",
                response.status()
            )));
        }
        
        let featured: FeaturedPlugins = response
            .json()
            .await
            .map_err(|e| PluginError::Internal(format!("Failed to parse featured plugins: {}", e)))?;
        
        Ok(featured)
    }
    
    /// Get all available tags.
    pub async fn get_tags(&self) -> Result<Vec<TagInfo>> {
        let url = format!("{}/tags", self.base_url);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| PluginError::Internal(format!("Tags request failed: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(PluginError::Internal(format!(
                "Tags request failed with status: {}",
                response.status()
            )));
        }
        
        let tags: Vec<TagInfo> = response
            .json()
            .await
            .map_err(|e| PluginError::Internal(format!("Failed to parse tags: {}", e)))?;
        
        Ok(tags)
    }
}

/// Tag information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagInfo {
    /// Tag name.
    pub name: String,
    /// Number of plugins with this tag.
    pub count: u64,
    /// Tag description.
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[tokio::test]
    async fn test_plugin_cache_creation() {
        let temp_dir = tempdir().unwrap();
        let cache = PluginCache::new(temp_dir.path().to_path_buf()).await.unwrap();
        
        assert!(cache.list_cached().is_empty());
    }
    
    #[tokio::test]
    async fn test_cache_plugin() {
        let temp_dir = tempdir().unwrap();
        let mut cache = PluginCache::new(temp_dir.path().to_path_buf()).await.unwrap();
        
        let manifest = PluginManifest::new("test-plugin", "1.0.0", "A test plugin");
        let wasm_bytes = b"fake wasm content";
        
        let path = cache.cache_plugin("test-plugin", "1.0.0", wasm_bytes, &manifest)
            .await
            .unwrap();
        
        assert!(path.exists());
        assert!(cache.is_cached("test-plugin"));
        
        let info = cache.get_cached("test-plugin").unwrap();
        assert_eq!(info.version, "1.0.0");
    }
    
    #[tokio::test]
    async fn test_remove_plugin() {
        let temp_dir = tempdir().unwrap();
        let mut cache = PluginCache::new(temp_dir.path().to_path_buf()).await.unwrap();
        
        let manifest = PluginManifest::new("test-plugin", "1.0.0", "A test plugin");
        let wasm_bytes = b"fake wasm content";
        
        cache.cache_plugin("test-plugin", "1.0.0", wasm_bytes, &manifest)
            .await
            .unwrap();
        
        cache.remove_plugin("test-plugin").await.unwrap();
        
        assert!(!cache.is_cached("test-plugin"));
    }
    
    #[test]
    fn test_search_query_default() {
        let query = PluginSearchQuery::default();
        assert_eq!(query.sort_by, PluginSortField::Downloads);
        assert_eq!(query.sort_order, SortOrder::Descending);
    }
}