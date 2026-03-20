//! WebSocket Handler for Real-time Streaming
//!
//! Provides bidirectional WebSocket communication for:
//! - Token-by-token streaming with minimal latency
//! - Stream control (pause, resume, cancel)
//! - Real-time progress updates
//! - Multi-client broadcast support
//! - Robust reconnection with state synchronization
//! - Message sequencing for deduplication

use crate::reconnection::{ConnectionState, MessageBuffer, WebSocketConfig};
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
use std::time::Instant;
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
    Ping {
        /// Client timestamp for RTT calculation
        #[serde(default)]
        timestamp_ms: Option<u64>,
    },
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
    /// Request state synchronization after reconnection
    SyncState {
        /// Last sequence number received by client
        last_received_seq: u64,
        /// Optional session ID for session resumption
        session_id: Option<String>,
    },
    /// Acknowledge receipt of a message (for reliable delivery)
    Ack {
        /// Sequence number being acknowledged
        seq: u64,
    },
}

/// Server-to-client WebSocket messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Connection established with session info
    Connected {
        /// Unique session identifier for reconnection
        session_id: String,
        /// Current server sequence number
        current_seq: u64,
        /// Server heartbeat interval in milliseconds
        heartbeat_interval_ms: u64,
    },
    /// Stream started
    StreamStart {
        /// Sequence number for this message
        #[serde(default)]
        seq: u64,
        /// Unique stream identifier
        stream_id: String,
        /// Model being used
        model: String,
        /// Conversation ID
        conversation_id: String,
    },
    /// Token chunk
    Token {
        /// Sequence number for this message
        #[serde(default)]
        seq: u64,
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
        /// Sequence number for this message
        #[serde(default)]
        seq: u64,
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
        /// Sequence number for this message
        #[serde(default)]
        seq: u64,
        /// Stream identifier
        stream_id: String,
    },
    /// Stream resumed
    StreamResumed {
        /// Sequence number for this message
        #[serde(default)]
        seq: u64,
        /// Stream identifier
        stream_id: String,
    },
    /// Stream completed
    StreamComplete {
        /// Sequence number for this message
        #[serde(default)]
        seq: u64,
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
        /// Sequence number for this message
        #[serde(default)]
        seq: u64,
        /// Stream identifier
        stream_id: String,
    },
    /// Error occurred
    Error {
        /// Sequence number for this message
        #[serde(default)]
        seq: u64,
        /// Stream identifier (if applicable)
        stream_id: Option<String>,
        /// Error code
        code: String,
        /// Error message
        message: String,
    },
    /// Pong response
    Pong {
        /// Server timestamp in milliseconds since UNIX epoch
        timestamp_ms: u64,
        /// Echo back client timestamp for RTT calculation
        client_timestamp_ms: Option<u64>,
    },
    /// System notification
    SystemNotification {
        /// Sequence number for this message
        #[serde(default)]
        seq: u64,
        /// Channel name
        channel: String,
        /// Notification payload
        payload: String,
    },
    /// State synchronization response
    SyncStateResponse {
        /// Number of messages being replayed
        replayed_count: u32,
        /// Current server sequence number
        current_seq: u64,
        /// Whether the session was successfully resumed
        session_resumed: bool,
        /// Any gaps in sequence numbers that couldn't be filled
        #[serde(default)]
        gaps: Vec<(u64, u64)>,
    },
    /// Connection state change notification
    ConnectionStateChanged {
        /// New connection state
        state: ConnectionState,
        /// Reason for state change (if applicable)
        reason: Option<String>,
    },
    /// Heartbeat timeout warning (connection may be unhealthy)
    HeartbeatWarning {
        /// Milliseconds since last pong received
        ms_since_last_pong: u64,
    },
}

// =============================================================================
// Stream Manager
// =============================================================================

/// Active stream state (internal)
pub(crate) struct ActiveStream {
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
    /// Global message sequence counter
    global_sequence: AtomicU64,
    /// Message buffer for replay after reconnection
    message_buffer: Arc<MessageBuffer<ServerMessage>>,
    /// WebSocket configuration
    config: WebSocketConfig,
    /// Client sessions for reconnection (session_id -> client state)
    client_sessions: RwLock<std::collections::HashMap<String, ClientSession>>,
}

