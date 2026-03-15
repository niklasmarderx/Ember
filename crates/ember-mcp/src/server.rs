//! MCP Server implementation.
//!
//! This module provides the MCP server that handles JSON-RPC requests
//! and exposes tools and resources to MCP clients.

use crate::{
    error::{Error, Result},
    types::*,
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Trait for MCP tool handlers.
#[async_trait]
pub trait MCPToolHandler: Send + Sync {
    /// Get the tool definition.
    fn definition(&self) -> MCPTool;

    /// Execute the tool with the given arguments.
    async fn execute(&self, arguments: HashMap<String, Value>) -> Result<CallToolResult>;
}

/// Trait for MCP resource providers.
#[async_trait]
pub trait MCPResourceProvider: Send + Sync {
    /// List available resources.
    fn list(&self) -> Vec<MCPResource>;

    /// Read a resource by URI.
    async fn read(&self, uri: &str) -> Result<ResourceContent>;
}

/// MCP Server state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerState {
    /// Server is not initialized
    Uninitialized,
    /// Server is initialized and ready
    Ready,
    /// Server is shutting down
    ShuttingDown,
}

/// MCP Server configuration
#[derive(Debug, Clone)]
pub struct MCPServerConfig {
    /// Server name
    pub name: String,
    /// Server version
    pub version: String,
    /// Whether to allow tool list changes
    pub tools_list_changed: bool,
    /// Whether to allow resource list changes
    pub resources_list_changed: bool,
    /// Whether to support resource subscriptions
    pub resources_subscribe: bool,
}

impl Default for MCPServerConfig {
    fn default() -> Self {
        Self {
            name: "ember-mcp".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            tools_list_changed: false,
            resources_list_changed: false,
            resources_subscribe: false,
        }
    }
}

/// The main MCP Server
pub struct MCPServer {
    /// Server configuration
    config: MCPServerConfig,
    /// Current state
    state: RwLock<ServerState>,
    /// Registered tools
    tools: RwLock<HashMap<String, Arc<dyn MCPToolHandler>>>,
    /// Registered resource providers
    resources: RwLock<Vec<Arc<dyn MCPResourceProvider>>>,
    /// Request counter for generating IDs
    request_counter: RwLock<i64>,
}

impl MCPServer {
    /// Create a new MCP server with default configuration.
    pub fn new() -> Self {
        Self::with_config(MCPServerConfig::default())
    }

    /// Create a new MCP server with custom configuration.
    pub fn with_config(config: MCPServerConfig) -> Self {
        Self {
            config,
            state: RwLock::new(ServerState::Uninitialized),
            tools: RwLock::new(HashMap::new()),
            resources: RwLock::new(Vec::new()),
            request_counter: RwLock::new(0),
        }
    }

    /// Get the server state.
    pub async fn state(&self) -> ServerState {
        *self.state.read().await
    }

    /// Register a tool handler.
    pub async fn register_tool(&self, handler: Arc<dyn MCPToolHandler>) {
        let def = handler.definition();
        let name = def.name.clone();
        self.tools.write().await.insert(name.clone(), handler);
        debug!(tool = %name, "Registered MCP tool");
    }

    /// Register a resource provider.
    pub async fn register_resource_provider(&self, provider: Arc<dyn MCPResourceProvider>) {
        self.resources.write().await.push(provider);
        debug!("Registered MCP resource provider");
    }

