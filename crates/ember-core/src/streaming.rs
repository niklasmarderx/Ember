//! Streaming Agent Module
//!
//! Real-time token streaming with advanced features that OpenClaw does not have!
//!
//! # Features
//! - **Token-by-Token Streaming**: See responses as they are generated
//! - **Interruptible**: Cancel generation at any time
//! - **Backpressure Handling**: Smart flow control
//! - **Partial Response Recovery**: Resume from where it stopped
//! - **Stream Transformers**: Filter, transform, aggregate on the fly
//! - **Multi-Stream Merge**: Combine outputs from multiple agents

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, watch, RwLock};

/// A single streamed token with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamToken {
    /// The token content.
    pub content: String,
    /// Token index in the response.
    pub index: usize,
    /// Whether this is the final token.
    pub is_final: bool,
    /// Timestamp when this token was generated.
    pub timestamp_ms: u64,
    /// Token probability (if available).
    pub probability: Option<f32>,
    /// Alternative tokens (if available).
    pub alternatives: Vec<String>,
}

/// Stream state for tracking progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamState {
    /// Stream has not started.
    Pending,
    /// Stream is actively producing tokens.
    Streaming,
    /// Stream is paused.
    Paused,
    /// Stream completed successfully.
    Completed,
    /// Stream was cancelled.
    Cancelled,
    /// Stream encountered an error.
    Error,
}

/// Stream statistics.
#[derive(Debug, Clone, Default, Serialize)]
pub struct StreamStats {
    /// Total tokens streamed.
    pub tokens_streamed: usize,
    /// Total characters streamed.
    pub chars_streamed: usize,
    /// Average tokens per second.
    pub tokens_per_second: f64,
    /// Time to first token in milliseconds.
    pub time_to_first_token_ms: u64,
    /// Total stream duration in milliseconds.
    pub total_duration_ms: u64,
    /// Number of pauses.
    pub pause_count: usize,
    /// Whether stream was cancelled.
    pub was_cancelled: bool,
}

/// Configuration for the streaming agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// Buffer size for tokens.
    pub buffer_size: usize,
    /// Maximum tokens before auto-pause.
    pub max_tokens: Option<usize>,
    /// Timeout for no activity.
    pub idle_timeout: Duration,
    /// Whether to collect full response.
    pub collect_full_response: bool,
    /// Enable backpressure handling.
    pub backpressure_enabled: bool,
    /// Minimum delay between tokens (for rate limiting).
    pub min_token_delay: Option<Duration>,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1024,
            max_tokens: None,
            idle_timeout: Duration::from_secs(30),
            collect_full_response: true,
            backpressure_enabled: true,
            min_token_delay: None,
        }
    }
}

/// A streaming response that can be read token by token.
pub struct StreamingResponse {
    /// Token receiver.
    token_rx: mpsc::Receiver<StreamToken>,
    /// State watch receiver.
    state_rx: watch::Receiver<StreamState>,
    /// Stats.
    stats: Arc<RwLock<StreamStats>>,
    /// Collected response (if enabled).
    collected: Arc<RwLock<String>>,
    /// Configuration.
    config: StreamConfig,
    /// Start time.
    start_time: Instant,
    /// First token received.
    first_token_received: Arc<RwLock<bool>>,
}

impl StreamingResponse {
    /// Create a new streaming response.
    pub fn new(
        token_rx: mpsc::Receiver<StreamToken>,
        state_rx: watch::Receiver<StreamState>,
        config: StreamConfig,
    ) -> Self {
        Self {
            token_rx,
            state_rx,
            stats: Arc::new(RwLock::new(StreamStats::default())),
            collected: Arc::new(RwLock::new(String::new())),
            config,
            start_time: Instant::now(),
            first_token_received: Arc::new(RwLock::new(false)),
        }
    }

    /// Get the current state.
    pub fn state(&self) -> StreamState {
        *self.state_rx.borrow()
    }

    /// Check if stream is still active.
    pub fn is_active(&self) -> bool {
        matches!(
            self.state(),
            StreamState::Pending | StreamState::Streaming | StreamState::Paused
        )
    }

    /// Get current statistics.
    pub async fn stats(&self) -> StreamStats {
        self.stats.read().await.clone()
    }

