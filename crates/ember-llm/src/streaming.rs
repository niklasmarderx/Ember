//! Enhanced streaming support with backpressure and optimization
//!
//! This module provides advanced streaming capabilities including:
//! - Backpressure handling to prevent memory overflow
//! - Chunked response optimization
//! - Memory-efficient streaming buffers
//! - Streaming metrics and monitoring

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

use crate::{Result, StreamChunk};

/// Configuration for streaming behavior
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// Maximum buffer size in bytes before applying backpressure
    pub max_buffer_size: usize,
    /// Size of each chunk in bytes
    pub chunk_size: usize,
    /// Maximum concurrent streams
    pub max_concurrent_streams: usize,
    /// Timeout for idle streams
    pub idle_timeout: Duration,
    /// Enable backpressure handling
    pub backpressure_enabled: bool,
    /// High water mark for backpressure (percentage of buffer)
    pub high_water_mark: f64,
    /// Low water mark for resuming (percentage of buffer)
    pub low_water_mark: f64,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            max_buffer_size: 1024 * 1024, // 1MB
            chunk_size: 4096,             // 4KB
            max_concurrent_streams: 100,
            idle_timeout: Duration::from_secs(30),
            backpressure_enabled: true,
            high_water_mark: 0.8, // 80%
            low_water_mark: 0.2,  // 20%
        }
    }
}

/// Metrics for streaming operations
#[derive(Debug, Default)]
pub struct StreamMetrics {
    /// Total bytes streamed
    pub bytes_streamed: AtomicU64,
    /// Total chunks processed
    pub chunks_processed: AtomicU64,
    /// Current buffer usage in bytes
    pub buffer_usage: AtomicUsize,
    /// Number of backpressure events
    pub backpressure_events: AtomicU64,
    /// Active streams count
    pub active_streams: AtomicUsize,
    /// Total streaming time in milliseconds
    pub total_stream_time_ms: AtomicU64,
}

