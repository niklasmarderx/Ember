//! Plugin SDK for developing Ember plugins
//!
//! This module provides utilities for plugin developers to easily create,
//! validate, test, and package Ember plugins.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use ember_plugins::sdk::{PluginBuilder, PluginType};
//!
//! let plugin = PluginBuilder::new("my-calculator")
//!     .version("1.0.0")
//!     .description("A simple calculator plugin")
//!     .author("Developer", "dev@example.com")
//!     .plugin_type(PluginType::Tool)
//!     .add_function("add")
//!         .description("Add two numbers")
//!         .param("a", "number", "First number")
//!         .param("b", "number", "Second number")
//!         .returns("number")
//!         .done()
//!     .add_function("multiply")
//!         .description("Multiply two numbers")
//!         .param("a", "number", "First number")
//!         .param("b", "number", "Second number")
//!         .returns("number")
//!         .done()
//!     .capability_network(false)
//!     .capability_filesystem(false)
//!     .build()
//!     .unwrap();
//! ```

use crate::manifest::{PluginCapabilities, PluginExport, PluginManifest, PluginParameter};
use crate::{PluginError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::info;

// =============================================================================
// Plugin Types and Categories
// =============================================================================

/// Type of plugin functionality
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    /// A tool plugin that can be called by the agent
    #[default]
    Tool,
    /// A provider plugin (LLM provider)
    Provider,
    /// A storage plugin
    Storage,
    /// A transformer plugin (data transformation)
    Transformer,
    /// A hook plugin (lifecycle hooks)
    Hook,
    /// A custom plugin type
    Custom,
}

/// Plugin category for organization
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginCategory {
    /// Productivity tools
    Productivity,
    /// Development tools
    Development,
    /// Data processing
    DataProcessing,
    /// Communication
    Communication,
    /// AI/ML related
    ArtificialIntelligence,
    /// File management
    FileManagement,
    /// Web/API
    WebApi,
    /// System utilities
    System,
    /// Custom category
    Custom(String),
}

impl Default for PluginCategory {
    fn default() -> Self {
        Self::Custom("other".to_string())
    }
}

// =============================================================================
// Plugin Builder
// =============================================================================

/// Builder for creating plugin manifests with a fluent API
#[derive(Debug, Clone)]
pub struct PluginBuilder {
    name: String,
    version: String,
    description: String,
    plugin_type: PluginType,
    category: PluginCategory,
    author_name: Option<String>,
    author_email: Option<String>,
    homepage: Option<String>,
    repository: Option<String>,
    license: Option<String>,
    keywords: Vec<String>,
    capabilities: PluginCapabilities,
    exports: Vec<PluginExport>,
    dependencies: HashMap<String, String>,
    metadata: HashMap<String, serde_json::Value>,
}

impl PluginBuilder {
    /// Create a new plugin builder with the given name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: "0.1.0".to_string(),
            description: String::new(),
            plugin_type: PluginType::default(),
            category: PluginCategory::default(),
            author_name: None,
            author_email: None,
            homepage: None,
            repository: None,
            license: None,
            keywords: Vec::new(),
            capabilities: PluginCapabilities::none(),
            exports: Vec::new(),
            dependencies: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    /// Set the plugin version
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Set the plugin description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set the plugin type
    pub fn plugin_type(mut self, plugin_type: PluginType) -> Self {
        self.plugin_type = plugin_type;
        self
    }

    /// Set the plugin category
    pub fn category(mut self, category: PluginCategory) -> Self {
        self.category = category;
        self
    }

    /// Set the author information
    pub fn author(mut self, name: impl Into<String>, email: impl Into<String>) -> Self {
        self.author_name = Some(name.into());
        self.author_email = Some(email.into());
        self
    }

    /// Set the homepage URL
    pub fn homepage(mut self, url: impl Into<String>) -> Self {
        self.homepage = Some(url.into());
        self
    }

    /// Set the repository URL
    pub fn repository(mut self, url: impl Into<String>) -> Self {
        self.repository = Some(url.into());
        self
    }

    /// Set the license
    pub fn license(mut self, license: impl Into<String>) -> Self {
        self.license = Some(license.into());
        self
    }

