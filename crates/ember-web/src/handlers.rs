//! HTTP request handlers.
//!
//! This module contains the handler functions for all API endpoints.

use crate::error::{Result, WebError};
use crate::state::AppState;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::Json;
use ember_llm::{CompletionRequest as LLMCompletionRequest, Message};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::pin::Pin;
use tokio_stream::StreamExt;
use tracing::{error, info};

// =============================================================================
// Health & Info Endpoints
// =============================================================================

/// Health check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Status (always "ok" if responding).
    pub status: String,
    /// Server version.
    pub version: String,
    /// Uptime in seconds.
    pub uptime_seconds: i64,
}

/// Health check handler.
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: state.uptime().num_seconds(),
    })
}

/// Server info response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoResponse {
    /// Server name.
    pub name: String,
    /// Server version.
    pub version: String,
    /// Active conversations.
    pub active_conversations: usize,
    /// Server started timestamp.
    pub started_at: String,
    /// Available LLM provider.
    pub llm_provider: String,
    /// Default model.
    pub default_model: String,
}

/// Server info handler.
pub async fn info(State(state): State<AppState>) -> Json<InfoResponse> {
    Json(InfoResponse {
        name: "Ember AI Agent".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        active_conversations: state.conversation_count().await,
        started_at: state.started_at.to_rfc3339(),
        llm_provider: state.llm_provider.name().to_string(),
        default_model: state.default_model().to_string(),
    })
}

// =============================================================================
// Chat Endpoints
// =============================================================================

/// Chat request body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// User message.
    pub message: String,
    /// Conversation ID (optional, creates new if not provided).
    pub conversation_id: Option<String>,
    /// System prompt override.
    pub system_prompt: Option<String>,
    /// Model to use.
    pub model: Option<String>,
    /// Temperature (0.0 to 2.0).
    pub temperature: Option<f32>,
    /// Maximum tokens to generate.
    pub max_tokens: Option<u32>,
    /// Enable streaming response.
    pub stream: Option<bool>,
    /// Previous messages in conversation.
    pub messages: Option<Vec<MessageInput>>,
}

/// Input message format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageInput {
    /// Role (user, assistant, system).
    pub role: String,
    /// Message content.
    pub content: String,
}

/// Chat response body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// Unique response ID.
    pub id: String,
    /// Conversation ID.
    pub conversation_id: String,
    /// Assistant's response message.
    pub message: String,
    /// Model used.
    pub model: String,
    /// Token usage.
    pub usage: TokenUsage,
    /// Response timestamp.
    pub created_at: String,
}

/// Token usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Tokens in the prompt.
    pub prompt_tokens: u32,
    /// Tokens in the completion.
    pub completion_tokens: u32,
    /// Total tokens.
    pub total_tokens: u32,
}

