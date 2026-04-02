//! MCP Client for connecting to external MCP servers.
//!
//! This module provides a client implementation for the Model Context Protocol,
//! allowing Ember to connect to and use external MCP servers just like Cline does.
//!
//! # Supported Transports
//!
//! - **Stdio**: Communicate via stdin/stdout with a subprocess
//! - **SSE**: Server-Sent Events over HTTP (coming soon)
//! - **WebSocket**: WebSocket connection (coming soon)
//!
//! # Example
//!
//! ```rust,no_run
//! use ember_mcp::{MCPClient, StdioTransport};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Connect to a filesystem MCP server
//!     let transport = StdioTransport::new("npx", &[
//!         "-y", "@modelcontextprotocol/server-filesystem", "/tmp"
//!     ]);
//!     
//!     let client = MCPClient::connect(transport).await?;
//!     
//!     // List available tools
//!     let tools = client.list_tools().await?;
//!     for tool in tools {
//!         println!("Tool: {} - {:?}", tool.name, tool.description);
//!     }
//!     
//!     // Call a tool
//!     let result = client.call_tool("read_file", serde_json::json!({
//!         "path": "/tmp/test.txt"
//!     })).await?;
//!     
//!     println!("Result: {:?}", result);
//!     
//!     Ok(())
//! }
//! ```

use crate::types::{
    CallToolParams, CallToolResult, ClientCapabilities, ClientInfo, InitializeParams,
    InitializeResult, JsonRpcRequest, JsonRpcResponse, MCPResource, MCPTool, ReadResourceParams,
    ReadResourceResult, PROTOCOL_VERSION,
};
use crate::{Error, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader};
use tokio::process::{Child as TokioChild, Command as TokioCommand};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

/// Transport trait for MCP communication.
#[async_trait]
pub trait MCPTransport: Send + Sync {
    /// Send a request and wait for response.
    async fn request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse>;

    /// Send a notification (no response expected).
    async fn notify(&self, method: &str, params: Option<Value>) -> Result<()>;

    /// Close the transport.
    async fn close(&self) -> Result<()>;
}

/// MCP Client for communicating with MCP servers.
pub struct MCPClient {
    /// Transport layer
    transport: Arc<dyn MCPTransport>,

    /// Server info after initialization
    server_info: RwLock<Option<InitializeResult>>,

    /// Request ID counter
    request_id: AtomicI64,

    /// Cached tools list
    cached_tools: RwLock<Option<Vec<MCPTool>>>,

    /// Cached resources list
    cached_resources: RwLock<Option<Vec<MCPResource>>>,

    /// Client configuration
    config: MCPClientConfig,
}

/// Configuration for MCP Client.
#[derive(Debug, Clone)]
pub struct MCPClientConfig {
    /// Client name for identification
    pub name: String,

    /// Client version
    pub version: String,

    /// Request timeout in seconds
    pub timeout_secs: u64,

    /// Whether to cache tool/resource lists
    pub enable_caching: bool,
}

impl Default for MCPClientConfig {
    fn default() -> Self {
        Self {
            name: "ember".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            timeout_secs: 30,
            enable_caching: true,
        }
    }
}

impl MCPClient {
    /// Connect to an MCP server using the specified transport.
    pub async fn connect(transport: impl MCPTransport + 'static) -> Result<Self> {
        Self::connect_with_config(transport, MCPClientConfig::default()).await
    }

    /// Connect with custom configuration.
    pub async fn connect_with_config(
        transport: impl MCPTransport + 'static,
        config: MCPClientConfig,
    ) -> Result<Self> {
        let client = Self {
            transport: Arc::new(transport),
            server_info: RwLock::new(None),
            request_id: AtomicI64::new(1),
            cached_tools: RwLock::new(None),
            cached_resources: RwLock::new(None),
            config,
        };

        // Initialize the connection
        client.initialize().await?;

        Ok(client)
    }

    /// Initialize the MCP connection.
    async fn initialize(&self) -> Result<InitializeResult> {
        let params = InitializeParams {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ClientCapabilities::default(),
            client_info: ClientInfo {
                name: self.config.name.clone(),
                version: self.config.version.clone(),
            },
        };

        let request = self.build_request("initialize", Some(serde_json::to_value(&params)?));
        let response = self.transport.request(request).await?;

        if let Some(error) = response.error {
            return Err(Error::Protocol {
                message: format!("Initialize failed: {}", error.message),
            });
        }

        let result: InitializeResult =
            serde_json::from_value(response.result.ok_or_else(|| Error::Protocol {
                message: "Missing initialize result".to_string(),
            })?)?;

        info!(
            server = %result.server_info.name,
            version = %result.server_info.version,
            "Connected to MCP server"
        );

        *self.server_info.write().await = Some(result.clone());

        // Send initialized notification
        self.transport
            .notify("notifications/initialized", None)
            .await?;

        Ok(result)
    }

    /// Build a JSON-RPC request with auto-incrementing ID.
    fn build_request(&self, method: &str, params: Option<Value>) -> JsonRpcRequest {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let mut request = JsonRpcRequest::new(id, method);
        if let Some(p) = params {
            request = request.with_params(p);
        }
        request
    }

    /// Get server information.
    pub async fn server_info(&self) -> Option<InitializeResult> {
        self.server_info.read().await.clone()
    }

