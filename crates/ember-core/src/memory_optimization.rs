//! Memory optimization utilities
//!
//! This module provides memory-efficient data handling including:
//! - Lazy loading of conversations
//! - Message pagination
//! - LRU cache with eviction strategies
//! - Memory pressure detection

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::atomic::{AtomicUsize, Ordering};
// use std::sync::Arc;  // Currently unused
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

/// Configuration for memory optimization
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Maximum memory usage in bytes (soft limit)
    pub max_memory_bytes: usize,
    /// Maximum items in cache
    pub max_cache_items: usize,
    /// Default page size for pagination
    pub default_page_size: usize,
    /// Maximum page size allowed
    pub max_page_size: usize,
    /// Cache TTL (time to live)
    pub cache_ttl: Duration,
    /// Enable automatic eviction
    pub auto_eviction: bool,
    /// Eviction check interval
    pub eviction_interval: Duration,
    /// Low memory threshold (percentage)
    pub low_memory_threshold: f64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: 100 * 1024 * 1024, // 100MB
            max_cache_items: 1000,
            default_page_size: 20,
            max_page_size: 100,
            cache_ttl: Duration::from_secs(3600), // 1 hour
            auto_eviction: true,
            eviction_interval: Duration::from_secs(60),
            low_memory_threshold: 0.8,
        }
    }
}

/// Memory usage statistics
#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    /// Current estimated memory usage
    pub current_usage: usize,
    /// Peak memory usage
    pub peak_usage: usize,
    /// Number of cache hits
    pub cache_hits: u64,
    /// Number of cache misses
    pub cache_misses: u64,
    /// Number of evictions
    pub evictions: u64,
    /// Number of items in cache
    pub cached_items: usize,
}

impl MemoryStats {
    /// Calculate cache hit ratio
    pub fn hit_ratio(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }
}

/// LRU cache entry with metadata
struct CacheEntry<V> {
    value: V,
    size: usize,
    created_at: Instant,
    last_accessed: Instant,
    access_count: u64,
}

impl<V> CacheEntry<V> {
    fn new(value: V, size: usize) -> Self {
        let now = Instant::now();
        Self {
            value,
            size,
            created_at: now,
            last_accessed: now,
            access_count: 1,
        }
    }

    fn touch(&mut self) {
        self.last_accessed = Instant::now();
        self.access_count += 1;
    }

    fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at.elapsed() > ttl
    }
}

/// LRU cache with size-based eviction
pub struct LruCache<K, V> {
    entries: HashMap<K, CacheEntry<V>>,
    order: VecDeque<K>,
    config: MemoryConfig,
    current_size: AtomicUsize,
    stats: Mutex<MemoryStats>,
}

impl<K: Hash + Eq + Clone, V> LruCache<K, V> {
    /// Create a new LRU cache
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            config,
            current_size: AtomicUsize::new(0),
            stats: Mutex::new(MemoryStats::default()),
        }
    }

    /// Insert an item into the cache
    pub async fn insert(&mut self, key: K, value: V, size: usize) {
        // Remove if exists
        if let Some(old) = self.entries.remove(&key) {
            self.current_size.fetch_sub(old.size, Ordering::Relaxed);
            self.order.retain(|k| k != &key);
        }

        // Evict if necessary
        while self.should_evict(size) {
            if !self.evict_oldest().await {
                break;
            }
        }

        // Insert new entry
        self.entries
            .insert(key.clone(), CacheEntry::new(value, size));
        self.order.push_back(key);
        self.current_size.fetch_add(size, Ordering::Relaxed);

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.current_usage = self.current_size.load(Ordering::Relaxed);
        stats.peak_usage = stats.peak_usage.max(stats.current_usage);
        stats.cached_items = self.entries.len();
    }

    /// Get an item from the cache
    pub async fn get(&mut self, key: &K) -> Option<&V> {
        let mut stats = self.stats.lock().await;

        if let Some(entry) = self.entries.get_mut(key) {
            if entry.is_expired(self.config.cache_ttl) {
                stats.cache_misses += 1;
                return None;
            }

            entry.touch();

            // Move to back of order
            self.order.retain(|k| k != key);
            self.order.push_back(key.clone());

            stats.cache_hits += 1;
            Some(&entry.value)
        } else {
            stats.cache_misses += 1;
            None
        }
    }

    /// Remove an item from the cache
    pub async fn remove(&mut self, key: &K) -> Option<V> {
        if let Some(entry) = self.entries.remove(key) {
            self.current_size.fetch_sub(entry.size, Ordering::Relaxed);
            self.order.retain(|k| k != key);

            let mut stats = self.stats.lock().await;
            stats.current_usage = self.current_size.load(Ordering::Relaxed);
            stats.cached_items = self.entries.len();

            Some(entry.value)
        } else {
            None
        }
    }

    /// Check if eviction is needed
    fn should_evict(&self, additional_size: usize) -> bool {
        let current = self.current_size.load(Ordering::Relaxed);
        current + additional_size > self.config.max_memory_bytes
            || self.entries.len() >= self.config.max_cache_items
    }

    /// Evict the oldest entry
    async fn evict_oldest(&mut self) -> bool {
        if let Some(key) = self.order.pop_front() {
            if let Some(entry) = self.entries.remove(&key) {
                self.current_size.fetch_sub(entry.size, Ordering::Relaxed);

                let mut stats = self.stats.lock().await;
                stats.evictions += 1;
                stats.current_usage = self.current_size.load(Ordering::Relaxed);
                stats.cached_items = self.entries.len();

                return true;
            }
        }
        false
    }

    /// Clean up expired entries
    pub async fn cleanup_expired(&mut self) {
        let expired_keys: Vec<K> = self
            .entries
            .iter()
            .filter(|(_, e)| e.is_expired(self.config.cache_ttl))
            .map(|(k, _)| k.clone())
            .collect();

        for key in expired_keys {
            self.remove(&key).await;
        }
    }

    /// Get current memory usage
    pub fn memory_usage(&self) -> usize {
        self.current_size.load(Ordering::Relaxed)
    }

    /// Get number of items
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get stats
    pub async fn stats(&self) -> MemoryStats {
        self.stats.lock().await.clone()
    }

    /// Clear all entries
    pub async fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.current_size.store(0, Ordering::Relaxed);

        let mut stats = self.stats.lock().await;
        stats.current_usage = 0;
        stats.cached_items = 0;
    }
}

