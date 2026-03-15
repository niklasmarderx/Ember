//! HTTP request handlers.
//!
//! This module contains the handler functions for all API endpoints.

use crate::error::{Result, WebError};
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::Json;
use ember_llm::model_registry::MODEL_REGISTRY;
use ember_llm::{CompletionRequest as LLMCompletionRequest, Message};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    /// Status (always "healthy" if responding).
    pub status: String,
    /// Server version.
    pub version: String,
    /// Uptime in seconds.
    pub uptime_seconds: i64,
}

/// Health check handler.
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: state.uptime().num_seconds(),
    })
}

/// Readiness check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessResponse {
    /// Status ("ready" or "not_ready").
    pub status: String,
    /// Individual dependency check results.
    pub checks: HashMap<String, String>,
}

/// Readiness check handler.
///
/// Verifies that all dependencies are available. Returns 200 if ready,
/// 503 Service Unavailable if any dependency check fails.
pub async fn ready(
    State(state): State<AppState>,
) -> std::result::Result<Json<ReadinessResponse>, (StatusCode, Json<ReadinessResponse>)> {
    let mut checks = HashMap::new();
    let mut all_ok = true;

    // Check LLM provider
    let provider_name = state.llm_provider.name();
    if provider_name.is_empty() {
        all_ok = false;
        checks.insert(
            "llm_provider".to_string(),
            "error: no provider configured".to_string(),
        );
    } else {
        checks.insert("llm_provider".to_string(), "ok".to_string());
    }

    // Database check (placeholder — not yet integrated)
    checks.insert("database".to_string(), "ok".to_string());

    // Storage check (placeholder — not yet integrated)
    checks.insert("storage".to_string(), "ok".to_string());

    let response = ReadinessResponse {
        status: if all_ok { "ready" } else { "not_ready" }.to_string(),
        checks,
    };

    if all_ok {
        Ok(Json(response))
    } else {
        Err((StatusCode::SERVICE_UNAVAILABLE, Json(response)))
    }
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

// =============================================================================
// Cost & Usage Endpoints
// =============================================================================

/// Cost estimation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimateRequest {
    /// Model to estimate cost for.
    pub model: String,
    /// Estimated input tokens.
    pub input_tokens: u32,
    /// Estimated output tokens.
    pub output_tokens: u32,
}

/// Cost estimation response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimateResponse {
    /// Model ID.
    pub model_id: String,
    /// Input tokens.
    pub input_tokens: u32,
    /// Output tokens.
    pub output_tokens: u32,
    /// Input cost in USD.
    pub input_cost: f64,
    /// Output cost in USD.
    pub output_cost: f64,
    /// Total cost in USD.
    pub total_cost: f64,
    /// Price per 1K input tokens.
    pub input_price_per_1k: f64,
    /// Price per 1K output tokens.
    pub output_price_per_1k: f64,
}

/// Estimate cost handler.
pub async fn estimate_cost(
    State(state): State<AppState>,
    Json(request): Json<CostEstimateRequest>,
) -> Result<Json<CostEstimateResponse>> {
    let estimate = state
        .cost_predictor
        .estimate(&request.model, request.input_tokens, request.output_tokens)
        .ok_or_else(|| WebError::NotFound(format!("Model {} not found", request.model)))?;

    Ok(Json(CostEstimateResponse {
        model_id: estimate.model_id,
        input_tokens: estimate.input_tokens,
        output_tokens: estimate.output_tokens,
        input_cost: estimate.input_cost,
        output_cost: estimate.output_cost,
        total_cost: estimate.total_cost,
        input_price_per_1k: estimate.input_price_per_1k,
        output_price_per_1k: estimate.output_price_per_1k,
    }))
}