    /// List available tools from the server.
    pub async fn list_tools(&self) -> Result<Vec<MCPTool>> {
        // Check cache first
        if self.config.enable_caching {
            if let Some(tools) = self.cached_tools.read().await.as_ref() {
                return Ok(tools.clone());
            }
        }

        let request = self.build_request("tools/list", None);
        let response = self.transport.request(request).await?;

        if let Some(error) = response.error {
            return Err(Error::Protocol {
                message: format!("Failed to list tools: {}", error.message),
            });
        }

        let result = response.result.ok_or_else(|| Error::Protocol {
            message: "Missing tools/list result".to_string(),
        })?;

        let tools: Vec<MCPTool> =
            serde_json::from_value(result.get("tools").cloned().unwrap_or(Value::Array(vec![])))?;

        // Update cache
        if self.config.enable_caching {
            *self.cached_tools.write().await = Some(tools.clone());
        }

        Ok(tools)
    }

    /// Call a tool on the MCP server.
    pub async fn call_tool(
        &self,
        name: impl Into<String>,
        arguments: Value,
    ) -> Result<CallToolResult> {
        let name = name.into();

        debug!(tool = %name, "Calling MCP tool");

        let params = CallToolParams {
            name: name.clone(),
            arguments: if let Value::Object(map) = arguments {
                map.into_iter().collect()
            } else {
                HashMap::new()
            },
        };

        let request = self.build_request("tools/call", Some(serde_json::to_value(&params)?));
        let response = self.transport.request(request).await?;

        if let Some(error) = response.error {
            return Err(Error::ToolExecution {
                tool: name,
                message: error.message,
            });
        }

        let result: CallToolResult =
            serde_json::from_value(response.result.ok_or_else(|| Error::Protocol {
                message: "Missing tools/call result".to_string(),
            })?)?;

        Ok(result)
    }

    /// List available resources from the server.
    pub async fn list_resources(&self) -> Result<Vec<MCPResource>> {
        // Check cache first
        if self.config.enable_caching {
            if let Some(resources) = self.cached_resources.read().await.as_ref() {
                return Ok(resources.clone());
            }
        }

        let request = self.build_request("resources/list", None);
        let response = self.transport.request(request).await?;

        if let Some(error) = response.error {
            return Err(Error::Protocol {
                message: format!("Failed to list resources: {}", error.message),
            });
        }

        let result = response.result.ok_or_else(|| Error::Protocol {
            message: "Missing resources/list result".to_string(),
        })?;

        let resources: Vec<MCPResource> = serde_json::from_value(
            result
                .get("resources")
                .cloned()
                .unwrap_or(Value::Array(vec![])),
        )?;

        // Update cache
        if self.config.enable_caching {
            *self.cached_resources.write().await = Some(resources.clone());
        }

        Ok(resources)
    }

    /// Read a resource from the server.
    pub async fn read_resource(&self, uri: impl Into<String>) -> Result<ReadResourceResult> {
        let uri = uri.into();

        debug!(uri = %uri, "Reading MCP resource");

        let params = ReadResourceParams { uri: uri.clone() };

        let request = self.build_request("resources/read", Some(serde_json::to_value(&params)?));
        let response = self.transport.request(request).await?;

        if let Some(error) = response.error {
            return Err(Error::Protocol {
                message: format!("Failed to read resource '{}': {}", uri, error.message),
            });
        }

        let result: ReadResourceResult =
            serde_json::from_value(response.result.ok_or_else(|| Error::Protocol {
                message: "Missing resources/read result".to_string(),
            })?)?;

        Ok(result)
    }

    /// Invalidate cached data (tools and resources).
    pub async fn invalidate_cache(&self) {
        *self.cached_tools.write().await = None;
        *self.cached_resources.write().await = None;
    }

    /// Close the client connection.
    pub async fn close(&self) -> Result<()> {
        self.transport.close().await
    }
}

// ============================================================================
// Stdio Transport
// ============================================================================

/// Transport using stdin/stdout communication with a subprocess.
pub struct StdioTransport {
    /// Child process
    child: Mutex<Option<TokioChild>>,

    /// Process stdin
    stdin: Mutex<Option<tokio::process::ChildStdin>>,

    /// Process stdout reader
    stdout: Mutex<Option<TokioBufReader<tokio::process::ChildStdout>>>,

    /// Command to run
    command: String,

    /// Command arguments
    args: Vec<String>,

    /// Environment variables
    env: HashMap<String, String>,
}

impl StdioTransport {
    /// Create a new stdio transport.
    pub fn new(command: impl Into<String>, args: &[&str]) -> Self {
        Self {
            child: Mutex::new(None),
            stdin: Mutex::new(None),
            stdout: Mutex::new(None),
            command: command.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            env: HashMap::new(),
        }
    }

    /// Add environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Start the subprocess.
    pub async fn start(&self) -> Result<()> {
        info!(command = %self.command, args = ?self.args, "Starting MCP server process");

        let mut cmd = TokioCommand::new(&self.command);
        cmd.args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in &self.env {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn().map_err(|e| Error::Transport {
            message: format!("Failed to start MCP server: {}", e),
        })?;

        let stdin = child.stdin.take().ok_or_else(|| Error::Transport {
            message: "Failed to open stdin".to_string(),
        })?;

        let stdout = child.stdout.take().ok_or_else(|| Error::Transport {
            message: "Failed to open stdout".to_string(),
        })?;

        *self.child.lock().await = Some(child);
        *self.stdin.lock().await = Some(stdin);
        *self.stdout.lock().await = Some(TokioBufReader::new(stdout));

        Ok(())
    }
}

#[async_trait]
impl MCPTransport for StdioTransport {
    async fn request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        // Ensure process is started
        if self.stdin.lock().await.is_none() {
            self.start().await?;
        }

        let json = serde_json::to_string(&request)?;

        debug!(method = %request.method, id = ?request.id, "Sending MCP request");

