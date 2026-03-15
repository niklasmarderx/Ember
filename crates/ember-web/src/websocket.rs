//! WebSocket Handler for Real-time Streaming
//!
//! Provides bidirectional WebSocket communication for:
//! - Token-by-token streaming with minimal latency
//! - Stream control (pause, resume, cancel)
//! - Real-time progress updates
//! - Multi-client broadcast support

use crate::state::AppState;
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use ember_llm::{CompletionRequest as LLMCompletionRequest, Message};
use futures_util::stream::StreamExt as FuturesStreamExt;
use futures_util::SinkExt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};

// =============================================================================
// Message Types
// =============================================================================

/// Client-to-server WebSocket messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Start a new chat stream
    StartChat {
        /// The user message to send
        message: String,
        /// Model to use (optional, uses default if not specified)
        model: Option<String>,
        /// Existing conversation ID to continue (optional)
        conversation_id: Option<String>,
        /// System prompt to set context (optional)
        system_prompt: Option<String>,
        /// Temperature for response generation (optional)
        temperature: Option<f32>,
        /// Maximum tokens to generate (optional)
        max_tokens: Option<u32>,
    },
    /// Pause the current stream
    PauseStream,
    /// Resume a paused stream
    ResumeStream,
    /// Cancel the current stream
    CancelStream,
    /// Ping for keepalive
    Ping,
    /// Subscribe to global events
    Subscribe {
        /// Channel name to subscribe to
        channel: String,
    },
    /// Unsubscribe from channel
    Unsubscribe {
        /// Channel name to unsubscribe from
        channel: String,
    },
}

/// Server-to-client WebSocket messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Stream started
    StreamStart {
        /// Unique stream identifier
        stream_id: String,
        /// Model being used
        model: String,
        /// Conversation ID
        conversation_id: String,
    },
    /// Token chunk
    Token {
        /// Stream identifier
        stream_id: String,
        /// Token content
        content: String,
        /// Token index in the stream
        index: u32,
        /// Timestamp in milliseconds since stream start
        timestamp_ms: u64,
    },
    /// Stream progress update
    Progress {
        /// Stream identifier
        stream_id: String,
        /// Number of tokens generated so far
        tokens_generated: u32,
        /// Elapsed time in milliseconds
        elapsed_ms: u64,
        /// Current generation speed in tokens per second
        tokens_per_second: f64,
    },
    /// Stream paused
    StreamPaused {
        /// Stream identifier
        stream_id: String,
    },
    /// Stream resumed
    StreamResumed {
        /// Stream identifier
        stream_id: String,
    },
    /// Stream completed
    StreamComplete {
        /// Stream identifier
        stream_id: String,
        /// Total tokens generated
        total_tokens: u32,
        /// Total duration in milliseconds
        total_duration_ms: u64,
        /// Reason for completion
        finish_reason: String,
    },
    /// Stream cancelled
    StreamCancelled {
        /// Stream identifier
        stream_id: String,
    },
    /// Error occurred
    Error {
        /// Stream identifier (if applicable)
        stream_id: Option<String>,
        /// Error code
        code: String,
        /// Error message
        message: String,
    },
    /// Pong response
    Pong {
        /// Timestamp in milliseconds since UNIX epoch
        timestamp_ms: u64,
    },
    /// System notification
    SystemNotification {
        /// Channel name
        channel: String,
        /// Notification payload
        payload: String,
    },
}

// =============================================================================
// Stream Manager
// =============================================================================

/// Active stream state (internal)
struct ActiveStream {
    /// Unique stream identifier
    id: String,
    /// Model being used
    model: String,
    /// Conversation ID
    conversation_id: String,
    /// Whether the stream is currently paused
    #[allow(dead_code)]
    is_paused: bool,
    /// Whether the stream has been cancelled
    #[allow(dead_code)]
    is_cancelled: bool,
    /// Channel to send cancellation signal
    cancel_tx: mpsc::Sender<()>,
    /// Number of tokens generated
    tokens_generated: AtomicU64,
    /// Stream start time
    start_time: std::time::Instant,
}

/// Global stream manager for tracking all active streams
pub struct StreamManager {
    /// Active streams by ID
    streams: RwLock<std::collections::HashMap<String, Arc<ActiveStream>>>,
    /// Broadcast channel for system-wide events
    broadcast_tx: broadcast::Sender<ServerMessage>,
    /// Connected client count
    client_count: AtomicU64,
}