    /// Handle an incoming JSON-RPC request.
    pub async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        debug!(method = %request.method, "Handling MCP request");

        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.params).await,
            "initialized" => self.handle_initialized().await,
            "tools/list" => self.handle_tools_list().await,
            "tools/call" => self.handle_tools_call(request.params).await,
            "resources/list" => self.handle_resources_list().await,
            "resources/read" => self.handle_resources_read(request.params).await,
            "ping" => Ok(serde_json::json!({})),
            _ => Err(Error::InvalidRequest(format!(
                "Unknown method: {}",
                request.method
            ))),
        };

        match result {
            Ok(value) => JsonRpcResponse::success(request.id, value),
            Err(e) => {
                warn!(error = %e, "MCP request failed");
                JsonRpcResponse::error(request.id, JsonRpcError::internal_error(e.to_string()))
            }
        }
    }

    /// Handle initialize request.
    async fn handle_initialize(&self, params: Option<Value>) -> Result<Value> {
        let state = *self.state.read().await;
        if state != ServerState::Uninitialized {
            return Err(Error::AlreadyInitialized);
        }

        let _params: InitializeParams = match params {
            Some(v) => serde_json::from_value(v)?,
            None => return Err(Error::invalid_request("Missing initialize parameters")),
        };

        // Build capabilities
        let capabilities = ServerCapabilities {
            tools: Some(ToolCapabilities {
                list_changed: self.config.tools_list_changed,
            }),
            resources: Some(ResourceCapabilities {
                subscribe: self.config.resources_subscribe,
                list_changed: self.config.resources_list_changed,
            }),
            prompts: None,
            logging: Some(LoggingCapabilities {}),
        };

        let result = InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities,
            server_info: ServerInfo {
                name: self.config.name.clone(),
                version: self.config.version.clone(),
                protocol_version: PROTOCOL_VERSION.to_string(),
            },
        };

        info!(
            server = %self.config.name,
            version = %self.config.version,
            "MCP server initialized"
        );

        Ok(serde_json::to_value(result)?)
    }

    /// Handle initialized notification.
    async fn handle_initialized(&self) -> Result<Value> {
        *self.state.write().await = ServerState::Ready;
        debug!("MCP server ready");
        Ok(serde_json::json!({}))
    }

    /// Handle tools/list request.
    async fn handle_tools_list(&self) -> Result<Value> {
        let tools = self.tools.read().await;
        let tool_list: Vec<MCPTool> = tools.values().map(|h| h.definition()).collect();

        Ok(serde_json::json!({
            "tools": tool_list
        }))
    }

    /// Handle tools/call request.
    async fn handle_tools_call(&self, params: Option<Value>) -> Result<Value> {
        let params: CallToolParams = match params {
            Some(v) => serde_json::from_value(v)?,
            None => return Err(Error::invalid_request("Missing tool call parameters")),
        };

        let tools = self.tools.read().await;
        let handler = tools
            .get(&params.name)
            .ok_or_else(|| Error::tool_not_found(&params.name))?
            .clone();

        // Release lock before executing
        drop(tools);

        debug!(tool = %params.name, "Executing MCP tool");
        let result = handler.execute(params.arguments).await?;

        Ok(serde_json::to_value(result)?)
    }

    /// Handle resources/list request.
    async fn handle_resources_list(&self) -> Result<Value> {
        let providers = self.resources.read().await;
        let mut all_resources = Vec::new();

        for provider in providers.iter() {
            all_resources.extend(provider.list());
        }

        Ok(serde_json::json!({
            "resources": all_resources
        }))
    }

    /// Handle resources/read request.
    async fn handle_resources_read(&self, params: Option<Value>) -> Result<Value> {
        let params: ReadResourceParams = match params {
            Some(v) => serde_json::from_value(v)?,
            None => return Err(Error::invalid_request("Missing resource read parameters")),
        };

        let providers = self.resources.read().await;

        for provider in providers.iter() {
            for resource in provider.list() {
                if resource.uri == params.uri {
                    let content = provider.read(&params.uri).await?;
                    return Ok(serde_json::to_value(ReadResourceResult {
                        contents: vec![content],
                    })?);
                }
            }
        }

        Err(Error::resource_not_found(&params.uri))
    }

    /// Generate a new request ID.
    pub async fn next_request_id(&self) -> RequestId {
        let mut counter = self.request_counter.write().await;
        *counter += 1;
        RequestId::Number(*counter)
    }

    /// Shutdown the server.
    pub async fn shutdown(&self) {
        *self.state.write().await = ServerState::ShuttingDown;
        info!("MCP server shutting down");
    }
}

