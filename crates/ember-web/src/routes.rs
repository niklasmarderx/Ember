//! Route definitions for the web server.
//!
//! This module defines all API routes and creates the Axum router.

use crate::handlers;
use crate::state::AppState;
use crate::websocket::{get_streams_info, websocket_handler};
use axum::body::Body;
use axum::http::{header, Response, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::Router;
use rust_embed::Embed;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

/// Embedded static files from the static directory.
#[derive(Embed)]
#[folder = "static/"]
struct StaticAssets;

/// Handler for serving embedded static files.
async fn serve_embedded(path: axum::extract::Path<String>) -> impl IntoResponse {
    let path = path.0;
    serve_embedded_file(&path)
}

/// Handler for serving the index.html file.
async fn serve_index() -> impl IntoResponse {
    serve_embedded_file("index.html")
}

/// Serve an embedded file by path.
fn serve_embedded_file(path: &str) -> Response<Body> {
    match StaticAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data.to_vec()))
                .unwrap()
        }
        None => {
            // For SPA routing, serve index.html for non-API, non-asset paths
            if !path.starts_with("api/") && !path.contains('.') {
                if let Some(content) = StaticAssets::get("index.html") {
                    return Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "text/html")
                        .body(Body::from(content.data.to_vec()))
                        .unwrap();
                }
            }
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("Not Found"))
                .unwrap()
        }
    }
}

/// Create the API routes without any static file serving.
fn create_api_routes() -> Router<AppState> {
    Router::new()
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
        .route("/models/extended", get(handlers::list_models_extended))
        // Tools
        .route("/tools", get(handlers::list_tools))
        // Cost & Usage
        .route("/costs/estimate", post(handlers::estimate_cost))
        .route("/usage", get(handlers::get_usage_stats))
        .route("/budget", get(handlers::get_budget_config))
        .route("/budget", put(handlers::update_budget_config))
        .route("/recommendations", get(handlers::get_recommendations))
        // WebSocket & Streaming
        .route("/ws", get(websocket_handler))
        .route("/streams", get(get_streams_info))
}

/// Create the main application router with embedded static files.
///
/// This is the default router that serves the embedded frontend UI.
///
/// # Arguments
///
/// * `state` - Shared application state
///
/// # Returns
///
/// Configured Axum router with all routes and embedded static file serving.
pub fn create_router(state: AppState) -> Router {
    let api_routes = create_api_routes();

    let mut router = Router::new()
        .nest("/api/v1", api_routes)
        // Serve embedded static files
        .route("/", get(serve_index))
        .route("/*path", get(serve_embedded))
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

/// Create the main application router without static file serving.
///
/// Use this for API-only deployments or when running the frontend separately.
///
/// # Arguments
///
/// * `state` - Shared application state
///
/// # Returns
///
/// Configured Axum router with API routes only.
pub fn create_router_api_only(state: AppState) -> Router {
    let api_routes = create_api_routes();

    // Root-level health/readiness endpoints for Kubernetes probes and load balancers
    let health_routes = Router::new()
        .route("/health", get(handlers::health))
        .route("/ready", get(handlers::ready));

    let mut router = Router::new()
        .merge(health_routes)
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
        .route("/models/extended", get(handlers::list_models_extended))
        // Tools
        .route("/tools", get(handlers::list_tools))
        // Cost & Usage
        .route("/costs/estimate", post(handlers::estimate_cost))
        .route("/usage", get(handlers::get_usage_stats))
        .route("/budget", get(handlers::get_budget_config))
        .route("/budget", put(handlers::update_budget_config))
        .route("/recommendations", get(handlers::get_recommendations))
        // WebSocket & Streaming
        .route("/ws", get(websocket_handler))
        .route("/streams", get(get_streams_info));

    // Root-level health/readiness endpoints for Kubernetes probes and load balancers
    let health_routes = Router::new()
        .route("/health", get(handlers::health))
        .route("/ready", get(handlers::ready));

    // Static file service for frontend
    let serve_dir = ServeDir::new(static_dir);

    let mut router = Router::new()
        .merge(health_routes)
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
    /// Health check endpoint (root-level, for Kubernetes liveness probes).
    pub const HEALTH: &str = "/health";
    /// Readiness check endpoint (root-level, for Kubernetes readiness probes).
    pub const READY: &str = "/ready";
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
    /// Extended models list endpoint.
    pub const MODELS_EXTENDED: &str = "/models/extended";
    /// Tools list endpoint.
    pub const TOOLS: &str = "/tools";
    /// Cost estimation endpoint.
    pub const COSTS_ESTIMATE: &str = "/costs/estimate";
    /// Usage statistics endpoint.
    pub const USAGE: &str = "/usage";
    /// Budget configuration endpoint.
    pub const BUDGET: &str = "/budget";
    /// Recommendations endpoint.
    pub const RECOMMENDATIONS: &str = "/recommendations";
    /// WebSocket endpoint for real-time streaming.
    pub const WS: &str = "/ws";
    /// Active streams info endpoint.
    pub const STREAMS: &str = "/streams";
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
    async fn test_root_health_endpoint() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_root_ready_endpoint() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/ready")
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
