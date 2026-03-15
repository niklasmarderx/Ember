//! WASM plugin runtime using Wasmtime.
//!
//! This module provides the runtime environment for executing WASM plugins.

#[cfg(feature = "wasmtime")]
use wasmtime::{Config, Engine, Module};

use crate::error::{PluginError, Result};
use crate::manifest::PluginManifest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Configuration for the plugin runtime.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Default memory limit for plugins (in bytes).
    pub default_memory_limit: usize,
    /// Default execution timeout.
    pub default_timeout: Duration,
    /// Enable fuel metering for execution limits.
    pub enable_fuel: bool,
    /// Initial fuel amount.
    pub initial_fuel: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            default_memory_limit: 64 * 1024 * 1024, // 64 MB
            default_timeout: Duration::from_secs(30),
            enable_fuel: true,
            initial_fuel: 1_000_000,
        }
    }
}

/// A loaded plugin instance.
pub struct LoadedPlugin {
    /// Plugin manifest.
    pub manifest: PluginManifest,
    /// Path to the WASM file.
    pub path: std::path::PathBuf,
    /// Whether the plugin is enabled.
    pub enabled: bool,
    /// Plugin-specific configuration.
    pub config: HashMap<String, serde_json::Value>,
    /// Compiled WASM module (wasmtime feature).
    #[cfg(feature = "wasmtime")]
    module: Option<Module>,
}

impl LoadedPlugin {
    /// Create a new loaded plugin.
    pub fn new(manifest: PluginManifest, path: std::path::PathBuf) -> Self {
        Self {
            manifest,
            path,
            enabled: true,
            config: HashMap::new(),
            #[cfg(feature = "wasmtime")]
            module: None,
        }
    }

    /// Check if the plugin has a specific capability.
    pub fn has_capability(&self, cap: &str) -> bool {
        match cap {
            "network" => self.manifest.capabilities.network,
            "filesystem" => self.manifest.capabilities.filesystem,
            "environment" => self.manifest.capabilities.environment,
            "shell" => self.manifest.capabilities.shell,
            _ => false,
        }
    }
}

/// Input for plugin function calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInput {
    /// Function name to call.
    pub function: String,
    /// Function arguments as JSON.
    pub arguments: serde_json::Value,
}

/// Output from plugin function calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginOutput {
    /// Whether the call was successful.
    pub success: bool,
    /// Return value (if successful).
    pub result: Option<serde_json::Value>,
    /// Error message (if failed).
    pub error: Option<String>,
    /// Execution duration.
    pub duration_ms: u64,
}

/// The plugin runtime manages WASM plugin execution.
pub struct PluginRuntime {
    #[allow(dead_code)]
    config: RuntimeConfig,
    plugins: Arc<RwLock<HashMap<String, LoadedPlugin>>>,
    #[cfg(feature = "wasmtime")]
    engine: Engine,
}

