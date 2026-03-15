//! Tool registry for managing available tools.

use crate::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

// Re-export ember_llm types for interoperability
pub use ember_llm::ToolCall as LLMToolCall;
pub use ember_llm::ToolDefinition as LLMToolDefinition;
pub use ember_llm::ToolResult as LLMToolResult;

/// Definition of a tool that can be used by an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name (must be unique)
    pub name: String,

    /// Human-readable description
    pub description: String,

    /// JSON Schema for the tool's parameters
    pub parameters: Value,
}

impl ToolDefinition {
    /// Create a new tool definition.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    /// Convert to `ember_llm` `ToolDefinition` for use with the agent.
    #[must_use]
    pub fn to_llm_definition(&self) -> LLMToolDefinition {
        LLMToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters: self.parameters.clone(),
        }
    }

    /// Create from `ember_llm` `ToolDefinition`.
    #[must_use]
    pub fn from_llm_definition(def: &LLMToolDefinition) -> Self {
        Self {
            name: def.name.clone(),
            description: def.description.clone(),
            parameters: def.parameters.clone(),
        }
    }

    /// Set the parameters schema.
    pub fn with_parameters(mut self, parameters: Value) -> Self {
        self.parameters = parameters;
        self
    }

    /// Add a string parameter.
    pub fn add_string_param(mut self, name: &str, description: &str, required: bool) -> Self {
        if let Some(props) = self.parameters.get_mut("properties") {
            props[name] = serde_json::json!({
                "type": "string",
                "description": description
            });
        }
        if required {
            if let Some(req) = self.parameters.get_mut("required") {
                if let Some(arr) = req.as_array_mut() {
                    arr.push(Value::String(name.to_string()));
                }
            }
        }
        self
    }

    /// Add an integer parameter.
    pub fn add_integer_param(mut self, name: &str, description: &str, required: bool) -> Self {
        if let Some(props) = self.parameters.get_mut("properties") {
            props[name] = serde_json::json!({
                "type": "integer",
                "description": description
            });
        }
        if required {
            if let Some(req) = self.parameters.get_mut("required") {
                if let Some(arr) = req.as_array_mut() {
                    arr.push(Value::String(name.to_string()));
                }
            }
        }
        self
    }

    /// Add a boolean parameter.
    pub fn add_boolean_param(mut self, name: &str, description: &str, required: bool) -> Self {
        if let Some(props) = self.parameters.get_mut("properties") {
            props[name] = serde_json::json!({
                "type": "boolean",
                "description": description
            });
        }
        if required {
            if let Some(req) = self.parameters.get_mut("required") {
                if let Some(arr) = req.as_array_mut() {
                    arr.push(Value::String(name.to_string()));
                }
            }
        }
        self
    }
}

/// Result of executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Whether the execution was successful
    pub success: bool,

    /// Output content
    pub output: String,

    /// Optional structured data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ToolOutput {
    /// Create a successful output.
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            data: None,
        }
    }

    /// Create a successful output with data.
    pub fn success_with_data(output: impl Into<String>, data: Value) -> Self {
        Self {
            success: true,
            output: output.into(),
            data: Some(data),
        }
    }

    /// Create a failed output.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            output: message.into(),
            data: None,
        }
    }

    /// Convert to `ember_llm` `ToolResult` for use with the agent.
    pub fn to_llm_result(&self, tool_call_id: impl Into<String>) -> LLMToolResult {
        if self.success {
            LLMToolResult::success(tool_call_id, &self.output)
        } else {
            LLMToolResult::failure(tool_call_id, &self.output)
        }
    }
}

/// Trait for tool handlers.
#[async_trait]
pub trait ToolHandler: Send + Sync {
    /// Get the tool definition.
    fn definition(&self) -> ToolDefinition;

    /// Execute the tool with the given arguments.
    async fn execute(&self, arguments: Value) -> Result<ToolOutput>;

    /// Check if this tool is enabled.
    fn is_enabled(&self) -> bool {
        true
    }
}

