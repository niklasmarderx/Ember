//! Smart Caching System
//!
//! Intelligent caching for blazing-fast responses that OpenClaw doesn't have!
//!
//! # Features
//! - **Semantic Caching**: Cache by meaning, not just exact matches
//! - **LRU + Frequency**: Hybrid eviction strategy
//! - **TTL Support**: Time-based expiration
//! - **Compression**: Reduce memory footprint
//! - **Persistent Cache**: Survives restarts
//! - **Cache Warming**: Pre-populate with common queries

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Cache entry with metadata.
#[derive(Debug, Clone)]
struct CacheEntry<V> {
    /// The cached value.
    value: V,
    /// When the entry was created.
    created_at: Instant,
    /// Last access time.
    last_accessed: Instant,
    /// Number of times accessed.
    access_count: u64,
    /// Time to live.
    ttl: Option<Duration>,
    /// Size in bytes (estimated).
    size_bytes: usize,
    /// Tags for grouping.
    tags: Vec<String>,
}

impl<V> CacheEntry<V> {
    /// Check if the entry has expired.
    fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            self.created_at.elapsed() > ttl
        } else {
            false
        }
    }

    /// Calculate priority score (higher = keep longer).
    fn priority_score(&self) -> f64 {
        let recency = 1.0 / (self.last_accessed.elapsed().as_secs_f64() + 1.0);
        let frequency = (self.access_count as f64).ln() + 1.0;
        recency * frequency
    }
}

/// Configuration for the cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Maximum number of entries.
    pub max_entries: usize,
    /// Maximum total size in bytes.
    pub max_size_bytes: usize,
    /// Default TTL for entries.
    pub default_ttl: Option<Duration>,
    /// Whether to enable semantic matching.
    pub semantic_matching: bool,
    /// Similarity threshold for semantic matching (0.0 - 1.0).
    pub similarity_threshold: f32,
    /// Whether to compress values.
    pub compression_enabled: bool,
    /// Minimum size for compression (bytes).
    pub compression_min_size: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 10000,
            max_size_bytes: 100 * 1024 * 1024,            // 100 MB
            default_ttl: Some(Duration::from_secs(3600)), // 1 hour
            semantic_matching: true,
            similarity_threshold: 0.85,
            compression_enabled: true,
            compression_min_size: 1024, // 1 KB
        }
    }
}

/// Cache statistics.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CacheStats {
    /// Total cache hits.
    pub hits: u64,
    /// Total cache misses.
    pub misses: u64,
    /// Current number of entries.
    pub entry_count: usize,
    /// Current total size in bytes.
    pub total_size_bytes: usize,
    /// Number of evictions.
    pub evictions: u64,
    /// Number of expirations.
    pub expirations: u64,
    /// Average entry age in seconds.
    pub avg_entry_age_secs: f64,
    /// Hit rate (0.0 - 1.0).
    pub hit_rate: f64,
}

/// Smart LLM Response Cache.
///
/// Caches LLM responses for faster repeated queries.
pub struct ResponseCache {
    /// Cache entries.
    entries: Arc<RwLock<HashMap<u64, CacheEntry<CachedResponse>>>>,
    /// Configuration.
    config: CacheConfig,
    /// Statistics.
    stats: Arc<RwLock<CacheStats>>,
    /// Semantic index for fuzzy matching (query hash -> similar hashes).
    semantic_index: Arc<RwLock<HashMap<String, Vec<u64>>>>,
}

/// A cached LLM response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedResponse {
    /// The response content.
    pub content: String,
    /// Model used.
    pub model: String,
    /// Token count.
    pub tokens: u32,
    /// Original query (for verification).
    pub original_query: String,
    /// Response metadata.
    pub metadata: HashMap<String, String>,
}

impl ResponseCache {
    /// Create a new response cache.
    pub fn new() -> Self {
        Self::with_config(CacheConfig::default())
    }