/// Chat handler with real LLM integration.
pub async fn chat(
    State(state): State<AppState>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>> {
    // Validate request
    if request.message.trim().is_empty() {
        return Err(WebError::Validation("Message cannot be empty".to_string()));
    }

    if let Some(temp) = request.temperature {
        if !(0.0..=2.0).contains(&temp) {
            return Err(WebError::Validation(
                "Temperature must be between 0.0 and 2.0".to_string(),
            ));
        }
    }

    state.increment_conversations().await;

    // Generate response ID and conversation ID
    let response_id = uuid::Uuid::new_v4().to_string();
    let conversation_id = request
        .conversation_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let model = request
        .model
        .clone()
        .unwrap_or_else(|| state.default_model().to_string());

    info!(
        conversation_id = %conversation_id,
        model = %model,
        message_length = request.message.len(),
        "Processing chat request"
    );

    // Build LLM completion request
    let mut llm_request = LLMCompletionRequest::new(&model);

    // Add system prompt if provided
    if let Some(system) = &request.system_prompt {
        llm_request = llm_request.with_message(Message::system(system));
    }

    // Add previous messages if provided
    if let Some(prev_messages) = &request.messages {
        for msg in prev_messages {
            let message = match msg.role.to_lowercase().as_str() {
                "user" => Message::user(&msg.content),
                "assistant" => Message::assistant(&msg.content),
                "system" => Message::system(&msg.content),
                _ => Message::user(&msg.content),
            };
            llm_request = llm_request.with_message(message);
        }
    }

    // Add the current user message
    llm_request = llm_request.with_message(Message::user(&request.message));

    // Set optional parameters
    if let Some(temp) = request.temperature {
        llm_request = llm_request.with_temperature(temp);
    }
    if let Some(max_tokens) = request.max_tokens {
        llm_request = llm_request.with_max_tokens(max_tokens);
    }

    // Call LLM provider
    let llm_response = state
        .llm_provider
        .complete(llm_request)
        .await
        .map_err(|e| {
            error!(error = %e, "LLM request failed");
            WebError::Internal(format!("LLM request failed: {}", e))
        })?;

    let assistant_message = llm_response.content;

    // Use actual token usage from response
    let usage = llm_response.usage;

    let response = ChatResponse {
        id: response_id,
        conversation_id,
        message: assistant_message,
        model,
        usage: TokenUsage {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        },
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    state.decrement_conversations().await;

    Ok(Json(response))
}

/// SSE streaming chat event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    /// Event type (start, chunk, end, error).
    pub event: String,
    /// Content chunk (for chunk events).
    pub content: Option<String>,
    /// Model used (for start events).
    pub model: Option<String>,
    /// Conversation ID (for start events).
    pub conversation_id: Option<String>,
    /// Error message (for error events).
    pub error: Option<String>,
}

/// SSE streaming chat handler.
pub async fn chat_stream(
    State(state): State<AppState>,
    Json(request): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = std::result::Result<Event, Infallible>>>> {
    // Validate request
    if request.message.trim().is_empty() {
        return Err(WebError::Validation("Message cannot be empty".to_string()));
    }

    let conversation_id = request
        .conversation_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let model = request
        .model
        .clone()
        .unwrap_or_else(|| state.default_model().to_string());

    info!(
        conversation_id = %conversation_id,
        model = %model,
        "Starting streaming chat"
    );

    // Build LLM completion request
    let mut llm_request = LLMCompletionRequest::new(&model);

    if let Some(system) = &request.system_prompt {
        llm_request = llm_request.with_message(Message::system(system));
    }

    if let Some(prev_messages) = &request.messages {
        for msg in prev_messages {
            let message = match msg.role.to_lowercase().as_str() {
                "user" => Message::user(&msg.content),
                "assistant" => Message::assistant(&msg.content),
                "system" => Message::system(&msg.content),
                _ => Message::user(&msg.content),
            };
            llm_request = llm_request.with_message(message);
        }
    }

    llm_request = llm_request.with_message(Message::user(&request.message));

    if let Some(temp) = request.temperature {
        llm_request = llm_request.with_temperature(temp);
    }
    if let Some(max_tokens) = request.max_tokens {
        llm_request = llm_request.with_max_tokens(max_tokens);
    }

    llm_request = llm_request.with_streaming(true);

    // Get the streaming response from LLM
    let provider = state.llm_provider.clone();
    let conv_id = conversation_id.clone();
    let model_name = model.clone();

    let stream = async_stream::stream! {
        // Send start event
        let start_event = StreamEvent {
            event: "start".to_string(),
            content: None,
            model: Some(model_name.clone()),
            conversation_id: Some(conv_id.clone()),
            error: None,
        };
        yield Ok(Event::default().data(serde_json::to_string(&start_event).unwrap_or_default()));

        // Stream from LLM
        match provider.complete_stream(llm_request).await {
            Ok(mut llm_stream) => {
                while let Some(chunk_result) = llm_stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            // Only send chunk event if there's content
                            if let Some(content) = chunk.content {
                                let chunk_event = StreamEvent {
                                    event: "chunk".to_string(),
                                    content: Some(content),
                                    model: None,
                                    conversation_id: None,
                                    error: None,
                                };
                                yield Ok(Event::default().data(serde_json::to_string(&chunk_event).unwrap_or_default()));
                            }
                        }
                        Err(e) => {
                            let error_event = StreamEvent {
                                event: "error".to_string(),
                                content: None,
                                model: None,
                                conversation_id: None,
                                error: Some(e.to_string()),
                            };
                            yield Ok(Event::default().data(serde_json::to_string(&error_event).unwrap_or_default()));
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                let error_event = StreamEvent {
                    event: "error".to_string(),
                    content: None,
                    model: None,
                    conversation_id: None,
                    error: Some(e.to_string()),
                };
                yield Ok(Event::default().data(serde_json::to_string(&error_event).unwrap_or_default()));
            }
        }

        // Send end event
        let end_event = StreamEvent {
            event: "end".to_string(),
            content: None,
            model: None,
            conversation_id: None,
            error: None,
        };
        yield Ok(Event::default().data(serde_json::to_string(&end_event).unwrap_or_default()));
    };

    Ok(Sse::new(Box::pin(stream)
        as Pin<
            Box<dyn Stream<Item = std::result::Result<Event, Infallible>> + Send>,
        >))
}

// =============================================================================
// Conversation Endpoints
// =============================================================================

/// List conversations response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationsResponse {
    /// List of conversations.
    pub conversations: Vec<ConversationSummary>,
    /// Total count.
    pub total: usize,
}

/// Conversation summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    /// Conversation ID.
    pub id: String,
    /// Title.
    pub title: Option<String>,
    /// Message count.
    pub message_count: usize,
    /// Created timestamp.
    pub created_at: String,
    /// Last updated timestamp.
    pub updated_at: String,
}

