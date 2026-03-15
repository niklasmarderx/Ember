//! Hot-reloading support for WASM plugins.
//!
//! This module provides functionality for:
//! - Watching plugin files for changes
//! - Automatically reloading plugins when they change
//! - Graceful plugin replacement without service interruption

use crate::error::{PluginError, Result};
use crate::manifest::PluginManifest;
use crate::runtime::{LoadedPlugin, PluginRuntime};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Events emitted by the hot-reload system.
#[derive(Debug, Clone)]
pub enum HotReloadEvent {
    /// A plugin file was modified.
    PluginModified {
        /// Plugin name.
        name: String,
        /// Path to the modified file.
        path: PathBuf,
    },
    /// A plugin was successfully reloaded.
    PluginReloaded {
        /// Plugin name.
        name: String,
        /// New version (if changed).
        version: String,
    },
    /// A plugin reload failed.
    ReloadFailed {
        /// Plugin name.
        name: String,
        /// Error message.
        error: String,
    },
    /// A new plugin was detected.
    PluginAdded {
        /// Plugin name.
        name: String,
        /// Path to the plugin.
        path: PathBuf,
    },
    /// A plugin was removed.
    PluginRemoved {
        /// Plugin name.
        name: String,
    },
}

/// Configuration for hot-reloading.
#[derive(Debug, Clone)]
pub struct HotReloadConfig {
    /// Debounce duration for file changes.
    pub debounce: Duration,
    /// Whether to auto-reload on changes.
    pub auto_reload: bool,
    /// Watch subdirectories recursively.
    pub recursive: bool,
    /// File extensions to watch.
    pub watch_extensions: Vec<String>,
}

impl Default for HotReloadConfig {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(500),
            auto_reload: true,
            recursive: true,
            watch_extensions: vec!["wasm".to_string(), "json".to_string()],
        }
    }
}

/// Tracks a watched plugin directory.
struct WatchedDirectory {
    /// Path to the directory.
    path: PathBuf,
    /// Plugins in this directory.
    plugins: HashMap<String, WatchedPlugin>,
}

/// Tracks a watched plugin.
struct WatchedPlugin {
    /// Plugin name.
    name: String,
    /// Path to the WASM file.
    wasm_path: PathBuf,
    /// Path to the manifest file.
    manifest_path: Option<PathBuf>,
    /// Last modification time.
    last_modified: std::time::SystemTime,
}

/// Hot-reload manager for plugins.
pub struct HotReloadManager {
    /// Configuration.
    config: HotReloadConfig,
    /// Reference to the plugin runtime.
    runtime: Arc<PluginRuntime>,
    /// Watched directories.
    watched_dirs: Arc<RwLock<HashMap<PathBuf, WatchedDirectory>>>,
    /// File system watcher.
    watcher: Option<RecommendedWatcher>,
    /// Event sender for internal use.
    event_tx: mpsc::Sender<notify::Result<Event>>,
    /// Event receiver.
    event_rx: Arc<RwLock<mpsc::Receiver<notify::Result<Event>>>>,
    /// Broadcast channel for external subscribers.
    broadcast_tx: broadcast::Sender<HotReloadEvent>,
    /// Shutdown flag.
    shutdown: Arc<RwLock<bool>>,
}

impl HotReloadManager {
    /// Create a new hot-reload manager.
    pub fn new(runtime: Arc<PluginRuntime>, config: HotReloadConfig) -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(100);
        let (broadcast_tx, _) = broadcast::channel(100);