/// Registry for managing tools.
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn ToolHandler>>,
}

impl ToolRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool handler.
    pub fn register<T: ToolHandler + 'static>(&mut self, handler: T) {
        let def = handler.definition();
        self.tools.insert(def.name.clone(), Arc::new(handler));
    }

    /// Register a tool handler from an Arc.
    pub fn register_arc(&mut self, handler: Arc<dyn ToolHandler>) {
        let def = handler.definition();
        self.tools.insert(def.name.clone(), handler);
    }

    /// Get a tool handler by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolHandler>> {
        self.tools.get(name).cloned()
    }

    /// Check if a tool exists.
    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Remove a tool.
    pub fn remove(&mut self, name: &str) -> Option<Arc<dyn ToolHandler>> {
        self.tools.remove(name)
    }

    /// Get all tool definitions.
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .filter(|h| h.is_enabled())
            .map(|h| h.definition())
            .collect()
    }

    /// Get all tool definitions as `ember_llm` types for use with the agent.
    pub fn llm_tool_definitions(&self) -> Vec<LLMToolDefinition> {
        self.tools
            .values()
            .filter(|h| h.is_enabled())
            .map(|h| h.definition().to_llm_definition())
            .collect()
    }

    /// Execute a tool from an LLM tool call and return an LLM tool result.
    ///
    /// This is a convenience method for integrating with the agent's tool calling.
    pub async fn execute_tool_call(&self, call: &LLMToolCall) -> Result<LLMToolResult> {
        let output = self.execute(&call.name, call.arguments.clone()).await?;
        Ok(output.to_llm_result(&call.id))
    }

    /// Get all tool names.
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Get the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Execute a tool by name.
    pub async fn execute(&self, name: &str, arguments: Value) -> Result<ToolOutput> {
        let handler = self
            .tools
            .get(name)
            .ok_or_else(|| Error::ToolNotFound(name.to_string()))?;

        if !handler.is_enabled() {
            return Err(Error::ToolDisabled(name.to_string()));
        }

        handler.execute(arguments).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockTool {
        name: String,
    }

    #[async_trait]
    impl ToolHandler for MockTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition::new(&self.name, "A mock tool for testing").add_string_param(
                "input",
                "Input string",
                true,
            )
        }

        async fn execute(&self, arguments: Value) -> Result<ToolOutput> {
            let input = arguments
                .get("input")
                .and_then(|v| v.as_str())
                .unwrap_or("default");
            Ok(ToolOutput::success(format!("Processed: {}", input)))
        }
    }

    #[tokio::test]
    async fn test_registry_basic() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool {
            name: "mock".to_string(),
        });

        assert!(registry.has("mock"));
        assert!(!registry.has("nonexistent"));
        assert_eq!(registry.len(), 1);
    }

    #[tokio::test]
    async fn test_registry_execute() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool {
            name: "mock".to_string(),
        });

        let args = serde_json::json!({ "input": "test" });
        let result = registry.execute("mock", args).await.unwrap();

        assert!(result.success);
        assert!(result.output.contains("test"));
    }

    #[tokio::test]
    async fn test_registry_not_found() {
        let registry = ToolRegistry::new();
        let result = registry.execute("nonexistent", Value::Null).await;

        assert!(matches!(result, Err(Error::ToolNotFound(_))));
    }

    #[test]
    fn test_tool_definition_builder() {
        let def = ToolDefinition::new("test", "A test tool")
            .add_string_param("name", "User name", true)
            .add_integer_param("age", "User age", false)
            .add_boolean_param("active", "Is active", false);

        assert_eq!(def.name, "test");
        assert!(def.parameters["properties"]["name"].is_object());
        assert!(def.parameters["properties"]["age"].is_object());
        assert!(def.parameters["properties"]["active"].is_object());

        let required = def.parameters["required"].as_array().unwrap();
        assert!(required.contains(&Value::String("name".to_string())));
    }
}