impl Default for MCPServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for MCPServer
pub struct MCPServerBuilder {
    config: MCPServerConfig,
    tools: Vec<Arc<dyn MCPToolHandler>>,
    resources: Vec<Arc<dyn MCPResourceProvider>>,
}

impl MCPServerBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            config: MCPServerConfig::default(),
            tools: Vec::new(),
            resources: Vec::new(),
        }
    }

    /// Set the server name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.config.name = name.into();
        self
    }

    /// Set the server version.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.config.version = version.into();
        self
    }

    /// Add a tool handler.
    pub fn tool(mut self, handler: impl MCPToolHandler + 'static) -> Self {
        self.tools.push(Arc::new(handler));
        self
    }

    /// Add a resource provider.
    pub fn resource_provider(mut self, provider: impl MCPResourceProvider + 'static) -> Self {
        self.resources.push(Arc::new(provider));
        self
    }

    /// Build the server.
    pub async fn build(self) -> MCPServer {
        let server = MCPServer::with_config(self.config);

        for tool in self.tools {
            server.register_tool(tool).await;
        }

        for provider in self.resources {
            server.register_resource_provider(provider).await;
        }

        server
    }
}

impl Default for MCPServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestTool;

    #[async_trait]
    impl MCPToolHandler for TestTool {
        fn definition(&self) -> MCPTool {
            MCPTool::new("test_tool")
                .with_description("A test tool")
                .with_input_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "input": {"type": "string"}
                    }
                }))
        }

        async fn execute(&self, arguments: HashMap<String, Value>) -> Result<CallToolResult> {
            let input = arguments
                .get("input")
                .and_then(|v| v.as_str())
                .unwrap_or("default");
            Ok(CallToolResult::text(format!("Result: {}", input)))
        }
    }

    #[tokio::test]
    async fn test_server_creation() {
        let server = MCPServer::new();
        assert_eq!(server.state().await, ServerState::Uninitialized);
    }

    #[tokio::test]
    async fn test_tool_registration() {
        let server = MCPServer::new();
        server.register_tool(Arc::new(TestTool)).await;

        let tools = server.tools.read().await;
        assert!(tools.contains_key("test_tool"));
    }

    #[tokio::test]
    async fn test_initialize() {
        let server = MCPServer::new();

        let request = JsonRpcRequest::new(1i64, "initialize").with_params(serde_json::json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        }));

        let response = server.handle_request(request).await;
        assert!(response.is_success());

        // Simulate initialized notification
        let init_request = JsonRpcRequest::new(2i64, "initialized");
        server.handle_request(init_request).await;

        assert_eq!(server.state().await, ServerState::Ready);
    }

    #[tokio::test]
    async fn test_tools_list() {
        let server = MCPServer::new();
        server.register_tool(Arc::new(TestTool)).await;

        let request = JsonRpcRequest::new(1i64, "tools/list");
        let response = server.handle_request(request).await;

        assert!(response.is_success());
        let result = response.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "test_tool");
    }

    #[tokio::test]
    async fn test_tools_call() {
        let server = MCPServer::new();
        server.register_tool(Arc::new(TestTool)).await;

        let request = JsonRpcRequest::new(1i64, "tools/call").with_params(serde_json::json!({
            "name": "test_tool",
            "arguments": {
                "input": "hello"
            }
        }));

        let response = server.handle_request(request).await;
        assert!(response.is_success());

        let result = response.result.unwrap();
        assert!(!result["isError"].as_bool().unwrap_or(true));
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let server = MCPServer::new();

        let request = JsonRpcRequest::new(1i64, "unknown/method");
        let response = server.handle_request(request).await;

        assert!(!response.is_success());
        assert!(response.error.is_some());
    }
}