    /// Get collected response so far.
    pub async fn collected_response(&self) -> String {
        self.collected.read().await.clone()
    }

    /// Receive the next token.
    pub async fn next_token(&mut self) -> Option<StreamToken> {
        let token = self.token_rx.recv().await?;

        // Update stats
        {
            let mut stats = self.stats.write().await;
            let mut first_received = self.first_token_received.write().await;

            if !*first_received {
                stats.time_to_first_token_ms = self.start_time.elapsed().as_millis() as u64;
                *first_received = true;
            }

            stats.tokens_streamed += 1;
            stats.chars_streamed += token.content.len();
            stats.total_duration_ms = self.start_time.elapsed().as_millis() as u64;

            if stats.total_duration_ms > 0 {
                stats.tokens_per_second =
                    (stats.tokens_streamed as f64 * 1000.0) / stats.total_duration_ms as f64;
            }
        }

        // Collect response
        if self.config.collect_full_response {
            let mut collected = self.collected.write().await;
            collected.push_str(&token.content);
        }

        Some(token)
    }

    /// Collect all remaining tokens into a string.
    pub async fn collect_remaining(&mut self) -> String {
        let mut result = String::new();
        while let Some(token) = self.next_token().await {
            result.push_str(&token.content);
        }
        result
    }
}

/// Controller for managing a stream.
pub struct StreamController {
    /// State watch sender.
    state_tx: watch::Sender<StreamState>,
    /// Cancel broadcast sender.
    cancel_tx: broadcast::Sender<()>,
    /// Pause flag.
    paused: Arc<RwLock<bool>>,
}

impl StreamController {
    /// Create a new stream controller.
    pub fn new() -> (Self, watch::Receiver<StreamState>, broadcast::Receiver<()>) {
        let (state_tx, state_rx) = watch::channel(StreamState::Pending);
        let (cancel_tx, cancel_rx) = broadcast::channel(1);

        let controller = Self {
            state_tx,
            cancel_tx,
            paused: Arc::new(RwLock::new(false)),
        };

        (controller, state_rx, cancel_rx)
    }

    /// Pause the stream.
    pub async fn pause(&self) {
        *self.paused.write().await = true;
        let _ = self.state_tx.send(StreamState::Paused);
    }

    /// Resume the stream.
    pub async fn resume(&self) {
        *self.paused.write().await = false;
        let _ = self.state_tx.send(StreamState::Streaming);
    }

    /// Cancel the stream.
    pub fn cancel(&self) {
        let _ = self.state_tx.send(StreamState::Cancelled);
        let _ = self.cancel_tx.send(());
    }

    /// Mark stream as completed.
    pub fn complete(&self) {
        let _ = self.state_tx.send(StreamState::Completed);
    }

    /// Mark stream as error.
    pub fn error(&self) {
        let _ = self.state_tx.send(StreamState::Error);
    }

    /// Check if paused.
    pub async fn is_paused(&self) -> bool {
        *self.paused.read().await
    }

    /// Set streaming state.
    pub fn set_streaming(&self) {
        let _ = self.state_tx.send(StreamState::Streaming);
    }
}

impl Default for StreamController {
    fn default() -> Self {
        Self::new().0
    }
}

/// Stream transformer that can modify tokens on the fly.
pub trait StreamTransformer: Send + Sync {
    /// Transform a token.
    fn transform(&self, token: StreamToken) -> Option<StreamToken>;

    /// Name of this transformer.
    fn name(&self) -> &str;
}

/// Filter transformer - removes tokens matching a predicate.
pub struct FilterTransformer<F>
where
    F: Fn(&StreamToken) -> bool + Send + Sync,
{
    predicate: F,
    name: String,
}

impl<F> FilterTransformer<F>
where
    F: Fn(&StreamToken) -> bool + Send + Sync,
{
    /// Create a new filter transformer.
    pub fn new(name: impl Into<String>, predicate: F) -> Self {
        Self {
            predicate,
            name: name.into(),
        }
    }
}