impl StreamMetrics {
    /// Create new metrics instance
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Record bytes streamed
    pub fn record_bytes(&self, bytes: usize) {
        self.bytes_streamed
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    /// Record chunk processed
    pub fn record_chunk(&self) {
        self.chunks_processed.fetch_add(1, Ordering::Relaxed);
    }

    /// Update buffer usage
    pub fn update_buffer(&self, bytes: usize) {
        self.buffer_usage.store(bytes, Ordering::Relaxed);
    }

    /// Record backpressure event
    pub fn record_backpressure(&self) {
        self.backpressure_events.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment active streams
    pub fn stream_started(&self) {
        self.active_streams.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active streams
    pub fn stream_ended(&self, duration: Duration) {
        self.active_streams.fetch_sub(1, Ordering::Relaxed);
        self.total_stream_time_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
    }

    /// Get current metrics snapshot
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            bytes_streamed: self.bytes_streamed.load(Ordering::Relaxed),
            chunks_processed: self.chunks_processed.load(Ordering::Relaxed),
            buffer_usage: self.buffer_usage.load(Ordering::Relaxed),
            backpressure_events: self.backpressure_events.load(Ordering::Relaxed),
            active_streams: self.active_streams.load(Ordering::Relaxed),
            total_stream_time_ms: self.total_stream_time_ms.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of stream metrics at a point in time
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    /// Total bytes streamed
    pub bytes_streamed: u64,
    /// Total chunks processed
    pub chunks_processed: u64,
    /// Current buffer usage in bytes
    pub buffer_usage: usize,
    /// Number of backpressure events
    pub backpressure_events: u64,
    /// Active streams count
    pub active_streams: usize,
    /// Total streaming time in milliseconds
    pub total_stream_time_ms: u64,
}

/// Backpressure-aware stream wrapper
pub struct BackpressureStream<S> {
    inner: S,
    config: StreamConfig,
    metrics: Arc<StreamMetrics>,
    buffer_size: AtomicUsize,
    paused: bool,
    start_time: Instant,
}

impl<S> BackpressureStream<S>
where
    S: Stream<Item = Result<StreamChunk>> + Unpin,
{
    /// Create a new backpressure-aware stream
    pub fn new(inner: S, config: StreamConfig, metrics: Arc<StreamMetrics>) -> Self {
        metrics.stream_started();
        Self {
            inner,
            config,
            metrics,
            buffer_size: AtomicUsize::new(0),
            paused: false,
            start_time: Instant::now(),
        }
    }

    /// Check if backpressure should be applied
    fn should_pause(&self) -> bool {
        if !self.config.backpressure_enabled {
            return false;
        }
        let usage = self.buffer_size.load(Ordering::Relaxed);
        let threshold = (self.config.max_buffer_size as f64 * self.config.high_water_mark) as usize;
        usage >= threshold
    }

    /// Check if stream can resume
    fn can_resume(&self) -> bool {
        let usage = self.buffer_size.load(Ordering::Relaxed);
        let threshold = (self.config.max_buffer_size as f64 * self.config.low_water_mark) as usize;
        usage <= threshold
    }

    /// Add to buffer size
    pub fn add_to_buffer(&self, bytes: usize) {
        self.buffer_size.fetch_add(bytes, Ordering::Relaxed);
        self.metrics
            .update_buffer(self.buffer_size.load(Ordering::Relaxed));
    }

    /// Remove from buffer size
    pub fn remove_from_buffer(&self, bytes: usize) {
        self.buffer_size.fetch_sub(bytes, Ordering::Relaxed);
        self.metrics
            .update_buffer(self.buffer_size.load(Ordering::Relaxed));
    }
}

impl<S> Stream for BackpressureStream<S>
where
    S: Stream<Item = Result<StreamChunk>> + Unpin,
{
    type Item = Result<StreamChunk>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Check backpressure
        if self.paused {
            if self.can_resume() {
                self.paused = false;
            } else {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        }

        if self.should_pause() {
            self.paused = true;
            self.metrics.record_backpressure();
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }

        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                let bytes = chunk.content.as_ref().map(|s| s.len()).unwrap_or(0);
                self.add_to_buffer(bytes);
                self.metrics.record_bytes(bytes);
                self.metrics.record_chunk();
                Poll::Ready(Some(Ok(chunk)))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => {
                self.metrics.stream_ended(self.start_time.elapsed());
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Stream manager for handling multiple concurrent streams
pub struct StreamManager {
    config: StreamConfig,
    metrics: Arc<StreamMetrics>,
    semaphore: Arc<Semaphore>,
}

impl StreamManager {
    /// Create a new stream manager
    pub fn new(config: StreamConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_streams));
        Self {
            config,
            metrics: StreamMetrics::new(),
            semaphore,
        }
    }

    /// Get current metrics
    pub fn metrics(&self) -> Arc<StreamMetrics> {
        Arc::clone(&self.metrics)
    }

    /// Get metrics snapshot
    pub fn metrics_snapshot(&self) -> MetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Wrap a stream with backpressure handling
    pub fn wrap_stream<S>(&self, stream: S) -> BackpressureStream<S>
    where
        S: Stream<Item = Result<StreamChunk>> + Unpin,
    {
        BackpressureStream::new(stream, self.config.clone(), Arc::clone(&self.metrics))
    }

    /// Acquire a permit for a new stream
    pub async fn acquire_permit(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.semaphore.clone().acquire_owned().await.ok()
    }

    /// Try to acquire a permit without waiting
    pub fn try_acquire_permit(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.semaphore.clone().try_acquire_owned().ok()
    }
}

/// Chunked buffer for memory-efficient streaming
pub struct ChunkedBuffer {
    chunks: Vec<Vec<u8>>,
    chunk_size: usize,
    total_size: usize,
    max_size: usize,
}

impl ChunkedBuffer {
    /// Create a new chunked buffer
    pub fn new(chunk_size: usize, max_size: usize) -> Self {
        Self {
            chunks: Vec::new(),
            chunk_size,
            total_size: 0,
            max_size,
        }
    }

    /// Add data to the buffer
    pub fn push(&mut self, data: &[u8]) -> bool {
        if self.total_size + data.len() > self.max_size {
            return false;
        }

        let mut remaining = data;
        while !remaining.is_empty() {
            if self.chunks.is_empty() || self.chunks.last().unwrap().len() >= self.chunk_size {
                self.chunks.push(Vec::with_capacity(self.chunk_size));
            }

            let current = self.chunks.last_mut().unwrap();
            let space = self.chunk_size - current.len();
            let to_copy = remaining.len().min(space);
            current.extend_from_slice(&remaining[..to_copy]);
            remaining = &remaining[to_copy..];
        }

        self.total_size += data.len();
        true
    }

    /// Get total size
    pub fn len(&self) -> usize {
        self.total_size
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.total_size == 0
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.chunks.clear();
        self.total_size = 0;
    }

    /// Drain all data
    pub fn drain(&mut self) -> Vec<u8> {
        let mut result = Vec::with_capacity(self.total_size);
        for chunk in self.chunks.drain(..) {
            result.extend(chunk);
        }
        self.total_size = 0;
        result
    }

    /// Get data as iterator over chunks
    pub fn chunks(&self) -> impl Iterator<Item = &[u8]> {
        self.chunks.iter().map(|c| c.as_slice())
    }
}

/// Trait for optimized streaming responses
#[async_trait]
pub trait OptimizedStreaming {
    /// Stream with automatic chunking and backpressure
    async fn stream_optimized(
        &self,
        config: &StreamConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>>;
}

/// Stream aggregator for combining multiple streams
pub struct StreamAggregator<S> {
    streams: Vec<S>,
    current_index: usize,
}

impl<S> StreamAggregator<S>
where
    S: Stream<Item = Result<StreamChunk>> + Unpin,
{
    /// Create a new stream aggregator
    pub fn new(streams: Vec<S>) -> Self {
        Self {
            streams,
            current_index: 0,
        }
    }
}

impl<S> Stream for StreamAggregator<S>
where
    S: Stream<Item = Result<StreamChunk>> + Unpin,
{
    type Item = Result<StreamChunk>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        while self.current_index < self.streams.len() {
            let idx = self.current_index;
            match Pin::new(&mut self.streams[idx]).poll_next(cx) {
                Poll::Ready(Some(item)) => return Poll::Ready(Some(item)),
                Poll::Ready(None) => {
                    self.current_index += 1;
                    continue;
                }
                Poll::Pending => return Poll::Pending,
            }
        }
        Poll::Ready(None)
    }
}

/// Rate-limited stream wrapper
pub struct RateLimitedStream<S> {
    inner: S,
    tokens_per_second: f64,
    last_emit: Option<Instant>,
    accumulated_tokens: f64,
}

impl<S> RateLimitedStream<S>
where
    S: Stream<Item = Result<StreamChunk>> + Unpin,
{
    /// Create a new rate-limited stream
    pub fn new(inner: S, tokens_per_second: f64) -> Self {
        Self {
            inner,
            tokens_per_second,
            last_emit: None,
            accumulated_tokens: tokens_per_second, // Start with full bucket
        }
    }
}

impl<S> Stream for RateLimitedStream<S>
where
    S: Stream<Item = Result<StreamChunk>> + Unpin,
{
    type Item = Result<StreamChunk>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let now = Instant::now();

        // Refill tokens based on elapsed time
        if let Some(last) = self.last_emit {
            let elapsed = now.duration_since(last).as_secs_f64();
            self.accumulated_tokens = (self.accumulated_tokens + elapsed * self.tokens_per_second)
                .min(self.tokens_per_second);
        }

        // Check if we have tokens to emit
        if self.accumulated_tokens < 1.0 {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }

        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(item)) => {
                self.accumulated_tokens -= 1.0;
                self.last_emit = Some(now);
                Poll::Ready(Some(item))
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    fn make_chunk(content: &str) -> StreamChunk {
        StreamChunk {
            content: Some(content.to_string()),
            done: false,
            tool_calls: None,
            finish_reason: None,
        }
    }

    #[tokio::test]
    async fn test_stream_metrics() {
        let metrics = StreamMetrics::new();

        metrics.record_bytes(100);
        metrics.record_chunk();
        metrics.stream_started();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.bytes_streamed, 100);
        assert_eq!(snapshot.chunks_processed, 1);
        assert_eq!(snapshot.active_streams, 1);
    }

    #[tokio::test]
    async fn test_chunked_buffer() {
        let mut buffer = ChunkedBuffer::new(10, 100);

        assert!(buffer.push(b"hello"));
        assert!(buffer.push(b"world"));
        assert_eq!(buffer.len(), 10);

        let data = buffer.drain();
        assert_eq!(data, b"helloworld");
        assert!(buffer.is_empty());
    }

    #[tokio::test]
    async fn test_chunked_buffer_max_size() {
        let mut buffer = ChunkedBuffer::new(10, 20);

        assert!(buffer.push(b"12345678901234567890")); // 20 bytes
        assert!(!buffer.push(b"extra")); // Should fail
        assert_eq!(buffer.len(), 20);
    }

    #[tokio::test]
    async fn test_stream_manager() {
        let config = StreamConfig::default();
        let manager = StreamManager::new(config);

        let permit = manager.try_acquire_permit();
        assert!(permit.is_some());

        let metrics = manager.metrics_snapshot();
        assert_eq!(metrics.active_streams, 0);
    }

    #[tokio::test]
    async fn test_stream_aggregator() {
        let stream1 = stream::iter(vec![Ok(make_chunk("a")), Ok(make_chunk("b"))]);
        let stream2 = stream::iter(vec![Ok(make_chunk("c"))]);

        let mut aggregator = StreamAggregator::new(vec![stream1, stream2]);

        let mut results = vec![];
        use futures::StreamExt;
        while let Some(Ok(chunk)) = aggregator.next().await {
            if let Some(content) = chunk.content {
                results.push(content);
            }
        }

        assert_eq!(results, vec!["a", "b", "c"]);
    }
}