        // Write request
        {
            let mut stdin_guard = self.stdin.lock().await;
            let stdin = stdin_guard.as_mut().ok_or_else(|| Error::Transport {
                message: "Stdin not available".to_string(),
            })?;

            stdin
                .write_all(json.as_bytes())
                .await
                .map_err(|e| Error::Transport {
                    message: format!("Failed to write request: {}", e),
                })?;
            stdin.write_all(b"\n").await.map_err(|e| Error::Transport {
                message: format!("Failed to write newline: {}", e),
            })?;
            stdin.flush().await.map_err(|e| Error::Transport {
                message: format!("Failed to flush: {}", e),
            })?;
        }

        // Read response
        let response = {
            let mut stdout_guard = self.stdout.lock().await;
            let stdout = stdout_guard.as_mut().ok_or_else(|| Error::Transport {
                message: "Stdout not available".to_string(),
            })?;

            let mut line = String::new();
            stdout
                .read_line(&mut line)
                .await
                .map_err(|e| Error::Transport {
                    message: format!("Failed to read response: {}", e),
                })?;

            if line.is_empty() {
                return Err(Error::Transport {
                    message: "Server closed connection".to_string(),
                });
            }

            serde_json::from_str::<JsonRpcResponse>(&line)?
        };

        debug!(id = ?response.id, success = response.is_success(), "Received MCP response");

        Ok(response)
    }

    async fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        // Ensure process is started
        if self.stdin.lock().await.is_none() {
            self.start().await?;
        }

        let notification = crate::types::JsonRpcNotification::new(method);
        let notification = if let Some(p) = params {
            notification.with_params(p)
        } else {
            notification
        };

        let json = serde_json::to_string(&notification)?;

        debug!(method = %method, "Sending MCP notification");

        let mut stdin_guard = self.stdin.lock().await;
        let stdin = stdin_guard.as_mut().ok_or_else(|| Error::Transport {
            message: "Stdin not available".to_string(),
        })?;

        stdin
            .write_all(json.as_bytes())
            .await
            .map_err(|e| Error::Transport {
                message: format!("Failed to write notification: {}", e),
            })?;
        stdin.write_all(b"\n").await.map_err(|e| Error::Transport {
            message: format!("Failed to write newline: {}", e),
        })?;
        stdin.flush().await.map_err(|e| Error::Transport {
            message: format!("Failed to flush: {}", e),
        })?;

        Ok(())
    }

    async fn close(&self) -> Result<()> {
        // Drop stdin to signal EOF
        *self.stdin.lock().await = None;

        // Kill the process if still running
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.kill().await;
        }

        info!("Closed MCP connection");
        Ok(())
    }
}

// ============================================================================
// Multi-Server Manager
// ============================================================================

/// Manages multiple MCP server connections.
pub struct MCPManager {
    /// Connected clients by name
    clients: RwLock<HashMap<String, Arc<MCPClient>>>,
}

impl MCPManager {
    /// Create a new MCP manager.
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
        }
    }

    /// Add a client connection.
    pub async fn add_client(&self, name: impl Into<String>, client: MCPClient) {
        let name = name.into();
        info!(name = %name, "Adding MCP client");
        self.clients.write().await.insert(name, Arc::new(client));
    }

    /// Get a client by name.
    pub async fn get_client(&self, name: &str) -> Option<Arc<MCPClient>> {
        self.clients.read().await.get(name).cloned()
    }

    /// Remove a client.
    pub async fn remove_client(&self, name: &str) -> Option<Arc<MCPClient>> {
        self.clients.write().await.remove(name)
    }

    /// List all client names.
    pub async fn list_clients(&self) -> Vec<String> {
        self.clients.read().await.keys().cloned().collect()
    }

    /// List all tools from all connected servers.
    pub async fn list_all_tools(&self) -> Result<Vec<(String, MCPTool)>> {
        let mut all_tools = Vec::new();

        for (name, client) in self.clients.read().await.iter() {
            match client.list_tools().await {
                Ok(tools) => {
                    for tool in tools {
                        all_tools.push((name.clone(), tool));
                    }
                }
                Err(e) => {
                    warn!(server = %name, error = %e, "Failed to list tools from server");
                }
            }
        }

        Ok(all_tools)
    }

    /// Call a tool on a specific server.
    pub async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Value,
    ) -> Result<CallToolResult> {
        let client = self
            .clients
            .read()
            .await
            .get(server)
            .cloned()
            .ok_or_else(|| Error::Protocol {
                message: format!("Server '{}' not found", server),
            })?;

        client.call_tool(tool, arguments).await
    }

    /// Close all connections.
    pub async fn close_all(&self) -> Result<()> {
        let clients: Vec<_> = self.clients.write().await.drain().collect();

        for (name, client) in clients {
            if let Err(e) = client.close().await {
                warn!(server = %name, error = %e, "Error closing MCP client");
            }
        }

        Ok(())
    }
}

impl Default for MCPManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Multi-transport enum + concrete HTTP/WebSocket transport stubs
// ============================================================================

/// Supported transport kinds for connecting to external MCP servers.
///
/// Used in [`McpServerConfig`] to describe how Ember should reach the server.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    /// Communicate via stdin/stdout with a child process.
    Stdio,
    /// Plain HTTP (request/response) — server must accept JSON-RPC over HTTP POST.
    Http,
    /// HTTP Server-Sent Events stream — server pushes responses on an SSE channel.
    Sse,
    /// Full-duplex WebSocket connection.
    WebSocket,
}

impl std::fmt::Display for McpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpTransport::Stdio => write!(f, "stdio"),
            McpTransport::Http => write!(f, "http"),
            McpTransport::Sse => write!(f, "sse"),
            McpTransport::WebSocket => write!(f, "websocket"),
        }
    }
}