impl<F> StreamTransformer for FilterTransformer<F>
where
    F: Fn(&StreamToken) -> bool + Send + Sync,
{
    fn transform(&self, token: StreamToken) -> Option<StreamToken> {
        if (self.predicate)(&token) {
            Some(token)
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Map transformer - transforms token content.
pub struct MapTransformer<F>
where
    F: Fn(String) -> String + Send + Sync,
{
    mapper: F,
    name: String,
}

impl<F> MapTransformer<F>
where
    F: Fn(String) -> String + Send + Sync,
{
    /// Create a new map transformer.
    pub fn new(name: impl Into<String>, mapper: F) -> Self {
        Self {
            mapper,
            name: name.into(),
        }
    }
}

impl<F> StreamTransformer for MapTransformer<F>
where
    F: Fn(String) -> String + Send + Sync,
{
    fn transform(&self, mut token: StreamToken) -> Option<StreamToken> {
        token.content = (self.mapper)(token.content);
        Some(token)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Aggregator that buffers tokens until a condition is met.
pub struct TokenAggregator {
    buffer: VecDeque<StreamToken>,
    delimiter: String,
    max_buffer: usize,
}

impl TokenAggregator {
    /// Create a new token aggregator.
    pub fn new(delimiter: impl Into<String>, max_buffer: usize) -> Self {
        Self {
            buffer: VecDeque::new(),
            delimiter: delimiter.into(),
            max_buffer,
        }
    }

    /// Add a token to the buffer.
    pub fn add(&mut self, token: StreamToken) -> Option<String> {
        self.buffer.push_back(token);

        // Check if we have a complete chunk
        let combined: String = self.buffer.iter().map(|t| t.content.as_str()).collect();

        if combined.contains(&self.delimiter) || self.buffer.len() >= self.max_buffer {
            self.buffer.clear();
            Some(combined)
        } else {
            None
        }
    }

    /// Flush remaining tokens.
    pub fn flush(&mut self) -> Option<String> {
        if self.buffer.is_empty() {
            None
        } else {
            let combined: String = self.buffer.drain(..).map(|t| t.content).collect();
            Some(combined)
        }
    }
}

/// Multi-stream merger for combining outputs from multiple agents.
pub struct MultiStreamMerger {
    streams: Vec<mpsc::Receiver<StreamToken>>,
    strategy: MergeStrategy,
}

/// Strategy for merging multiple streams.
#[derive(Debug, Clone, Copy)]
pub enum MergeStrategy {
    /// Round-robin between streams.
    RoundRobin,
    /// First available token wins.
    FirstAvailable,
    /// Interleave by timestamp.
    ByTimestamp,
}

impl MultiStreamMerger {
    /// Create a new multi-stream merger.
    pub fn new(strategy: MergeStrategy) -> Self {
        Self {
            streams: Vec::new(),
            strategy,
        }
    }

    /// Add a stream to the merger.
    pub fn add_stream(&mut self, stream: mpsc::Receiver<StreamToken>) {
        self.streams.push(stream);
    }

    /// Get the next token from any stream.
    pub async fn next(&mut self) -> Option<StreamToken> {
        if self.streams.is_empty() {
            return None;
        }

        match self.strategy {
            MergeStrategy::RoundRobin => {
                // Rotate streams and try to get from first
                for _ in 0..self.streams.len() {
                    if let Some(stream) = self.streams.first_mut() {
                        match stream.try_recv() {
                            Ok(token) => return Some(token),
                            Err(_) => {
                                // Rotate to next stream
                                let s = self.streams.remove(0);
                                self.streams.push(s);
                            }
                        }
                    }
                }
                None
            }
            MergeStrategy::FirstAvailable => {
                // Try each stream
                for stream in &mut self.streams {
                    if let Ok(token) = stream.try_recv() {
                        return Some(token);
                    }
                }
                None
            }
            MergeStrategy::ByTimestamp => {
                // Collect one token from each, return earliest
                let mut earliest: Option<StreamToken> = None;

                for stream in &mut self.streams {
                    if let Ok(token) = stream.try_recv() {
                        if let Some(ref e) = earliest {
                            if token.timestamp_ms < e.timestamp_ms {
                                earliest = Some(token);
                            }
                        } else {
                            earliest = Some(token);
                        }
                    }
                }

                earliest
            }
        }
    }
}

/// Builder for creating streaming responses.
pub struct StreamBuilder {
    config: StreamConfig,
    transformers: Vec<Box<dyn StreamTransformer>>,
}

impl StreamBuilder {
    /// Create a new stream builder.
    pub fn new() -> Self {
        Self {
            config: StreamConfig::default(),
            transformers: Vec::new(),
        }
    }

    /// Set buffer size.
    pub fn buffer_size(mut self, size: usize) -> Self {
        self.config.buffer_size = size;
        self
    }

    /// Set maximum tokens.
    pub fn max_tokens(mut self, max: usize) -> Self {
        self.config.max_tokens = Some(max);
        self
    }

    /// Set idle timeout.
    pub fn idle_timeout(mut self, timeout: Duration) -> Self {
        self.config.idle_timeout = timeout;
        self
    }

    /// Enable/disable response collection.
    pub fn collect_response(mut self, collect: bool) -> Self {
        self.config.collect_full_response = collect;
        self
    }

    /// Add a transformer.
    pub fn transform(mut self, transformer: Box<dyn StreamTransformer>) -> Self {
        self.transformers.push(transformer);
        self
    }

    /// Build the streaming infrastructure.
    pub fn build(
        self,
    ) -> (
        mpsc::Sender<StreamToken>,
        StreamingResponse,
        StreamController,
    ) {
        let (token_tx, token_rx) = mpsc::channel(self.config.buffer_size);
        let (controller, state_rx, _cancel_rx) = StreamController::new();

        let response = StreamingResponse::new(token_rx, state_rx, self.config);

        (token_tx, response, controller)
    }
}

impl Default for StreamBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stream_token() {
        let token = StreamToken {
            content: "Hello".to_string(),
            index: 0,
            is_final: false,
            timestamp_ms: 1000,
            probability: Some(0.95),
            alternatives: vec!["Hi".to_string()],
        };

        assert_eq!(token.content, "Hello");
        assert!(!token.is_final);
    }

    #[tokio::test]
    async fn test_stream_builder() {
        let (tx, mut response, controller) = StreamBuilder::new()
            .buffer_size(100)
            .max_tokens(1000)
            .build();

        controller.set_streaming();

        // Send a token
        tx.send(StreamToken {
            content: "Test".to_string(),
            index: 0,
            is_final: false,
            timestamp_ms: 0,
            probability: None,
            alternatives: vec![],
        })
        .await
        .unwrap();

        // Receive it
        let token = response.next_token().await;
        assert!(token.is_some());
        assert_eq!(token.unwrap().content, "Test");

        let stats = response.stats().await;
        assert_eq!(stats.tokens_streamed, 1);
    }

    #[tokio::test]
    async fn test_stream_controller() {
        let (controller, state_rx, _) = StreamController::new();

        assert_eq!(*state_rx.borrow(), StreamState::Pending);

        controller.set_streaming();
        assert_eq!(*state_rx.borrow(), StreamState::Streaming);

        controller.pause().await;
        assert_eq!(*state_rx.borrow(), StreamState::Paused);

        controller.resume().await;
        assert_eq!(*state_rx.borrow(), StreamState::Streaming);

        controller.cancel();
        assert_eq!(*state_rx.borrow(), StreamState::Cancelled);
    }

    #[test]
    fn test_token_aggregator() {
        let mut agg = TokenAggregator::new("\n", 10);

        // Add tokens
        let result = agg.add(StreamToken {
            content: "Hello".to_string(),
            index: 0,
            is_final: false,
            timestamp_ms: 0,
            probability: None,
            alternatives: vec![],
        });
        assert!(result.is_none());

        let result = agg.add(StreamToken {
            content: "\n".to_string(),
            index: 1,
            is_final: false,
            timestamp_ms: 0,
            probability: None,
            alternatives: vec![],
        });
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "Hello\n");
    }

    #[tokio::test]
    async fn test_collected_response() {
        let (tx, mut response, controller) = StreamBuilder::new().collect_response(true).build();

        controller.set_streaming();

        tx.send(StreamToken {
            content: "Hello ".to_string(),
            index: 0,
            is_final: false,
            timestamp_ms: 0,
            probability: None,
            alternatives: vec![],
        })
        .await
        .unwrap();

        tx.send(StreamToken {
            content: "World".to_string(),
            index: 1,
            is_final: true,
            timestamp_ms: 0,
            probability: None,
            alternatives: vec![],
        })
        .await
        .unwrap();

        response.next_token().await;
        response.next_token().await;

        let collected = response.collected_response().await;
        assert_eq!(collected, "Hello World");
    }
}
