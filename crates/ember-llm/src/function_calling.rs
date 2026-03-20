//! Function Calling Support for LLM Providers
//!
//! This module provides a unified interface for function/tool calling
//! across different LLM providers with standardized JSON schemas.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// A function/tool definition for LLM function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// The name of the function
    pub name: String,
    /// A description of what the function does
    pub description: String,
    /// JSON Schema for the function parameters
    pub parameters: JsonSchema,
    /// Whether the function is required or optional
    #[serde(default)]
    pub required: bool,
}

/// JSON Schema definition for function parameters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JsonSchema {
    /// The type of the schema (usually "object")
    #[serde(rename = "type")]
    pub schema_type: String,
    /// Properties of the object
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub properties: HashMap<String, PropertySchema>,
    /// Required properties
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
    /// Additional properties allowed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<bool>,
}

/// Schema for a single property
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertySchema {
    /// The type of the property
    #[serde(rename = "type")]
    pub property_type: PropertyType,
    /// Description of the property
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Enum values if this is an enum type
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    /// Default value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// Items schema for arrays
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<PropertySchema>>,
    /// Minimum value for numbers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    /// Maximum value for numbers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    /// Pattern for strings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

/// JSON Schema property types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PropertyType {
    /// String type
    String,
    /// Number type (floating point)
    Number,
    /// Integer type (whole numbers)
    Integer,
    /// Boolean type (true/false)
    Boolean,
    /// Array type (list of items)
    Array,
    /// Object type (nested structure)
    Object,
    /// Null type (no value)
    Null,
}

/// A function call made by the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// The ID of the function call (for tracking)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The name of the function to call
    pub name: String,
    /// The arguments to pass to the function (as JSON string)
    pub arguments: String,
}

impl FunctionCall {
    /// Parse the arguments as a specific type
    pub fn parse_arguments<T: for<'de> Deserialize<'de>>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.arguments)
    }

    /// Parse the arguments as a generic JSON Value
    pub fn arguments_value(&self) -> Result<Value, serde_json::Error> {
        serde_json::from_str(&self.arguments)
    }
}

/// Result of a function call execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResult {
    /// The ID of the function call this is responding to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    /// The name of the function that was called
    pub name: String,
    /// The result content
    pub content: String,
    /// Whether the function call was successful
    pub success: bool,
    /// Error message if not successful
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Tool choice options for the model
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    /// Let the model decide whether to use tools
    #[default]
    Auto,
    /// Don't use any tools
    None,
    /// The model must use a tool
    Required,
    /// Force use of a specific tool
    Function {
        /// Name of the function to use
        name: String,
    },
}

/// Builder for creating function definitions easily
pub struct FunctionBuilder {
    name: String,
    description: String,
    properties: HashMap<String, PropertySchema>,
    required: Vec<String>,
}

impl FunctionBuilder {
    /// Create a new function builder
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            properties: HashMap::new(),
            required: Vec::new(),
        }
    }

    /// Add a string parameter
    pub fn string_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();
        self.properties.insert(
            name.clone(),
            PropertySchema {
                property_type: PropertyType::String,
                description: Some(description.into()),
                enum_values: None,
                default: None,
                items: None,
                minimum: None,
                maximum: None,
                pattern: None,
            },
        );
        if required {
            self.required.push(name);
        }
        self
    }

    /// Add a number parameter
    pub fn number_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();
        self.properties.insert(
            name.clone(),
            PropertySchema {
                property_type: PropertyType::Number,
                description: Some(description.into()),
                enum_values: None,
                default: None,
                items: None,
                minimum: None,
                maximum: None,
                pattern: None,
            },
        );
        if required {
            self.required.push(name);
        }
        self
    }

    /// Add an integer parameter
    pub fn integer_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();
        self.properties.insert(
            name.clone(),
            PropertySchema {
                property_type: PropertyType::Integer,
                description: Some(description.into()),
                enum_values: None,
                default: None,
                items: None,
                minimum: None,
                maximum: None,
                pattern: None,
            },
        );
        if required {
            self.required.push(name);
        }
        self
    }

    /// Add a boolean parameter
    pub fn boolean_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();
        self.properties.insert(
            name.clone(),
            PropertySchema {
                property_type: PropertyType::Boolean,
                description: Some(description.into()),
                enum_values: None,
                default: None,
                items: None,
                minimum: None,
                maximum: None,
                pattern: None,
            },
        );
        if required {
            self.required.push(name);
        }
        self
    }

    /// Add an enum parameter
    pub fn enum_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        values: Vec<String>,
        required: bool,
    ) -> Self {
        let name = name.into();
        self.properties.insert(
            name.clone(),
            PropertySchema {
                property_type: PropertyType::String,
                description: Some(description.into()),
                enum_values: Some(values),
                default: None,
                items: None,
                minimum: None,
                maximum: None,
                pattern: None,
            },
        );
        if required {
            self.required.push(name);
        }
        self
    }

    /// Add an array parameter
    pub fn array_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        item_type: PropertyType,
        required: bool,
    ) -> Self {
        let name = name.into();
        self.properties.insert(
            name.clone(),
            PropertySchema {
                property_type: PropertyType::Array,
                description: Some(description.into()),
                enum_values: None,
                default: None,
                items: Some(Box::new(PropertySchema {
                    property_type: item_type,
                    description: None,
                    enum_values: None,
                    default: None,
                    items: None,
                    minimum: None,
                    maximum: None,
                    pattern: None,
                })),
                minimum: None,
                maximum: None,
                pattern: None,
            },
        );
        if required {
            self.required.push(name);
        }
        self
    }

    /// Build the function definition
    pub fn build(self) -> FunctionDefinition {
        FunctionDefinition {
            name: self.name,
            description: self.description,
            parameters: JsonSchema {
                schema_type: "object".to_string(),
                properties: self.properties,
                required: self.required,
                additional_properties: Some(false),
            },
            required: false,
        }
    }
}

