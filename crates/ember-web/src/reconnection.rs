//! WebSocket Reconnection and State Synchronization
//!
//! Implements robust reconnection strategy with:
//! - Exponential backoff with jitter
//! - Message buffering for state replay
//! - Sequence number tracking for deduplication
//! - Configurable retry limits and timeouts

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, warn};

// =============================================================================
// Configuration
// =============================================================================

/// WebSocket connection configuration with reconnection parameters
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    /// Initial backoff delay before first reconnection attempt
    pub initial_backoff: Duration,
    /// Maximum backoff delay (capped at ~30s as per community feedback)
    pub max_backoff: Duration,
    /// Multiplier for exponential backoff (typically 2.0)
    pub backoff_multiplier: f64,
    /// Maximum number of reconnection attempts (None = unlimited)
    pub max_retries: Option<u32>,
    /// Interval between heartbeat pings
    pub heartbeat_interval: Duration,
    /// Timeout for heartbeat response before considering connection dead
    pub heartbeat_timeout: Duration,
    /// Maximum number of messages to buffer for replay
    pub message_buffer_size: usize,
    /// How long to retain messages in buffer for replay
    pub message_retention: Duration,
    /// Maximum outbound queue size (client-side)
    pub outbound_queue_size: usize,
    /// Timeout for queued messages before they're dropped
    pub message_timeout: Duration,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            // Reconnection settings
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(30), // Cap at 30s per m13v's feedback
            backoff_multiplier: 2.0,
            max_retries: Some(10),

            // Heartbeat settings
            heartbeat_interval: Duration::from_secs(30),
            heartbeat_timeout: Duration::from_secs(10),

            // Message buffer settings (server-side replay)
            message_buffer_size: 1000,
            message_retention: Duration::from_secs(300), // 5 minutes

            // Outbound queue settings (client-side)
            outbound_queue_size: 100,
            message_timeout: Duration::from_secs(60),
        }
    }
}

impl WebSocketConfig {
    /// Create a new configuration with custom settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set initial backoff delay
    pub fn with_initial_backoff(mut self, duration: Duration) -> Self {
        self.initial_backoff = duration;
        self
    }

    /// Set maximum backoff delay
    pub fn with_max_backoff(mut self, duration: Duration) -> Self {
        self.max_backoff = duration;
        self
    }

    /// Set maximum retry attempts
    pub fn with_max_retries(mut self, retries: Option<u32>) -> Self {
        self.max_retries = retries;
        self
    }

    /// Set heartbeat interval
    pub fn with_heartbeat_interval(mut self, duration: Duration) -> Self {
        self.heartbeat_interval = duration;
        self
    }

    /// Set message buffer size
    pub fn with_message_buffer_size(mut self, size: usize) -> Self {
        self.message_buffer_size = size;
        self
    }
}

// =============================================================================
// Backoff Calculator
// =============================================================================

/// Calculates exponential backoff with jitter to prevent thundering herd
#[derive(Debug, Clone)]
pub struct BackoffCalculator {
    config: WebSocketConfig,
    current_backoff: Duration,
    attempts: u32,
}

impl BackoffCalculator {
    /// Create a new backoff calculator with the given configuration
    pub fn new(config: WebSocketConfig) -> Self {
        Self {
            current_backoff: config.initial_backoff,
            attempts: 0,
            config,
        }
    }

    /// Reset backoff after successful connection
    pub fn reset(&mut self) {
        self.current_backoff = self.config.initial_backoff;
        self.attempts = 0;
    }

    /// Get the next backoff duration with jitter
    ///
    /// Returns None if max retries exceeded
    pub fn next_backoff(&mut self) -> Option<Duration> {
        // Check if we've exceeded max retries
        if let Some(max) = self.config.max_retries {
            if self.attempts >= max {
                return None;
            }
        }

        self.attempts += 1;

        // Add jitter (0-25% of current backoff)
        let jitter_factor = rand_jitter() * 0.25;
        let jitter = self.current_backoff.mul_f64(jitter_factor);
        let sleep_duration = self.current_backoff + jitter;

        // Calculate next backoff with exponential increase, capped at max
        self.current_backoff = std::cmp::min(
            self.current_backoff.mul_f64(self.config.backoff_multiplier),
            self.config.max_backoff,
        );

        debug!(
            attempt = self.attempts,
            backoff_ms = sleep_duration.as_millis(),
            "Calculated next backoff"
        );

        Some(sleep_duration)
    }