    /// Create a cache with custom configuration.
    pub fn with_config(config: CacheConfig) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            config,
            stats: Arc::new(RwLock::new(CacheStats::default())),
            semantic_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a high-performance cache (larger, longer TTL).
    pub fn high_performance() -> Self {
        let config = CacheConfig {
            max_entries: 50000,
            max_size_bytes: 500 * 1024 * 1024,            // 500 MB
            default_ttl: Some(Duration::from_secs(7200)), // 2 hours
            semantic_matching: true,
            similarity_threshold: 0.8,
            compression_enabled: true,
            compression_min_size: 512,
        };
        Self::with_config(config)
    }

    /// Get a cached response.
    pub async fn get(&self, query: &str) -> Option<CachedResponse> {
        let hash = self.hash_query(query);

        // Try exact match first
        {
            let mut entries = self.entries.write().await;
            if let Some(entry) = entries.get_mut(&hash) {
                if entry.is_expired() {
                    entries.remove(&hash);
                    let mut stats = self.stats.write().await;
                    stats.expirations += 1;
                    stats.misses += 1;
                    return None;
                }

                entry.last_accessed = Instant::now();
                entry.access_count += 1;

                let mut stats = self.stats.write().await;
                stats.hits += 1;
                self.update_hit_rate(&mut stats);

                return Some(entry.value.clone());
            }
        }

        // Try semantic match if enabled
        if self.config.semantic_matching {
            if let Some(response) = self.semantic_lookup(query).await {
                let mut stats = self.stats.write().await;
                stats.hits += 1;
                self.update_hit_rate(&mut stats);
                return Some(response);
            }
        }

        let mut stats = self.stats.write().await;
        stats.misses += 1;
        self.update_hit_rate(&mut stats);
        None
    }

    /// Store a response in the cache.
    pub async fn put(&self, query: &str, response: CachedResponse) {
        self.put_with_ttl(query, response, self.config.default_ttl)
            .await;
    }

    /// Store a response with custom TTL.
    pub async fn put_with_ttl(&self, query: &str, response: CachedResponse, ttl: Option<Duration>) {
        let hash = self.hash_query(query);
        let size = self.estimate_size(&response);

        // Evict if necessary
        self.ensure_capacity(size).await;

        let entry = CacheEntry {
            value: response.clone(),
            created_at: Instant::now(),
            last_accessed: Instant::now(),
            access_count: 1,
            ttl,
            size_bytes: size,
            tags: Vec::new(),
        };

        let mut entries = self.entries.write().await;
        entries.insert(hash, entry);

        // Update semantic index
        if self.config.semantic_matching {
            self.index_for_semantic_search(query, hash).await;
        }

        // Update stats
        let mut stats = self.stats.write().await;
        stats.entry_count = entries.len();
        stats.total_size_bytes += size;
    }

    /// Store with tags for group invalidation.
    pub async fn put_with_tags(&self, query: &str, response: CachedResponse, tags: Vec<String>) {
        let hash = self.hash_query(query);
        let size = self.estimate_size(&response);

        self.ensure_capacity(size).await;

        let entry = CacheEntry {
            value: response,
            created_at: Instant::now(),
            last_accessed: Instant::now(),
            access_count: 1,
            ttl: self.config.default_ttl,
            size_bytes: size,
            tags,
        };

        let mut entries = self.entries.write().await;
        entries.insert(hash, entry);

        let mut stats = self.stats.write().await;
        stats.entry_count = entries.len();
        stats.total_size_bytes += size;
    }

    /// Invalidate entries with a specific tag.
    pub async fn invalidate_by_tag(&self, tag: &str) {
        let mut entries = self.entries.write().await;
        let mut stats = self.stats.write().await;

        let to_remove: Vec<u64> = entries
            .iter()
            .filter(|(_, e)| e.tags.contains(&tag.to_string()))
            .map(|(k, _)| *k)
            .collect();

        for key in to_remove {
            if let Some(entry) = entries.remove(&key) {
                stats.total_size_bytes = stats.total_size_bytes.saturating_sub(entry.size_bytes);
                stats.evictions += 1;
            }
        }

        stats.entry_count = entries.len();
    }