/// Configuration for an HTTP-based MCP transport.
///
/// Sends JSON-RPC requests as HTTP POST to `endpoint`.
#[derive(Debug, Clone)]
pub struct HttpTransport {
    /// Base URL of the MCP server, e.g. `http://localhost:3000/mcp`
    pub endpoint: String,
    /// Optional bearer token for authentication.
    pub auth_token: Option<String>,
    /// Request timeout in seconds (defaults to 30).
    pub timeout_secs: u64,
}

impl HttpTransport {
    /// Create a new HTTP transport pointed at `endpoint`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            auth_token: None,
            timeout_secs: 30,
        }
    }

    /// Attach a bearer auth token.
    pub fn with_auth(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// Override the request timeout.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
}

/// Configuration for a WebSocket-based MCP transport.
///
/// Establishes a persistent WS connection to `url`.
#[derive(Debug, Clone)]
pub struct WebSocketTransport {
    /// WebSocket URL, e.g. `ws://localhost:3001/mcp`
    pub url: String,
    /// Optional bearer token sent in the `Authorization` header during the
    /// WS handshake upgrade request.
    pub auth_token: Option<String>,
    /// Reconnect automatically on disconnection.
    pub auto_reconnect: bool,
}

impl WebSocketTransport {
    /// Create a new WebSocket transport for `url`.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            auth_token: None,
            auto_reconnect: true,
        }
    }

    /// Attach a bearer auth token.
    pub fn with_auth(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// Disable automatic reconnection.
    pub fn without_reconnect(mut self) -> Self {
        self.auto_reconnect = false;
        self
    }
}

// ============================================================================
// McpServerConfig
// ============================================================================

/// Complete configuration for a single external MCP server.
///
/// Stored in [`McpClientRegistry`] and used to connect, discover tools, and
/// route calls to the right server.
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    /// Unique logical name for this server (used as tool namespace prefix).
    ///
    /// Must be non-empty and contain only `[a-z0-9_-]` characters after
    /// normalisation via [`normalize_mcp_name`].
    pub name: String,
    /// How to reach the server.
    pub transport: McpTransport,
    /// Command to execute (required for [`McpTransport::Stdio`]).
    pub command: Option<String>,
    /// Arguments passed to the command (stdio only).
    pub args: Vec<String>,
    /// Environment variables injected into the server process (stdio only).
    pub env: HashMap<String, String>,
    /// HTTP/SSE endpoint URL (required for [`McpTransport::Http`] /
    /// [`McpTransport::Sse`]).
    pub endpoint: Option<String>,
    /// WebSocket URL (required for [`McpTransport::WebSocket`]).
    pub ws_url: Option<String>,
    /// Optional auth token (HTTP, SSE, WebSocket).
    pub auth_token: Option<String>,
    /// Whether this server is currently enabled.
    pub enabled: bool,
}

impl McpServerConfig {
    /// Create a stdio-transport server config.
    pub fn stdio(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            transport: McpTransport::Stdio,
            command: Some(command.into()),
            args: Vec::new(),
            env: HashMap::new(),
            endpoint: None,
            ws_url: None,
            auth_token: None,
            enabled: true,
        }
    }

    /// Create an HTTP-transport server config.
    pub fn http(name: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            env: HashMap::new(),
            endpoint: Some(endpoint.into()),
            ws_url: None,
            auth_token: None,
            enabled: true,
        }
    }

    /// Create an SSE-transport server config.
    pub fn sse(name: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            transport: McpTransport::Sse,
            command: None,
            args: Vec::new(),
            env: HashMap::new(),
            endpoint: Some(endpoint.into()),
            ws_url: None,
            auth_token: None,
            enabled: true,
        }
    }

    /// Create a WebSocket-transport server config.
    pub fn websocket(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            transport: McpTransport::WebSocket,
            command: None,
            args: Vec::new(),
            env: HashMap::new(),
            endpoint: None,
            ws_url: Some(url.into()),
            auth_token: None,
            enabled: true,
        }
    }

    /// Attach command-line arguments (stdio only).
    pub fn with_args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args = args.into_iter().map(|a| a.into()).collect();
        self
    }

    /// Inject an environment variable (stdio only).
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Attach a bearer auth token.
    pub fn with_auth(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// Mark the server as disabled.
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

// ============================================================================
// McpToolDefinition / McpToolResult / McpContent
// ============================================================================

/// A tool exposed by an external MCP server, enriched with registry metadata.
///
/// The `namespaced_name` field holds the fully-qualified tool identifier in the
/// form `mcp__{server}__{tool}` (see [`mcp_tool_name`]).
#[derive(Debug, Clone)]
pub struct McpToolDefinition {
    /// Fully-qualified namespaced name: `mcp__{server}__{tool}`.
    pub namespaced_name: String,
    /// Logical name of the server this tool belongs to.
    pub server_name: String,
    /// Original tool name as reported by the server.
    pub tool_name: String,
    /// Human-readable description.
    pub description: Option<String>,
    /// JSON Schema for the tool's input parameters.
    pub input_schema: serde_json::Value,
}

impl McpToolDefinition {
    /// Build a definition from a server name and an [`crate::types::MCPTool`] value.
    pub fn from_mcp_tool(server_name: &str, tool: &crate::types::MCPTool) -> Self {
        Self {
            namespaced_name: mcp_tool_name(server_name, &tool.name),
            server_name: server_name.to_string(),
            tool_name: tool.name.clone(),
            description: tool.description.clone(),
            input_schema: tool.input_schema.clone(),
        }
    }
}

/// Content item returned by a tool call.
///
/// Mirrors the three variants defined in the MCP specification.
#[derive(Debug, Clone)]
pub enum McpContent {
    /// Plain text output.
    Text(String),
    /// Base64-encoded image with a MIME type.
    Image {
        /// Base64-encoded image data.
        data: String,
        /// MIME type, e.g. `image/png`.
        mime_type: String,
    },
    /// Reference to a resource URI.
    Resource {
        /// Resource URI.
        uri: String,
        /// Optional MIME type.
        mime_type: Option<String>,
        /// Optional inline text.
        text: Option<String>,
    },
}

impl McpContent {
    /// Create a plain-text content item.
    pub fn text(text: impl Into<String>) -> Self {
        McpContent::Text(text.into())
    }

    /// Create an image content item.
    pub fn image(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        McpContent::Image {
            data: data.into(),
            mime_type: mime_type.into(),
        }
    }

    /// Create a resource content item.
    pub fn resource(uri: impl Into<String>) -> Self {
        McpContent::Resource {
            uri: uri.into(),
            mime_type: None,
            text: None,
        }
    }

    /// Returns the text payload if this is a [`McpContent::Text`] variant.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            McpContent::Text(t) => Some(t.as_str()),
            _ => None,
        }
    }
}