        Ok(Self {
            config,
            runtime,
            watched_dirs: Arc::new(RwLock::new(HashMap::new())),
            watcher: None,
            event_tx,
            event_rx: Arc::new(RwLock::new(event_rx)),
            broadcast_tx,
            shutdown: Arc::new(RwLock::new(false)),
        })
    }

    /// Subscribe to hot-reload events.
    pub fn subscribe(&self) -> broadcast::Receiver<HotReloadEvent> {
        self.broadcast_tx.subscribe()
    }

    /// Start watching a directory for plugin changes.
    pub async fn watch_directory(&mut self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(PluginError::Internal(format!(
                "Directory does not exist: {}",
                path.display()
            )));
        }

        if !path.is_dir() {
            return Err(PluginError::Internal(format!(
                "Path is not a directory: {}",
                path.display()
            )));
        }

        let canonical = path
            .canonicalize()
            .map_err(|e| PluginError::Internal(e.to_string()))?;

        // Check if already watching
        {
            let dirs = self.watched_dirs.read().await;
            if dirs.contains_key(&canonical) {
                info!(path = %canonical.display(), "Directory already being watched");
                return Ok(());
            }
        }

        // Scan directory for existing plugins
        let watched_dir = self.scan_directory(&canonical).await?;

        // Add to watched directories
        {
            let mut dirs = self.watched_dirs.write().await;
            dirs.insert(canonical.clone(), watched_dir);
        }

        // Initialize watcher if not already done
        if self.watcher.is_none() {
            self.init_watcher()?;
        }

        // Add path to watcher
        if let Some(watcher) = &mut self.watcher {
            let mode = if self.config.recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };

            watcher
                .watch(&canonical, mode)
                .map_err(|e| PluginError::Internal(format!("Failed to watch directory: {}", e)))?;
        }

        info!(path = %canonical.display(), "Started watching directory for plugins");

        Ok(())
    }

    /// Stop watching a directory.
    pub async fn unwatch_directory(&mut self, path: &Path) -> Result<()> {
        let canonical = path
            .canonicalize()
            .map_err(|e| PluginError::Internal(e.to_string()))?;

        // Remove from watcher
        if let Some(watcher) = &mut self.watcher {
            watcher.unwatch(&canonical).map_err(|e| {
                PluginError::Internal(format!("Failed to unwatch directory: {}", e))
            })?;
        }

        // Remove from watched directories
        {
            let mut dirs = self.watched_dirs.write().await;
            dirs.remove(&canonical);
        }

        info!(path = %canonical.display(), "Stopped watching directory");

        Ok(())
    }

    /// Start the hot-reload event loop.
    pub async fn start(&self) {
        let event_rx = self.event_rx.clone();
        let watched_dirs = self.watched_dirs.clone();
        let runtime = self.runtime.clone();
        let broadcast_tx = self.broadcast_tx.clone();
        let config = self.config.clone();
        let shutdown = self.shutdown.clone();

        tokio::spawn(async move {
            let mut pending_changes: HashMap<PathBuf, std::time::Instant> = HashMap::new();

            loop {
                // Check shutdown
                if *shutdown.read().await {
                    break;
                }

                // Process pending changes (debounce)
                let now = std::time::Instant::now();
                let mut to_process = Vec::new();

                pending_changes.retain(|path, time| {
                    if now.duration_since(*time) >= config.debounce {
                        to_process.push(path.clone());
                        false
                    } else {
                        true
                    }
                });

                // Process debounced changes
                for path in to_process {
                    if config.auto_reload {
                        Self::handle_file_change(&path, &watched_dirs, &runtime, &broadcast_tx)
                            .await;
                    }
                }

                // Try to receive new events
                let mut rx = event_rx.write().await;
                match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
                    Ok(Some(Ok(event))) => {
                        // Filter by event kind
                        match event.kind {
                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                                for path in event.paths {
                                    // Check extension
                                    if let Some(ext) = path.extension() {
                                        let ext_str = ext.to_string_lossy().to_string();
                                        if config.watch_extensions.contains(&ext_str) {
                                            pending_changes.insert(path, now);
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Ok(Some(Err(e))) => {
                        warn!(error = %e, "File watcher error");
                    }
                    Ok(None) => {
                        // Channel closed
                        break;
                    }
                    Err(_) => {
                        // Timeout - continue loop
                    }
                }
            }

            info!("Hot-reload event loop stopped");
        });

        info!("Hot-reload manager started");
    }

    /// Stop the hot-reload manager.
    pub async fn stop(&self) {
        let mut shutdown = self.shutdown.write().await;
        *shutdown = true;
        info!("Hot-reload manager stopping");
    }

    /// Manually trigger a reload for a specific plugin.
    pub async fn reload_plugin(&self, name: &str) -> Result<()> {
        let dirs = self.watched_dirs.read().await;

        for watched_dir in dirs.values() {
            if let Some(plugin) = watched_dir.plugins.get(name) {
                // Load manifest
                let manifest = if let Some(manifest_path) = &plugin.manifest_path {
                    Self::load_manifest(manifest_path).await?
                } else {
                    PluginManifest::new(name, "0.0.0", "Auto-detected plugin")
                };

                // Unload existing plugin if loaded
                if self.runtime.is_loaded(name).await {
                    self.runtime.unload_plugin(name).await?;
                }

                // Reload plugin
                self.runtime
                    .load_plugin(&plugin.wasm_path, manifest)
                    .await?;

                let _ = self.broadcast_tx.send(HotReloadEvent::PluginReloaded {
                    name: name.to_string(),
                    version: "unknown".to_string(),
                });

                info!(name = %name, "Plugin manually reloaded");
                return Ok(());
            }
        }

        Err(PluginError::NotFound(name.to_string()))
    }

    /// Get list of watched plugins.
    pub async fn list_watched(&self) -> Vec<WatchedPluginInfo> {
        let dirs = self.watched_dirs.read().await;
        let mut result = Vec::new();

        for watched_dir in dirs.values() {
            for plugin in watched_dir.plugins.values() {
                result.push(WatchedPluginInfo {
                    name: plugin.name.clone(),
                    wasm_path: plugin.wasm_path.clone(),
                    manifest_path: plugin.manifest_path.clone(),
                    last_modified: plugin.last_modified,
                });
            }
        }

        result
    }

    /// Initialize the file system watcher.
    fn init_watcher(&mut self) -> Result<()> {
        let tx = self.event_tx.clone();

        let watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.blocking_send(res);
            },
            Config::default(),
        )
        .map_err(|e| PluginError::Internal(format!("Failed to create watcher: {}", e)))?;

        self.watcher = Some(watcher);

        Ok(())
    }

    /// Scan a directory for plugins.
    async fn scan_directory(&self, path: &Path) -> Result<WatchedDirectory> {
        let mut plugins = HashMap::new();
        let mut entries = tokio::fs::read_dir(path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let entry_path = entry.path();

            if entry_path.is_file() {
                if let Some(ext) = entry_path.extension() {
                    if ext == "wasm" {
                        // Found a WASM file
                        let name = entry_path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "unknown".to_string());

                        // Look for corresponding manifest
                        let manifest_path = entry_path.with_extension("json");
                        let manifest_exists = manifest_path.exists();

                        let metadata = tokio::fs::metadata(&entry_path).await?;
                        let last_modified = metadata
                            .modified()
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

                        let watched_plugin = WatchedPlugin {
                            name: name.clone(),
                            wasm_path: entry_path,
                            manifest_path: if manifest_exists {
                                Some(manifest_path)
                            } else {
                                None
                            },
                            last_modified,
                        };

                        plugins.insert(name, watched_plugin);
                    }
                }
            } else if entry_path.is_dir() && self.config.recursive {
                // Recursively scan subdirectories
                if let Ok(sub_dir) = Box::pin(self.scan_directory(&entry_path)).await {
                    for (name, plugin) in sub_dir.plugins {
                        plugins.insert(name, plugin);
                    }
                }
            }
        }

        Ok(WatchedDirectory {
            path: path.to_path_buf(),
            plugins,
        })
    }

    /// Handle a file change event.
    async fn handle_file_change(
        path: &Path,
        watched_dirs: &Arc<RwLock<HashMap<PathBuf, WatchedDirectory>>>,
        runtime: &Arc<PluginRuntime>,
        broadcast_tx: &broadcast::Sender<HotReloadEvent>,
    ) {
        // Determine plugin name from path
        let plugin_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());

        let Some(name) = plugin_name else {
            return;
        };

        let extension = path.extension().and_then(|s| s.to_str());

        match extension {
            Some("wasm") => {
                // WASM file changed
                if !path.exists() {
                    // Plugin was removed
                    if runtime.is_loaded(&name).await {
                        if let Err(e) = runtime.unload_plugin(&name).await {
                            error!(name = %name, error = %e, "Failed to unload removed plugin");
                        }
                    }

                    let _ = broadcast_tx.send(HotReloadEvent::PluginRemoved { name: name.clone() });

                    // Remove from watched plugins
                    let mut dirs = watched_dirs.write().await;
                    for dir in dirs.values_mut() {
                        dir.plugins.remove(&name);
                    }

                    info!(name = %name, "Plugin removed");
                } else {
                    // Plugin was modified or added
                    let _ = broadcast_tx.send(HotReloadEvent::PluginModified {
                        name: name.clone(),
                        path: path.to_path_buf(),
                    });

                    // Try to load manifest
                    let manifest_path = path.with_extension("json");
                    let manifest = if manifest_path.exists() {
                        match Self::load_manifest(&manifest_path).await {
                            Ok(m) => m,
                            Err(e) => {
                                warn!(error = %e, "Failed to load manifest, using default");
                                PluginManifest::new(&name, "0.0.0", "Auto-detected plugin")
                            }
                        }
                    } else {
                        PluginManifest::new(&name, "0.0.0", "Auto-detected plugin")
                    };

                    // Unload if already loaded
                    if runtime.is_loaded(&name).await {
                        if let Err(e) = runtime.unload_plugin(&name).await {
                            error!(name = %name, error = %e, "Failed to unload plugin for reload");
                            let _ = broadcast_tx.send(HotReloadEvent::ReloadFailed {
                                name: name.clone(),
                                error: e.to_string(),
                            });
                            return;
                        }
                    }

                    // Load the plugin
                    match runtime.load_plugin(path, manifest.clone()).await {
                        Ok(()) => {
                            let _ = broadcast_tx.send(HotReloadEvent::PluginReloaded {
                                name: name.clone(),
                                version: manifest.version.clone(),
                            });
                            info!(name = %name, version = %manifest.version, "Plugin reloaded");
                        }
                        Err(e) => {
                            let _ = broadcast_tx.send(HotReloadEvent::ReloadFailed {
                                name: name.clone(),
                                error: e.to_string(),
                            });
                            error!(name = %name, error = %e, "Failed to reload plugin");
                        }
                    }
                }
            }
            Some("json") => {
                // Manifest file changed - trigger reload of corresponding plugin
                let wasm_path = path.with_extension("wasm");
                if wasm_path.exists() {
                    // Recursively handle WASM file
                    Box::pin(Self::handle_file_change(
                        &wasm_path,
                        watched_dirs,
                        runtime,
                        broadcast_tx,
                    ))
                    .await;
                }
            }
            _ => {}
        }
    }

    /// Load a plugin manifest from file.
    async fn load_manifest(path: &Path) -> Result<PluginManifest> {
        let content = tokio::fs::read_to_string(path).await?;
        let manifest: PluginManifest = serde_json::from_str(&content)
            .map_err(|e| PluginError::InvalidManifest(e.to_string()))?;
        Ok(manifest)
    }
}