    /// Get the current attempt count
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Check if max retries exceeded
    pub fn is_exhausted(&self) -> bool {
        self.config
            .max_retries
            .map(|max| self.attempts >= max)
            .unwrap_or(false)
    }
}

/// Generate a random jitter factor between 0.0 and 1.0
fn rand_jitter() -> f64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    // Use RandomState for a simple pseudo-random value
    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    hasher.write_u64(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64,
    );
    let hash = hasher.finish();
    (hash as f64) / (u64::MAX as f64)
}

// =============================================================================
// Message Buffer for State Sync
// =============================================================================

/// A buffered message with sequence number and timestamp
#[derive(Debug, Clone)]
pub struct BufferedMessage<T: Clone> {
    /// Sequence number for ordering and deduplication
    pub seq: u64,
    /// The actual message payload
    pub message: T,
    /// When the message was created
    pub created_at: Instant,
}

/// Server-side message buffer for replay after client reconnection
///
/// Implements a bounded buffer with automatic pruning of old messages.
/// Messages are identified by sequence numbers for reliable replay.
pub struct MessageBuffer<T: Clone + Send + Sync> {
    /// Buffered messages in order
    messages: RwLock<VecDeque<BufferedMessage<T>>>,
    /// Current sequence number counter
    sequence: AtomicU64,
    /// Maximum buffer size
    max_size: usize,
    /// Message retention duration
    retention: Duration,
}

impl<T: Clone + Send + Sync> MessageBuffer<T> {
    /// Create a new message buffer with the given configuration
    pub fn new(config: &WebSocketConfig) -> Self {
        Self {
            messages: RwLock::new(VecDeque::with_capacity(config.message_buffer_size)),
            sequence: AtomicU64::new(0),
            max_size: config.message_buffer_size,
            retention: config.message_retention,
        }
    }

    /// Add a message to the buffer and return its sequence number
    pub async fn push(&self, message: T) -> u64 {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        let buffered = BufferedMessage {
            seq,
            message,
            created_at: Instant::now(),
        };

        let mut messages = self.messages.write().await;

        // Prune old messages if at capacity
        while messages.len() >= self.max_size {
            messages.pop_front();
        }

        // Also prune expired messages
        let now = Instant::now();
        while let Some(front) = messages.front() {
            if now.duration_since(front.created_at) > self.retention {
                messages.pop_front();
            } else {
                break;
            }
        }

        messages.push_back(buffered);
        seq
    }

    /// Get all messages with sequence number greater than the given value
    ///
    /// Used for replaying messages after client reconnection
    pub async fn get_since(&self, last_seq: u64) -> Vec<BufferedMessage<T>> {
        let messages = self.messages.read().await;
        let now = Instant::now();

        messages
            .iter()
            .filter(|m| m.seq > last_seq && now.duration_since(m.created_at) <= self.retention)
            .cloned()
            .collect()
    }

    /// Get the current sequence number
    pub fn current_sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }

    /// Get the number of buffered messages
    pub async fn len(&self) -> usize {
        self.messages.read().await.len()
    }

    /// Check if the buffer is empty
    pub async fn is_empty(&self) -> bool {
        self.messages.read().await.is_empty()
    }

    /// Clear all messages from the buffer
    pub async fn clear(&self) {
        self.messages.write().await.clear();
    }

    /// Prune expired messages
    pub async fn prune_expired(&self) -> usize {
        let mut messages = self.messages.write().await;
        let now = Instant::now();
        let initial_len = messages.len();

        messages.retain(|m| now.duration_since(m.created_at) <= self.retention);

        let pruned = initial_len - messages.len();
        if pruned > 0 {
            debug!(pruned = pruned, "Pruned expired messages from buffer");
        }
        pruned
    }
}

// =============================================================================
// Connection State
// =============================================================================

/// Represents the current state of a WebSocket connection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// Not connected, initial state
    Disconnected,
    /// Attempting to connect
    Connecting,
    /// Successfully connected
    Connected,
    /// Connection lost, attempting to reconnect
    Reconnecting,
    /// Max retries exceeded, connection failed
    Failed,
    /// Connection intentionally closed
    Closed,
}

impl ConnectionState {
    /// Check if the connection is in a healthy state
    pub fn is_healthy(&self) -> bool {
        matches!(self, ConnectionState::Connected)
    }