/// The result of a tool invocation via [`McpClientRegistry`].
#[derive(Debug, Clone)]
pub struct McpToolResult {
    /// Namespaced tool name that produced this result.
    pub tool_name: String,
    /// Content items returned by the tool.
    pub content: Vec<McpContent>,
    /// `true` if the server signalled an error response.
    pub is_error: bool,
}

impl McpToolResult {
    /// Convenience constructor for a successful text result.
    pub fn text(tool_name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            content: vec![McpContent::text(text)],
            is_error: false,
        }
    }

    /// Convenience constructor for an error result.
    pub fn error(tool_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            content: vec![McpContent::text(message)],
            is_error: true,
        }
    }
}

// ============================================================================
// Name helpers
// ============================================================================

/// Normalise a raw server or tool name so it is safe to embed in identifiers.
///
/// Rules applied (in order):
/// 1. Convert to lowercase.
/// 2. Replace any character that is not `[a-z0-9_-]` with `_`.
/// 3. Collapse consecutive `_` characters into one.
/// 4. Strip leading / trailing `_` characters.
/// 5. If the result is empty, substitute `"unknown"`.
///
/// # Examples
///
/// ```
/// use ember_mcp::client::normalize_mcp_name;
///
/// assert_eq!(normalize_mcp_name("My Server!"), "my_server");
/// assert_eq!(normalize_mcp_name("fs-tools"), "fs-tools"); // hyphens preserved
/// assert_eq!(normalize_mcp_name("  "), "unknown");
/// ```
pub fn normalize_mcp_name(raw: &str) -> String {
    let lower = raw.to_lowercase();
    // Replace disallowed chars with underscore
    let replaced: String = lower
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // Collapse multiple consecutive underscores
    let mut collapsed = String::with_capacity(replaced.len());
    let mut prev_under = false;
    for c in replaced.chars() {
        if c == '_' {
            if !prev_under {
                collapsed.push(c);
            }
            prev_under = true;
        } else {
            collapsed.push(c);
            prev_under = false;
        }
    }
    // Trim leading/trailing underscores
    let trimmed = collapsed.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed
    }
}

/// Build the fully-qualified, namespaced tool name used inside Ember.
///
/// Format: `mcp__{server}__{tool}` — double-underscores delimit the parts.
///
/// Both `server_name` and `tool_name` are normalised via [`normalize_mcp_name`]
/// before being joined.
///
/// # Examples
///
/// ```
/// use ember_mcp::client::mcp_tool_name;
///
/// assert_eq!(mcp_tool_name("filesystem", "read_file"), "mcp__filesystem__read_file");
/// assert_eq!(mcp_tool_name("My Server", "List Files!"), "mcp__my_server__list_files");
/// ```
pub fn mcp_tool_name(server_name: &str, tool_name: &str) -> String {
    format!(
        "mcp__{}__{}",
        normalize_mcp_name(server_name),
        normalize_mcp_name(tool_name)
    )
}

/// Parse a namespaced tool name produced by [`mcp_tool_name`] back into its
/// `(server_name, tool_name)` components.
///
/// Returns `None` if `namespaced` does not match the `mcp__{server}__{tool}`
/// pattern.
///
/// # Examples
///
/// ```
/// use ember_mcp::client::parse_mcp_tool_name;
///
/// assert_eq!(
///     parse_mcp_tool_name("mcp__filesystem__read_file"),
///     Some(("filesystem".to_string(), "read_file".to_string()))
/// );
/// assert_eq!(parse_mcp_tool_name("read_file"), None);
/// assert_eq!(parse_mcp_tool_name("mcp__only_server"), None);
/// ```
pub fn parse_mcp_tool_name(namespaced: &str) -> Option<(String, String)> {
    let rest = namespaced.strip_prefix("mcp__")?;
    // Find the first double-underscore delimiter after the prefix.
    let sep = rest.find("__")?;
    let server = &rest[..sep];
    let tool = &rest[sep + 2..];
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    Some((server.to_string(), tool.to_string()))
}

// ============================================================================
// McpClientRegistry
// ============================================================================

