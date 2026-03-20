# Plugin Development Guide

This guide covers everything you need to know to create plugins for Ember.

## Overview

Ember plugins extend the agent's capabilities by adding:
- **Custom Tools**: New actions the agent can perform
- **Providers**: Additional LLM integrations
- **Storage Backends**: Custom data persistence
- **Middleware**: Request/response transformations

## Plugin Structure

A typical Ember plugin has the following structure:

```
my-plugin/
├── manifest.json       # Plugin metadata
├── src/
│   └── lib.rs         # Plugin implementation
├── Cargo.toml         # Rust dependencies
├── README.md          # Documentation
└── tests/
    └── integration.rs # Tests
```

## Quick Start

### 1. Create Plugin Scaffold

```bash
# Using the Ember CLI
ember plugin new my-plugin

# Or manually create the structure
mkdir my-plugin && cd my-plugin
cargo init --lib
```

### 2. Add Dependencies

```toml
# Cargo.toml
[package]
name = "my-plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
ember-plugins = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.0", features = ["full"] }
async-trait = "0.1"
```

### 3. Create manifest.json

```json
{
  "name": "my-plugin",
  "version": "0.1.0",
  "description": "My awesome Ember plugin",
  "author": "Your Name",
  "license": "MIT",
  "ember_version": ">=1.0.0",
  "entry_point": "libmy_plugin",
  "tools": [
    {
      "name": "my_tool",
      "description": "Does something useful",
      "parameters": {
        "input": {
          "type": "string",
          "description": "Input parameter",
          "required": true
        }
      }
    }
  ],
  "permissions": [
    "network",
    "filesystem:read"
  ],
  "config_schema": {
    "api_key": {
      "type": "string",
      "description": "API key for the service",
      "secret": true
    }
  }
}
```

### 4. Implement the Plugin

```rust
// src/lib.rs
use ember_plugins::prelude::*;
use serde::{Deserialize, Serialize};

/// Plugin metadata and entry point
#[plugin]
pub struct MyPlugin {
    config: MyPluginConfig,
}

#[derive(Debug, Deserialize)]
struct MyPluginConfig {
    api_key: Option<String>,
}

#[plugin_impl]
impl Plugin for MyPlugin {
    fn name(&self) -> &str {
        "my-plugin"
    }

    fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }

    async fn initialize(&mut self, config: Value) -> Result<()> {
        self.config = serde_json::from_value(config)?;
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<()> {
        // Cleanup resources
        Ok(())
    }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        vec![Box::new(MyTool::new(self.config.api_key.clone()))]
    }
}

/// Custom tool implementation
struct MyTool {
    api_key: Option<String>,
}

impl MyTool {
    fn new(api_key: Option<String>) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str {
        "my_tool"
    }

    fn description(&self) -> &str {
        "Does something useful with the input"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Input to process"
                }
            },
            "required": ["input"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let input: String = serde_json::from_value(params["input"].clone())?;
        
        // Do something with the input
        let output = format!("Processed: {}", input);
        
        Ok(ToolResult::success(output))
    }
}
```

## Tool Development

### Tool Interface

Every tool must implement the `Tool` trait:

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique tool name
    fn name(&self) -> &str;
    
    /// Human-readable description
    fn description(&self) -> &str;
    
    /// JSON Schema for parameters
    fn parameters(&self) -> Value;
    
    /// Execute the tool
    async fn execute(&self, params: Value) -> Result<ToolResult>;
    
    /// Optional: Validate parameters before execution
    fn validate(&self, params: &Value) -> Result<()> {
        Ok(())
    }
    
    /// Optional: Check if tool requires confirmation
    fn requires_confirmation(&self) -> bool {
        false
    }
}
```

### Tool Results

```rust
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub data: Option<Value>,
    pub artifacts: Vec<Artifact>,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            data: None,
            artifacts: vec![],
        }
    }
    
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            output: message.into(),
            data: None,
            artifacts: vec![],
        }
    }
    
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
    
    pub fn with_artifact(mut self, artifact: Artifact) -> Self {
        self.artifacts.push(artifact);
        self
    }
}
```

### Complex Tool Example

```rust
/// A tool that fetches weather data
struct WeatherTool {
    client: reqwest::Client,
    api_key: String,
}