    /// Add a keyword
    pub fn keyword(mut self, keyword: impl Into<String>) -> Self {
        self.keywords.push(keyword.into());
        self
    }

    /// Add multiple keywords
    pub fn keywords(mut self, keywords: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.keywords.extend(keywords.into_iter().map(Into::into));
        self
    }

    /// Enable or disable network capability
    pub fn capability_network(mut self, enabled: bool) -> Self {
        self.capabilities.network = enabled;
        self
    }

    /// Enable or disable filesystem capability
    pub fn capability_filesystem(mut self, enabled: bool) -> Self {
        self.capabilities.filesystem = enabled;
        self
    }

    /// Enable or disable environment capability
    pub fn capability_environment(mut self, enabled: bool) -> Self {
        self.capabilities.environment = enabled;
        self
    }

    /// Enable or disable shell capability
    pub fn capability_shell(mut self, enabled: bool) -> Self {
        self.capabilities.shell = enabled;
        self
    }

    /// Start building a function export
    pub fn add_function(self, name: impl Into<String>) -> FunctionBuilder {
        FunctionBuilder::new(self, name.into())
    }

    /// Add a pre-built function export
    pub fn with_export(mut self, export: PluginExport) -> Self {
        self.exports.push(export);
        self
    }

    /// Add a dependency
    pub fn dependency(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.dependencies.insert(name.into(), version.into());
        self
    }

    /// Add metadata
    pub fn metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Build the plugin manifest
    pub fn build(self) -> Result<PluginDefinition> {
        // Validate
        if self.name.is_empty() {
            return Err(PluginError::InvalidManifest(
                "Plugin name is required".to_string(),
            ));
        }

        if self.exports.is_empty() {
            return Err(PluginError::InvalidManifest(
                "Plugin must have at least one export".to_string(),
            ));
        }

        let mut manifest = PluginManifest::new(&self.name, &self.version, &self.description)
            .with_capabilities(self.capabilities.clone());

        // Add exports one by one since with_exports doesn't exist
        for export in self.exports.clone() {
            manifest = manifest.with_export(export);
        }

        Ok(PluginDefinition {
            manifest,
            plugin_type: self.plugin_type,
            category: self.category,
            author_name: self.author_name,
            author_email: self.author_email,
            homepage: self.homepage,
            repository: self.repository,
            license: self.license,
            keywords: self.keywords,
            dependencies: self.dependencies,
            metadata: self.metadata,
        })
    }

    // Internal method to add export
    fn add_export(mut self, export: PluginExport) -> Self {
        self.exports.push(export);
        self
    }
}

// =============================================================================
// Function Builder
// =============================================================================

/// Builder for function exports
pub struct FunctionBuilder {
    parent: PluginBuilder,
    name: String,
    description: String,
    parameters: Vec<PluginParameter>,
    returns: String,
}

impl FunctionBuilder {
    fn new(parent: PluginBuilder, name: String) -> Self {
        Self {
            parent,
            name,
            description: String::new(),
            parameters: Vec::new(),
            returns: "void".to_string(),
        }
    }

    /// Set the function description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Add a parameter
    pub fn param(
        mut self,
        name: impl Into<String>,
        param_type: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        self.parameters.push(PluginParameter {
            name: name.into(),
            param_type: param_type.into(),
            description: description.into(),
            required: true,
            default: None,
        });
        self
    }

    /// Add an optional parameter with default value
    pub fn optional_param(
        mut self,
        name: impl Into<String>,
        param_type: impl Into<String>,
        description: impl Into<String>,
        default: serde_json::Value,
    ) -> Self {
        self.parameters.push(PluginParameter {
            name: name.into(),
            param_type: param_type.into(),
            description: description.into(),
            required: false,
            default: Some(default),
        });
        self
    }

    /// Set the return type
    pub fn returns(mut self, return_type: impl Into<String>) -> Self {
        self.returns = return_type.into();
        self
    }

    /// Finish building this function and return to the plugin builder
    pub fn done(self) -> PluginBuilder {
        let export = PluginExport {
            name: self.name,
            description: self.description,
            parameters: self.parameters,
            returns: Some(self.returns),
        };
        self.parent.add_export(export)
    }
}