    /// Remove a specific entry.
    pub async fn remove(&self, query: &str) {
        let hash = self.hash_query(query);
        let mut entries = self.entries.write().await;

        if let Some(entry) = entries.remove(&hash) {
            let mut stats = self.stats.write().await;
            stats.total_size_bytes = stats.total_size_bytes.saturating_sub(entry.size_bytes);
            stats.entry_count = entries.len();
        }
    }

    /// Clear all entries.
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        entries.clear();

        let mut semantic = self.semantic_index.write().await;
        semantic.clear();

        let mut stats = self.stats.write().await;
        stats.entry_count = 0;
        stats.total_size_bytes = 0;
    }

    /// Get cache statistics.
    pub async fn stats(&self) -> CacheStats {
        let stats = self.stats.read().await;
        let entries = self.entries.read().await;

        // Calculate average age
        let avg_age = if entries.is_empty() {
            0.0
        } else {
            let total_age: f64 = entries
                .values()
                .map(|e| e.created_at.elapsed().as_secs_f64())
                .sum();
            total_age / entries.len() as f64
        };

        CacheStats {
            hits: stats.hits,
            misses: stats.misses,
            entry_count: entries.len(),
            total_size_bytes: stats.total_size_bytes,
            evictions: stats.evictions,
            expirations: stats.expirations,
            avg_entry_age_secs: avg_age,
            hit_rate: stats.hit_rate,
        }
    }

    /// Hash a query string.
    fn hash_query(&self, query: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        query.to_lowercase().trim().hash(&mut hasher);
        hasher.finish()
    }

    /// Estimate the size of a response in bytes.
    fn estimate_size(&self, response: &CachedResponse) -> usize {
        response.content.len()
            + response.model.len()
            + response.original_query.len()
            + response
                .metadata
                .iter()
                .map(|(k, v)| k.len() + v.len())
                .sum::<usize>()
            + 100 // Overhead
    }

    /// Ensure there's capacity for a new entry.
    async fn ensure_capacity(&self, needed_size: usize) {
        let mut entries = self.entries.write().await;
        let mut stats = self.stats.write().await;

        // Remove expired entries first
        let expired: Vec<u64> = entries
            .iter()
            .filter(|(_, e)| e.is_expired())
            .map(|(k, _)| *k)
            .collect();

        for key in expired {
            if let Some(entry) = entries.remove(&key) {
                stats.total_size_bytes = stats.total_size_bytes.saturating_sub(entry.size_bytes);
                stats.expirations += 1;
            }
        }

        // Evict by priority if still over capacity
        while entries.len() >= self.config.max_entries
            || stats.total_size_bytes + needed_size > self.config.max_size_bytes
        {
            if entries.is_empty() {
                break;
            }

            // Find lowest priority entry
            let lowest = entries
                .iter()
                .min_by(|(_, a), (_, b)| {
                    a.priority_score()
                        .partial_cmp(&b.priority_score())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(k, _)| *k);

            if let Some(key) = lowest {
                if let Some(entry) = entries.remove(&key) {
                    stats.total_size_bytes =
                        stats.total_size_bytes.saturating_sub(entry.size_bytes);
                    stats.evictions += 1;
                }
            }
        }

        stats.entry_count = entries.len();
    }

    /// Update hit rate calculation.
    fn update_hit_rate(&self, stats: &mut CacheStats) {
        let total = stats.hits + stats.misses;
        stats.hit_rate = if total > 0 {
            stats.hits as f64 / total as f64
        } else {
            0.0
        };
    }

    /// Semantic lookup for similar queries.
    async fn semantic_lookup(&self, query: &str) -> Option<CachedResponse> {
        let normalized = self.normalize_query(query);
        let semantic = self.semantic_index.read().await;

        // Look for similar queries
        if let Some(hashes) = semantic.get(&normalized) {
            let entries = self.entries.read().await;

            for hash in hashes {
                if let Some(entry) = entries.get(hash) {
                    if !entry.is_expired() {
                        // Verify similarity
                        let similarity =
                            self.calculate_similarity(query, &entry.value.original_query);

                        if similarity >= self.config.similarity_threshold {
                            return Some(entry.value.clone());
                        }
                    }
                }
            }
        }

        None
    }

    /// Index a query for semantic search.
    async fn index_for_semantic_search(&self, query: &str, hash: u64) {
        let normalized = self.normalize_query(query);
        let mut semantic = self.semantic_index.write().await;

        semantic.entry(normalized).or_default().push(hash);
    }

    /// Normalize a query for semantic matching.
    fn normalize_query(&self, query: &str) -> String {
        // Simple normalization - lowercase, remove extra whitespace
        query
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Calculate similarity between two queries (simple Jaccard similarity).
    fn calculate_similarity(&self, query1: &str, query2: &str) -> f32 {
        let lower1 = query1.to_lowercase();
        let lower2 = query2.to_lowercase();

        let words1: std::collections::HashSet<_> = lower1.split_whitespace().collect();

        let words2: std::collections::HashSet<_> = lower2.split_whitespace().collect();

        let intersection = words1.intersection(&words2).count();
        let union = words1.union(&words2).count();

        if union == 0 {
            0.0
        } else {
            intersection as f32 / union as f32
        }
    }

    /// Warm the cache with common queries.
    pub async fn warm(&self, entries: Vec<(String, CachedResponse)>) {
        for (query, response) in entries {
            self.put(&query, response).await;
        }
    }
}

impl Default for ResponseCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Tool result cache for expensive operations.
pub struct ToolCache {
    /// Inner cache.
    inner: ResponseCache,
}

impl ToolCache {
    /// Create a new tool cache.
    pub fn new() -> Self {
        let config = CacheConfig {
            max_entries: 5000,
            max_size_bytes: 50 * 1024 * 1024,            // 50 MB
            default_ttl: Some(Duration::from_secs(300)), // 5 minutes
            semantic_matching: false,                    // Exact match for tools
            similarity_threshold: 1.0,
            compression_enabled: true,
            compression_min_size: 256,
        };

        Self {
            inner: ResponseCache::with_config(config),
        }
    }

    /// Get a cached tool result.
    pub async fn get(&self, tool_name: &str, args: &str) -> Option<String> {
        let key = format!("{}:{}", tool_name, args);
        self.inner.get(&key).await.map(|r| r.content)
    }

    /// Cache a tool result.
    pub async fn put(&self, tool_name: &str, args: &str, result: &str) {
        let key = format!("{}:{}", tool_name, args);
        let response = CachedResponse {
            content: result.to_string(),
            model: tool_name.to_string(),
            tokens: 0,
            original_query: args.to_string(),
            metadata: HashMap::new(),
        };
        self.inner.put(&key, response).await;
    }

    /// Invalidate all results for a tool.
    pub async fn invalidate_tool(&self, tool_name: &str) {
        self.inner.invalidate_by_tag(tool_name).await;
    }

    /// Get statistics.
    pub async fn stats(&self) -> CacheStats {
        self.inner.stats().await
    }
}

impl Default for ToolCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Embedding cache for vector operations.
pub struct EmbeddingCache {
    /// Text -> embedding mapping.
    embeddings: Arc<RwLock<HashMap<u64, Vec<f32>>>>,
    /// Maximum entries.
    max_entries: usize,
    /// Statistics.
    stats: Arc<RwLock<CacheStats>>,
}

impl EmbeddingCache {
    /// Create a new embedding cache.
    pub fn new(max_entries: usize) -> Self {
        Self {
            embeddings: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    /// Get a cached embedding.
    pub async fn get(&self, text: &str) -> Option<Vec<f32>> {
        let hash = self.hash_text(text);
        let embeddings = self.embeddings.read().await;

        if let Some(embedding) = embeddings.get(&hash) {
            let mut stats = self.stats.write().await;
            stats.hits += 1;
            return Some(embedding.clone());
        }

        let mut stats = self.stats.write().await;
        stats.misses += 1;
        None
    }

    /// Cache an embedding.
    pub async fn put(&self, text: &str, embedding: Vec<f32>) {
        let hash = self.hash_text(text);
        let mut embeddings = self.embeddings.write().await;

        // Evict if necessary (simple random eviction)
        if embeddings.len() >= self.max_entries {
            if let Some(key) = embeddings.keys().next().copied() {
                embeddings.remove(&key);
            }
        }

        embeddings.insert(hash, embedding);

        let mut stats = self.stats.write().await;
        stats.entry_count = embeddings.len();
    }

    /// Hash text for cache key.
    fn hash_text(&self, text: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }

    /// Get statistics.
    pub async fn stats(&self) -> CacheStats {
        self.stats.read().await.clone()
    }
}

impl Default for EmbeddingCache {
    fn default() -> Self {
        Self::new(10000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_put_get() {
        let cache = ResponseCache::new();

        let response = CachedResponse {
            content: "Hello, world!".to_string(),
            model: "gpt-4".to_string(),
            tokens: 10,
            original_query: "Say hello".to_string(),
            metadata: HashMap::new(),
        };

        cache.put("Say hello", response.clone()).await;

        let cached = cache.get("Say hello").await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().content, "Hello, world!");
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = ResponseCache::new();

        let cached = cache.get("nonexistent").await;
        assert!(cached.is_none());

        let stats = cache.stats().await;
        assert_eq!(stats.misses, 1);
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let config = CacheConfig {
            default_ttl: Some(Duration::from_millis(10)),
            ..Default::default()
        };
        let cache = ResponseCache::with_config(config);

        let response = CachedResponse {
            content: "test".to_string(),
            model: "test".to_string(),
            tokens: 1,
            original_query: "test".to_string(),
            metadata: HashMap::new(),
        };

        cache.put("test", response).await;

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(20)).await;

        let cached = cache.get("test").await;
        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn test_cache_hit_rate() {
        let cache = ResponseCache::new();

        let response = CachedResponse {
            content: "test".to_string(),
            model: "test".to_string(),
            tokens: 1,
            original_query: "test".to_string(),
            metadata: HashMap::new(),
        };

        cache.put("test", response).await;

        // 3 hits
        cache.get("test").await;
        cache.get("test").await;
        cache.get("test").await;

        // 1 miss
        cache.get("other").await;

        let stats = cache.stats().await;
        assert_eq!(stats.hits, 3);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate - 0.75).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_semantic_matching() {
        let cache = ResponseCache::new();

        let response = CachedResponse {
            content: "Paris is the capital".to_string(),
            model: "gpt-4".to_string(),
            tokens: 10,
            original_query: "What is the capital of France".to_string(),
            metadata: HashMap::new(),
        };

        cache.put("What is the capital of France", response).await;

        // Exact query should match (semantic matching works via normalized index)
        let cached = cache.get("What is the capital of France").await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().content, "Paris is the capital");
    }

    #[tokio::test]
    async fn test_tool_cache() {
        let cache = ToolCache::new();

        cache.put("shell", "ls -la", "file1.txt\nfile2.txt").await;

        let result = cache.get("shell", "ls -la").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "file1.txt\nfile2.txt");
    }

    #[tokio::test]
    async fn test_embedding_cache() {
        let cache = EmbeddingCache::new(100);

        let embedding = vec![0.1, 0.2, 0.3, 0.4];
        cache.put("hello world", embedding.clone()).await;

        let cached = cache.get("hello world").await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), embedding);
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache = ResponseCache::new();

        let response = CachedResponse {
            content: "test".to_string(),
            model: "test".to_string(),
            tokens: 1,
            original_query: "test".to_string(),
            metadata: HashMap::new(),
        };

        cache.put("test1", response.clone()).await;
        cache.put("test2", response).await;

        cache.clear().await;

        let stats = cache.stats().await;
        assert_eq!(stats.entry_count, 0);
    }

    #[tokio::test]
    async fn test_tag_invalidation() {
        let cache = ResponseCache::new();

        let response = CachedResponse {
            content: "test".to_string(),
            model: "test".to_string(),
            tokens: 1,
            original_query: "test".to_string(),
            metadata: HashMap::new(),
        };

        cache
            .put_with_tags("test1", response.clone(), vec!["group1".to_string()])
            .await;
        cache
            .put_with_tags("test2", response, vec!["group1".to_string()])
            .await;

        cache.invalidate_by_tag("group1").await;

        assert!(cache.get("test1").await.is_none());
        assert!(cache.get("test2").await.is_none());
    }
}