/// Information about a watched plugin.
#[derive(Debug, Clone)]
pub struct WatchedPluginInfo {
    /// Plugin name.
    pub name: String,
    /// Path to the WASM file.
    pub wasm_path: PathBuf,
    /// Path to the manifest file (if exists).
    pub manifest_path: Option<PathBuf>,
    /// Last modification time.
    pub last_modified: std::time::SystemTime,
}

/// Builder for HotReloadManager.
pub struct HotReloadManagerBuilder {
    config: HotReloadConfig,
    watch_dirs: Vec<PathBuf>,
}

impl HotReloadManagerBuilder {
    /// Create a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            config: HotReloadConfig::default(),
            watch_dirs: Vec::new(),
        }
    }

    /// Set the debounce duration.
    pub fn debounce(mut self, duration: Duration) -> Self {
        self.config.debounce = duration;
        self
    }

    /// Enable or disable auto-reload.
    pub fn auto_reload(mut self, enabled: bool) -> Self {
        self.config.auto_reload = enabled;
        self
    }

    /// Enable or disable recursive watching.
    pub fn recursive(mut self, enabled: bool) -> Self {
        self.config.recursive = enabled;
        self
    }

    /// Set file extensions to watch.
    pub fn watch_extensions(mut self, extensions: Vec<String>) -> Self {
        self.config.watch_extensions = extensions;
        self
    }

    /// Add a directory to watch.
    pub fn watch_dir(mut self, path: PathBuf) -> Self {
        self.watch_dirs.push(path);
        self
    }

    /// Build the hot-reload manager.
    pub async fn build(self, runtime: Arc<PluginRuntime>) -> Result<HotReloadManager> {
        let mut manager = HotReloadManager::new(runtime, self.config)?;

        for dir in self.watch_dirs {
            manager.watch_directory(&dir).await?;
        }

        Ok(manager)
    }
}