/// Usage statistics response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStatsResponse {
    /// Total requests made.
    pub total_requests: u64,
    /// Total input tokens.
    pub total_input_tokens: u64,
    /// Total output tokens.
    pub total_output_tokens: u64,
    /// Total cost in USD.
    pub total_cost: f64,
    /// Cost today in USD.
    pub daily_cost: f64,
    /// Cost this hour in USD.
    pub hourly_cost: f64,
    /// Average cost per request.
    pub avg_cost_per_request: f64,
    /// Average tokens per request.
    pub avg_tokens_per_request: f64,
    /// Cost breakdown by model.
    pub cost_by_model: std::collections::HashMap<String, f64>,
    /// Cost breakdown by provider.
    pub cost_by_provider: std::collections::HashMap<String, f64>,
    /// Requests by model.
    pub requests_by_model: std::collections::HashMap<String, u64>,
}

/// Get usage statistics handler.
pub async fn get_usage_stats(State(state): State<AppState>) -> Json<UsageStatsResponse> {
    let stats = state.cost_predictor.get_stats();

    Json(UsageStatsResponse {
        total_requests: stats.total_requests,
        total_input_tokens: stats.total_input_tokens,
        total_output_tokens: stats.total_output_tokens,
        total_cost: stats.total_cost,
        daily_cost: state.cost_predictor.get_daily_spend(),
        hourly_cost: state.cost_predictor.get_hourly_spend(),
        avg_cost_per_request: stats.avg_cost_per_request,
        avg_tokens_per_request: stats.avg_tokens_per_request,
        cost_by_model: stats.cost_by_model,
        cost_by_provider: stats.cost_by_provider,
        requests_by_model: stats.requests_by_model,
    })
}

/// Budget configuration response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfigResponse {
    /// Maximum cost per request in USD.
    pub max_cost_per_request: Option<f64>,
    /// Maximum cost per hour in USD.
    pub max_cost_per_hour: Option<f64>,
    /// Maximum cost per day in USD.
    pub max_cost_per_day: Option<f64>,
    /// Maximum total cost in USD.
    pub max_total_cost: Option<f64>,
    /// Alert threshold (0.0 - 1.0).
    pub alert_threshold: f64,
    /// Whether limits are enforced.
    pub enforce_limits: bool,
}

/// Get budget configuration handler.
pub async fn get_budget_config(State(state): State<AppState>) -> Json<BudgetConfigResponse> {
    let config = state.cost_predictor.config();

    Json(BudgetConfigResponse {
        max_cost_per_request: config.max_cost_per_request,
        max_cost_per_hour: config.max_cost_per_hour,
        max_cost_per_day: config.max_cost_per_day,
        max_total_cost: config.max_total_cost,
        alert_threshold: config.alert_threshold,
        enforce_limits: config.enforce_limits,
    })
}

/// Update budget configuration request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBudgetRequest {
    /// Maximum cost per request in USD.
    pub max_cost_per_request: Option<f64>,
    /// Maximum cost per hour in USD.
    pub max_cost_per_hour: Option<f64>,
    /// Maximum cost per day in USD.
    pub max_cost_per_day: Option<f64>,
    /// Maximum total cost in USD.
    pub max_total_cost: Option<f64>,
    /// Alert threshold (0.0 - 1.0).
    pub alert_threshold: Option<f64>,
    /// Whether to enforce limits.
    pub enforce_limits: Option<bool>,
}

/// Update budget configuration handler.
pub async fn update_budget_config(
    State(state): State<AppState>,
    Json(request): Json<UpdateBudgetRequest>,
) -> Json<BudgetConfigResponse> {
    use ember_core::BudgetConfig;

    let current = state.cost_predictor.config();

    let new_config = BudgetConfig {
        max_cost_per_request: request
            .max_cost_per_request
            .or(current.max_cost_per_request),
        max_cost_per_hour: request.max_cost_per_hour.or(current.max_cost_per_hour),
        max_cost_per_day: request.max_cost_per_day.or(current.max_cost_per_day),
        max_total_cost: request.max_total_cost.or(current.max_total_cost),
        alert_threshold: request.alert_threshold.unwrap_or(current.alert_threshold),
        enforce_limits: request.enforce_limits.unwrap_or(current.enforce_limits),
    };

    state.cost_predictor.set_config(new_config.clone());

    Json(BudgetConfigResponse {
        max_cost_per_request: new_config.max_cost_per_request,
        max_cost_per_hour: new_config.max_cost_per_hour,
        max_cost_per_day: new_config.max_cost_per_day,
        max_total_cost: new_config.max_total_cost,
        alert_threshold: new_config.alert_threshold,
        enforce_limits: new_config.enforce_limits,
    })
}

