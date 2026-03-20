//! Semantic Caching for LLM Responses
//!
//! This module provides a semantic caching layer that can identify and return
//! cached responses for semantically similar queries, reducing API costs and
//! improving response times.
//!
//! # Features
//!
//! - **Embedding-based similarity matching**: Uses cosine similarity to find similar queries
//! - **Configurable threshold**: Adjust sensitivity (default: 0.95)
//! - **TTL support**: Automatic expiration of cache entries
//! - **LRU eviction**: Removes least recently used entries when full
//! - **Context awareness**: Different cache keys for different contexts
//! - **Statistics tracking**: Hit rate, miss rate, estimated savings

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info};

/// Configuration for the semantic cache.
#[derive(Debug, Clone)]
pub struct SemanticCacheConfig {
    /// Minimum similarity threshold for cache hits (0.0 - 1.0).
    pub similarity_threshold: f32,
    /// Maximum number of entries in the cache.
    pub max_entries: usize,
    /// Default TTL in seconds for cache entries.
    pub default_ttl_seconds: u64,
    /// Whether to include context hash in cache lookup.
    pub context_aware: bool,
}

impl Default for SemanticCacheConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.95,
            max_entries: 10000,
            default_ttl_seconds: 3600, // 1 hour
            context_aware: true,
        }
    }
}

/// A single cache entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Original query text.
    pub query: String,
    /// Query embedding vector.
    pub embedding: Vec<f32>,
    /// Cached response.
    pub response: String,
    /// Context hash (system prompt + conversation summary).
    pub context_hash: u64,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// TTL in seconds.
    pub ttl_seconds: u64,
    /// Number of cache hits.
    pub hit_count: u64,
    /// Model used for response.
    pub model: String,
    /// Estimated tokens in response.
    pub response_tokens: u32,
    /// Last access time for LRU.
    pub last_accessed: DateTime<Utc>,
}

impl CacheEntry {
    /// Check if the entry has expired.
    pub fn is_expired(&self) -> bool {
        let now = Utc::now();
        let age = now.signed_duration_since(self.created_at);
        age.num_seconds() as u64 > self.ttl_seconds
    }
}

/// Context information for cache key generation.
#[derive(Debug, Clone, Default)]
pub struct CacheContext {
    /// System prompt used.
    pub system_prompt: Option<String>,
    /// Conversation history summary.
    pub conversation_summary: Option<String>,
    /// Model name.
    pub model: Option<String>,
    /// Temperature setting.
    pub temperature: Option<f32>,
}

impl CacheContext {
    /// Compute a hash of the context.
    pub fn compute_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        if let Some(ref sp) = self.system_prompt {
            sp.hash(&mut hasher);
        }
        if let Some(ref cs) = self.conversation_summary {
            cs.hash(&mut hasher);
        }
        if let Some(ref m) = self.model {
            m.hash(&mut hasher);
        }
        hasher.finish()
    }
}

/// Statistics for the semantic cache.
#[derive(Debug, Default)]
pub struct CacheStats {
    /// Number of cache hits.
    pub hits: AtomicU64,
    /// Number of cache misses.
    pub misses: AtomicU64,
    /// Total similarity of hits (for averaging).
    pub total_hit_similarity: AtomicU64,
    /// Estimated tokens saved.
    pub estimated_tokens_saved: AtomicU64,
    /// Estimated cost saved (in millicents).
    pub estimated_cost_saved_millicents: AtomicU64,
}

impl CacheStats {
    /// Record a cache hit.
    pub fn record_hit(&self, similarity: f32, tokens_saved: u32) {
        self.hits.fetch_add(1, Ordering::Relaxed);
        // Store similarity as integer (multiplied by 1000)
        self.total_hit_similarity
            .fetch_add((similarity * 1000.0) as u64, Ordering::Relaxed);
        self.estimated_tokens_saved
            .fetch_add(tokens_saved as u64, Ordering::Relaxed);
        // Estimate $0.002 per 1000 tokens = 0.2 millicents per token
        self.estimated_cost_saved_millicents
            .fetch_add((tokens_saved as f64 * 0.2) as u64, Ordering::Relaxed);
    }

    /// Record a cache miss.
    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Get the hit rate.
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// Get the average similarity of hits.
    pub fn average_hit_similarity(&self) -> f32 {
        let hits = self.hits.load(Ordering::Relaxed);
        let total_sim = self.total_hit_similarity.load(Ordering::Relaxed);
        if hits == 0 {
            0.0
        } else {
            (total_sim as f32 / hits as f32) / 1000.0
        }
    }