/// Registry that maps server configurations and their available tools.
///
/// `McpClientRegistry` acts as the single source of truth about which MCP
/// servers Ember knows about and which tools each server exposes.  It does
/// **not** maintain live connections — that is the job of [`MCPManager`].
/// Instead it stores [`McpServerConfig`] entries and the [`McpToolDefinition`]s
/// that have been fetched and registered for each server.
///
/// # Tool naming
///
/// Every tool is stored under its *namespaced* name:
/// `mcp__{server}__{tool}`.  This avoids collisions when multiple servers
/// expose tools with the same bare name.
///
/// # Thread safety
///
/// All state is protected by `tokio::sync::RwLock` — concurrent reads are
/// cheap; writes (`register_*`, `unregister_*`) take an exclusive lock.
pub struct McpClientRegistry {
    /// Server configurations keyed by (normalised) server name.
    servers: RwLock<HashMap<String, McpServerConfig>>,
    /// Tool definitions keyed by namespaced tool name.
    tools: RwLock<HashMap<String, McpToolDefinition>>,
}

impl McpClientRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            servers: RwLock::new(HashMap::new()),
            tools: RwLock::new(HashMap::new()),
        }
    }

    // ------------------------------------------------------------------
    // Server management
    // ------------------------------------------------------------------

    /// Register (or replace) a server configuration.
    ///
    /// The server's `name` field is normalised via [`normalize_mcp_name`]
    /// before storage.  If a server with the same normalised name already
    /// exists its configuration is overwritten.
    pub async fn register_server(&self, mut config: McpServerConfig) {
        config.name = normalize_mcp_name(&config.name);
        let name = config.name.clone();
        info!(server = %name, transport = %config.transport, "Registering MCP server");
        self.servers.write().await.insert(name, config);
    }

    /// Remove a server and **all tools belonging to it** from the registry.
    ///
    /// Returns the removed [`McpServerConfig`] if it was present.
    pub async fn unregister_server(&self, server_name: &str) -> Option<McpServerConfig> {
        let normalised = normalize_mcp_name(server_name);
        let removed = self.servers.write().await.remove(&normalised);
        if removed.is_some() {
            // Purge all tools that belong to this server.
            self.tools
                .write()
                .await
                .retain(|_, def| def.server_name != normalised);
            info!(server = %normalised, "Unregistered MCP server and its tools");
        }
        removed
    }

    /// Return a clone of the configuration for `server_name`, if known.
    pub async fn server(&self, server_name: &str) -> Option<McpServerConfig> {
        let normalised = normalize_mcp_name(server_name);
        self.servers.read().await.get(&normalised).cloned()
    }

    /// Return the names of all registered servers.
    pub async fn servers(&self) -> Vec<String> {
        self.servers.read().await.keys().cloned().collect()
    }

    /// Return the number of registered servers.
    pub async fn server_count(&self) -> usize {
        self.servers.read().await.len()
    }

    // ------------------------------------------------------------------
    // Tool management
    // ------------------------------------------------------------------

    /// Register a batch of tools for `server_name`.
    ///
    /// Any existing tools for that server are replaced.  Each tool's
    /// `namespaced_name` is derived from `server_name` and the tool's own
    /// name via [`mcp_tool_name`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Protocol`] if `server_name` is not already registered.
    pub async fn register_tools(
        &self,
        server_name: &str,
        mcp_tools: Vec<crate::types::MCPTool>,
    ) -> Result<usize> {
        let normalised = normalize_mcp_name(server_name);
        if !self.servers.read().await.contains_key(&normalised) {
            return Err(Error::Protocol {
                message: format!(
                    "Cannot register tools: server '{}' is not registered",
                    normalised
                ),
            });
        }

        let mut tools_guard = self.tools.write().await;
        // Remove stale tools for this server first.
        tools_guard.retain(|_, def| def.server_name != normalised);

        let count = mcp_tools.len();
        for mcp_tool in mcp_tools {
            let def = McpToolDefinition::from_mcp_tool(&normalised, &mcp_tool);
            debug!(tool = %def.namespaced_name, "Registering MCP tool");
            tools_guard.insert(def.namespaced_name.clone(), def);
        }
        info!(server = %normalised, count = count, "Registered MCP tools");
        Ok(count)
    }

    /// Look up a single tool by its namespaced name (`mcp__{server}__{tool}`).
    pub async fn get_tool(&self, namespaced_name: &str) -> Option<McpToolDefinition> {
        self.tools.read().await.get(namespaced_name).cloned()
    }

    /// Return all registered tools across all servers.
    pub async fn all_tools(&self) -> Vec<McpToolDefinition> {
        self.tools.read().await.values().cloned().collect()
    }

    /// Return the total number of registered tools.
    pub async fn tool_count(&self) -> usize {
        self.tools.read().await.len()
    }
}