// =============================================================================
// Enhanced Model Endpoints with Pricing
// =============================================================================

/// Extended model information with pricing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedModelInfo {
    /// Model identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Provider.
    pub provider: String,
    /// Maximum context length.
    pub context_length: u32,
    /// Maximum output tokens.
    pub max_output_tokens: u32,
    /// Input price per 1K tokens.
    pub input_price_per_1k: f64,
    /// Output price per 1K tokens.
    pub output_price_per_1k: f64,
    /// Cached input price per 1K tokens.
    pub cached_input_price_per_1k: Option<f64>,
    /// Whether the model supports tools/function calling.
    pub supports_tools: bool,
    /// Whether the model supports vision.
    pub supports_vision: bool,
    /// Whether the model has reasoning capabilities.
    pub supports_reasoning: bool,
    /// Whether the model supports JSON mode.
    pub supports_json_mode: bool,
    /// Whether the model supports streaming.
    pub supports_streaming: bool,
    /// Model description.
    pub description: Option<String>,
}

/// Extended models response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedModelsResponse {
    /// List of models with full details.
    pub models: Vec<ExtendedModelInfo>,
    /// Total count.
    pub total: usize,
    /// Provider counts.
    pub providers: std::collections::HashMap<String, usize>,
}

/// List models with extended information (pricing, capabilities).
pub async fn list_models_extended() -> Json<ExtendedModelsResponse> {
    let all_models = MODEL_REGISTRY.all();

    let mut providers: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    let models: Vec<ExtendedModelInfo> = all_models
        .iter()
        .map(|m| {
            *providers.entry(m.provider.clone()).or_insert(0) += 1;

            ExtendedModelInfo {
                id: m.id.clone(),
                name: m.name.clone(),
                provider: m.provider.clone(),
                context_length: m.context_window,
                max_output_tokens: m.max_output_tokens,
                input_price_per_1k: m.input_price_per_1k,
                output_price_per_1k: m.output_price_per_1k,
                cached_input_price_per_1k: m.cached_input_price_per_1k,
                supports_tools: m.capabilities.tools,
                supports_vision: m.capabilities.vision,
                supports_reasoning: m.capabilities.reasoning,
                supports_json_mode: m.capabilities.json_mode,
                supports_streaming: m.capabilities.streaming,
                description: m.description.clone(),
            }
        })
        .collect();

    let total = models.len();

    Json(ExtendedModelsResponse {
        models,
        total,
        providers,
    })
}

/// Get recommendations for cost optimization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationsResponse {
    /// List of recommendations.
    pub recommendations: Vec<RecommendationInfo>,
}

/// Individual recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationInfo {
    /// Description.
    pub description: String,
    /// Potential savings in USD.
    pub potential_savings: f64,
    /// Alternative model (if applicable).
    pub alternative_model: Option<String>,
    /// Priority (1 = high, 3 = low).
    pub priority: u8,
}

/// Get cost optimization recommendations handler.
pub async fn get_recommendations(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<RecommendationsResponse> {
    let model = params
        .get("model")
        .cloned()
        .unwrap_or_else(|| "gpt-4o".to_string());
    let input_tokens: u32 = params
        .get("input_tokens")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);
    let output_tokens: u32 = params
        .get("output_tokens")
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);

    let result = state
        .cost_predictor
        .predict(&model, input_tokens, output_tokens);

    let recommendations: Vec<RecommendationInfo> = result
        .recommendations
        .into_iter()
        .map(|r| RecommendationInfo {
            description: r.description,
            potential_savings: r.potential_savings,
            alternative_model: r.alternative_model,
            priority: r.priority,
        })
        .collect();

    Json(RecommendationsResponse { recommendations })
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