impl PluginRuntime {
    /// Create a new plugin runtime.
    ///
    /// # Errors
    ///
    /// Returns an error if the WASM engine cannot be initialized.
    pub fn new(config: RuntimeConfig) -> Result<Self> {
        #[cfg(feature = "wasmtime")]
        let engine = {
            let mut wasm_config = Config::new();
            wasm_config.async_support(true);
            if config.enable_fuel {
                wasm_config.consume_fuel(true);
            }
            Engine::new(&wasm_config).map_err(|e| PluginError::Internal(e.to_string()))?
        };

        info!("Plugin runtime initialized");

        Ok(Self {
            config,
            plugins: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "wasmtime")]
            engine,
        })
    }

    /// Load a plugin from a WASM file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the WASM file
    /// * `manifest` - Plugin manifest
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin cannot be loaded.
    pub async fn load_plugin(&self, path: &Path, manifest: PluginManifest) -> Result<()> {
        let name = manifest.name.clone();

        // Check if already loaded
        {
            let plugins = self.plugins.read().await;
            if plugins.contains_key(&name) {
                return Err(PluginError::AlreadyLoaded(name));
            }
        }

        // Verify file exists
        if !path.exists() {
            return Err(PluginError::LoadFailed {
                path: path.to_path_buf(),
                reason: "File not found".to_string(),
            });
        }

        let mut plugin = LoadedPlugin::new(manifest, path.to_path_buf());

        // Compile the WASM module
        #[cfg(feature = "wasmtime")]
        {
            let wasm_bytes = tokio::fs::read(path).await?;
            let module = Module::new(&self.engine, &wasm_bytes)
                .map_err(|e| PluginError::WasmCompilation(e.to_string()))?;
            plugin.module = Some(module);
        }

        let mut plugins = self.plugins.write().await;
        plugins.insert(name.clone(), plugin);

        info!(plugin = %name, "Plugin loaded");
        Ok(())
    }

    /// Unload a plugin.
    pub async fn unload_plugin(&self, name: &str) -> Result<()> {
        let mut plugins = self.plugins.write().await;

        if plugins.remove(name).is_none() {
            return Err(PluginError::NotFound(name.to_string()));
        }

        info!(plugin = %name, "Plugin unloaded");
        Ok(())
    }

    /// Get a list of loaded plugins.
    pub async fn list_plugins(&self) -> Vec<PluginManifest> {
        let plugins = self.plugins.read().await;
        plugins.values().map(|p| p.manifest.clone()).collect()
    }

    /// Check if a plugin is loaded.
    pub async fn is_loaded(&self, name: &str) -> bool {
        let plugins = self.plugins.read().await;
        plugins.contains_key(name)
    }

    /// Enable a plugin.
    pub async fn enable_plugin(&self, name: &str) -> Result<()> {
        let mut plugins = self.plugins.write().await;

        if let Some(plugin) = plugins.get_mut(name) {
            plugin.enabled = true;
            info!(plugin = %name, "Plugin enabled");
            Ok(())
        } else {
            Err(PluginError::NotFound(name.to_string()))
        }
    }

    /// Disable a plugin.
    pub async fn disable_plugin(&self, name: &str) -> Result<()> {
        let mut plugins = self.plugins.write().await;

        if let Some(plugin) = plugins.get_mut(name) {
            plugin.enabled = false;
            info!(plugin = %name, "Plugin disabled");
            Ok(())
        } else {
            Err(PluginError::NotFound(name.to_string()))
        }
    }

    /// Call a plugin function.
    ///
    /// # Arguments
    ///
    /// * `plugin_name` - Name of the plugin
    /// * `input` - Function input
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin or function is not found, or execution fails.
    #[cfg(feature = "wasmtime")]
    pub async fn call(&self, plugin_name: &str, input: PluginInput) -> Result<PluginOutput> {
        let start = std::time::Instant::now();

        let plugins = self.plugins.read().await;
        let plugin = plugins
            .get(plugin_name)
            .ok_or_else(|| PluginError::NotFound(plugin_name.to_string()))?;

        if !plugin.enabled {
            return Err(PluginError::ExecutionFailed(format!(
                "Plugin '{}' is disabled",
                plugin_name
            )));
        }

        // Verify the function exists in the manifest
        let export = plugin
            .manifest
            .exports
            .iter()
            .find(|e| e.name == input.function);
        if export.is_none() {
            return Err(PluginError::FunctionNotFound {
                plugin: plugin_name.to_string(),
                function: input.function,
            });
        }

        let module = plugin
            .module
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Module not compiled".to_string()))?;

        // Note: Full WASM execution requires proper WASI context setup.
        // For now, we simulate the execution. A complete implementation would:
        // 1. Create proper WasiP1Ctx context
        // 2. Set up linker with WASI imports
        // 3. Instantiate and call the module
        //
        // This is left as a TODO for the full WASM plugin support.
        let _ = module; // Suppress unused warning

        // For now, return a placeholder - full implementation would call the actual function
        let duration = start.elapsed();

        warn!(
            plugin = %plugin_name,
            function = %input.function,
            "Plugin call simulation - real execution not yet implemented"
        );

        Ok(PluginOutput {
            success: true,
            result: Some(serde_json::json!({
                "message": "Plugin function called (simulation)",
                "plugin": plugin_name,
                "function": input.function
            })),
            error: None,
            duration_ms: duration.as_millis() as u64,
        })
    }

    /// Call a plugin function (non-wasmtime stub).
    #[cfg(not(feature = "wasmtime"))]
    pub async fn call(&self, plugin_name: &str, input: PluginInput) -> Result<PluginOutput> {
        Err(PluginError::Internal(
            "WASM runtime not available - compile with 'wasmtime' feature".to_string(),
        ))
    }

    /// Get plugin configuration.
    pub async fn get_plugin_config(
        &self,
        name: &str,
    ) -> Result<HashMap<String, serde_json::Value>> {
        let plugins = self.plugins.read().await;
        let plugin = plugins
            .get(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;
        Ok(plugin.config.clone())
    }

    /// Set plugin configuration.
    pub async fn set_plugin_config(
        &self,
        name: &str,
        config: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        let plugin = plugins
            .get_mut(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;
        plugin.config = config;
        debug!(plugin = %name, "Plugin configuration updated");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PluginCapabilities;

    #[tokio::test]
    async fn test_runtime_creation() {
        let config = RuntimeConfig::default();
        let runtime = PluginRuntime::new(config);
        assert!(runtime.is_ok());
    }

    #[tokio::test]
    async fn test_list_plugins_empty() {
        let runtime = PluginRuntime::new(RuntimeConfig::default()).unwrap();
        let plugins = runtime.list_plugins().await;
        assert!(plugins.is_empty());
    }

    #[tokio::test]
    async fn test_plugin_not_found() {
        let runtime = PluginRuntime::new(RuntimeConfig::default()).unwrap();
        let result = runtime.unload_plugin("nonexistent").await;
        assert!(matches!(result, Err(PluginError::NotFound(_))));
    }

    #[test]
    fn test_plugin_input_serialization() {
        let input = PluginInput {
            function: "add".to_string(),
            arguments: serde_json::json!({"a": 1, "b": 2}),
        };
        let json = serde_json::to_string(&input).unwrap();
        let parsed: PluginInput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.function, "add");
    }

    #[test]
    fn test_loaded_plugin_capabilities() {
        let manifest = PluginManifest::new("test", "1.0.0", "Test")
            .with_capabilities(PluginCapabilities::none().with_network());
        let plugin = LoadedPlugin::new(manifest, std::path::PathBuf::from("test.wasm"));

        assert!(plugin.has_capability("network"));
        assert!(!plugin.has_capability("filesystem"));
    }
}
