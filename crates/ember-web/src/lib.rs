//! Web server and API for the Ember AI agent framework.
//!
//! This crate provides a REST API and web interface for interacting with
//! Ember agents. It uses Axum for the web framework and supports CORS,
//! WebSockets for streaming, and API key authentication.
//!
//! # Example
//!
//! ```rust,no_run
//! use ember_web::{Server, ServerConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create server with default configuration
//!     let config = ServerConfig::new("0.0.0.0", 3000);
//!     let server = Server::new(config);
//!     
//!     // Start the server
//!     server.run().await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! # API Endpoints
//!
//! ## Health & Info
//! - `GET /api/v1/health` - Health check
//! - `GET /api/v1/info` - Server information
//!
//! ## Chat
//! - `POST /api/v1/chat` - Send a message and get a response
//! - `POST /api/v1/chat/stream` - Send a message and get a streaming SSE response
//!
//! ## Conversations
//! - `GET /api/v1/conversations` - List all conversations
//! - `GET /api/v1/conversations/:id` - Get a specific conversation
//! - `DELETE /api/v1/conversations/:id` - Delete a conversation
//!
//! ## Models
//! - `GET /api/v1/models` - List available models
//!
//! ## Tools
//! - `GET /api/v1/tools` - List available tools
//!
//! ## WebSocket
//! - `WS /api/v1/ws` - WebSocket connection for real-time streaming
//! - `GET /api/v1/streams` - Get active stream information

#![deny(missing_docs)]
#![deny(unsafe_code)]

pub mod error;
pub mod handlers;
pub mod routes;
pub mod state;
pub mod websocket;

// Re-exports
pub use error::{ErrorResponse, Result, WebError};
pub use handlers::{
    ChatRequest, ChatResponse, ConversationSummary, ConversationsResponse, HealthResponse,
    InfoResponse, MessageInput, ModelInfo, ModelsResponse, StreamEvent, TokenUsage, ToolInfo,
    ToolsResponse,
};
pub use routes::{create_router, create_router_with_static, paths, API_PREFIX};
pub use state::{AppState, LLMProviderType, ServerConfig};
pub use websocket::{ClientMessage, ServerMessage, StreamManager, StreamsInfoResponse};

use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

/// The Ember web server.
pub struct Server {
    /// Server configuration.
    config: ServerConfig,
}

impl Server {
    /// Create a new server instance.
    ///
    /// # Arguments
    ///
    /// * `config` - Server configuration
    pub fn new(config: ServerConfig) -> Self {
        Self { config }
    }

    /// Run the server.
    ///
    /// This will bind to the configured address and start accepting connections.
    ///
    /// # Errors
    ///
    /// Returns an error if the server cannot bind to the address.
    pub async fn run(self) -> std::io::Result<()> {
        let state = AppState::new(self.config.clone());
        let app = create_router(state);

        let addr: SocketAddr = self
            .config
            .address()
            .parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

        info!(address = %addr, "Starting Ember web server");

        let listener = TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }

    /// Run the server with graceful shutdown.
    ///
    /// # Arguments
    ///
    /// * `shutdown_signal` - A future that completes when shutdown is requested
    ///
    /// # Errors
    ///
    /// Returns an error if the server cannot bind to the address.
    pub async fn run_with_shutdown<F>(self, shutdown_signal: F) -> std::io::Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let state = AppState::new(self.config.clone());
        let app = create_router(state);

        let addr: SocketAddr = self
            .config
            .address()
            .parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

        info!(address = %addr, "Starting Ember web server with graceful shutdown");

        let listener = TcpListener::bind(addr).await?;
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await?;

        info!("Server shutdown complete");
        Ok(())
    }
}

/// Prelude module for convenient imports.
pub mod prelude {
    pub use crate::error::{ErrorResponse, Result, WebError};
    pub use crate::handlers::{
        ChatRequest, ChatResponse, HealthResponse, InfoResponse, ModelsResponse, StreamEvent,
        ToolsResponse,
    };
    pub use crate::routes::{create_router, create_router_with_static};
    pub use crate::state::{AppState, LLMProviderType, ServerConfig};
    pub use crate::Server;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_creation() {
        let config = ServerConfig::default();
        let _server = Server::new(config);
    }

    #[test]
    fn test_server_config_address() {
        let config = ServerConfig::new("0.0.0.0", 8080);
        let server = Server::new(config);
        assert_eq!(server.config.address(), "0.0.0.0:8080");
    }
}