#[async_trait]
impl Tool for WeatherTool {
    fn name(&self) -> &str {
        "get_weather"
    }

    fn description(&self) -> &str {
        "Get current weather for a location"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "City name or coordinates"
                },
                "units": {
                    "type": "string",
                    "enum": ["metric", "imperial"],
                    "default": "metric"
                }
            },
            "required": ["location"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let location = params["location"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing location"))?;
        let units = params["units"].as_str().unwrap_or("metric");
        
        let url = format!(
            "https://api.weather.com/v1/current?q={}&units={}&key={}",
            location, units, self.api_key
        );
        
        let response = self.client.get(&url).send().await?;
        
        if !response.status().is_success() {
            return Ok(ToolResult::error(format!(
                "Weather API error: {}",
                response.status()
            )));
        }
        
        let data: WeatherData = response.json().await?;
        
        Ok(ToolResult::success(format!(
            "Weather in {}: {}C, {}",
            data.location, data.temperature, data.description
        )).with_data(serde_json::to_value(&data)?))
    }
}
```

## Permissions System

Plugins operate in a sandboxed environment and must declare required permissions.

### Available Permissions

| Permission | Description |
|------------|-------------|
| `network` | Make HTTP requests |
| `filesystem:read` | Read files |
| `filesystem:write` | Write files |
| `filesystem:*` | Full filesystem access |
| `shell` | Execute shell commands |
| `env` | Access environment variables |
| `clipboard` | Access clipboard |

### Requesting Permissions

```json
{
  "permissions": [
    "network",
    "filesystem:read",
    "env:API_KEY"
  ]
}
```

### Runtime Permission Checks

```rust
async fn execute(&self, params: Value) -> Result<ToolResult> {
    // Check permission before action
    if !self.context.has_permission("filesystem:write") {
        return Ok(ToolResult::error("Write permission denied"));
    }
    
    // Proceed with operation
    // ...
}
```

## Configuration

### Config Schema

Define configuration options in manifest.json:

```json
{
  "config_schema": {
    "api_key": {
      "type": "string",
      "description": "API key",
      "secret": true,
      "required": true
    },
    "endpoint": {
      "type": "string",
      "description": "API endpoint URL",
      "default": "https://api.example.com"
    },
    "timeout": {
      "type": "integer",
      "description": "Request timeout in seconds",
      "default": 30,
      "minimum": 1,
      "maximum": 300
    },
    "features": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Enabled features"
    }
  }
}
```

### Accessing Configuration

```rust
#[derive(Debug, Deserialize)]
struct PluginConfig {
    api_key: String,
    endpoint: String,
    timeout: u32,
    features: Vec<String>,
}

async fn initialize(&mut self, config: Value) -> Result<()> {
    self.config = serde_json::from_value(config)?;
    
    // Validate configuration
    if self.config.api_key.is_empty() {
        return Err(anyhow::anyhow!("API key is required"));
    }
    
    Ok(())
}
```

## Testing Plugins

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_execution() {
        let tool = MyTool::new(Some("test-key".into()));
        
        let params = serde_json::json!({
            "input": "test data"
        });
        
        let result = tool.execute(params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Processed"));
    }

    #[test]
    fn test_parameter_validation() {
        let tool = MyTool::new(None);
        
        let invalid = serde_json::json!({});
        assert!(tool.validate(&invalid).is_err());
        
        let valid = serde_json::json!({ "input": "test" });
        assert!(tool.validate(&valid).is_ok());
    }
}
```

### Integration Tests

