//! # Ember Tool SDK
//!
//! This module provides the Software Development Kit (SDK) for creating
//! custom tools for Ember AI agents. It includes helper traits, macros,
//! and utilities to simplify tool development.
//!
//! ## Quick Start
//!
//! Creating a custom tool involves three steps:
//!
//! 1. Define your tool struct
//! 2. Implement the `ToolHandler` trait (or use the SDK helpers)
//! 3. Register with a `ToolRegistry`
//!
//! ## Example: Basic Tool
//!
//! ```rust,no_run
//! use ember_tools::sdk::{SimpleTool, SimpleToolConfig};
//! use ember_tools::{ToolRegistry, ToolOutput};
//! use serde_json::Value;
//!
//! // Create a simple tool using the builder pattern
//! let greeting_tool = SimpleTool::builder("greet")
//!     .description("Greets a user by name")
//!     .string_param("name", "The name to greet", true)
//!     .handler(|args| {
//!         let name = args.get("name")
//!             .and_then(|v| v.as_str())
//!             .unwrap_or("World");
//!         Ok(ToolOutput::success(format!("Hello, {}!", name)))
//!     })
//!     .build();
//!
//! let mut registry = ToolRegistry::new();
//! registry.register(greeting_tool);
//! ```

use crate::{Error, Result, ToolDefinition, ToolHandler, ToolOutput};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Configuration for a simple synchronous tool.
#[derive(Clone)]
pub struct SimpleToolConfig {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Parameter definitions
    pub params: Vec<ParamDef>,
    /// Whether the tool is enabled
    pub enabled: bool,
}

/// Parameter definition for tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDef {
    /// Parameter name
    pub name: String,
    /// Parameter description
    pub description: String,
    /// Parameter type
    pub param_type: ParamType,
    /// Whether the parameter is required
    pub required: bool,
    /// Default value (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// Enum values (for enum type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

/// Supported parameter types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamType {
    /// String parameter
    String,
    /// Integer parameter
    Integer,
    /// Number (float) parameter
    Number,
    /// Boolean parameter
    Boolean,
    /// Array parameter
    Array,
    /// Object parameter
    Object,
}

impl ParamType {
    /// Get the JSON Schema type string.
    pub fn as_schema_type(&self) -> &'static str {
        match self {
            ParamType::String => "string",
            ParamType::Integer => "integer",
            ParamType::Number => "number",
            ParamType::Boolean => "boolean",
            ParamType::Array => "array",
            ParamType::Object => "object",
        }
    }
}

/// Type alias for synchronous tool handlers.
pub type SyncHandler = Arc<dyn Fn(Value) -> Result<ToolOutput> + Send + Sync>;

/// Type alias for async tool handlers.
pub type AsyncHandler =
    Arc<dyn Fn(Value) -> Pin<Box<dyn Future<Output = Result<ToolOutput>> + Send>> + Send + Sync>;

/// A simple tool with a synchronous handler function.
pub struct SimpleTool {
    config: SimpleToolConfig,
    handler: SyncHandler,
}

impl SimpleTool {
    /// Create a new builder for a simple tool.
    pub fn builder(name: impl Into<String>) -> SimpleToolBuilder {
        SimpleToolBuilder::new(name)
    }
}