/// Trait for providers that support function calling
pub trait FunctionCallingCapable {
    /// Check if the provider supports function calling for a specific model
    fn supports_function_calling(&self, model: &str) -> bool;
    
    /// Get the maximum number of functions/tools supported
    fn max_functions(&self, model: &str) -> Option<usize>;
    
    /// Check if parallel function calls are supported
    fn supports_parallel_calls(&self, model: &str) -> bool;
}

/// Models that support function calling
pub struct FunctionCallingModels;

impl FunctionCallingModels {
    /// Check if a model supports function calling
    pub fn supports_function_calling(provider: &str, model: &str) -> bool {
        match provider.to_lowercase().as_str() {
            "openai" => {
                model.starts_with("gpt-4") || model.starts_with("gpt-3.5-turbo")
            }
            "anthropic" => {
                model.contains("claude-3") || model.contains("claude-2")
            }
            "google" | "gemini" => {
                model.contains("gemini")
            }
            "groq" => {
                model.contains("llama") || model.contains("mixtral")
            }
            "mistral" => {
                model.contains("mistral") && !model.contains("tiny")
            }
            "ollama" => {
                // Most Ollama models support function calling
                true
            }
            "openrouter" => {
                // Depends on the underlying model
                true
            }
            _ => false,
        }
    }

    /// Get models that support parallel function calls
    pub fn supports_parallel_calls(provider: &str, model: &str) -> bool {
        match provider.to_lowercase().as_str() {
            "openai" => model.starts_with("gpt-4"),
            "anthropic" => model.contains("claude-3"),
            _ => false,
        }
    }
}

/// Common built-in function definitions
pub mod builtin {
    use super::*;

    /// Get current weather function definition
    pub fn get_weather() -> FunctionDefinition {
        FunctionBuilder::new(
            "get_weather",
            "Get the current weather in a given location",
        )
        .string_param("location", "The city and state, e.g. San Francisco, CA", true)
        .enum_param(
            "unit",
            "The temperature unit",
            vec!["celsius".to_string(), "fahrenheit".to_string()],
            false,
        )
        .build()
    }

    /// Search the web function definition
    pub fn web_search() -> FunctionDefinition {
        FunctionBuilder::new(
            "web_search",
            "Search the web for information",
        )
        .string_param("query", "The search query", true)
        .integer_param("num_results", "Number of results to return", false)
        .build()
    }

    /// Execute shell command function definition
    pub fn execute_command() -> FunctionDefinition {
        FunctionBuilder::new(
            "execute_command",
            "Execute a shell command on the system",
        )
        .string_param("command", "The shell command to execute", true)
        .string_param("working_directory", "The directory to run the command in", false)
        .integer_param("timeout", "Timeout in seconds", false)
        .build()
    }

    /// Read file function definition
    pub fn read_file() -> FunctionDefinition {
        FunctionBuilder::new(
            "read_file",
            "Read the contents of a file",
        )
        .string_param("path", "The path to the file to read", true)
        .string_param("encoding", "The file encoding (default: utf-8)", false)
        .build()
    }

    /// Write file function definition
    pub fn write_file() -> FunctionDefinition {
        FunctionBuilder::new(
            "write_file",
            "Write content to a file",
        )
        .string_param("path", "The path to the file to write", true)
        .string_param("content", "The content to write to the file", true)
        .boolean_param("create_dirs", "Create parent directories if they don't exist", false)
        .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_builder() {
        let func = FunctionBuilder::new("test_function", "A test function")
            .string_param("name", "The name", true)
            .integer_param("age", "The age", false)
            .build();

        assert_eq!(func.name, "test_function");
        assert_eq!(func.description, "A test function");
        assert_eq!(func.parameters.required, vec!["name"]);
        assert!(func.parameters.properties.contains_key("name"));
        assert!(func.parameters.properties.contains_key("age"));
    }

    #[test]
    fn test_function_call_parsing() {
        let call = FunctionCall {
            id: Some("call_123".to_string()),
            name: "get_weather".to_string(),
            arguments: r#"{"location": "San Francisco, CA"}"#.to_string(),
        };

        let value = call.arguments_value().unwrap();
        assert_eq!(value["location"], "San Francisco, CA");
    }

    #[test]
    fn test_builtin_functions() {
        let weather = builtin::get_weather();
        assert_eq!(weather.name, "get_weather");
        assert!(weather.parameters.properties.contains_key("location"));

        let search = builtin::web_search();
        assert_eq!(search.name, "web_search");
    }

    #[test]
    fn test_function_calling_support() {
        assert!(FunctionCallingModels::supports_function_calling("openai", "gpt-4"));
        assert!(FunctionCallingModels::supports_function_calling("anthropic", "claude-3-opus"));
        assert!(FunctionCallingModels::supports_function_calling("gemini", "gemini-1.5-pro"));
    }
}