/// Pagination request
#[derive(Debug, Clone)]
pub struct PageRequest {
    /// Page number (0-indexed)
    pub page: usize,
    /// Items per page
    pub page_size: usize,
    /// Sort order
    pub sort_order: SortOrder,
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            page: 0,
            page_size: 20,
            sort_order: SortOrder::Descending,
        }
    }
}

impl PageRequest {
    /// Create a new page request
    pub fn new(page: usize, page_size: usize) -> Self {
        Self {
            page,
            page_size,
            sort_order: SortOrder::Descending,
        }
    }

    /// Calculate offset
    pub fn offset(&self) -> usize {
        self.page * self.page_size
    }

    /// With sort order
    pub fn with_sort_order(mut self, order: SortOrder) -> Self {
        self.sort_order = order;
        self
    }
}

/// Sort order for pagination
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    /// Newest first
    Descending,
    /// Oldest first
    Ascending,
}

/// Pagination response
#[derive(Debug, Clone)]
pub struct PageResponse<T> {
    /// Items in this page
    pub items: Vec<T>,
    /// Total number of items
    pub total_items: usize,
    /// Total number of pages
    pub total_pages: usize,
    /// Current page number
    pub current_page: usize,
    /// Items per page
    pub page_size: usize,
    /// Has more pages
    pub has_next: bool,
    /// Has previous pages
    pub has_previous: bool,
}

impl<T> PageResponse<T> {
    /// Create a new page response
    pub fn new(items: Vec<T>, total_items: usize, page: usize, page_size: usize) -> Self {
        let total_pages = (total_items + page_size - 1) / page_size;
        Self {
            items,
            total_items,
            total_pages,
            current_page: page,
            page_size,
            has_next: page + 1 < total_pages,
            has_previous: page > 0,
        }
    }

    /// Map items to another type
    pub fn map<U, F: FnMut(T) -> U>(self, f: F) -> PageResponse<U> {
        PageResponse {
            items: self.items.into_iter().map(f).collect(),
            total_items: self.total_items,
            total_pages: self.total_pages,
            current_page: self.current_page,
            page_size: self.page_size,
            has_next: self.has_next,
            has_previous: self.has_previous,
        }
    }
}

/// Lazy loader for conversations
pub struct LazyConversationLoader<T> {
    /// Loaded conversations cache
    cache: RwLock<LruCache<String, T>>,
    /// Configuration
    config: MemoryConfig,
}

impl<T: Clone + Send + Sync> LazyConversationLoader<T> {
    /// Create a new lazy loader
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            cache: RwLock::new(LruCache::new(config.clone())),
            config,
        }
    }

    /// Load a conversation (from cache or loader function)
    pub async fn load<F, Fut>(&self, id: &str, loader: F) -> Option<T>
    where
        F: FnOnce(String) -> Fut,
        Fut: std::future::Future<Output = Option<T>>,
    {
        // Try cache first
        {
            let mut cache = self.cache.write().await;
            if let Some(conv) = cache.get(&id.to_string()).await {
                return Some(conv.clone());
            }
        }

        // Load from source
        if let Some(conv) = loader(id.to_string()).await {
            let size = std::mem::size_of::<T>();
            let mut cache = self.cache.write().await;
            cache.insert(id.to_string(), conv.clone(), size).await;
            Some(conv)
        } else {
            None
        }
    }

    /// Invalidate a cached conversation
    pub async fn invalidate(&self, id: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(&id.to_string()).await;
    }

    /// Clear all cached conversations
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear().await;
    }

    /// Get cache statistics
    pub async fn stats(&self) -> MemoryStats {
        let cache = self.cache.read().await;
        cache.stats().await
    }
}

