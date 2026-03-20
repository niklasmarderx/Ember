//! Web Server Integration with Ember
//!
//! This example demonstrates how to build a simple web API server
//! that uses Ember as the AI backend. It shows:
//! - Setting up an Axum web server
//! - Creating REST endpoints for chat
//! - Handling streaming responses via Server-Sent Events (SSE)
//! - Managing conversation state
//!
//! Run with: `cargo run --example web_server_integration`
//! Then test with: `curl -X POST http://localhost:3000/chat -H "Content-Type: application/json" -d '{"message":"Hello"}'`

use axum::{
    extract::{Json, State},
    response::{sse::Event, Sse},
    routing::{get, post},
    Router,
};
use ember_core::{Agent, AgentBuilder, Conversation, Message};
use ember_llm::{OllamaProvider, OpenAIProvider, Provider};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::Infallible,
    sync::Arc,
    time::Duration,
};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

// =============================================================================
// Request/Response Types
// =============================================================================

/// Chat request payload
#[derive(Debug, Deserialize)]
struct ChatRequest {
    /// The user's message
    message: String,
    /// Optional conversation ID for multi-turn conversations
    conversation_id: Option<String>,
    /// Optional model override
    model: Option<String>,
}

/// Chat response payload
#[derive(Debug, Serialize)]
struct ChatResponse {
    /// The AI's response
    response: String,
    /// Conversation ID for follow-up messages
    conversation_id: String,
    /// Number of tokens used
    tokens_used: Option<u32>,
    /// Model that was used
    model: String,
}

/// Health check response
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    providers: Vec<String>,
}

/// Error response
#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    code: String,
}

// =============================================================================
// Application State
// =============================================================================

/// Shared application state
struct AppState {
    /// The AI agent
    agent: Agent,
    /// Active conversations (in production, use Redis or a database)
    conversations: RwLock<HashMap<String, Conversation>>,
    /// Default model to use
    default_model: String,
}

impl AppState {
    fn new(agent: Agent, default_model: String) -> Self {
        Self {
            agent,
            conversations: RwLock::new(HashMap::new()),
            default_model,
        }
    }

    /// Get or create a conversation
    async fn get_or_create_conversation(&self, id: Option<String>) -> (String, Conversation) {
        let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let mut conversations = self.conversations.write().await;

        if let Some(conv) = conversations.get(&id) {
            (id, conv.clone())
        } else {
            let conv = Conversation::new();
            conversations.insert(id.clone(), conv.clone());
            (id, conv)
        }
    }

    /// Update a conversation
    async fn update_conversation(&self, id: &str, conversation: Conversation) {
        let mut conversations = self.conversations.write().await;
        conversations.insert(id.to_string(), conversation);
    }
}

// =============================================================================
// Route Handlers
// =============================================================================

/// Health check endpoint
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        providers: vec![
            "openai".to_string(),
            "ollama".to_string(),
            "anthropic".to_string(),
        ],
    })
}