#[async_trait]
impl ToolHandler for SimpleTool {
    fn definition(&self) -> ToolDefinition {
        let mut def = ToolDefinition::new(&self.config.name, &self.config.description);
        
        // Build parameters schema
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        
        for param in &self.config.params {
            let mut prop = serde_json::json!({
                "type": param.param_type.as_schema_type(),
                "description": param.description
            });
            
            if let Some(default) = &param.default {
                prop["default"] = default.clone();
            }
            
            if let Some(enum_vals) = &param.enum_values {
                prop["enum"] = serde_json::json!(enum_vals);
            }
            
            properties.insert(param.name.clone(), prop);
            
            if param.required {
                required.push(param.name.clone());
            }
        }
        
        def.parameters = serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required
        });
        
        def
    }

    async fn execute(&self, arguments: Value) -> Result<ToolOutput> {
        (self.handler)(arguments)
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

/// Builder for creating simple tools.
pub struct SimpleToolBuilder {
    name: String,
    description: String,
    params: Vec<ParamDef>,
    enabled: bool,
}

impl SimpleToolBuilder {
    /// Create a new builder.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            params: Vec::new(),
            enabled: true,
        }
    }

    /// Set the tool description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Add a string parameter.
    pub fn string_param(mut self, name: &str, description: &str, required: bool) -> Self {
        self.params.push(ParamDef {
            name: name.to_string(),
            description: description.to_string(),
            param_type: ParamType::String,
            required,
            default: None,
            enum_values: None,
        });
        self
    }

    /// Add a string parameter with a default value.
    pub fn string_param_default(
        mut self,
        name: &str,
        description: &str,
        default: &str,
    ) -> Self {
        self.params.push(ParamDef {
            name: name.to_string(),
            description: description.to_string(),
            param_type: ParamType::String,
            required: false,
            default: Some(Value::String(default.to_string())),
            enum_values: None,
        });
        self
    }

    /// Add an enum parameter (string with predefined values).
    pub fn enum_param(
        mut self,
        name: &str,
        description: &str,
        values: &[&str],
        required: bool,
    ) -> Self {
        self.params.push(ParamDef {
            name: name.to_string(),
            description: description.to_string(),
            param_type: ParamType::String,
            required,
            default: None,
            enum_values: Some(values.iter().map(|s| s.to_string()).collect()),
        });
        self
    }

    /// Add an integer parameter.
    pub fn integer_param(mut self, name: &str, description: &str, required: bool) -> Self {
        self.params.push(ParamDef {
            name: name.to_string(),
            description: description.to_string(),
            param_type: ParamType::Integer,
            required,
            default: None,
            enum_values: None,
        });
        self
    }

    /// Add an integer parameter with a default value.
    pub fn integer_param_default(
        mut self,
        name: &str,
        description: &str,
        default: i64,
    ) -> Self {
        self.params.push(ParamDef {
            name: name.to_string(),
            description: description.to_string(),
            param_type: ParamType::Integer,
            required: false,
            default: Some(Value::Number(serde_json::Number::from(default))),
            enum_values: None,
        });
        self
    }

    /// Add a number (float) parameter.
    pub fn number_param(mut self, name: &str, description: &str, required: bool) -> Self {
        self.params.push(ParamDef {
            name: name.to_string(),
            description: description.to_string(),
            param_type: ParamType::Number,
            required,
            default: None,
            enum_values: None,
        });
        self
    }

    /// Add a boolean parameter.
    pub fn boolean_param(mut self, name: &str, description: &str, required: bool) -> Self {
        self.params.push(ParamDef {
            name: name.to_string(),
            description: description.to_string(),
            param_type: ParamType::Boolean,
            required,
            default: None,
            enum_values: None,
        });
        self
    }

    /// Add a boolean parameter with a default value.
    pub fn boolean_param_default(
        mut self,
        name: &str,
        description: &str,
        default: bool,
    ) -> Self {
        self.params.push(ParamDef {
            name: name.to_string(),
            description: description.to_string(),
            param_type: ParamType::Boolean,
            required: false,
            default: Some(Value::Bool(default)),
            enum_values: None,
        });
        self
    }

    /// Add an array parameter.
    pub fn array_param(mut self, name: &str, description: &str, required: bool) -> Self {
        self.params.push(ParamDef {
            name: name.to_string(),
            description: description.to_string(),
            param_type: ParamType::Array,
            required,
            default: None,
            enum_values: None,
        });
        self
    }

    /// Add an object parameter.
    pub fn object_param(mut self, name: &str, description: &str, required: bool) -> Self {
        self.params.push(ParamDef {
            name: name.to_string(),
            description: description.to_string(),
            param_type: ParamType::Object,
            required,
            default: None,
            enum_values: None,
        });
        self
    }

    /// Add a custom parameter definition.
    pub fn param(mut self, param: ParamDef) -> Self {
        self.params.push(param);
        self
    }

    /// Set whether the tool is enabled.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Build the tool with a synchronous handler.
    pub fn handler<F>(self, f: F) -> SimpleTool
    where
        F: Fn(Value) -> Result<ToolOutput> + Send + Sync + 'static,
    {
        SimpleTool {
            config: SimpleToolConfig {
                name: self.name,
                description: self.description,
                params: self.params,
                enabled: self.enabled,
            },
            handler: Arc::new(f),
        }
    }

    /// Build the tool with an async handler.
    pub fn async_handler<F, Fut>(self, f: F) -> AsyncTool
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ToolOutput>> + Send + 'static,
    {
        let f = Arc::new(f);
        AsyncTool {
            config: SimpleToolConfig {
                name: self.name,
                description: self.description,
                params: self.params,
                enabled: self.enabled,
            },
            handler: Arc::new(move |args| {
                let f = Arc::clone(&f);
                Box::pin(async move { f(args).await })
            }),
        }
    }
}