// =============================================================================
// Plugin Definition
// =============================================================================

/// Complete plugin definition with extended metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDefinition {
    /// Core manifest
    pub manifest: PluginManifest,
    /// Plugin type
    pub plugin_type: PluginType,
    /// Plugin category
    pub category: PluginCategory,
    /// Author name
    pub author_name: Option<String>,
    /// Author email
    pub author_email: Option<String>,
    /// Homepage URL
    pub homepage: Option<String>,
    /// Repository URL
    pub repository: Option<String>,
    /// License identifier
    pub license: Option<String>,
    /// Keywords for search
    pub keywords: Vec<String>,
    /// Plugin dependencies
    pub dependencies: HashMap<String, String>,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl PluginDefinition {
    /// Get the plugin name
    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    /// Get the plugin version
    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    /// Convert to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| PluginError::Internal(format!("Failed to serialize: {}", e)))
    }

    /// Load from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .map_err(|e| PluginError::InvalidManifest(format!("Failed to parse: {}", e)))
    }

    /// Save to file
    pub async fn save_to_file(&self, path: &Path) -> Result<()> {
        let json = self.to_json()?;
        tokio::fs::write(path, json)
            .await
            .map_err(|e| PluginError::Internal(format!("Failed to write file: {}", e)))?;
        info!(path = %path.display(), "Plugin definition saved");
        Ok(())
    }

    /// Load from file
    pub async fn load_from_file(path: &Path) -> Result<Self> {
        let json = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| PluginError::Internal(format!("Failed to read file: {}", e)))?;
        Self::from_json(&json)
    }
}

// =============================================================================
// Plugin Validator
// =============================================================================

/// Validation result with details
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed
    pub valid: bool,
    /// List of errors
    pub errors: Vec<ValidationError>,
    /// List of warnings
    pub warnings: Vec<ValidationWarning>,
}

/// Validation error
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Location in the manifest
    pub location: Option<String>,
}

/// Validation warning
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    /// Warning code
    pub code: String,
    /// Warning message
    pub message: String,
    /// Suggestion for fix
    pub suggestion: Option<String>,
}

/// Plugin validator for checking manifest and WASM validity
pub struct PluginValidator {
    /// Check for recommended fields
    check_recommended: bool,
    /// Check for security issues
    check_security: bool,
    /// Check for best practices
    check_best_practices: bool,
}

impl Default for PluginValidator {
    fn default() -> Self {
        Self {
            check_recommended: true,
            check_security: true,
            check_best_practices: true,
        }
    }
}

impl PluginValidator {
    /// Create a new validator
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable/disable recommended field checks
    pub fn with_recommended_checks(mut self, enabled: bool) -> Self {
        self.check_recommended = enabled;
        self
    }

    /// Enable/disable security checks
    pub fn with_security_checks(mut self, enabled: bool) -> Self {
        self.check_security = enabled;
        self
    }

    /// Enable/disable best practice checks
    pub fn with_best_practice_checks(mut self, enabled: bool) -> Self {
        self.check_best_practices = enabled;
        self
    }

    /// Validate a plugin definition
    pub fn validate(&self, plugin: &PluginDefinition) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Required field checks
        if plugin.manifest.name.is_empty() {
            errors.push(ValidationError {
                code: "E001".to_string(),
                message: "Plugin name is required".to_string(),
                location: Some("manifest.name".to_string()),
            });
        }

        if !Self::is_valid_name(&plugin.manifest.name) {
            errors.push(ValidationError {
                code: "E002".to_string(),
                message: "Plugin name must be lowercase, alphanumeric, with dashes only"
                    .to_string(),
                location: Some("manifest.name".to_string()),
            });
        }

        if !Self::is_valid_version(&plugin.manifest.version) {
            errors.push(ValidationError {
                code: "E003".to_string(),
                message: "Version must be valid semver (e.g., 1.0.0)".to_string(),
                location: Some("manifest.version".to_string()),
            });
        }

        if plugin.manifest.exports.is_empty() {
            errors.push(ValidationError {
                code: "E004".to_string(),
                message: "Plugin must export at least one function".to_string(),
                location: Some("manifest.exports".to_string()),
            });
        }