    /// Check if reconnection is in progress
    pub fn is_reconnecting(&self) -> bool {
        matches!(
            self,
            ConnectionState::Reconnecting | ConnectionState::Connecting
        )
    }

    /// Check if the connection has permanently failed
    pub fn is_terminal(&self) -> bool {
        matches!(self, ConnectionState::Failed | ConnectionState::Closed)
    }
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionState::Disconnected => write!(f, "disconnected"),
            ConnectionState::Connecting => write!(f, "connecting"),
            ConnectionState::Connected => write!(f, "connected"),
            ConnectionState::Reconnecting => write!(f, "reconnecting"),
            ConnectionState::Failed => write!(f, "failed"),
            ConnectionState::Closed => write!(f, "closed"),
        }
    }
}

// =============================================================================
// Client State Tracker
// =============================================================================

/// Tracks the state of a connected client for reconnection handling
pub struct ClientStateTracker {
    /// Unique client identifier
    pub client_id: String,
    /// Last received sequence number from this client
    pub last_received_seq: AtomicU64,
    /// Last sent sequence number to this client
    pub last_sent_seq: AtomicU64,
    /// Connection state
    state: RwLock<ConnectionState>,
    /// Last activity timestamp
    last_activity: RwLock<Instant>,
    /// Pending messages to send after reconnect
    pending_messages: RwLock<VecDeque<Vec<u8>>>,
}

impl ClientStateTracker {
    /// Create a new client state tracker
    pub fn new(client_id: String) -> Self {
        Self {
            client_id,
            last_received_seq: AtomicU64::new(0),
            last_sent_seq: AtomicU64::new(0),
            state: RwLock::new(ConnectionState::Connecting),
            last_activity: RwLock::new(Instant::now()),
            pending_messages: RwLock::new(VecDeque::new()),
        }
    }

    /// Update the connection state
    pub async fn set_state(&self, state: ConnectionState) {
        *self.state.write().await = state;
    }

    /// Get the current connection state
    pub async fn get_state(&self) -> ConnectionState {
        *self.state.read().await
    }

    /// Update the last received sequence number
    pub fn update_received_seq(&self, seq: u64) {
        self.last_received_seq.fetch_max(seq, Ordering::SeqCst);
    }

    /// Update the last sent sequence number
    pub fn update_sent_seq(&self, seq: u64) {
        self.last_sent_seq.fetch_max(seq, Ordering::SeqCst);
    }

    /// Mark activity (for heartbeat tracking)
    pub async fn mark_activity(&self) {
        *self.last_activity.write().await = Instant::now();
    }

    /// Get time since last activity
    pub async fn time_since_activity(&self) -> Duration {
        Instant::now().duration_since(*self.last_activity.read().await)
    }

    /// Queue a message for sending after reconnection
    pub async fn queue_message(&self, message: Vec<u8>, max_queue_size: usize) {
        let mut queue = self.pending_messages.write().await;
        while queue.len() >= max_queue_size {
            queue.pop_front(); // FIFO drop
            warn!(client_id = %self.client_id, "Dropped oldest queued message due to queue overflow");
        }
        queue.push_back(message);
    }

    /// Drain all pending messages
    pub async fn drain_pending_messages(&self) -> Vec<Vec<u8>> {
        let mut queue = self.pending_messages.write().await;
        queue.drain(..).collect()
    }

    /// Get the number of pending messages
    pub async fn pending_count(&self) -> usize {
        self.pending_messages.read().await.len()
    }
}

// =============================================================================
// Sync Messages
// =============================================================================

/// Request from client to synchronize state after reconnection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStateRequest {
    /// Last sequence number received by client before disconnect
    pub last_received_seq: u64,
    /// Client's unique session ID (for session resumption)
    pub session_id: Option<String>,
}