impl Default for HotReloadManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::RuntimeConfig;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_hot_reload_config_default() {
        let config = HotReloadConfig::default();
        assert!(config.auto_reload);
        assert!(config.recursive);
        assert!(config.watch_extensions.contains(&"wasm".to_string()));
    }

    #[tokio::test]
    async fn test_hot_reload_manager_creation() {
        let runtime = Arc::new(PluginRuntime::new(RuntimeConfig::default()).unwrap());
        let config = HotReloadConfig::default();
        let manager = HotReloadManager::new(runtime, config);

        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_watch_nonexistent_directory() {
        let runtime = Arc::new(PluginRuntime::new(RuntimeConfig::default()).unwrap());
        let config = HotReloadConfig::default();
        let mut manager = HotReloadManager::new(runtime, config).unwrap();

        let result = manager
            .watch_directory(Path::new("/nonexistent/path"))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_watch_directory() {
        let temp_dir = tempdir().unwrap();
        let runtime = Arc::new(PluginRuntime::new(RuntimeConfig::default()).unwrap());
        let config = HotReloadConfig::default();
        let mut manager = HotReloadManager::new(runtime, config).unwrap();

        let result = manager.watch_directory(temp_dir.path()).await;
        assert!(result.is_ok());

        let watched = manager.list_watched().await;
        assert!(watched.is_empty()); // No plugins in empty dir
    }

    #[tokio::test]
    async fn test_builder_pattern() {
        let temp_dir = tempdir().unwrap();
        let runtime = Arc::new(PluginRuntime::new(RuntimeConfig::default()).unwrap());

        let manager = HotReloadManagerBuilder::new()
            .debounce(Duration::from_millis(100))
            .auto_reload(true)
            .recursive(false)
            .watch_dir(temp_dir.path().to_path_buf())
            .build(runtime)
            .await;

        assert!(manager.is_ok());
    }
}