/// Chat endpoint - synchronous response
async fn chat(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, Json<ErrorResponse>> {
    // Get or create conversation
    let (conversation_id, mut conversation) =
        state.get_or_create_conversation(request.conversation_id).await;

    // Add user message to conversation
    conversation.add_message(Message::user(&request.message));

    // Get model to use
    let model = request.model.unwrap_or_else(|| state.default_model.clone());

    // Generate response
    let response = match state.agent.chat(&conversation, &model).await {
        Ok(resp) => resp,
        Err(e) => {
            return Err(Json(ErrorResponse {
                error: e.to_string(),
                code: "CHAT_ERROR".to_string(),
            }));
        }
    };

    // Add assistant response to conversation
    conversation.add_message(Message::assistant(&response.content));

    // Save updated conversation
    state.update_conversation(&conversation_id, conversation).await;

    Ok(Json(ChatResponse {
        response: response.content,
        conversation_id,
        tokens_used: response.usage.map(|u| u.total_tokens),
        model,
    }))
}

/// Streaming chat endpoint using Server-Sent Events
async fn chat_stream(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (conversation_id, mut conversation) =
        state.get_or_create_conversation(request.conversation_id).await;

    conversation.add_message(Message::user(&request.message));

    let model = request.model.unwrap_or_else(|| state.default_model.clone());

    // Create a stream that yields SSE events
    let stream = async_stream::stream! {
        // Send conversation ID first
        yield Ok(Event::default()
            .event("conversation_id")
            .data(conversation_id.clone()));

        // Stream the response chunks
        match state.agent.chat_stream(&conversation, &model).await {
            Ok(mut response_stream) => {
                let mut full_response = String::new();

                while let Some(chunk) = response_stream.next().await {
                    match chunk {
                        Ok(text) => {
                            full_response.push_str(&text);
                            yield Ok(Event::default()
                                .event("chunk")
                                .data(text));
                        }
                        Err(e) => {
                            yield Ok(Event::default()
                                .event("error")
                                .data(e.to_string()));
                            break;
                        }
                    }
                }

                // Update conversation with full response
                conversation.add_message(Message::assistant(&full_response));
                state.update_conversation(&conversation_id, conversation).await;

                // Send completion event
                yield Ok(Event::default()
                    .event("done")
                    .data(""));
            }
            Err(e) => {
                yield Ok(Event::default()
                    .event("error")
                    .data(e.to_string()));
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// List available models
async fn list_models(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<String>> {
    // In a real implementation, this would query the provider
    Json(vec![
        "gpt-4".to_string(),
        "gpt-3.5-turbo".to_string(),
        "llama2".to_string(),
        "mistral".to_string(),
    ])
}

/// Delete a conversation
async fn delete_conversation(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, Json<ErrorResponse>> {
    let mut conversations = state.conversations.write().await;

    if conversations.remove(&id).is_some() {
        Ok(Json(serde_json::json!({
            "deleted": true,
            "conversation_id": id
        })))
    } else {
        Err(Json(ErrorResponse {
            error: "Conversation not found".to_string(),
            code: "NOT_FOUND".to_string(),
        }))
    }
}

// =============================================================================
// Main Function
// =============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        )
        .init();

    println!("=== Ember Web Server Example ===\n");

    // Create the AI agent
    // In production, you would configure this based on environment variables
    let provider: Box<dyn Provider> = if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
        println!("Using OpenAI provider");
        Box::new(OpenAIProvider::new(&api_key))
    } else {
        println!("Using Ollama provider (fallback)");
        println!("Tip: Set OPENAI_API_KEY for OpenAI support\n");
        Box::new(OllamaProvider::new("http://localhost:11434"))
    };

    let agent = AgentBuilder::new()
        .provider(provider)
        .build()?;

    // Create application state
    let state = Arc::new(AppState::new(agent, "gpt-3.5-turbo".to_string()));

    // Configure CORS for development
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build the router
    let app = Router::new()
        // Health check
        .route("/health", get(health_check))
        // Chat endpoints
        .route("/chat", post(chat))
        .route("/chat/stream", post(chat_stream))
        // Model management
        .route("/models", get(list_models))
        // Conversation management
        .route("/conversations/:id", axum::routing::delete(delete_conversation))
        // Add middleware
        .layer(cors)
        // Add shared state
        .with_state(state);

    // Start the server
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Server listening on http://{}", addr);
    println!("\nAvailable endpoints:");
    println!("  GET  /health              - Health check");
    println!("  POST /chat                - Send a chat message");
    println!("  POST /chat/stream         - Send a chat message (streaming)");
    println!("  GET  /models              - List available models");
    println!("  DELETE /conversations/:id - Delete a conversation");
    println!("\nExample usage:");
    println!(r#"  curl -X POST http://localhost:3000/chat \"#);
    println!(r#"    -H "Content-Type: application/json" \"#);
    println!(r#"    -d '{{"message": "Hello, how are you?"}}'"#);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_health_endpoint() {
        // Create a mock agent for testing
        let agent = AgentBuilder::new()
            .provider(Box::new(OllamaProvider::new("http://localhost:11434")))
            .build()
            .unwrap();

        let state = Arc::new(AppState::new(agent, "test-model".to_string()));

        let app = Router::new()
            .route("/health", get(health_check))
            .with_state(state);

        let response = app
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_conversation_management() {
        let agent = AgentBuilder::new()
            .provider(Box::new(OllamaProvider::new("http://localhost:11434")))
            .build()
            .unwrap();

        let state = Arc::new(AppState::new(agent, "test-model".to_string()));

        // Create a conversation
        let (id, _) = state.get_or_create_conversation(None).await;
        assert!(!id.is_empty());

        // Verify it exists
        let (same_id, _) = state.get_or_create_conversation(Some(id.clone())).await;
        assert_eq!(id, same_id);
    }
}