/// A tool with an asynchronous handler function.
pub struct AsyncTool {
    config: SimpleToolConfig,
    handler: AsyncHandler,
}

impl AsyncTool {
    /// Create a new builder for an async tool.
    pub fn builder(name: impl Into<String>) -> SimpleToolBuilder {
        SimpleToolBuilder::new(name)
    }
}

#[async_trait]
impl ToolHandler for AsyncTool {
    fn definition(&self) -> ToolDefinition {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        
        for param in &self.config.params {
            let mut prop = serde_json::json!({
                "type": param.param_type.as_schema_type(),
                "description": param.description
            });
            
            if let Some(default) = &param.default {
                prop["default"] = default.clone();
            }
            
            if let Some(enum_vals) = &param.enum_values {
                prop["enum"] = serde_json::json!(enum_vals);
            }
            
            properties.insert(param.name.clone(), prop);
            
            if param.required {
                required.push(param.name.clone());
            }
        }
        
        let mut def = ToolDefinition::new(&self.config.name, &self.config.description);
        def.parameters = serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required
        });
        def
    }

    async fn execute(&self, arguments: Value) -> Result<ToolOutput> {
        (self.handler)(arguments).await
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

/// Trait for extracting typed parameters from tool arguments.
pub trait ParamExtractor {
    /// Extract a required string parameter.
    fn get_string(&self, name: &str) -> Result<&str>;
    
    /// Extract an optional string parameter.
    fn get_string_opt(&self, name: &str) -> Option<&str>;
    
    /// Extract a required integer parameter.
    fn get_integer(&self, name: &str) -> Result<i64>;
    
    /// Extract an optional integer parameter.
    fn get_integer_opt(&self, name: &str) -> Option<i64>;
    
    /// Extract a required number parameter.
    fn get_number(&self, name: &str) -> Result<f64>;
    
    /// Extract an optional number parameter.
    fn get_number_opt(&self, name: &str) -> Option<f64>;
    
    /// Extract a required boolean parameter.
    fn get_boolean(&self, name: &str) -> Result<bool>;
    
    /// Extract an optional boolean parameter.
    fn get_boolean_opt(&self, name: &str) -> Option<bool>;
    
    /// Extract a required array parameter.
    fn get_array(&self, name: &str) -> Result<&Vec<Value>>;
    
    /// Extract an optional array parameter.
    fn get_array_opt(&self, name: &str) -> Option<&Vec<Value>>;
}

impl ParamExtractor for Value {
    fn get_string(&self, name: &str) -> Result<&str> {
        self.get(name)
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::MissingParameter(name.to_string()))
    }
    
    fn get_string_opt(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(|v| v.as_str())
    }
    
    fn get_integer(&self, name: &str) -> Result<i64> {
        self.get(name)
            .and_then(|v| v.as_i64())
            .ok_or_else(|| Error::MissingParameter(name.to_string()))
    }
    
    fn get_integer_opt(&self, name: &str) -> Option<i64> {
        self.get(name).and_then(|v| v.as_i64())
    }
    
    fn get_number(&self, name: &str) -> Result<f64> {
        self.get(name)
            .and_then(|v| v.as_f64())
            .ok_or_else(|| Error::MissingParameter(name.to_string()))
    }
    
    fn get_number_opt(&self, name: &str) -> Option<f64> {
        self.get(name).and_then(|v| v.as_f64())
    }
    
    fn get_boolean(&self, name: &str) -> Result<bool> {
        self.get(name)
            .and_then(|v| v.as_bool())
            .ok_or_else(|| Error::MissingParameter(name.to_string()))
    }
    
    fn get_boolean_opt(&self, name: &str) -> Option<bool> {
        self.get(name).and_then(|v| v.as_bool())
    }
    
    fn get_array(&self, name: &str) -> Result<&Vec<Value>> {
        self.get(name)
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::MissingParameter(name.to_string()))
    }
    
    fn get_array_opt(&self, name: &str) -> Option<&Vec<Value>> {
        self.get(name).and_then(|v| v.as_array())
    }
}

/// Validation helpers for tool parameters.
pub mod validation {
    use super::*;