    /// Get estimated cost savings in dollars.
    pub fn estimated_savings_usd(&self) -> f64 {
        self.estimated_cost_saved_millicents.load(Ordering::Relaxed) as f64 / 100_000.0
    }

    /// Get a summary of the stats.
    pub fn summary(&self) -> CacheStatsSummary {
        CacheStatsSummary {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            hit_rate: self.hit_rate(),
            average_similarity: self.average_hit_similarity(),
            tokens_saved: self.estimated_tokens_saved.load(Ordering::Relaxed),
            estimated_savings_usd: self.estimated_savings_usd(),
        }
    }
}

/// Summary of cache statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStatsSummary {
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
    pub average_similarity: f32,
    pub tokens_saved: u64,
    pub estimated_savings_usd: f64,
}

/// Trait for embedding providers.
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding for the given text.
    fn embed(&self, text: &str) -> Result<Vec<f32>, String>;

    /// Get the embedding dimension.
    fn dimension(&self) -> usize;
}

/// Simple local embedder using basic text features.
/// For production, use a proper embedding model like sentence-transformers.
#[derive(Debug, Clone)]
pub struct SimpleEmbedder {
    dimension: usize,
}

impl Default for SimpleEmbedder {
    fn default() -> Self {
        Self::new(384)
    }
}

impl SimpleEmbedder {
    /// Create a new simple embedder with the given dimension.
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }

    /// Generate a simple hash-based embedding (for testing/development).
    fn hash_embedding(&self, text: &str) -> Vec<f32> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut embedding = vec![0.0f32; self.dimension];

        for (i, word) in words.iter().enumerate() {
            let mut hasher = DefaultHasher::new();
            word.to_lowercase().hash(&mut hasher);
            let hash = hasher.finish();

            // Distribute the hash across multiple dimensions
            for j in 0..8 {
                let idx = ((hash >> (j * 8)) as usize + i * 7) % self.dimension;
                embedding[idx] += 1.0;
            }
        }

        // Normalize the embedding
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut embedding {
                *x /= norm;
            }
        }

        embedding
    }
}

impl EmbeddingProvider for SimpleEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        Ok(self.hash_embedding(text))
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

/// Compute cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a * norm_b)
}

/// The main semantic cache.
pub struct SemanticCache {
    /// Cache entries indexed by a simple key.
    entries: RwLock<Vec<CacheEntry>>,
    /// Embedding provider.
    embedder: Arc<dyn EmbeddingProvider>,
    /// Configuration.
    config: SemanticCacheConfig,
    /// Statistics.
    stats: Arc<CacheStats>,
}