impl Default for McpClientRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests (existing + new)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Existing tests preserved
    // ------------------------------------------------------------------

    #[test]
    fn test_client_config_default() {
        let config = MCPClientConfig::default();
        assert_eq!(config.name, "ember");
        assert_eq!(config.timeout_secs, 30);
        assert!(config.enable_caching);
    }

    #[test]
    fn test_stdio_transport_creation() {
        let transport = StdioTransport::new("echo", &["hello"]).with_env("TEST_VAR", "test_value");

        assert_eq!(transport.command, "echo");
        assert_eq!(transport.args, vec!["hello".to_string()]);
        assert_eq!(
            transport.env.get("TEST_VAR"),
            Some(&"test_value".to_string())
        );
    }

    #[test]
    fn test_mcp_manager_creation() {
        let manager = MCPManager::new();
        assert!(manager.clients.try_read().is_ok());
    }

    // ------------------------------------------------------------------
    // McpTransport enum
    // ------------------------------------------------------------------

    #[test]
    fn test_mcp_transport_display() {
        assert_eq!(McpTransport::Stdio.to_string(), "stdio");
        assert_eq!(McpTransport::Http.to_string(), "http");
        assert_eq!(McpTransport::Sse.to_string(), "sse");
        assert_eq!(McpTransport::WebSocket.to_string(), "websocket");
    }

    #[test]
    fn test_mcp_transport_serialization() {
        let json = serde_json::to_string(&McpTransport::Sse).unwrap();
        assert_eq!(json, r#""sse""#);
        let round: McpTransport = serde_json::from_str(&json).unwrap();
        assert_eq!(round, McpTransport::Sse);
    }

    // ------------------------------------------------------------------
    // HttpTransport / WebSocketTransport
    // ------------------------------------------------------------------

    #[test]
    fn test_http_transport_builder() {
        let t = HttpTransport::new("http://localhost:3000/mcp")
            .with_auth("secret-token")
            .with_timeout(60);
        assert_eq!(t.endpoint, "http://localhost:3000/mcp");
        assert_eq!(t.auth_token.as_deref(), Some("secret-token"));
        assert_eq!(t.timeout_secs, 60);
    }

    #[test]
    fn test_websocket_transport_builder() {
        let t = WebSocketTransport::new("ws://localhost:3001/mcp")
            .with_auth("tok")
            .without_reconnect();
        assert_eq!(t.url, "ws://localhost:3001/mcp");
        assert_eq!(t.auth_token.as_deref(), Some("tok"));
        assert!(!t.auto_reconnect);
    }

    // ------------------------------------------------------------------
    // McpServerConfig
    // ------------------------------------------------------------------

    #[test]
    fn test_server_config_stdio() {
        let cfg = McpServerConfig::stdio("fs-server", "npx")
            .with_args(["-y", "@modelcontextprotocol/server-filesystem", "/tmp"])
            .with_env("HOME", "/root");
        assert_eq!(cfg.transport, McpTransport::Stdio);
        assert_eq!(cfg.command.as_deref(), Some("npx"));
        assert_eq!(cfg.args.len(), 3);
        assert_eq!(cfg.env.get("HOME").map(|s| s.as_str()), Some("/root"));
        assert!(cfg.enabled);
    }

    #[test]
    fn test_server_config_http() {
        let cfg = McpServerConfig::http("web-tools", "http://api.example.com/mcp");
        assert_eq!(cfg.transport, McpTransport::Http);
        assert_eq!(
            cfg.endpoint.as_deref(),
            Some("http://api.example.com/mcp")
        );
    }

    #[test]
    fn test_server_config_disabled() {
        let cfg = McpServerConfig::sse("stream-server", "http://stream.example.com/events")
            .disabled();
        assert!(!cfg.enabled);
        assert_eq!(cfg.transport, McpTransport::Sse);
    }

    #[test]
    fn test_server_config_websocket() {
        let cfg =
            McpServerConfig::websocket("live-tools", "ws://live.example.com/mcp").with_auth("jwt");
        assert_eq!(cfg.transport, McpTransport::WebSocket);
        assert_eq!(cfg.auth_token.as_deref(), Some("jwt"));
    }

    // ------------------------------------------------------------------
    // normalize_mcp_name
    // ------------------------------------------------------------------

    #[test]
    fn test_normalize_mcp_name_basic() {
        assert_eq!(normalize_mcp_name("filesystem"), "filesystem");
        assert_eq!(normalize_mcp_name("my-server"), "my-server");
        assert_eq!(normalize_mcp_name("my_server"), "my_server");
    }

    #[test]
    fn test_normalize_mcp_name_uppercase() {
        assert_eq!(normalize_mcp_name("FileSystem"), "filesystem");
        assert_eq!(normalize_mcp_name("MY_SERVER"), "my_server");
    }

    #[test]
    fn test_normalize_mcp_name_special_chars() {
        assert_eq!(normalize_mcp_name("My Server!"), "my_server");
        assert_eq!(normalize_mcp_name("server@v2"), "server_v2");
        assert_eq!(normalize_mcp_name("a  b"), "a_b");
    }

    #[test]
    fn test_normalize_mcp_name_empty() {
        assert_eq!(normalize_mcp_name(""), "unknown");
        assert_eq!(normalize_mcp_name("   "), "unknown");
        assert_eq!(normalize_mcp_name("!!!"), "unknown");
    }

    // ------------------------------------------------------------------
    // mcp_tool_name / parse_mcp_tool_name
    // ------------------------------------------------------------------

    #[test]
    fn test_mcp_tool_name() {
        assert_eq!(
            mcp_tool_name("filesystem", "read_file"),
            "mcp__filesystem__read_file"
        );
        assert_eq!(
            mcp_tool_name("My Server", "List Files!"),
            "mcp__my_server__list_files"
        );
    }

    #[test]
    fn test_parse_mcp_tool_name_valid() {
        assert_eq!(
            parse_mcp_tool_name("mcp__filesystem__read_file"),
            Some(("filesystem".to_string(), "read_file".to_string()))
        );
        assert_eq!(
            parse_mcp_tool_name("mcp__web-tools__fetch_url"),
            Some(("web-tools".to_string(), "fetch_url".to_string()))
        );
    }

    #[test]
    fn test_parse_mcp_tool_name_invalid() {
        assert_eq!(parse_mcp_tool_name("read_file"), None);
        assert_eq!(parse_mcp_tool_name("mcp__only_server"), None);
        assert_eq!(parse_mcp_tool_name("mcp____"), None);
        assert_eq!(parse_mcp_tool_name(""), None);
    }

    #[test]
    fn test_mcp_tool_name_roundtrip() {
        let namespaced = mcp_tool_name("filesystem", "read_file");
        let (srv, tool) = parse_mcp_tool_name(&namespaced).unwrap();
        assert_eq!(srv, "filesystem");
        assert_eq!(tool, "read_file");
    }

    // ------------------------------------------------------------------
    // McpContent / McpToolResult
    // ------------------------------------------------------------------

    #[test]
    fn test_mcp_content_variants() {
        let text = McpContent::text("hello");
        assert_eq!(text.as_text(), Some("hello"));

        let img = McpContent::image("base64data", "image/png");
        assert!(img.as_text().is_none());

        let res = McpContent::resource("file:///tmp/test.txt");
        assert!(res.as_text().is_none());
    }

    #[test]
    fn test_mcp_tool_result_constructors() {
        let ok = McpToolResult::text("mcp__fs__read", "file contents");
        assert!(!ok.is_error);
        assert_eq!(ok.content.len(), 1);
        assert_eq!(ok.content[0].as_text(), Some("file contents"));

        let err = McpToolResult::error("mcp__fs__read", "permission denied");
        assert!(err.is_error);
        assert_eq!(err.content[0].as_text(), Some("permission denied"));
    }

    // ------------------------------------------------------------------
    // McpToolDefinition
    // ------------------------------------------------------------------

    #[test]
    fn test_mcp_tool_definition_from_mcp_tool() {
        let mcp_tool = crate::types::MCPTool::new("read_file")
            .with_description("Reads a file from disk")
            .with_input_schema(serde_json::json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }));

        let def = McpToolDefinition::from_mcp_tool("filesystem", &mcp_tool);
        assert_eq!(def.namespaced_name, "mcp__filesystem__read_file");
        assert_eq!(def.server_name, "filesystem");
        assert_eq!(def.tool_name, "read_file");
        assert_eq!(def.description.as_deref(), Some("Reads a file from disk"));
    }

    // ------------------------------------------------------------------
    // McpClientRegistry (async)
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_registry_register_and_count_servers() {
        let reg = McpClientRegistry::new();
        assert_eq!(reg.server_count().await, 0);

        reg.register_server(McpServerConfig::stdio("fs-server", "npx"))
            .await;
        reg.register_server(McpServerConfig::http(
            "web-tools",
            "http://localhost:3000/mcp",
        ))
        .await;

        assert_eq!(reg.server_count().await, 2);
    }

    #[tokio::test]
    async fn test_registry_unregister_server_removes_tools() {
        let reg = McpClientRegistry::new();
        reg.register_server(McpServerConfig::stdio("fs-server", "npx"))
            .await;

        let tools = vec![crate::types::MCPTool::new("read_file")];
        reg.register_tools("fs-server", tools).await.unwrap();
        assert_eq!(reg.tool_count().await, 1);

        reg.unregister_server("fs-server").await;
        assert_eq!(reg.server_count().await, 0);
        assert_eq!(reg.tool_count().await, 0);
    }

    #[tokio::test]
    async fn test_registry_register_tools_unknown_server() {
        let reg = McpClientRegistry::new();
        let result = reg
            .register_tools("ghost-server", vec![crate::types::MCPTool::new("do_thing")])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_get_tool() {
        let reg = McpClientRegistry::new();
        reg.register_server(McpServerConfig::stdio("fs", "npx"))
            .await;
        reg.register_tools(
            "fs",
            vec![
                crate::types::MCPTool::new("read_file"),
                crate::types::MCPTool::new("write_file"),
            ],
        )
        .await
        .unwrap();

        let tool = reg.get_tool("mcp__fs__read_file").await;
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().tool_name, "read_file");

        let missing = reg.get_tool("mcp__fs__delete_everything").await;
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_registry_all_tools_and_tool_count() {
        let reg = McpClientRegistry::new();
        reg.register_server(McpServerConfig::stdio("fs", "npx"))
            .await;
        reg.register_server(McpServerConfig::http("web", "http://localhost/mcp"))
            .await;

        reg.register_tools(
            "fs",
            vec![
                crate::types::MCPTool::new("read_file"),
                crate::types::MCPTool::new("write_file"),
                crate::types::MCPTool::new("list_dir"),
            ],
        )
        .await
        .unwrap();

        reg.register_tools("web", vec![crate::types::MCPTool::new("fetch_url")])
            .await
            .unwrap();

        assert_eq!(reg.tool_count().await, 4);
        let all = reg.all_tools().await;
        assert_eq!(all.len(), 4);
    }

    #[tokio::test]
    async fn test_registry_servers_list() {
        let reg = McpClientRegistry::new();
        reg.register_server(McpServerConfig::stdio("alpha", "cmd"))
            .await;
        reg.register_server(McpServerConfig::stdio("beta", "cmd"))
            .await;
        let mut names = reg.servers().await;
        names.sort();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[tokio::test]
    async fn test_registry_server_name_normalised_on_register() {
        let reg = McpClientRegistry::new();
        reg.register_server(McpServerConfig::stdio("My Cool Server!", "npx"))
            .await;
        // normalised name used for lookup
        assert!(reg.server("my_cool_server").await.is_some());
        assert_eq!(reg.server_count().await, 1);
    }

    #[tokio::test]
    async fn test_registry_replace_tools_on_reregister() {
        let reg = McpClientRegistry::new();
        reg.register_server(McpServerConfig::stdio("srv", "npx"))
            .await;

        reg.register_tools("srv", vec![crate::types::MCPTool::new("tool_a")])
            .await
            .unwrap();
        assert_eq!(reg.tool_count().await, 1);

        // Re-register with different tools — old ones should be replaced.
        reg.register_tools(
            "srv",
            vec![
                crate::types::MCPTool::new("tool_b"),
                crate::types::MCPTool::new("tool_c"),
            ],
        )
        .await
        .unwrap();
        assert_eq!(reg.tool_count().await, 2);
        assert!(reg.get_tool("mcp__srv__tool_a").await.is_none());
        assert!(reg.get_tool("mcp__srv__tool_b").await.is_some());
    }
}
