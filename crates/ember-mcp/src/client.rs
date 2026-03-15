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

#[cfg(test)]
mod tests {
    use super::*;

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
}
