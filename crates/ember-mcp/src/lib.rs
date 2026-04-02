//! Ember MCP - Model Context Protocol implementation.
//!
//! This crate provides a full implementation of the Model Context Protocol (MCP)
//! for Ember, enabling interoperability with other MCP-compatible tools and
//! AI assistants like Cline, Continue, and others.
//!
//! # Features
//!
//! - JSON-RPC based protocol implementation
//! - Tool registration and execution
//! - Resource provider support
//! - Full MCP specification compliance
//!
//! # Example
//!
//! ```rust,no_run
//! use ember_mcp::{MCPServer, MCPServerBuilder, MCPToolHandler, MCPTool, CallToolResult};
//! use async_trait::async_trait;
//! use std::collections::HashMap;
//! use serde_json::Value;
//!
//! struct EchoTool;
//!
//! #[async_trait]
//! impl MCPToolHandler for EchoTool {
//!     fn definition(&self) -> MCPTool {
//!         MCPTool::new("echo")
//!             .with_description("Echo back the input")
//!             .with_input_schema(serde_json::json!({
//!                 "type": "object",
//!                 "properties": {
//!                     "message": {"type": "string"}
//!                 },
//!                 "required": ["message"]
//!             }))
//!     }
//!
//!     async fn execute(&self, args: HashMap<String, Value>) -> ember_mcp::Result<CallToolResult> {
//!         let msg = args.get("message")
//!             .and_then(|v| v.as_str())
//!             .unwrap_or("no message");
//!         Ok(CallToolResult::text(msg))
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let server = MCPServerBuilder::new()
//!         .name("my-server")
//!         .tool(EchoTool)
//!         .build()
//!         .await;
//! }
//! ```

#![deny(missing_docs)]
#![deny(unsafe_code)]

pub mod client;
pub mod error;
pub mod server;
pub mod types;

// Re-export commonly used types
pub use error::{Error, Result};

// Client exports
pub use client::{
    // name helpers
    mcp_tool_name,
    normalize_mcp_name,
    parse_mcp_tool_name,
    // new multi-transport registry
    HttpTransport,
    // existing
    MCPClient,
    MCPClientConfig,
    MCPManager,
    MCPTransport,
    McpClientRegistry,
    McpContent,
    McpServerConfig,
    McpToolDefinition,
    McpToolResult,
    McpTransport,
    StdioTransport,
    WebSocketTransport,
};

// Server exports
pub use server::{
    MCPResourceProvider, MCPServer, MCPServerBuilder, MCPServerConfig, MCPToolHandler, ServerState,
};
pub use types::{
    CallToolParams, CallToolResult, ClientCapabilities, ClientInfo, InitializeParams,
    InitializeResult, JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse,
    MCPPrompt, MCPResource, MCPTool, PromptArgument, PromptCapabilities, ReadResourceParams,
    ReadResourceResult, RequestId, ResourceCapabilities, ResourceContent, ResourceReference,
    ServerCapabilities, ServerInfo, ToolCapabilities, ToolContent, JSONRPC_VERSION,
    PROTOCOL_VERSION,
};