impl SemanticCache {
    /// Create a new semantic cache with default embedder.
    pub fn new(config: SemanticCacheConfig) -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
            embedder: Arc::new(SimpleEmbedder::default()),
            config,
            stats: Arc::new(CacheStats::default()),
        }
    }

    /// Create a new semantic cache with a custom embedder.
    pub fn with_embedder(
        config: SemanticCacheConfig,
        embedder: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
            embedder,
            config,
            stats: Arc::new(CacheStats::default()),
        }
    }

    /// Try to get a cached response for the given query.
    pub fn get(&self, query: &str, context: &CacheContext) -> Option<CacheHit> {
        let query_embedding = match self.embedder.embed(query) {
            Ok(e) => e,
            Err(_) => return None,
        };

        let context_hash = if self.config.context_aware {
            context.compute_hash()
        } else {
            0
        };

        let mut entries = self.entries.write();

        // Find best matching entry
        let mut best_match: Option<(usize, f32)> = None;

        for (idx, entry) in entries.iter().enumerate() {
            // Skip if context doesn't match (when context-aware)
            if self.config.context_aware && entry.context_hash != context_hash {
                continue;
            }

            // Skip expired entries
            if entry.is_expired() {
                continue;
            }

            let similarity = cosine_similarity(&query_embedding, &entry.embedding);

            if similarity >= self.config.similarity_threshold {
                if best_match.map(|(_, s)| similarity > s).unwrap_or(true) {
                    best_match = Some((idx, similarity));
                }
            }
        }

        if let Some((idx, similarity)) = best_match {
            // Update access time and hit count
            let entry = &mut entries[idx];
            entry.last_accessed = Utc::now();
            entry.hit_count += 1;

            let response = entry.response.clone();
            let tokens = entry.response_tokens;

            // Record stats
            self.stats.record_hit(similarity, tokens);

            debug!(
                similarity = similarity,
                query = query,
                "Semantic cache hit"
            );

            return Some(CacheHit {
                response,
                similarity,
                original_query: entry.query.clone(),
                hit_count: entry.hit_count,
            });
        }

        self.stats.record_miss();
        None
    }

    /// Store a response in the cache.
    pub fn put(
        &self,
        query: &str,
        response: &str,
        context: &CacheContext,
        model: &str,
    ) -> Result<(), String> {
        let embedding = self.embedder.embed(query)?;
        let context_hash = if self.config.context_aware {
            context.compute_hash()
        } else {
            0
        };

        // Estimate tokens (rough: 4 chars per token)
        let response_tokens = (response.len() / 4) as u32;

        let entry = CacheEntry {
            query: query.to_string(),
            embedding,
            response: response.to_string(),
            context_hash,
            created_at: Utc::now(),
            ttl_seconds: self.config.default_ttl_seconds,
            hit_count: 0,
            model: model.to_string(),
            response_tokens,
            last_accessed: Utc::now(),
        };

        let mut entries = self.entries.write();

        // Evict expired entries first
        entries.retain(|e| !e.is_expired());

        // If still over capacity, evict LRU entries
        if entries.len() >= self.config.max_entries {
            self.evict_lru(&mut entries);
        }

        entries.push(entry);

        debug!(query = query, model = model, "Cached response");

        Ok(())
    }

    /// Evict least recently used entries.
    fn evict_lru(&self, entries: &mut Vec<CacheEntry>) {
        if entries.is_empty() {
            return;
        }

        // Sort by last_accessed (oldest first)
        entries.sort_by(|a, b| a.last_accessed.cmp(&b.last_accessed));

        // Remove oldest 10% of entries
        let to_remove = entries.len() / 10;
        let to_remove = to_remove.max(1);
        entries.drain(0..to_remove);

        debug!(removed = to_remove, "Evicted LRU cache entries");
    }

    /// Clear the entire cache.
    pub fn clear(&self) {
        let mut entries = self.entries.write();
        entries.clear();
        info!("Semantic cache cleared");
    }

    /// Get the current number of entries.
    pub fn len(&self) -> usize {
        self.entries.read().len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }

    /// Get cache statistics.
    pub fn stats(&self) -> CacheStatsSummary {
        self.stats.summary()
    }

    /// Get the configuration.
    pub fn config(&self) -> &SemanticCacheConfig {
        &self.config
    }

    /// Invalidate entries matching a pattern.
    pub fn invalidate_pattern(&self, pattern: &str) {
        let mut entries = self.entries.write();
        let pattern_lower = pattern.to_lowercase();
        entries.retain(|e| !e.query.to_lowercase().contains(&pattern_lower));
        debug!(pattern = pattern, "Invalidated cache entries");
    }

    /// Invalidate entries for a specific model.
    pub fn invalidate_model(&self, model: &str) {
        let mut entries = self.entries.write();
        entries.retain(|e| e.model != model);
        debug!(model = model, "Invalidated cache entries for model");
    }
}

/// Result of a successful cache hit.
#[derive(Debug, Clone)]
pub struct CacheHit {
    /// The cached response.
    pub response: String,
    /// Similarity score (0.0 - 1.0).
    pub similarity: f32,
    /// The original query that generated this response.
    pub original_query: String,
    /// Number of times this entry has been hit.
    pub hit_count: u64,
}

/// Builder for SemanticCache.
pub struct SemanticCacheBuilder {
    config: SemanticCacheConfig,
    embedder: Option<Arc<dyn EmbeddingProvider>>,
}

impl Default for SemanticCacheBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticCacheBuilder {
    /// Create a new builder with default config.
    pub fn new() -> Self {
        Self {
            config: SemanticCacheConfig::default(),
            embedder: None,
        }
    }

    /// Set the similarity threshold.
    pub fn similarity_threshold(mut self, threshold: f32) -> Self {
        self.config.similarity_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Set the maximum number of entries.
    pub fn max_entries(mut self, max: usize) -> Self {
        self.config.max_entries = max;
        self
    }

    /// Set the default TTL in seconds.
    pub fn default_ttl_seconds(mut self, ttl: u64) -> Self {
        self.config.default_ttl_seconds = ttl;
        self
    }

    /// Set whether the cache is context-aware.
    pub fn context_aware(mut self, aware: bool) -> Self {
        self.config.context_aware = aware;
        self
    }

    /// Set a custom embedding provider.
    pub fn embedder(mut self, embedder: Arc<dyn EmbeddingProvider>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    /// Build the cache.
    pub fn build(self) -> SemanticCache {
        match self.embedder {
            Some(e) => SemanticCache::with_embedder(self.config, e),
            None => SemanticCache::new(self.config),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c)).abs() < 0.001);

        let d = vec![0.707, 0.707, 0.0];
        let sim = cosine_similarity(&a, &d);
        assert!(sim > 0.7 && sim < 0.8);
    }