```rust
// tests/integration.rs
use ember_plugins::testing::*;

#[tokio::test]
async fn test_plugin_lifecycle() {
    let mut harness = PluginTestHarness::new();
    
    // Load plugin
    harness.load_plugin("target/debug/libmy_plugin.so").await.unwrap();
    
    // Initialize with config
    harness.initialize(serde_json::json!({
        "api_key": "test-key"
    })).await.unwrap();
    
    // Execute tool
    let result = harness.execute_tool("my_tool", serde_json::json!({
        "input": "test"
    })).await.unwrap();
    
    assert!(result.success);
    
    // Shutdown
    harness.shutdown().await.unwrap();
}
```

## Publishing Plugins

### 1. Prepare for Publishing

```bash
# Build release version
cargo build --release

# Run tests
cargo test

# Check manifest
ember plugin validate
```

### 2. Create Package

```bash
ember plugin package
# Creates my-plugin-0.1.0.tar.gz
```

### 3. Publish to Marketplace

```bash
# Login to marketplace
ember marketplace login

# Publish
ember plugin publish my-plugin-0.1.0.tar.gz
```

### 4. Versioning

Follow semantic versioning:
- **MAJOR**: Breaking changes
- **MINOR**: New features, backwards compatible
- **PATCH**: Bug fixes

## Best Practices

### 1. Error Handling

```rust
async fn execute(&self, params: Value) -> Result<ToolResult> {
    // Validate input
    let input = params["input"].as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: input"))?;
    
    // Handle external errors gracefully
    match self.call_api(input).await {
        Ok(data) => Ok(ToolResult::success(data)),
        Err(e) => {
            tracing::error!("API call failed: {}", e);
            Ok(ToolResult::error(format!("Operation failed: {}", e)))
        }
    }
}
```

### 2. Resource Management

```rust
impl Drop for MyPlugin {
    fn drop(&mut self) {
        // Cleanup resources
        if let Some(handle) = self.background_task.take() {
            handle.abort();
        }
    }
}
```

### 3. Logging

```rust
use tracing::{debug, info, warn, error};

async fn execute(&self, params: Value) -> Result<ToolResult> {
    debug!("Executing with params: {:?}", params);
    
    info!("Processing request");
    
    if something_unexpected {
        warn!("Unexpected condition, using fallback");
    }
    
    Ok(ToolResult::success("Done"))
}
```

### 4. Documentation

```rust
/// Fetches weather data for a given location.
///
/// # Parameters
/// - `location`: City name (e.g., "London") or coordinates ("51.5,-0.1")
/// - `units`: Temperature units - "metric" (Celsius) or "imperial" (Fahrenheit)
///
/// # Returns
/// Weather information including temperature, humidity, and description.
///
/// # Errors
/// - Returns error if location is not found
/// - Returns error if API is unavailable
///
/// # Example
/// ```json
/// {
///   "location": "London",
///   "units": "metric"
/// }
/// ```
```

## SDK Reference

### Plugin Macro

```rust
#[plugin]
pub struct MyPlugin {
    // Plugin state
}
```

### Plugin Trait

```rust
#[plugin_impl]
impl Plugin for MyPlugin {
    // Required
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    
    // Optional
    async fn initialize(&mut self, config: Value) -> Result<()>;
    async fn shutdown(&mut self) -> Result<()>;
    fn tools(&self) -> Vec<Box<dyn Tool>>;
    fn providers(&self) -> Vec<Box<dyn Provider>>;
    fn middleware(&self) -> Vec<Box<dyn Middleware>>;
}
```

### Context

```rust
pub struct PluginContext {
    pub config: Value,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl PluginContext {
    pub fn has_permission(&self, permission: &str) -> bool;
    pub fn get_secret(&self, key: &str) -> Option<String>;
    pub fn emit_event(&self, event: Event);
}
```

## See Also

- [Custom Tools](../custom-tools.md) - Creating tools without plugins
- [API Reference](../api/index.md) - Full API documentation
- [Plugin Marketplace](https://marketplace.ember.ai) - Browse available plugins