/// Memory pressure detector
pub struct MemoryPressureDetector {
    /// Configuration
    config: MemoryConfig,
    /// Last check time
    last_check: Mutex<Instant>,
    /// Current pressure level
    pressure_level: AtomicUsize, // 0-100
}

impl MemoryPressureDetector {
    /// Create a new detector
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            config,
            last_check: Mutex::new(Instant::now()),
            pressure_level: AtomicUsize::new(0),
        }
    }

    /// Update pressure level based on current usage
    pub async fn update(&self, current_usage: usize) {
        let max = self.config.max_memory_bytes;
        let pressure = ((current_usage as f64 / max as f64) * 100.0) as usize;
        self.pressure_level
            .store(pressure.min(100), Ordering::Relaxed);

        let mut last_check = self.last_check.lock().await;
        *last_check = Instant::now();
    }

    /// Get current pressure level (0-100)
    pub fn pressure_level(&self) -> usize {
        self.pressure_level.load(Ordering::Relaxed)
    }

    /// Check if under memory pressure
    pub fn is_under_pressure(&self) -> bool {
        let level = self.pressure_level.load(Ordering::Relaxed) as f64 / 100.0;
        level >= self.config.low_memory_threshold
    }

    /// Get pressure state
    pub fn state(&self) -> PressureState {
        let level = self.pressure_level.load(Ordering::Relaxed);
        match level {
            0..=50 => PressureState::Normal,
            51..=80 => PressureState::Moderate,
            81..=95 => PressureState::High,
            _ => PressureState::Critical,
        }
    }
}

/// Memory pressure state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressureState {
    /// Normal operation
    Normal,
    /// Moderate pressure - consider eviction
    Moderate,
    /// High pressure - aggressive eviction
    High,
    /// Critical - emergency eviction
    Critical,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lru_cache_basic() {
        let config = MemoryConfig {
            max_cache_items: 3,
            max_memory_bytes: 1000,
            ..Default::default()
        };
        let mut cache: LruCache<String, String> = LruCache::new(config);

        cache
            .insert("a".to_string(), "value_a".to_string(), 10)
            .await;
        cache
            .insert("b".to_string(), "value_b".to_string(), 10)
            .await;

        assert_eq!(cache.len(), 2);
        assert_eq!(
            cache.get(&"a".to_string()).await,
            Some(&"value_a".to_string())
        );
    }

    #[tokio::test]
    async fn test_lru_cache_eviction() {
        let config = MemoryConfig {
            max_cache_items: 2,
            max_memory_bytes: 1000,
            ..Default::default()
        };
        let mut cache: LruCache<String, String> = LruCache::new(config);

        cache
            .insert("a".to_string(), "value_a".to_string(), 10)
            .await;
        cache
            .insert("b".to_string(), "value_b".to_string(), 10)
            .await;
        cache
            .insert("c".to_string(), "value_c".to_string(), 10)
            .await;

        // "a" should be evicted
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&"a".to_string()).await.is_none());
    }

    #[tokio::test]
    async fn test_page_request() {
        let request = PageRequest::new(2, 10);
        assert_eq!(request.offset(), 20);
    }

    #[tokio::test]
    async fn test_page_response() {
        let items = vec![1, 2, 3, 4, 5];
        let response = PageResponse::new(items, 25, 1, 5);

        assert_eq!(response.total_pages, 5);
        assert!(response.has_next);
        assert!(response.has_previous);
    }

    #[tokio::test]
    async fn test_memory_pressure_detector() {
        let config = MemoryConfig {
            max_memory_bytes: 100,
            low_memory_threshold: 0.8,
            ..Default::default()
        };
        let detector = MemoryPressureDetector::new(config);

        detector.update(50).await;
        assert_eq!(detector.pressure_level(), 50);
        assert!(!detector.is_under_pressure());
        assert_eq!(detector.state(), PressureState::Normal);

        detector.update(90).await;
        assert!(detector.is_under_pressure());
        assert_eq!(detector.state(), PressureState::High);
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let config = MemoryConfig::default();
        let mut cache: LruCache<String, String> = LruCache::new(config);

        cache.insert("a".to_string(), "value".to_string(), 10).await;
        let _ = cache.get(&"a".to_string()).await;
        let _ = cache.get(&"b".to_string()).await;

        let stats = cache.stats().await;
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
        assert!((stats.hit_ratio() - 0.5).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_lazy_loader() {
        let config = MemoryConfig::default();
        let loader: LazyConversationLoader<String> = LazyConversationLoader::new(config);

        let result = loader
            .load("test", |id| async move { Some(format!("loaded_{}", id)) })
            .await;

        assert_eq!(result, Some("loaded_test".to_string()));

        // Second load should come from cache
        let result2 = loader
            .load("test", |_| async move {
                panic!("Should not be called");
            })
            .await;

        assert_eq!(result2, Some("loaded_test".to_string()));
    }
}
