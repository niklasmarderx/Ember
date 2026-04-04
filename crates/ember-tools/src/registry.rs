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

    /// Whether this tool supports streaming output (line-by-line).
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Execute the tool with streaming output.
    ///
    /// Sends individual output lines through `tx` as they become available,
    /// and returns the final aggregated `ToolOutput` when done.
    /// Default implementation falls back to `execute()` with no streaming.
    async fn execute_streaming(
        &self,
        arguments: Value,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<ToolOutput> {
        let _ = tx; // unused in default impl
        self.execute(arguments).await
    }

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

    /// Execute multiple tool calls in parallel and return results in order.
    ///
    /// Each result is `(call_id, Result<ToolOutput>)`. Safe because handlers
    /// are `Arc<dyn ToolHandler + Send + Sync>`.
    pub async fn execute_parallel(
        &self,
        calls: &[LLMToolCall],
    ) -> Vec<(String, Result<ToolOutput>)> {
        let futs: Vec<_> = calls
            .iter()
            .map(|call| {
                let name = call.name.clone();
                let args = call.arguments.clone();
                let id = call.id.clone();
                async move {
                    let result = self.execute(&name, args).await;
                    (id, result)
                }
            })
            .collect();
        futures::future::join_all(futs).await
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

    /// Check if a tool supports streaming.
    pub fn supports_streaming(&self, name: &str) -> bool {
        self.tools
            .get(name)
            .map(|h| h.supports_streaming())
            .unwrap_or(false)
    }

    /// Execute a tool with streaming output.
    ///
    /// Lines are sent through `tx` as they become available.
    /// Returns the final aggregated `ToolOutput`.
    pub async fn execute_streaming(
        &self,
        name: &str,
        arguments: Value,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<ToolOutput> {
        let handler = self
            .tools
            .get(name)
            .ok_or_else(|| Error::ToolNotFound(name.to_string()))?;

        if !handler.is_enabled() {
            return Err(Error::ToolDisabled(name.to_string()));
        }

        handler.execute_streaming(arguments, tx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockTool {
        name: String,
        enabled: bool,
    }

    impl MockTool {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                enabled: true,
            }
        }

        fn disabled(name: &str) -> Self {
            Self {
                name: name.to_string(),
                enabled: false,
            }
        }
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

        fn is_enabled(&self) -> bool {
            self.enabled
        }
    }

    // Tool that returns an error
    struct FailingTool;

    #[async_trait]
    impl ToolHandler for FailingTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition::new("failing_tool", "A tool that fails")
        }

        async fn execute(&self, _arguments: Value) -> Result<ToolOutput> {
            Ok(ToolOutput::error("Execution failed"))
        }
    }

    // ==================== Basic Registry Tests ====================

    #[tokio::test]
    async fn test_registry_new_is_empty() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[tokio::test]
    async fn test_registry_register_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("test_tool"));

        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
        assert!(registry.has("test_tool"));
    }

    #[tokio::test]
    async fn test_registry_register_multiple_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("tool1"));
        registry.register(MockTool::new("tool2"));
        registry.register(MockTool::new("tool3"));

        assert_eq!(registry.len(), 3);
        assert!(registry.has("tool1"));
        assert!(registry.has("tool2"));
        assert!(registry.has("tool3"));
    }

    #[tokio::test]
    async fn test_registry_register_duplicate_overwrites() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("duplicate"));
        registry.register(MockTool::new("duplicate"));

        // Duplicate registration should overwrite (not error)
        assert_eq!(registry.len(), 1);
        assert!(registry.has("duplicate"));
    }

    // ==================== Lookup Tests ====================

    #[tokio::test]
    async fn test_registry_get_existing_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("test_tool"));

        let tool = registry.get("test_tool");
        assert!(tool.is_some());
    }

    #[tokio::test]
    async fn test_registry_get_nonexistent_tool() {
        let registry = ToolRegistry::new();
        let tool = registry.get("nonexistent");
        assert!(tool.is_none());
    }

    #[tokio::test]
    async fn test_registry_has_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("exists"));

        assert!(registry.has("exists"));
        assert!(!registry.has("does_not_exist"));
    }

    // ==================== Remove Tests ====================

    #[tokio::test]
    async fn test_registry_remove_existing_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("to_remove"));

        let removed = registry.remove("to_remove");
        assert!(removed.is_some());
        assert!(!registry.has("to_remove"));
        assert_eq!(registry.len(), 0);
    }

    #[tokio::test]
    async fn test_registry_remove_nonexistent_tool() {
        let mut registry = ToolRegistry::new();
        let removed = registry.remove("nonexistent");
        assert!(removed.is_none());
    }

    // ==================== Listing Tests ====================

    #[tokio::test]
    async fn test_registry_tool_names() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("alpha"));
        registry.register(MockTool::new("beta"));
        registry.register(MockTool::new("gamma"));

        let names = registry.tool_names();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"alpha".to_string()));
        assert!(names.contains(&"beta".to_string()));
        assert!(names.contains(&"gamma".to_string()));
    }

    #[tokio::test]
    async fn test_registry_tool_definitions() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("tool1"));
        registry.register(MockTool::new("tool2"));

        let definitions = registry.tool_definitions();
        assert_eq!(definitions.len(), 2);

        let names: Vec<_> = definitions.iter().map(|d| d.name.clone()).collect();
        assert!(names.contains(&"tool1".to_string()));
        assert!(names.contains(&"tool2".to_string()));
    }

    #[tokio::test]
    async fn test_registry_tool_definitions_excludes_disabled() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("enabled_tool"));
        registry.register(MockTool::disabled("disabled_tool"));

        let definitions = registry.tool_definitions();
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].name, "enabled_tool");
    }

    #[tokio::test]
    async fn test_registry_llm_tool_definitions() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("llm_tool"));

        let llm_defs = registry.llm_tool_definitions();
        assert_eq!(llm_defs.len(), 1);
        assert_eq!(llm_defs[0].name, "llm_tool");
    }

    // ==================== Execution Tests ====================

    #[tokio::test]
    async fn test_registry_execute_with_valid_input() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("mock"));

        let args = serde_json::json!({ "input": "hello world" });
        let result = registry.execute("mock", args).await.unwrap();

        assert!(result.success);
        assert!(result.output.contains("hello world"));
    }

    #[tokio::test]
    async fn test_registry_execute_with_default_input() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("mock"));

        // Missing required input - uses default
        let args = serde_json::json!({});
        let result = registry.execute("mock", args).await.unwrap();

        assert!(result.success);
        assert!(result.output.contains("default"));
    }

    #[tokio::test]
    async fn test_registry_execute_nonexistent_tool() {
        let registry = ToolRegistry::new();
        let result = registry.execute("nonexistent", Value::Null).await;

        assert!(matches!(result, Err(Error::ToolNotFound(_))));
    }

    #[tokio::test]
    async fn test_registry_execute_disabled_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::disabled("disabled"));

        let result = registry.execute("disabled", Value::Null).await;
        assert!(matches!(result, Err(Error::ToolDisabled(_))));
    }

    #[tokio::test]
    async fn test_registry_execute_failing_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(FailingTool);

        let result = registry.execute("failing_tool", Value::Null).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.output, "Execution failed");
    }

    // ==================== Tool Call Integration Tests ====================

    #[tokio::test]
    async fn test_registry_execute_tool_call() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("mock"));

        let tool_call = LLMToolCall::new(
            "call_123",
            "mock",
            serde_json::json!({ "input": "test input" }),
        );

        let result = registry.execute_tool_call(&tool_call).await.unwrap();
        assert!(result.output.contains("test input"));
    }

    // ==================== ToolDefinition Builder Tests ====================

    #[test]
    fn test_tool_definition_new() {
        let def = ToolDefinition::new("my_tool", "My tool description");

        assert_eq!(def.name, "my_tool");
        assert_eq!(def.description, "My tool description");
        assert!(def.parameters["properties"].is_object());
    }

    #[test]
    fn test_tool_definition_add_string_param_required() {
        let def =
            ToolDefinition::new("test", "Test tool").add_string_param("name", "User name", true);

        assert!(def.parameters["properties"]["name"].is_object());
        assert_eq!(def.parameters["properties"]["name"]["type"], "string");
        assert_eq!(
            def.parameters["properties"]["name"]["description"],
            "User name"
        );

        let required = def.parameters["required"].as_array().unwrap();
        assert!(required.contains(&Value::String("name".to_string())));
    }

    #[test]
    fn test_tool_definition_add_string_param_optional() {
        let def =
            ToolDefinition::new("test", "Test tool").add_string_param("name", "User name", false);

        assert!(def.parameters["properties"]["name"].is_object());

        let required = def.parameters["required"].as_array().unwrap();
        assert!(!required.contains(&Value::String("name".to_string())));
    }

    #[test]
    fn test_tool_definition_add_integer_param() {
        let def =
            ToolDefinition::new("test", "Test tool").add_integer_param("count", "Count", true);

        assert_eq!(def.parameters["properties"]["count"]["type"], "integer");
    }

    #[test]
    fn test_tool_definition_add_boolean_param() {
        let def =
            ToolDefinition::new("test", "Test tool").add_boolean_param("active", "Is active", true);

        assert_eq!(def.parameters["properties"]["active"]["type"], "boolean");
    }

    #[test]
    fn test_tool_definition_add_multiple_params() {
        let def = ToolDefinition::new("test", "A test tool")
            .add_string_param("name", "User name", true)
            .add_integer_param("age", "User age", false)
            .add_boolean_param("active", "Is active", false);

        assert_eq!(def.name, "test");
        assert!(def.parameters["properties"]["name"].is_object());
        assert!(def.parameters["properties"]["age"].is_object());
        assert!(def.parameters["properties"]["active"].is_object());

        let required = def.parameters["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert!(required.contains(&Value::String("name".to_string())));
    }

    #[test]
    fn test_tool_definition_with_parameters() {
        let custom_params = serde_json::json!({
            "type": "object",
            "properties": {
                "custom": { "type": "string" }
            },
            "required": ["custom"]
        });

        let def = ToolDefinition::new("test", "Test").with_parameters(custom_params.clone());

        assert_eq!(def.parameters, custom_params);
    }

    #[test]
    fn test_tool_definition_to_llm_definition() {
        let def = ToolDefinition::new("my_tool", "My description")
            .add_string_param("input", "Input", true);

        let llm_def = def.to_llm_definition();

        assert_eq!(llm_def.name, "my_tool");
        assert_eq!(llm_def.description, "My description");
        assert_eq!(llm_def.parameters, def.parameters);
    }

    #[test]
    fn test_tool_definition_from_llm_definition() {
        let llm_def = LLMToolDefinition {
            name: "llm_tool".to_string(),
            description: "LLM tool description".to_string(),
            parameters: serde_json::json!({ "type": "object" }),
        };

        let def = ToolDefinition::from_llm_definition(&llm_def);

        assert_eq!(def.name, "llm_tool");
        assert_eq!(def.description, "LLM tool description");
    }

    // ==================== ToolOutput Tests ====================

    #[test]
    fn test_tool_output_success() {
        let output = ToolOutput::success("Operation completed");

        assert!(output.success);
        assert_eq!(output.output, "Operation completed");
        assert!(output.data.is_none());
    }

    #[test]
    fn test_tool_output_success_with_data() {
        let data = serde_json::json!({ "result": 42 });
        let output = ToolOutput::success_with_data("Done", data.clone());

        assert!(output.success);
        assert_eq!(output.output, "Done");
        assert_eq!(output.data, Some(data));
    }

    #[test]
    fn test_tool_output_error() {
        let output = ToolOutput::error("Something went wrong");

        assert!(!output.success);
        assert_eq!(output.output, "Something went wrong");
        assert!(output.data.is_none());
    }

    #[test]
    fn test_tool_output_to_llm_result_success() {
        let output = ToolOutput::success("Result content");
        let llm_result = output.to_llm_result("call_123");

        assert!(llm_result.output.contains("Result content"));
    }

    #[test]
    fn test_tool_output_to_llm_result_error() {
        let output = ToolOutput::error("Error message");
        let llm_result = output.to_llm_result("call_456");

        assert!(llm_result.output.contains("Error message"));
    }

    // ==================== Concurrent Access Tests ====================

    #[tokio::test]
    async fn test_registry_concurrent_reads() {
        use std::sync::Arc;

        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("concurrent_tool"));

        let registry = Arc::new(registry);
        let mut handles = vec![];

        // Spawn multiple concurrent read tasks
        for _ in 0..10 {
            let reg = Arc::clone(&registry);
            handles.push(tokio::spawn(async move {
                assert!(reg.has("concurrent_tool"));
                assert_eq!(reg.len(), 1);
                reg.get("concurrent_tool").is_some()
            }));
        }

        // All reads should succeed
        for handle in handles {
            assert!(handle.await.unwrap());
        }
    }

    #[tokio::test]
    async fn test_registry_concurrent_executions() {
        use std::sync::Arc;

        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("concurrent_exec"));

        let registry = Arc::new(registry);
        let mut handles = vec![];

        // Spawn multiple concurrent execution tasks
        for i in 0..10 {
            let reg = Arc::clone(&registry);
            handles.push(tokio::spawn(async move {
                let args = serde_json::json!({ "input": format!("input_{}", i) });
                reg.execute("concurrent_exec", args).await
            }));
        }

        // All executions should succeed
        for handle in handles {
            let result = handle.await.unwrap().unwrap();
            assert!(result.success);
        }
    }

    // ==================== Edge Cases ====================

    #[tokio::test]
    async fn test_registry_empty_tool_names() {
        let registry = ToolRegistry::new();
        let names = registry.tool_names();
        assert!(names.is_empty());
    }

    #[tokio::test]
    async fn test_registry_empty_tool_definitions() {
        let registry = ToolRegistry::new();
        let definitions = registry.tool_definitions();
        assert!(definitions.is_empty());
    }

    #[tokio::test]
    async fn test_registry_register_arc() {
        let mut registry = ToolRegistry::new();
        let handler: Arc<dyn ToolHandler> = Arc::new(MockTool::new("arc_tool"));
        registry.register_arc(handler);

        assert!(registry.has("arc_tool"));
        assert_eq!(registry.len(), 1);
    }
}