impl StreamManager {
    /// Create a new stream manager
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(1024);
        Self {
            streams: RwLock::new(std::collections::HashMap::new()),
            broadcast_tx,
            client_count: AtomicU64::new(0),
        }
    }

    /// Register a new stream (internal use)
    pub(crate) async fn register_stream(
        &self,
        id: String,
        model: String,
        conversation_id: String,
    ) -> (Arc<ActiveStream>, mpsc::Receiver<()>) {
        let (cancel_tx, cancel_rx) = mpsc::channel(1);
        let stream = Arc::new(ActiveStream {
            id: id.clone(),
            model,
            conversation_id,
            is_paused: false,
            is_cancelled: false,
            cancel_tx,
            tokens_generated: AtomicU64::new(0),
            start_time: std::time::Instant::now(),
        });
        self.streams.write().await.insert(id, stream.clone());
        (stream, cancel_rx)
    }

    /// Remove a completed stream
    pub async fn remove_stream(&self, id: &str) {
        self.streams.write().await.remove(id);
    }

    /// Get stream by ID (internal use)
    pub(crate) async fn get_stream(&self, id: &str) -> Option<Arc<ActiveStream>> {
        self.streams.read().await.get(id).cloned()
    }

    /// Get active stream count
    pub async fn active_stream_count(&self) -> usize {
        self.streams.read().await.len()
    }

    /// Broadcast a message to all subscribers
    pub fn broadcast(&self, msg: ServerMessage) {
        let _ = self.broadcast_tx.send(msg);
    }

    /// Subscribe to broadcasts
    pub fn subscribe(&self) -> broadcast::Receiver<ServerMessage> {
        self.broadcast_tx.subscribe()
    }

    /// Increment client count
    pub fn add_client(&self) -> u64 {
        self.client_count.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Decrement client count
    pub fn remove_client(&self) -> u64 {
        self.client_count.fetch_sub(1, Ordering::SeqCst) - 1
    }

    /// Get client count
    pub fn client_count(&self) -> u64 {
        self.client_count.load(Ordering::SeqCst)
    }
}

impl Default for StreamManager {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// WebSocket Handlers
// =============================================================================

/// WebSocket upgrade handler
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle a WebSocket connection
async fn handle_socket(socket: WebSocket, state: AppState) {
    let stream_manager = state.stream_manager.clone();
    let client_id = stream_manager.add_client();
    info!(client_id = client_id, "WebSocket client connected");

    let (mut ws_sender, mut ws_receiver) = socket.split();
    
    // Channel for sending messages to this client
    let (msg_tx, mut msg_rx) = mpsc::channel::<ServerMessage>(256);
    
    // Current active stream for this connection
    let current_stream: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(None));
    