        // Validate exports
        for (i, export) in plugin.manifest.exports.iter().enumerate() {
            if export.name.is_empty() {
                errors.push(ValidationError {
                    code: "E005".to_string(),
                    message: format!("Export {} has no name", i),
                    location: Some(format!("manifest.exports[{}].name", i)),
                });
            }

            if export.description.is_empty() && self.check_recommended {
                warnings.push(ValidationWarning {
                    code: "W001".to_string(),
                    message: format!("Export '{}' has no description", export.name),
                    suggestion: Some("Add a description for better documentation".to_string()),
                });
            }
        }

        // Recommended field checks
        if self.check_recommended {
            if plugin.manifest.description.is_empty() {
                warnings.push(ValidationWarning {
                    code: "W002".to_string(),
                    message: "Plugin has no description".to_string(),
                    suggestion: Some(
                        "Add a description to help users understand the plugin".to_string(),
                    ),
                });
            }

            if plugin.author_name.is_none() {
                warnings.push(ValidationWarning {
                    code: "W003".to_string(),
                    message: "No author specified".to_string(),
                    suggestion: Some("Add author information for attribution".to_string()),
                });
            }

            if plugin.license.is_none() {
                warnings.push(ValidationWarning {
                    code: "W004".to_string(),
                    message: "No license specified".to_string(),
                    suggestion: Some("Add a license (e.g., MIT, Apache-2.0)".to_string()),
                });
            }

            if plugin.keywords.is_empty() {
                warnings.push(ValidationWarning {
                    code: "W005".to_string(),
                    message: "No keywords specified".to_string(),
                    suggestion: Some("Add keywords for better discoverability".to_string()),
                });
            }
        }

        // Security checks
        if self.check_security {
            let caps = &plugin.manifest.capabilities;
            let cap_count = [caps.network, caps.filesystem, caps.environment, caps.shell]
                .iter()
                .filter(|&&x| x)
                .count();

            if cap_count >= 3 {
                warnings.push(ValidationWarning {
                    code: "S001".to_string(),
                    message: "Plugin requests many capabilities".to_string(),
                    suggestion: Some("Consider if all capabilities are necessary".to_string()),
                });
            }

            if caps.shell {
                warnings.push(ValidationWarning {
                    code: "S002".to_string(),
                    message: "Plugin requests shell capability".to_string(),
                    suggestion: Some(
                        "Shell access is powerful - ensure it's necessary".to_string(),
                    ),
                });
            }
        }

        // Best practice checks
        if self.check_best_practices {
            if plugin.manifest.name.len() > 50 {
                warnings.push(ValidationWarning {
                    code: "B001".to_string(),
                    message: "Plugin name is very long".to_string(),
                    suggestion: Some("Keep names under 50 characters".to_string()),
                });
            }

            if plugin.manifest.exports.len() > 20 {
                warnings.push(ValidationWarning {
                    code: "B002".to_string(),
                    message: "Plugin exports many functions".to_string(),
                    suggestion: Some("Consider splitting into multiple plugins".to_string()),
                });
            }
        }

        ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
        }
    }

    /// Validate a plugin name
    fn is_valid_name(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 100
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
            && !name.starts_with('-')
            && !name.ends_with('-')
    }

    /// Validate a semver version
    fn is_valid_version(version: &str) -> bool {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return false;
        }
        parts.iter().all(|p| p.parse::<u32>().is_ok())
    }
}

// =============================================================================
// Plugin Template Generator
// =============================================================================

/// Template type for plugin generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateType {
    /// Basic tool plugin
    BasicTool,
    /// HTTP API plugin
    HttpApi,
    /// File processor plugin
    FileProcessor,
    /// Data transformer plugin
    DataTransformer,
    /// Provider plugin
    Provider,
}

/// Plugin template generator
pub struct PluginTemplate {
    template_type: TemplateType,
    name: String,
    description: String,
}