    #[test]
    fn test_simple_embedder() {
        let embedder = SimpleEmbedder::default();

        let e1 = embedder.embed("hello world").unwrap();
        let e2 = embedder.embed("hello world").unwrap();
        let e3 = embedder.embed("goodbye universe").unwrap();

        // Same text should produce same embedding
        assert_eq!(e1, e2);

        // Different text should produce different embedding
        assert_ne!(e1, e3);

        // Embedding should be normalized (length ~1)
        let norm: f32 = e1.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_cache_put_get() {
        let cache = SemanticCacheBuilder::new()
            .similarity_threshold(0.9)
            .max_entries(100)
            .build();

        let context = CacheContext::default();

        // Store a response
        cache
            .put(
                "What is the capital of France?",
                "The capital of France is Paris.",
                &context,
                "gpt-4",
            )
            .unwrap();

        // Exact match should hit
        let hit = cache.get("What is the capital of France?", &context);
        assert!(hit.is_some());
        let hit = hit.unwrap();
        assert!(hit.similarity > 0.99);
        assert_eq!(hit.response, "The capital of France is Paris.");

        // Similar query should hit (depending on embedder quality)
        let hit = cache.get("Tell me the capital city of France", &context);
        // With simple embedder, this may or may not hit
        // In production with proper embeddings, this should hit
    }

    #[test]
    fn test_cache_stats() {
        let cache = SemanticCacheBuilder::new()
            .similarity_threshold(0.9)
            .build();

        let context = CacheContext::default();

        // Miss
        let _ = cache.get("test query", &context);

        // Put
        cache
            .put("test query", "test response", &context, "gpt-4")
            .unwrap();

        // Hit
        let _ = cache.get("test query", &context);

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_cache_context_awareness() {
        let cache = SemanticCacheBuilder::new()
            .similarity_threshold(0.9)
            .context_aware(true)
            .build();

        let context1 = CacheContext {
            system_prompt: Some("You are a helpful assistant.".to_string()),
            ..Default::default()
        };

        let context2 = CacheContext {
            system_prompt: Some("You are a pirate.".to_string()),
            ..Default::default()
        };

        cache
            .put("Hello", "Hi there!", &context1, "gpt-4")
            .unwrap();

        // Same context should hit
        let hit = cache.get("Hello", &context1);
        assert!(hit.is_some());

        // Different context should miss
        let hit = cache.get("Hello", &context2);
        assert!(hit.is_none());
    }

    #[test]
    fn test_cache_expiration() {
        let cache = SemanticCacheBuilder::new()
            .default_ttl_seconds(0) // Expire immediately
            .build();

        let context = CacheContext::default();

        cache
            .put("test", "response", &context, "gpt-4")
            .unwrap();

        // Should miss because TTL is 0
        std::thread::sleep(std::time::Duration::from_millis(10));
        let hit = cache.get("test", &context);
        assert!(hit.is_none());
    }

    #[test]
    fn test_cache_eviction() {
        let cache = SemanticCacheBuilder::new()
            .max_entries(3)
            .similarity_threshold(0.99)
            .build();

        let context = CacheContext::default();

        for i in 0..5 {
            cache
                .put(&format!("query{}", i), &format!("response{}", i), &context, "gpt-4")
                .unwrap();
        }

        // Should have evicted some entries
        assert!(cache.len() <= 3);
    }

    #[test]
    fn test_cache_invalidation() {
        let cache = SemanticCacheBuilder::new().build();

        let context = CacheContext::default();

        cache
            .put("weather in Paris", "Sunny", &context, "gpt-4")
            .unwrap();
        cache
            .put("weather in London", "Rainy", &context, "gpt-4")
            .unwrap();
        cache
            .put("news today", "Headlines", &context, "gpt-4")
            .unwrap();

        assert_eq!(cache.len(), 3);

        cache.invalidate_pattern("weather");

        assert_eq!(cache.len(), 1);
    }
}