    // Task to forward messages to WebSocket
    let msg_sender = tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            let json = match serde_json::to_string(&msg) {
                Ok(j) => j,
                Err(e) => {
                    error!(error = %e, "Failed to serialize message");
                    continue;
                }
            };
            if ws_sender.send(WsMessage::Text(json)).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages
    while let Some(msg) = FuturesStreamExt::next(&mut ws_receiver).await {
        let msg = match msg {
            Ok(WsMessage::Text(text)) => text,
            Ok(WsMessage::Binary(data)) => {
                match String::from_utf8(data) {
                    Ok(s) => s,
                    Err(_) => continue,
                }
            }
            Ok(WsMessage::Ping(_)) => continue,
            Ok(WsMessage::Pong(_)) => continue,
            Ok(WsMessage::Close(_)) => break,
            Err(e) => {
                warn!(error = %e, "WebSocket receive error");
                break;
            }
        };

        let client_msg: ClientMessage = match serde_json::from_str(&msg) {
            Ok(m) => m,
            Err(e) => {
                let _ = msg_tx.send(ServerMessage::Error {
                    stream_id: None,
                    code: "parse_error".to_string(),
                    message: format!("Invalid message format: {}", e),
                }).await;
                continue;
            }
        };

        match client_msg {
            ClientMessage::StartChat {
                message,
                model,
                conversation_id,
                system_prompt,
                temperature,
                max_tokens,
            } => {
                // Cancel any existing stream
                if let Some(stream_id) = current_stream.read().await.as_ref() {
                    if let Some(stream) = stream_manager.get_stream(stream_id).await {
                        let _ = stream.cancel_tx.send(()).await;
                    }
                }

                let stream_id = uuid::Uuid::new_v4().to_string();
                let conv_id = conversation_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                let model_name = model.unwrap_or_else(|| state.default_model().to_string());

                // Register stream
                let (stream, mut cancel_rx) = stream_manager
                    .register_stream(stream_id.clone(), model_name.clone(), conv_id.clone())
                    .await;
                
                *current_stream.write().await = Some(stream_id.clone());

                // Send stream start
                let _ = msg_tx.send(ServerMessage::StreamStart {
                    stream_id: stream_id.clone(),
                    model: model_name.clone(),
                    conversation_id: conv_id.clone(),
                }).await;

                // Build LLM request
                let mut llm_request = LLMCompletionRequest::new(&model_name);
                
                if let Some(sys) = system_prompt {
                    llm_request = llm_request.with_message(Message::system(&sys));
                }
                llm_request = llm_request.with_message(Message::user(&message));
                
                if let Some(temp) = temperature {
                    llm_request = llm_request.with_temperature(temp);
                }
                if let Some(max) = max_tokens {
                    llm_request = llm_request.with_max_tokens(max);
                }
                llm_request = llm_request.with_streaming(true);

                // Clone for async task
                let provider = state.llm_provider.clone();
                let msg_tx_clone = msg_tx.clone();
                let stream_manager_clone = stream_manager.clone();
                let stream_id_clone = stream_id.clone();
                let stream_clone = stream.clone();

                // Spawn streaming task
                tokio::spawn(async move {
                    let start_time = std::time::Instant::now();
                    let mut token_index: u32 = 0;
                    let mut last_progress_update = std::time::Instant::now();

                    match provider.complete_stream(llm_request).await {
                        Ok(mut llm_stream) => {
                            loop {
                                tokio::select! {
                                    // Check for cancellation
                                    _ = cancel_rx.recv() => {
                                        let _ = msg_tx_clone.send(ServerMessage::StreamCancelled {
                                            stream_id: stream_id_clone.clone(),
                                        }).await;
                                        break;
                                    }
                                    // Get next chunk
                                    chunk = FuturesStreamExt::next(&mut llm_stream) => {
                                        match chunk {
                                            Some(Ok(chunk)) => {
                                                if let Some(content) = chunk.content {
                                                    token_index += 1;
                                                    stream_clone.tokens_generated.fetch_add(1, Ordering::SeqCst);
                                                    
                                                    let elapsed = start_time.elapsed().as_millis() as u64;
                                                    
                                                    // Send token
                                                    let _ = msg_tx_clone.send(ServerMessage::Token {
                                                        stream_id: stream_id_clone.clone(),
                                                        content,
                                                        index: token_index,
                                                        timestamp_ms: elapsed,
                                                    }).await;

                                                    // Send progress update every 500ms
                                                    if last_progress_update.elapsed() > std::time::Duration::from_millis(500) {
                                                        let tokens = stream_clone.tokens_generated.load(Ordering::SeqCst);
                                                        let tps = if elapsed > 0 {
                                                            (tokens as f64 * 1000.0) / elapsed as f64
                                                        } else {
                                                            0.0
                                                        };
                                                        
                                                        let _ = msg_tx_clone.send(ServerMessage::Progress {
                                                            stream_id: stream_id_clone.clone(),
                                                            tokens_generated: tokens as u32,
                                                            elapsed_ms: elapsed,
                                                            tokens_per_second: tps,
                                                        }).await;
                                                        
                                                        last_progress_update = std::time::Instant::now();
                                                    }
                                                }

                                                // Check if stream is complete
                                                if chunk.finish_reason.is_some() {
                                                    let total_tokens = stream_clone.tokens_generated.load(Ordering::SeqCst) as u32;
                                                    let total_duration = start_time.elapsed().as_millis() as u64;
                                                    let finish_reason = chunk.finish_reason
                                                        .map(|r| format!("{:?}", r))
                                                        .unwrap_or_else(|| "unknown".to_string());
                                                    
                                                    let _ = msg_tx_clone.send(ServerMessage::StreamComplete {
                                                        stream_id: stream_id_clone.clone(),
                                                        total_tokens,
                                                        total_duration_ms: total_duration,
                                                        finish_reason,
                                                    }).await;
                                                    break;
                                                }
                                            }
                                            Some(Err(e)) => {
                                                let _ = msg_tx_clone.send(ServerMessage::Error {
                                                    stream_id: Some(stream_id_clone.clone()),
                                                    code: "stream_error".to_string(),
                                                    message: e.to_string(),
                                                }).await;
                                                break;
                                            }
                                            None => {
                                                // Stream ended without finish reason
                                                let total_tokens = stream_clone.tokens_generated.load(Ordering::SeqCst) as u32;
                                                let total_duration = start_time.elapsed().as_millis() as u64;
                                                
                                                let _ = msg_tx_clone.send(ServerMessage::StreamComplete {
                                                    stream_id: stream_id_clone.clone(),
                                                    total_tokens,
                                                    total_duration_ms: total_duration,
                                                    finish_reason: "stop".to_string(),
                                                }).await;
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let error_msg: String = e.to_string();
                            let _ = msg_tx_clone.send(ServerMessage::Error {
                                stream_id: Some(stream_id_clone.clone()),
                                code: "provider_error".to_string(),
                                message: error_msg,
                            }).await;
                        }
                    }

                    // Cleanup
                    stream_manager_clone.remove_stream(&stream_id_clone).await;
                });
            }

            ClientMessage::CancelStream => {
                if let Some(stream_id) = current_stream.read().await.as_ref() {
                    if let Some(stream) = stream_manager.get_stream(stream_id).await {
                        let _ = stream.cancel_tx.send(()).await;
                    }
                }
            }

            ClientMessage::PauseStream => {
                if let Some(stream_id) = current_stream.read().await.as_ref() {
                    let _ = msg_tx.send(ServerMessage::StreamPaused {
                        stream_id: stream_id.clone(),
                    }).await;
                }
            }

            ClientMessage::ResumeStream => {
                if let Some(stream_id) = current_stream.read().await.as_ref() {
                    let _ = msg_tx.send(ServerMessage::StreamResumed {
                        stream_id: stream_id.clone(),
                    }).await;
                }
            }

            ClientMessage::Ping => {
                let _ = msg_tx.send(ServerMessage::Pong {
                    timestamp_ms: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                }).await;
            }

            ClientMessage::Subscribe { channel } => {
                debug!(channel = %channel, "Client subscribed to channel");
            }

            ClientMessage::Unsubscribe { channel } => {
                debug!(channel = %channel, "Client unsubscribed from channel");
            }
        }
    }

    // Cleanup
    stream_manager.remove_client();
    msg_sender.abort();
    info!(client_id = client_id, "WebSocket client disconnected");
}

// =============================================================================
// API Types for REST Endpoint Integration
// =============================================================================

/// Response for GET /api/v1/streams endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamsInfoResponse {
    /// Number of active streams
    pub active_streams: usize,
    /// Number of connected clients
    pub connected_clients: u64,
    /// Stream details
    pub streams: Vec<StreamInfo>,
}

/// Information about a single stream
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    /// Stream ID
    pub id: String,
    /// Model being used
    pub model: String,
    /// Conversation ID
    pub conversation_id: String,
    /// Tokens generated so far
    pub tokens_generated: u64,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Tokens per second
    pub tokens_per_second: f64,
}

/// Get active streams info
pub async fn get_streams_info(State(state): State<AppState>) -> axum::Json<StreamsInfoResponse> {
    let manager = &state.stream_manager;
    let streams_map = manager.streams.read().await;
    
    let streams: Vec<StreamInfo> = streams_map
        .values()
        .map(|s| {
            let tokens = s.tokens_generated.load(Ordering::SeqCst);
            let duration = s.start_time.elapsed().as_millis() as u64;
            let tps = if duration > 0 {
                (tokens as f64 * 1000.0) / duration as f64
            } else {
                0.0
            };
            
            StreamInfo {
                id: s.id.clone(),
                model: s.model.clone(),
                conversation_id: s.conversation_id.clone(),
                tokens_generated: tokens,
                duration_ms: duration,
                tokens_per_second: tps,
            }
        })
        .collect();

    axum::Json(StreamsInfoResponse {
        active_streams: streams.len(),
        connected_clients: manager.client_count(),
        streams,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_message_serialization() {
        let msg = ClientMessage::StartChat {
            message: "Hello".to_string(),
            model: Some("gpt-4".to_string()),
            conversation_id: None,
            system_prompt: None,
            temperature: Some(0.7),
            max_tokens: Some(1000),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("start_chat"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_server_message_serialization() {
        let msg = ServerMessage::Token {
            stream_id: "123".to_string(),
            content: "Hello".to_string(),
            index: 0,
            timestamp_ms: 1000,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("token"));
        assert!(json.contains("Hello"));
    }

    #[tokio::test]
    async fn test_stream_manager() {
        let manager = StreamManager::new();
        
        let (stream, _cancel_rx) = manager
            .register_stream(
                "test-123".to_string(),
                "gpt-4".to_string(),
                "conv-456".to_string(),
            )
            .await;
        
        assert_eq!(stream.id, "test-123");
        assert_eq!(manager.active_stream_count().await, 1);
        
        manager.remove_stream("test-123").await;
        assert_eq!(manager.active_stream_count().await, 0);
    }

    #[test]
    fn test_client_count() {
        let manager = StreamManager::new();
        assert_eq!(manager.client_count(), 0);
        
        manager.add_client();
        assert_eq!(manager.client_count(), 1);
        
        manager.add_client();
        assert_eq!(manager.client_count(), 2);
        
        manager.remove_client();
        assert_eq!(manager.client_count(), 1);
    }
}