/// Represents a client session that can be resumed after reconnection
#[derive(Debug)]
pub struct ClientSession {
    /// Session creation time
    pub created_at: Instant,
    /// Last activity time
    pub last_activity: Instant,
    /// Last sequence number sent to this client
    pub last_sent_seq: u64,
    /// Active stream IDs for this session
    pub active_streams: Vec<String>,
}

impl StreamManager {
    /// Create a new stream manager with default configuration
    pub fn new() -> Self {
        Self::with_config(WebSocketConfig::default())
    }

    /// Create a new stream manager with custom configuration
    pub fn with_config(config: WebSocketConfig) -> Self {
        let (broadcast_tx, _) = broadcast::channel(1024);
        let message_buffer = Arc::new(MessageBuffer::new(&config));
        Self {
            streams: RwLock::new(std::collections::HashMap::new()),
            broadcast_tx,
            client_count: AtomicU64::new(0),
            global_sequence: AtomicU64::new(0),
            message_buffer,
            config,
            client_sessions: RwLock::new(std::collections::HashMap::new()),
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

    /// Get next sequence number
    pub fn next_sequence(&self) -> u64 {
        self.global_sequence.fetch_add(1, Ordering::SeqCst)
    }

    /// Get current sequence number (without incrementing)
    pub fn current_sequence(&self) -> u64 {
        self.global_sequence.load(Ordering::SeqCst)
    }

    /// Buffer a message for potential replay
    pub async fn buffer_message(&self, message: ServerMessage) -> u64 {
        self.message_buffer.push(message).await
    }

    /// Get messages for replay after reconnection
    pub async fn get_messages_since(&self, last_seq: u64) -> Vec<ServerMessage> {
        self.message_buffer
            .get_since(last_seq)
            .await
            .into_iter()
            .map(|bm| bm.message)
            .collect()
    }

    /// Register a new client session
    pub async fn register_session(&self, session_id: String) {
        let mut sessions = self.client_sessions.write().await;
        sessions.insert(
            session_id,
            ClientSession {
                created_at: Instant::now(),
                last_activity: Instant::now(),
                last_sent_seq: 0,
                active_streams: Vec::new(),
            },
        );
    }

    /// Update session last activity
    pub async fn touch_session(&self, session_id: &str) {
        if let Some(session) = self.client_sessions.write().await.get_mut(session_id) {
            session.last_activity = Instant::now();
        }
    }

    /// Get session info for resumption
    pub async fn get_session(&self, session_id: &str) -> Option<ClientSession> {
        self.client_sessions.read().await.get(session_id).map(|s| ClientSession {
            created_at: s.created_at,
            last_activity: s.last_activity,
            last_sent_seq: s.last_sent_seq,
            active_streams: s.active_streams.clone(),
        })
    }

    /// Update session's last sent sequence
    pub async fn update_session_seq(&self, session_id: &str, seq: u64) {
        if let Some(session) = self.client_sessions.write().await.get_mut(session_id) {
            session.last_sent_seq = seq;
        }
    }

    /// Clean up expired sessions
    pub async fn cleanup_expired_sessions(&self) {
        let retention = self.config.message_retention;
        let mut sessions = self.client_sessions.write().await;
        let now = Instant::now();
        sessions.retain(|_, session| now.duration_since(session.last_activity) < retention);
    }

    /// Get the WebSocket configuration
    pub fn config(&self) -> &WebSocketConfig {
        &self.config
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
    let session_id = uuid::Uuid::new_v4().to_string();
    
    info!(client_id = client_id, session_id = %session_id, "WebSocket client connected");

    // Register session for reconnection support
    stream_manager.register_session(session_id.clone()).await;

    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Channel for sending messages to this client
    let (msg_tx, mut msg_rx) = mpsc::channel::<ServerMessage>(256);

    // Current active stream for this connection
    let current_stream: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(None));

    // Track last pong received for heartbeat monitoring
    let last_pong = Arc::new(RwLock::new(Instant::now()));
    
    // Session ID for this connection
    let session_id_clone = session_id.clone();
    let stream_manager_clone = stream_manager.clone();
    
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
            // Update session activity
            stream_manager_clone.touch_session(&session_id_clone).await;
        }
    });

    // Send initial connected message with session info
    let config = stream_manager.config();
    let _ = msg_tx
        .send(ServerMessage::Connected {
            session_id: session_id.clone(),
            current_seq: stream_manager.current_sequence(),
            heartbeat_interval_ms: config.heartbeat_interval.as_millis() as u64,
        })
        .await;

    // Handle incoming messages
    while let Some(msg) = FuturesStreamExt::next(&mut ws_receiver).await {
        let msg = match msg {
            Ok(WsMessage::Text(text)) => text,
            Ok(WsMessage::Binary(data)) => match String::from_utf8(data) {
                Ok(s) => s,
                Err(_) => continue,
            },
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
                let _ = msg_tx
                    .send(ServerMessage::Error {
                        seq: 0,
                        stream_id: None,
                        code: "parse_error".to_string(),
                        message: format!("Invalid message format: {}", e),
                    })
                    .await;
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
                let _ = msg_tx
                    .send(ServerMessage::StreamStart {
                        seq: stream_manager.next_sequence(),
                        stream_id: stream_id.clone(),
                        model: model_name.clone(),
                        conversation_id: conv_id.clone(),
                    })
                    .await;

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
                                            seq: 0,
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
                                                        seq: 0, // Tokens don't need sequence for replay
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
                                                            seq: 0,
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
                                                        seq: 0,
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
                                                    seq: 0,
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
                                                    seq: 0,
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
                            let _ = msg_tx_clone
                                .send(ServerMessage::Error {
                                    seq: 0,
                                    stream_id: Some(stream_id_clone.clone()),
                                    code: "provider_error".to_string(),
                                    message: error_msg,
                                })
                                .await;
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
                    let _ = msg_tx
                        .send(ServerMessage::StreamPaused {
                            seq: 0,
                            stream_id: stream_id.clone(),
                        })
                        .await;
                }
            }

            ClientMessage::ResumeStream => {
                if let Some(stream_id) = current_stream.read().await.as_ref() {
                    let _ = msg_tx
                        .send(ServerMessage::StreamResumed {
                            seq: 0,
                            stream_id: stream_id.clone(),
                        })
                        .await;
                }
            }

            ClientMessage::Ping { timestamp_ms } => {
                // Update last pong time (client is responsive)
                *last_pong.write().await = Instant::now();
                
                let _ = msg_tx
                    .send(ServerMessage::Pong {
                        timestamp_ms: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                        client_timestamp_ms: timestamp_ms,
                    })
                    .await;
            }

            ClientMessage::Subscribe { channel } => {
                debug!(channel = %channel, "Client subscribed to channel");
            }

            ClientMessage::Unsubscribe { channel } => {
                debug!(channel = %channel, "Client unsubscribed from channel");
            }

            ClientMessage::SyncState {
                last_received_seq,
                session_id: client_session_id,
            } => {
                info!(
                    last_seq = last_received_seq,
                    session = ?client_session_id,
                    "Client requesting state sync"
                );

                // Check if we can resume the session
                let session_resumed = if let Some(sid) = &client_session_id {
                    stream_manager.get_session(sid).await.is_some()
                } else {
                    false
                };

                // Get messages to replay
                let messages_to_replay = stream_manager.get_messages_since(last_received_seq).await;
                let replayed_count = messages_to_replay.len() as u32;

                // Send sync response
                let _ = msg_tx
                    .send(ServerMessage::SyncStateResponse {
                        replayed_count,
                        current_seq: stream_manager.current_sequence(),
                        session_resumed,
                        gaps: vec![], // Could calculate actual gaps if needed
                    })
                    .await;

                // Replay missed messages
                for msg in messages_to_replay {
                    let _ = msg_tx.send(msg).await;
                }

                info!(
                    replayed = replayed_count,
                    session_resumed = session_resumed,
                    "State sync completed"
                );
            }

            ClientMessage::Ack { seq } => {
                // Client acknowledges receipt - update session tracking
                stream_manager.update_session_seq(&session_id, seq).await;
                debug!(seq = seq, "Message acknowledged by client");
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