    /// Validate that a string is not empty.
    pub fn non_empty_string(value: &str, param_name: &str) -> Result<()> {
        if value.trim().is_empty() {
            return Err(Error::InvalidParameter {
                name: param_name.to_string(),
                reason: "must not be empty".to_string(),
            });
        }
        Ok(())
    }

    /// Validate that an integer is within a range.
    pub fn in_range(value: i64, min: i64, max: i64, param_name: &str) -> Result<()> {
        if value < min || value > max {
            return Err(Error::InvalidParameter {
                name: param_name.to_string(),
                reason: format!("must be between {} and {}", min, max),
            });
        }
        Ok(())
    }

    /// Validate that a string matches a regex pattern.
    pub fn matches_pattern(value: &str, pattern: &str, param_name: &str) -> Result<()> {
        let re = regex::Regex::new(pattern).map_err(|e| Error::InvalidParameter {
            name: param_name.to_string(),
            reason: format!("invalid pattern: {}", e),
        })?;
        
        if !re.is_match(value) {
            return Err(Error::InvalidParameter {
                name: param_name.to_string(),
                reason: format!("must match pattern: {}", pattern),
            });
        }
        Ok(())
    }

    /// Validate that a value is one of the allowed options.
    pub fn one_of<T: PartialEq + Debug>(value: &T, options: &[T], param_name: &str) -> Result<()> {
        if !options.contains(value) {
            return Err(Error::InvalidParameter {
                name: param_name.to_string(),
                reason: format!("must be one of: {:?}", options),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_tool_builder() {
        let tool = SimpleTool::builder("test_tool")
            .description("A test tool")
            .string_param("name", "User name", true)
            .integer_param("count", "Number of items", false)
            .boolean_param_default("verbose", "Enable verbose output", false)
            .handler(|args| {
                let name = args.get_string("name")?;
                Ok(ToolOutput::success(format!("Hello, {}!", name)))
            });

        let def = tool.definition();
        assert_eq!(def.name, "test_tool");
        assert_eq!(def.description, "A test tool");
        
        let required = def.parameters["required"].as_array().unwrap();
        assert!(required.contains(&Value::String("name".to_string())));
        assert!(!required.contains(&Value::String("count".to_string())));
    }

    #[tokio::test]
    async fn test_simple_tool_execute() {
        let tool = SimpleTool::builder("greet")
            .description("Greets a user")
            .string_param("name", "Name to greet", true)
            .handler(|args| {
                let name = args.get_string_opt("name").unwrap_or("World");
                Ok(ToolOutput::success(format!("Hello, {}!", name)))
            });

        let args = serde_json::json!({ "name": "Alice" });
        let result = tool.execute(args).await.unwrap();
        
        assert!(result.success);
        assert_eq!(result.output, "Hello, Alice!");
    }

    #[tokio::test]
    async fn test_async_tool() {
        let tool = AsyncTool::builder("async_greet")
            .description("Async greeting")
            .string_param("name", "Name", true)
            .async_handler(|args| async move {
                let name = args.get_string_opt("name").unwrap_or("World");
                // Simulate async operation
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                Ok(ToolOutput::success(format!("Async hello, {}!", name)))
            });

        let args = serde_json::json!({ "name": "Bob" });
        let result = tool.execute(args).await.unwrap();
        
        assert!(result.success);
        assert!(result.output.contains("Bob"));
    }

    #[test]
    fn test_enum_param() {
        let tool = SimpleTool::builder("format")
            .description("Format text")
            .enum_param("style", "Output style", &["json", "yaml", "toml"], true)
            .handler(|_| Ok(ToolOutput::success("formatted")));

        let def = tool.definition();
        let enum_vals = def.parameters["properties"]["style"]["enum"]
            .as_array()
            .unwrap();
        
        assert_eq!(enum_vals.len(), 3);
    }

    #[test]
    fn test_validation_helpers() {
        // non_empty_string
        assert!(validation::non_empty_string("hello", "test").is_ok());
        assert!(validation::non_empty_string("", "test").is_err());
        assert!(validation::non_empty_string("   ", "test").is_err());

        // in_range
        assert!(validation::in_range(5, 1, 10, "count").is_ok());
        assert!(validation::in_range(0, 1, 10, "count").is_err());
        assert!(validation::in_range(11, 1, 10, "count").is_err());

        // one_of
        assert!(validation::one_of(&"a", &["a", "b", "c"], "choice").is_ok());
        assert!(validation::one_of(&"x", &["a", "b", "c"], "choice").is_err());
    }
}