impl PluginTemplate {
    /// Create a new template
    pub fn new(template_type: TemplateType, name: impl Into<String>) -> Self {
        Self {
            template_type,
            name: name.into(),
            description: String::new(),
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Generate the manifest
    pub fn generate_manifest(&self) -> PluginDefinition {
        let builder = PluginBuilder::new(&self.name)
            .version("0.1.0")
            .description(&self.description);

        let builder = match self.template_type {
            TemplateType::BasicTool => builder
                .plugin_type(PluginType::Tool)
                .category(PluginCategory::Productivity)
                .add_function("execute")
                .description("Execute the tool")
                .param("input", "string", "Input data")
                .returns("string")
                .done(),

            TemplateType::HttpApi => builder
                .plugin_type(PluginType::Tool)
                .category(PluginCategory::WebApi)
                .capability_network(true)
                .add_function("get")
                .description("HTTP GET request")
                .param("url", "string", "URL to fetch")
                .optional_param("headers", "object", "HTTP headers", serde_json::json!({}))
                .returns("object")
                .done()
                .add_function("post")
                .description("HTTP POST request")
                .param("url", "string", "URL to post to")
                .param("body", "object", "Request body")
                .optional_param("headers", "object", "HTTP headers", serde_json::json!({}))
                .returns("object")
                .done(),

            TemplateType::FileProcessor => builder
                .plugin_type(PluginType::Tool)
                .category(PluginCategory::FileManagement)
                .capability_filesystem(true)
                .add_function("process")
                .description("Process a file")
                .param("path", "string", "Path to file")
                .optional_param(
                    "options",
                    "object",
                    "Processing options",
                    serde_json::json!({}),
                )
                .returns("object")
                .done()
                .add_function("list")
                .description("List files in directory")
                .param("path", "string", "Directory path")
                .returns("array")
                .done(),

            TemplateType::DataTransformer => builder
                .plugin_type(PluginType::Transformer)
                .category(PluginCategory::DataProcessing)
                .add_function("transform")
                .description("Transform data")
                .param("data", "any", "Input data")
                .param("format", "string", "Target format")
                .returns("any")
                .done()
                .add_function("validate")
                .description("Validate data")
                .param("data", "any", "Data to validate")
                .param("schema", "object", "Validation schema")
                .returns("object")
                .done(),

            TemplateType::Provider => builder
                .plugin_type(PluginType::Provider)
                .category(PluginCategory::ArtificialIntelligence)
                .capability_network(true)
                .add_function("complete")
                .description("Generate completion")
                .param("messages", "array", "Conversation messages")
                .optional_param(
                    "temperature",
                    "number",
                    "Sampling temperature",
                    serde_json::json!(0.7),
                )
                .optional_param(
                    "max_tokens",
                    "number",
                    "Maximum tokens",
                    serde_json::json!(1000),
                )
                .returns("object")
                .done()
                .add_function("stream")
                .description("Generate streaming completion")
                .param("messages", "array", "Conversation messages")
                .returns("stream")
                .done(),
        };

        builder.build().expect("Template should always be valid")
    }

    /// Generate Rust source code template
    pub fn generate_rust_source(&self) -> String {
        let fn_impls = match self.template_type {
            TemplateType::BasicTool => {
                r#"
#[no_mangle]
pub extern "C" fn execute(input_ptr: *const u8, input_len: usize) -> *mut u8 {
    // Parse input
    let input = unsafe {
        let slice = std::slice::from_raw_parts(input_ptr, input_len);
        String::from_utf8_lossy(slice).to_string()
    };
    
    // TODO: Implement your tool logic here
    let result = format!("Processed: {}", input);
    
    // Return result
    Box::into_raw(result.into_bytes().into_boxed_slice()) as *mut u8
}"#
            }
            TemplateType::HttpApi => {
                r#"
#[no_mangle]
pub extern "C" fn get(url_ptr: *const u8, url_len: usize) -> *mut u8 {
    // TODO: Implement HTTP GET
    let url = unsafe {
        let slice = std::slice::from_raw_parts(url_ptr, url_len);
        String::from_utf8_lossy(slice).to_string()
    };
    
    let result = serde_json::json!({
        "status": 200,
        "url": url,
        "body": "Not implemented"
    });
    
    Box::into_raw(result.to_string().into_bytes().into_boxed_slice()) as *mut u8
}

#[no_mangle]
pub extern "C" fn post(/* params */) -> *mut u8 {
    // TODO: Implement HTTP POST
    unimplemented!()
}"#
            }
            _ => {
                r#"
#[no_mangle]
pub extern "C" fn execute(/* params */) -> *mut u8 {
    // TODO: Implement your plugin logic
    unimplemented!()
}"#
            }
        };

        format!(
            r#"//! {} Plugin
//!
//! {}

#![no_std]
#![no_main]

extern crate alloc;

use alloc::{{boxed::Box, string::String, vec::Vec}};

// Plugin entry point
{}

// Memory allocation for WASM
#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {{
    let layout = alloc::alloc::Layout::from_size_align(size, 1).unwrap();
    unsafe {{ alloc::alloc::alloc(layout) }}
}}

#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, size: usize) {{
    let layout = alloc::alloc::Layout::from_size_align(size, 1).unwrap();
    unsafe {{ alloc::alloc::dealloc(ptr, layout) }}
}}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {{
    loop {{}}
}}
"#,
            self.name,
            if self.description.is_empty() {
                "A plugin for Ember"
            } else {
                &self.description
            },
            fn_impls
        )
    }

    /// Generate Cargo.toml template
    pub fn generate_cargo_toml(&self) -> String {
        format!(
            r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
serde = {{ version = "1.0", default-features = false, features = ["alloc", "derive"] }}
serde_json = {{ version = "1.0", default-features = false, features = ["alloc"] }}

[profile.release]
opt-level = "s"
lto = true
"#,
            self.name
        )
    }
}