/// List conversations handler.
pub async fn list_conversations() -> Json<ConversationsResponse> {
    // TODO: Integrate with ember-storage
    Json(ConversationsResponse {
        conversations: vec![],
        total: 0,
    })
}

/// Get conversation handler.
pub async fn get_conversation(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<ConversationSummary>> {
    // TODO: Integrate with ember-storage
    Err(WebError::NotFound(format!("Conversation {} not found", id)))
}

/// Delete conversation handler.
pub async fn delete_conversation(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>> {
    // TODO: Integrate with ember-storage
    info!(conversation_id = %id, "Deleting conversation");
    Ok(Json(serde_json::json!({
        "deleted": true,
        "id": id
    })))
}

// =============================================================================
// Model Endpoints
// =============================================================================

/// Available models response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsResponse {
    /// List of available models.
    pub models: Vec<ModelInfo>,
}

/// Model information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Provider (openai, ollama, etc.).
    pub provider: String,
    /// Maximum context length.
    pub context_length: u32,
}

/// List models handler.
pub async fn list_models(State(state): State<AppState>) -> Json<ModelsResponse> {
    let provider_name = state.llm_provider.name();

    // Return models based on provider
    let models = match provider_name {
        "openai" => vec![
            ModelInfo {
                id: "gpt-4".to_string(),
                name: "GPT-4".to_string(),
                provider: "openai".to_string(),
                context_length: 8192,
            },
            ModelInfo {
                id: "gpt-4-turbo".to_string(),
                name: "GPT-4 Turbo".to_string(),
                provider: "openai".to_string(),
                context_length: 128000,
            },
            ModelInfo {
                id: "gpt-4o".to_string(),
                name: "GPT-4o".to_string(),
                provider: "openai".to_string(),
                context_length: 128000,
            },
            ModelInfo {
                id: "gpt-3.5-turbo".to_string(),
                name: "GPT-3.5 Turbo".to_string(),
                provider: "openai".to_string(),
                context_length: 16385,
            },
        ],
        "ollama" => vec![
            ModelInfo {
                id: "llama3".to_string(),
                name: "Llama 3".to_string(),
                provider: "ollama".to_string(),
                context_length: 8192,
            },
            ModelInfo {
                id: "llama3.1".to_string(),
                name: "Llama 3.1".to_string(),
                provider: "ollama".to_string(),
                context_length: 128000,
            },
            ModelInfo {
                id: "codellama".to_string(),
                name: "Code Llama".to_string(),
                provider: "ollama".to_string(),
                context_length: 16384,
            },
            ModelInfo {
                id: "mistral".to_string(),
                name: "Mistral".to_string(),
                provider: "ollama".to_string(),
                context_length: 8192,
            },
            ModelInfo {
                id: "mixtral".to_string(),
                name: "Mixtral".to_string(),
                provider: "ollama".to_string(),
                context_length: 32768,
            },
        ],
        _ => vec![],
    };

    Json(ModelsResponse { models })
}

// =============================================================================
// Tools Endpoints
// =============================================================================

/// Available tools response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsResponse {
    /// List of available tools.
    pub tools: Vec<ToolInfo>,
}

/// Tool information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    /// Tool identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Whether the tool is enabled.
    pub enabled: bool,
}

/// List tools handler.
pub async fn list_tools() -> Json<ToolsResponse> {
    Json(ToolsResponse {
        tools: vec![
            ToolInfo {
                id: "shell".to_string(),
                name: "Shell".to_string(),
                description: "Execute shell commands".to_string(),
                enabled: true,
            },
            ToolInfo {
                id: "filesystem".to_string(),
                name: "Filesystem".to_string(),
                description: "Read and write files".to_string(),
                enabled: true,
            },
            ToolInfo {
                id: "web".to_string(),
                name: "Web".to_string(),
                description: "Make HTTP requests".to_string(),
                enabled: true,
            },
            ToolInfo {
                id: "browser".to_string(),
                name: "Browser".to_string(),
                description: "Control a web browser for automation".to_string(),
                enabled: true,
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_request_serialization() {
        let request = ChatRequest {
            message: "Hello".to_string(),
            conversation_id: None,
            system_prompt: None,
            model: Some("gpt-4".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(1000),
            stream: None,
            messages: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("Hello"));
        assert!(json.contains("gpt-4"));
    }

    #[test]
    fn test_chat_response_serialization() {
        let response = ChatResponse {
            id: "123".to_string(),
            conversation_id: "conv-456".to_string(),
            message: "Hello there!".to_string(),
            model: "gpt-4".to_string(),
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            },
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        let parsed: ChatResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "123");
    }

    #[test]
    fn test_stream_event_serialization() {
        let event = StreamEvent {
            event: "chunk".to_string(),
            content: Some("Hello".to_string()),
            model: None,
            conversation_id: None,
            error: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("chunk"));
        assert!(json.contains("Hello"));
    }
}
