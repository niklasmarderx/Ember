//! Plugin manifest types.
//!
//! This module defines the manifest format for Ember plugins.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Plugin manifest describing a WASM plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin name (unique identifier).
    pub name: String,
    /// Plugin version (semver).
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Plugin author.
    pub author: Option<String>,
    /// Plugin license.
    pub license: Option<String>,
    /// Minimum Ember version required.
    pub ember_version: Option<String>,
    /// Plugin capabilities/permissions.
    #[serde(default)]
    pub capabilities: PluginCapabilities,
    /// Exported functions.
    #[serde(default)]
    pub exports: Vec<PluginExport>,
    /// Plugin configuration schema.
    #[serde(default)]
    pub config_schema: Option<serde_json::Value>,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl PluginManifest {
    /// Create a new plugin manifest with required fields.
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: description.into(),
            author: None,
            license: None,
            ember_version: None,
            capabilities: PluginCapabilities::default(),
            exports: Vec::new(),
            config_schema: None,
            metadata: HashMap::new(),
        }
    }

    /// Add an exported function.
    pub fn with_export(mut self, export: PluginExport) -> Self {
        self.exports.push(export);
        self
    }

    /// Set the author.
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Set capabilities.
    pub fn with_capabilities(mut self, capabilities: PluginCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }
}

/// Plugin capabilities define what a plugin can do.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginCapabilities {
    /// Can access the network.
    #[serde(default)]
    pub network: bool,
    /// Can access the filesystem (within allowed paths).
    #[serde(default)]
    pub filesystem: bool,
    /// Can access environment variables.
    #[serde(default)]
    pub environment: bool,
    /// Can execute shell commands.
    #[serde(default)]
    pub shell: bool,
    /// Maximum memory in bytes (0 = default limit).
    #[serde(default)]
    pub max_memory: usize,
    /// Maximum execution time in milliseconds (0 = default limit).
    #[serde(default)]
    pub max_execution_time_ms: u64,
}

impl PluginCapabilities {
    /// Create capabilities with no permissions.
    pub fn none() -> Self {
        Self::default()
    }

    /// Create capabilities with all permissions.
    pub fn all() -> Self {
        Self {
            network: true,
            filesystem: true,
            environment: true,
            shell: true,
            max_memory: 0,
            max_execution_time_ms: 0,
        }
    }

    /// Enable network access.
    pub fn with_network(mut self) -> Self {
        self.network = true;
        self
    }

    /// Enable filesystem access.
    pub fn with_filesystem(mut self) -> Self {
        self.filesystem = true;
        self
    }

    /// Set memory limit.
    pub fn with_max_memory(mut self, bytes: usize) -> Self {
        self.max_memory = bytes;
        self
    }

    /// Set execution time limit.
    pub fn with_max_execution_time(mut self, ms: u64) -> Self {
        self.max_execution_time_ms = ms;
        self
    }
}

/// An exported function from a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginExport {
    /// Function name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Function parameters.
    #[serde(default)]
    pub parameters: Vec<PluginParameter>,
    /// Return type description.
    pub returns: Option<String>,
}

impl PluginExport {
    /// Create a new plugin export.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: Vec::new(),
            returns: None,
        }
    }

    /// Add a parameter.
    pub fn with_parameter(mut self, param: PluginParameter) -> Self {
        self.parameters.push(param);
        self
    }

    /// Set the return type.
    pub fn with_returns(mut self, returns: impl Into<String>) -> Self {
        self.returns = Some(returns.into());
        self
    }
}

/// A parameter for a plugin function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginParameter {
    /// Parameter name.
    pub name: String,
    /// Parameter type (e.g., "string", "number", "boolean", "object").
    #[serde(rename = "type")]
    pub param_type: String,
    /// Human-readable description.
    pub description: String,
    /// Whether the parameter is required.
    #[serde(default = "default_true")]
    pub required: bool,
    /// Default value if not required.
    pub default: Option<serde_json::Value>,
}

fn default_true() -> bool {
    true
}

impl PluginParameter {
    /// Create a new required parameter.
    pub fn new(
        name: impl Into<String>,
        param_type: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            param_type: param_type.into(),
            description: description.into(),
            required: true,
            default: None,
        }
    }

    /// Make the parameter optional with a default value.
    pub fn optional(mut self, default: serde_json::Value) -> Self {
        self.required = false;
        self.default = Some(default);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_creation() {
        let manifest = PluginManifest::new("calculator", "1.0.0", "A simple calculator plugin")
            .with_author("Ember Team")
            .with_export(
                PluginExport::new("add", "Add two numbers")
                    .with_parameter(PluginParameter::new("a", "number", "First number"))
                    .with_parameter(PluginParameter::new("b", "number", "Second number"))
                    .with_returns("number"),
            );

        assert_eq!(manifest.name, "calculator");
        assert_eq!(manifest.exports.len(), 1);
        assert_eq!(manifest.exports[0].parameters.len(), 2);
    }

    #[test]
    fn test_manifest_serialization() {
        let manifest = PluginManifest::new("test", "1.0.0", "Test plugin");
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test");
    }

    #[test]
    fn test_capabilities() {
        let caps = PluginCapabilities::none()
            .with_network()
            .with_max_memory(1024 * 1024);

        assert!(caps.network);
        assert!(!caps.filesystem);
        assert_eq!(caps.max_memory, 1024 * 1024);
    }

    #[test]
    fn test_optional_parameter() {
        let param = PluginParameter::new("count", "number", "Number of items")
            .optional(serde_json::json!(10));

        assert!(!param.required);
        assert_eq!(param.default, Some(serde_json::json!(10)));
    }
}
