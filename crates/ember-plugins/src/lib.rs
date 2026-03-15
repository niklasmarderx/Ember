//! WASM plugin system for the Ember AI agent framework.
//!
//! This crate provides a secure plugin system using WebAssembly (WASM)
//! for extending Ember with custom functionality.
//!
//! # Features
//!
//! - `wasmtime` (default): Enable the Wasmtime WASM runtime
//!
//! # Example
//!
//! ```rust,no_run
//! use ember_plugins::{PluginRuntime, RuntimeConfig, PluginManifest, PluginInput};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create plugin runtime
//!     let config = RuntimeConfig::default();
//!     let runtime = PluginRuntime::new(config)?;
//!
//!     // Create a plugin manifest
//!     let manifest = PluginManifest::new(
//!         "calculator",
//!         "1.0.0",
//!         "A simple calculator plugin"
//!     );
//!
//!     // Load plugin from WASM file
//!     // runtime.load_plugin(Path::new("calculator.wasm"), manifest).await?;
//!
//!     // Call a plugin function
//!     // let output = runtime.call("calculator", PluginInput {
//!     //     function: "add".to_string(),
//!     //     arguments: serde_json::json!({"a": 1, "b": 2}),
//!     // }).await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! # Plugin Development
//!
//! Plugins are WASM modules that export functions callable by Ember.
//! Each plugin must have a manifest describing its capabilities and exports.
//!
//! ## Manifest Format (JSON)
//!
//! ```json
//! {
//!   "name": "my-plugin",
//!   "version": "1.0.0",
//!   "description": "My awesome plugin",
//!   "capabilities": {
//!     "network": false,
//!     "filesystem": false
//!   },
//!   "exports": [
//!     {
//!       "name": "my_function",
//!       "description": "Does something useful",
//!       "parameters": [
//!         {"name": "input", "type": "string", "description": "Input value"}
//!       ],
//!       "returns": "string"
//!     }
//!   ]
//! }
//! ```

#![deny(missing_docs)]
#![deny(unsafe_code)]

pub mod error;
pub mod manifest;
pub mod runtime;

#[cfg(feature = "marketplace")]
pub mod marketplace;

#[cfg(feature = "hot-reload")]
pub mod hot_reload;

// Re-exports
pub use error::{PluginError, Result};
pub use manifest::{PluginCapabilities, PluginExport, PluginManifest, PluginParameter};
pub use runtime::{LoadedPlugin, PluginInput, PluginOutput, PluginRuntime, RuntimeConfig};

#[cfg(feature = "marketplace")]
pub use marketplace::{
    CacheStats, CachedPluginInfo, FeaturedPlugins, MarketplaceClient, PluginAuthor, PluginCache,
    PluginRegistryEntry, PluginSearchQuery, PluginSearchResults, PluginSortField, SortOrder,
    TagInfo,
};

#[cfg(feature = "hot-reload")]
pub use hot_reload::{
    HotReloadConfig, HotReloadEvent, HotReloadManager, HotReloadManagerBuilder, WatchedPluginInfo,
};

/// Prelude module for convenient imports.
pub mod prelude {
    pub use crate::error::{PluginError, Result};
    pub use crate::manifest::{PluginCapabilities, PluginExport, PluginManifest, PluginParameter};
    pub use crate::runtime::{
        LoadedPlugin, PluginInput, PluginOutput, PluginRuntime, RuntimeConfig,
    };

    #[cfg(feature = "marketplace")]
    pub use crate::marketplace::{
        CachedPluginInfo, MarketplaceClient, PluginCache, PluginRegistryEntry, PluginSearchQuery,
    };

    #[cfg(feature = "hot-reload")]
    pub use crate::hot_reload::{HotReloadConfig, HotReloadEvent, HotReloadManager};
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_crate_compiles() {
        // Basic compilation test
        assert!(true);
    }
}