// =============================================================================
// Plugin Packager
// =============================================================================

/// Package format for distribution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageFormat {
    /// Single WASM file with embedded manifest
    WasmBundle,
    /// Tar.gz archive
    TarGz,
    /// Zip archive
    Zip,
}

/// Plugin package metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadata {
    /// Package format version
    pub format_version: String,
    /// Plugin definition
    pub definition: PluginDefinition,
    /// File checksums
    pub checksums: HashMap<String, String>,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Plugin packager for creating distributable packages
pub struct PluginPackager {
    definition: PluginDefinition,
    wasm_path: Option<PathBuf>,
    extra_files: Vec<(PathBuf, String)>,
}

impl PluginPackager {
    /// Create a new packager
    pub fn new(definition: PluginDefinition) -> Self {
        Self {
            definition,
            wasm_path: None,
            extra_files: Vec::new(),
        }
    }

    /// Set the WASM file path
    pub fn with_wasm(mut self, path: impl Into<PathBuf>) -> Self {
        self.wasm_path = Some(path.into());
        self
    }

    /// Add an extra file to the package
    pub fn add_file(mut self, path: impl Into<PathBuf>, archive_name: impl Into<String>) -> Self {
        self.extra_files.push((path.into(), archive_name.into()));
        self
    }

    /// Create the package metadata
    pub fn create_metadata(&self) -> PackageMetadata {
        PackageMetadata {
            format_version: "1.0.0".to_string(),
            definition: self.definition.clone(),
            checksums: HashMap::new(),
            created_at: chrono::Utc::now(),
        }
    }