/// Response from server with state synchronization data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStateResponse {
    /// Number of messages being replayed
    pub replayed_count: u32,
    /// Current server sequence number
    pub current_seq: u64,
    /// Whether the session was successfully resumed
    pub session_resumed: bool,
    /// Any gaps in sequence numbers that couldn't be filled
    pub gaps: Vec<(u64, u64)>,
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WebSocketConfig::default();
        assert_eq!(config.initial_backoff, Duration::from_millis(100));
        assert_eq!(config.max_backoff, Duration::from_secs(30));
        assert_eq!(config.backoff_multiplier, 2.0);
        assert_eq!(config.max_retries, Some(10));
    }

    #[test]
    fn test_backoff_calculator() {
        let config = WebSocketConfig {
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(1),
            backoff_multiplier: 2.0,
            max_retries: Some(3),
            ..Default::default()
        };

        let mut calc = BackoffCalculator::new(config);

        // First attempt
        let backoff1 = calc.next_backoff().unwrap();
        assert!(backoff1 >= Duration::from_millis(100));
        assert!(backoff1 <= Duration::from_millis(125)); // With jitter

        // Second attempt (doubled)
        let backoff2 = calc.next_backoff().unwrap();
        assert!(backoff2 >= Duration::from_millis(200));

        // Third attempt
        let _backoff3 = calc.next_backoff().unwrap();

        // Fourth attempt should fail (max_retries = 3)
        assert!(calc.next_backoff().is_none());
        assert!(calc.is_exhausted());
    }

    #[test]
    fn test_backoff_reset() {
        let config = WebSocketConfig {
            initial_backoff: Duration::from_millis(100),
            max_retries: Some(3),
            ..Default::default()
        };

        let mut calc = BackoffCalculator::new(config);

        calc.next_backoff();
        calc.next_backoff();
        assert_eq!(calc.attempts(), 2);

        calc.reset();
        assert_eq!(calc.attempts(), 0);
    }

    #[tokio::test]
    async fn test_message_buffer() {
        let config = WebSocketConfig {
            message_buffer_size: 3,
            message_retention: Duration::from_secs(60),
            ..Default::default()
        };

        let buffer: MessageBuffer<String> = MessageBuffer::new(&config);

        // Add messages
        let seq1 = buffer.push("msg1".to_string()).await;
        let seq2 = buffer.push("msg2".to_string()).await;
        let seq3 = buffer.push("msg3".to_string()).await;

        assert_eq!(seq1, 0);
        assert_eq!(seq2, 1);
        assert_eq!(seq3, 2);
        assert_eq!(buffer.len().await, 3);

        // Add another message (should evict first)
        let seq4 = buffer.push("msg4".to_string()).await;
        assert_eq!(seq4, 3);
        assert_eq!(buffer.len().await, 3);

        // Get messages since seq1 (should return msg3, msg4)
        let since = buffer.get_since(1).await;
        assert_eq!(since.len(), 2);
        assert_eq!(since[0].message, "msg3");
        assert_eq!(since[1].message, "msg4");
    }

    #[test]
    fn test_connection_state() {
        assert!(ConnectionState::Connected.is_healthy());
        assert!(!ConnectionState::Reconnecting.is_healthy());

        assert!(ConnectionState::Reconnecting.is_reconnecting());
        assert!(ConnectionState::Connecting.is_reconnecting());
        assert!(!ConnectionState::Connected.is_reconnecting());

        assert!(ConnectionState::Failed.is_terminal());
        assert!(ConnectionState::Closed.is_terminal());
        assert!(!ConnectionState::Connected.is_terminal());
    }

    #[tokio::test]
    async fn test_client_state_tracker() {
        let tracker = ClientStateTracker::new("client-123".to_string());

        assert_eq!(tracker.get_state().await, ConnectionState::Connecting);

        tracker.set_state(ConnectionState::Connected).await;
        assert_eq!(tracker.get_state().await, ConnectionState::Connected);

        tracker.update_received_seq(5);
        tracker.update_received_seq(3); // Should not decrease
        assert_eq!(tracker.last_received_seq.load(Ordering::SeqCst), 5);

        // Test message queueing
        tracker.queue_message(vec![1, 2, 3], 10).await;
        tracker.queue_message(vec![4, 5, 6], 10).await;
        assert_eq!(tracker.pending_count().await, 2);

        let messages = tracker.drain_pending_messages().await;
        assert_eq!(messages.len(), 2);
        assert_eq!(tracker.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_queue_overflow() {
        let tracker = ClientStateTracker::new("client-456".to_string());

        // Queue 5 messages with max size 3
        for i in 0..5 {
            tracker.queue_message(vec![i], 3).await;
        }

        // Should only have last 3 messages
        assert_eq!(tracker.pending_count().await, 3);

        let messages = tracker.drain_pending_messages().await;
        assert_eq!(messages[0], vec![2]);
        assert_eq!(messages[1], vec![3]);
        assert_eq!(messages[2], vec![4]);
    }
}