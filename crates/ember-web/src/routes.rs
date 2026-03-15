//! Route definitions for the web server.
//!
//! This module defines all API routes and creates the Axum router.

use crate::handlers;
use crate::state::AppState;
use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

/// Create the main application router.
///
/// # Arguments
///
/// * `state` - Shared application state
///
/// # Returns
///
/// Configured Axum router with all routes.
pub fn create_router(state: AppState) -> Router {
    let api_routes = Router::new()
        // Health & Info
        .route("/health", get(handlers::health))
        .route("/info", get(handlers::info))
        // Chat (non-streaming)
        .route("/chat", post(handlers::chat))
        // Chat (SSE streaming)
        .route("/chat/stream", post(handlers::chat_stream))
        // Conversations
        .route("/conversations", get(handlers::list_conversations))
        .route("/conversations/:id", get(handlers::get_conversation))
        .route("/conversations/:id", delete(handlers::delete_conversation))
        // Models
        .route("/models", get(handlers::list_models))
        // Tools
        .route("/tools", get(handlers::list_tools));

    let mut router = Router::new()
        .nest("/api/v1", api_routes)
        .with_state(state.clone());

    // Add CORS if enabled
    if state.config.cors_enabled {
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);
        router = router.layer(cors);
    }

    // Add request tracing
    router = router.layer(TraceLayer::new_for_http());

    router
}

/// Create the main router with static file serving.
///
/// This version serves static files from a directory for the web UI frontend.
///
/// # Arguments
///
/// * `state` - Shared application state
/// * `static_dir` - Path to the static files directory
///
/// # Returns
///
/// Configured Axum router with API routes and static file serving.
pub fn create_router_with_static(state: AppState, static_dir: &str) -> Router {
    let api_routes = Router::new()
        // Health & Info
        .route("/health", get(handlers::health))
        .route("/info", get(handlers::info))
        // Chat (non-streaming)
        .route("/chat", post(handlers::chat))
        // Chat (SSE streaming)
        .route("/chat/stream", post(handlers::chat_stream))
        // Conversations
        .route("/conversations", get(handlers::list_conversations))
        .route("/conversations/:id", get(handlers::get_conversation))
        .route("/conversations/:id", delete(handlers::delete_conversation))
        // Models
        .route("/models", get(handlers::list_models))
        // Tools
        .route("/tools", get(handlers::list_tools));

    // Static file service for frontend
    let serve_dir = ServeDir::new(static_dir);

    let mut router = Router::new()
        .nest("/api/v1", api_routes)
        .fallback_service(serve_dir)
        .with_state(state.clone());

    // Add CORS if enabled
    if state.config.cors_enabled {
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);
        router = router.layer(cors);
    }

    // Add request tracing
    router = router.layer(TraceLayer::new_for_http());

    router
}

/// API version prefix.
pub const API_PREFIX: &str = "/api/v1";

/// API endpoint paths.
pub mod paths {
    /// Health check endpoint.
    pub const HEALTH: &str = "/health";
    /// Server info endpoint.
    pub const INFO: &str = "/info";
    /// Chat endpoint (non-streaming).
    pub const CHAT: &str = "/chat";
    /// Chat streaming endpoint (SSE).
    pub const CHAT_STREAM: &str = "/chat/stream";
    /// Conversations list endpoint.
    pub const CONVERSATIONS: &str = "/conversations";
    /// Single conversation endpoint (with :id parameter).
    pub const CONVERSATION: &str = "/conversations/:id";
    /// Models list endpoint.
    pub const MODELS: &str = "/models";
    /// Tools list endpoint.
    pub const TOOLS: &str = "/tools";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ServerConfig;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::util::ServiceExt;

    fn create_test_app() -> Router {
        let config = ServerConfig::default();
        let state = AppState::new(config);
        create_router(state)
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_info_endpoint() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_models_endpoint() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_tools_endpoint() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/tools")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_not_found() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