    /// Package to a directory (for testing/development)
    pub async fn package_to_dir(&self, output_dir: &Path) -> Result<PathBuf> {
        // Create output directory
        tokio::fs::create_dir_all(output_dir)
            .await
            .map_err(|e| PluginError::Internal(format!("Failed to create directory: {}", e)))?;

        // Write manifest
        let manifest_path = output_dir.join("plugin.json");
        self.definition.save_to_file(&manifest_path).await?;

        // Copy WASM file if present
        if let Some(wasm_path) = &self.wasm_path {
            let dest = output_dir.join("plugin.wasm");
            tokio::fs::copy(wasm_path, &dest)
                .await
                .map_err(|e| PluginError::Internal(format!("Failed to copy WASM: {}", e)))?;
        }

        // Copy extra files
        for (src, name) in &self.extra_files {
            let dest = output_dir.join(name);
            tokio::fs::copy(src, &dest)
                .await
                .map_err(|e| PluginError::Internal(format!("Failed to copy file: {}", e)))?;
        }

        info!(
            plugin = %self.definition.name(),
            output = %output_dir.display(),
            "Plugin packaged"
        );

        Ok(output_dir.to_path_buf())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_builder() {
        let plugin = PluginBuilder::new("test-plugin")
            .version("1.0.0")
            .description("A test plugin")
            .author("Test Author", "test@example.com")
            .plugin_type(PluginType::Tool)
            .category(PluginCategory::Productivity)
            .keyword("test")
            .keyword("example")
            .capability_network(false)
            .capability_filesystem(false)
            .add_function("test_fn")
            .description("A test function")
            .param("input", "string", "Input value")
            .returns("string")
            .done()
            .build()
            .unwrap();

        assert_eq!(plugin.name(), "test-plugin");
        assert_eq!(plugin.version(), "1.0.0");
        assert_eq!(plugin.manifest.exports.len(), 1);
        assert_eq!(plugin.keywords.len(), 2);
    }

    #[test]
    fn test_plugin_builder_validation() {
        // Empty name should fail
        let result = PluginBuilder::new("")
            .add_function("test")
            .returns("void")
            .done()
            .build();
        assert!(result.is_err());

        // No exports should fail
        let result = PluginBuilder::new("test").build();
        assert!(result.is_err());
    }

    #[test]
    fn test_validator() {
        let plugin = PluginBuilder::new("valid-plugin")
            .version("1.0.0")
            .description("A valid plugin")
            .add_function("test")
            .description("Test function")
            .returns("void")
            .done()
            .build()
            .unwrap();

        let validator = PluginValidator::new();
        let result = validator.validate(&plugin);

        assert!(result.valid);
    }

    #[test]
    fn test_validator_warnings() {
        let plugin = PluginBuilder::new("test")
            .add_function("test")
            .returns("void")
            .done()
            .build()
            .unwrap();

        let validator = PluginValidator::new();
        let result = validator.validate(&plugin);

        // Should have warnings for missing description, author, license, etc.
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_name_validation() {
        assert!(PluginValidator::is_valid_name("my-plugin"));
        assert!(PluginValidator::is_valid_name("plugin_123"));
        assert!(!PluginValidator::is_valid_name("My-Plugin")); // uppercase
        assert!(!PluginValidator::is_valid_name("-plugin")); // starts with dash
        assert!(!PluginValidator::is_valid_name("")); // empty
    }

    #[test]
    fn test_version_validation() {
        assert!(PluginValidator::is_valid_version("1.0.0"));
        assert!(PluginValidator::is_valid_version("0.1.0"));
        assert!(PluginValidator::is_valid_version("10.20.30"));
        assert!(!PluginValidator::is_valid_version("1.0")); // missing patch
        assert!(!PluginValidator::is_valid_version("v1.0.0")); // has 'v'
        assert!(!PluginValidator::is_valid_version("1.0.0-beta")); // has suffix
    }

    #[test]
    fn test_template_generation() {
        let template = PluginTemplate::new(TemplateType::BasicTool, "my-tool")
            .with_description("My custom tool");

        let plugin = template.generate_manifest();
        assert_eq!(plugin.name(), "my-tool");
        assert_eq!(plugin.plugin_type, PluginType::Tool);

        let rust_code = template.generate_rust_source();
        assert!(rust_code.contains("execute"));

        let cargo_toml = template.generate_cargo_toml();
        assert!(cargo_toml.contains("my-tool"));
    }

    #[test]
    fn test_http_api_template() {
        let template = PluginTemplate::new(TemplateType::HttpApi, "http-client");
        let plugin = template.generate_manifest();

        assert!(plugin.manifest.capabilities.network);
        assert!(plugin.manifest.exports.iter().any(|e| e.name == "get"));
        assert!(plugin.manifest.exports.iter().any(|e| e.name == "post"));
    }

    #[test]
    fn test_serialization() {
        let plugin = PluginBuilder::new("test")
            .version("1.0.0")
            .add_function("test")
            .returns("void")
            .done()
            .build()
            .unwrap();

        let json = plugin.to_json().unwrap();
        let parsed = PluginDefinition::from_json(&json).unwrap();

        assert_eq!(parsed.name(), plugin.name());
    }
}
