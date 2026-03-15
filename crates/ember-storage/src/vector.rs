//! Simple vector storage for semantic search.
//!
//! This module provides a basic in-memory vector database for semantic
//! similarity search using cosine similarity. For production use with
//! large datasets, consider integrating with external vector databases.

use crate::error::{Result, StorageError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

/// Vector storage configuration.
#[derive(Debug, Clone)]
pub struct VectorConfig {
    /// Expected embedding dimension.
    pub dimension: usize,
    /// Maximum number of vectors to store.
    pub max_vectors: usize,
    /// Similarity threshold for search results.
    pub similarity_threshold: f32,
}

impl Default for VectorConfig {
    fn default() -> Self {
        Self {
            dimension: 1536, // OpenAI ada-002 dimension
            max_vectors: 100_000,
            similarity_threshold: 0.7,
        }
    }
}

/// A stored vector with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorEntry {
    /// Unique identifier.
    pub id: String,
    /// The embedding vector.
    pub embedding: Vec<f32>,
    /// Associated text content.
    pub content: String,
    /// Optional metadata.
    pub metadata: HashMap<String, String>,
}

/// Search result with similarity score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The matching vector entry.
    pub entry: VectorEntry,
    /// Cosine similarity score (0.0 to 1.0).
    pub score: f32,
}

/// In-memory vector storage with semantic search.
pub struct VectorStorage {
    config: VectorConfig,
    vectors: Arc<RwLock<HashMap<String, VectorEntry>>>,
}

impl VectorStorage {
    /// Create a new vector storage instance.
    ///
    /// # Arguments
    ///
    /// * `config` - Vector storage configuration
    pub fn new(config: VectorConfig) -> Self {
        info!(
            dimension = config.dimension,
            max_vectors = config.max_vectors,
            "Vector storage initialized"
        );

        Self {
            config,
            vectors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Store a vector embedding.
    ///
    /// # Arguments
    ///
    /// * `content` - The text content associated with this embedding
    /// * `embedding` - The embedding vector
    /// * `metadata` - Optional metadata key-value pairs
    ///
    /// # Errors
    ///
    /// Returns an error if the vector dimension doesn't match or capacity is exceeded.
    pub async fn store(
        &self,
        content: String,
        embedding: Vec<f32>,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<String> {
        // Validate dimension
        if embedding.len() != self.config.dimension {
            return Err(StorageError::DimensionMismatch {
                expected: self.config.dimension,
                actual: embedding.len(),
            });
        }

        let mut vectors = self.vectors.write().await;

        // Check capacity
        if vectors.len() >= self.config.max_vectors {
            return Err(StorageError::CapacityExceeded {
                current: vectors.len(),
                limit: self.config.max_vectors,
            });
        }

        let id = Uuid::new_v4().to_string();
        let entry = VectorEntry {
            id: id.clone(),
            embedding,
            content,
            metadata: metadata.unwrap_or_default(),
        };

        vectors.insert(id.clone(), entry);
        debug!(vector_id = %id, "Stored vector");

        Ok(id)
    }

    /// Search for similar vectors using cosine similarity.
    ///
    /// # Arguments
    ///
    /// * `query_embedding` - The query embedding vector
    /// * `limit` - Maximum number of results to return
    ///
    /// # Errors
    ///
    /// Returns an error if the query dimension doesn't match.
    pub async fn search(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        // Validate dimension
        if query_embedding.len() != self.config.dimension {
            return Err(StorageError::DimensionMismatch {
                expected: self.config.dimension,
                actual: query_embedding.len(),
            });
        }

        let vectors = self.vectors.read().await;

        // Calculate similarities and sort
        let mut results: Vec<SearchResult> = vectors
            .values()
            .map(|entry| {
                let score = cosine_similarity(query_embedding, &entry.embedding);
                SearchResult {
                    entry: entry.clone(),
                    score,
                }
            })
            .filter(|r| r.score >= self.config.similarity_threshold)
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit results
        results.truncate(limit);

        debug!(
            num_results = results.len(),
            threshold = self.config.similarity_threshold,
            "Vector search completed"
        );

        Ok(results)
    }

    /// Get a vector by ID.
    pub async fn get(&self, id: &str) -> Result<Option<VectorEntry>> {
        let vectors = self.vectors.read().await;
        Ok(vectors.get(id).cloned())
    }

    /// Delete a vector by ID.
    pub async fn delete(&self, id: &str) -> Result<bool> {
        let mut vectors = self.vectors.write().await;
        Ok(vectors.remove(id).is_some())
    }

    /// Get the number of stored vectors.
    pub async fn count(&self) -> usize {
        let vectors = self.vectors.read().await;
        vectors.len()
    }

    /// Clear all vectors.
    pub async fn clear(&self) {
        let mut vectors = self.vectors.write().await;
        vectors.clear();
        info!("Vector storage cleared");
    }

    /// Update vector metadata.
    pub async fn update_metadata(
        &self,
        id: &str,
        metadata: HashMap<String, String>,
    ) -> Result<bool> {
        let mut vectors = self.vectors.write().await;

        if let Some(entry) = vectors.get_mut(id) {
            entry.metadata = metadata;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Search with metadata filter.
    pub async fn search_with_filter(
        &self,
        query_embedding: &[f32],
        limit: usize,
        filter: &HashMap<String, String>,
    ) -> Result<Vec<SearchResult>> {
        // Validate dimension
        if query_embedding.len() != self.config.dimension {
            return Err(StorageError::DimensionMismatch {
                expected: self.config.dimension,
                actual: query_embedding.len(),
            });
        }

        let vectors = self.vectors.read().await;

        // Filter and calculate similarities
        let mut results: Vec<SearchResult> = vectors
            .values()
            .filter(|entry| {
                // Check if all filter conditions match
                filter.iter().all(|(key, value)| {
                    entry.metadata.get(key).map(|v| v == value).unwrap_or(false)
                })
            })
            .map(|entry| {
                let score = cosine_similarity(query_embedding, &entry.embedding);
                SearchResult {
                    entry: entry.clone(),
                    score,
                }
            })
            .filter(|r| r.score >= self.config.similarity_threshold)
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit results
        results.truncate(limit);

        Ok(results)
    }
}

/// Calculate cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude_a * magnitude_b)
}

/// Normalize a vector to unit length.
pub fn normalize_vector(v: &mut [f32]) {
    let magnitude: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for x in v.iter_mut() {
            *x /= magnitude;
        }
    }
}

/// Calculate Euclidean distance between two vectors.
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::MAX;
    }

    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_embedding(dim: usize, value: f32) -> Vec<f32> {
        vec![value; dim]
    }

    #[tokio::test]
    async fn test_vector_storage_creation() {
        let config = VectorConfig {
            dimension: 4,
            max_vectors: 100,
            similarity_threshold: 0.5,
        };
        let storage = VectorStorage::new(config);
        assert_eq!(storage.count().await, 0);
    }

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let config = VectorConfig {
            dimension: 4,
            max_vectors: 100,
            similarity_threshold: 0.5,
        };
        let storage = VectorStorage::new(config);

        let embedding = vec![1.0, 0.0, 0.0, 0.0];
        let id = storage
            .store("Test content".to_string(), embedding.clone(), None)
            .await
            .unwrap();

        let entry = storage.get(&id).await.unwrap();
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().content, "Test content");
    }

    #[tokio::test]
    async fn test_dimension_mismatch() {
        let config = VectorConfig {
            dimension: 4,
            max_vectors: 100,
            similarity_threshold: 0.5,
        };
        let storage = VectorStorage::new(config);

        let wrong_embedding = vec![1.0, 0.0]; // Wrong dimension
        let result = storage
            .store("Test".to_string(), wrong_embedding, None)
            .await;

        assert!(result.is_err());
        if let Err(StorageError::DimensionMismatch { expected, actual }) = result {
            assert_eq!(expected, 4);
            assert_eq!(actual, 2);
        }
    }

    #[tokio::test]
    async fn test_similarity_search() {
        let config = VectorConfig {
            dimension: 4,
            max_vectors: 100,
            similarity_threshold: 0.5,
        };
        let storage = VectorStorage::new(config);

        // Store some vectors
        storage
            .store("North".to_string(), vec![1.0, 0.0, 0.0, 0.0], None)
            .await
            .unwrap();
        storage
            .store("East".to_string(), vec![0.0, 1.0, 0.0, 0.0], None)
            .await
            .unwrap();
        storage
            .store("NorthEast".to_string(), vec![0.707, 0.707, 0.0, 0.0], None)
            .await
            .unwrap();

        // Search for vectors similar to North
        let query = vec![1.0, 0.0, 0.0, 0.0];
        let results = storage.search(&query, 10).await.unwrap();

        assert!(!results.is_empty());
        assert_eq!(results[0].entry.content, "North");
        assert!((results[0].score - 1.0).abs() < 0.001); // Should be ~1.0 (identical)
    }

    #[tokio::test]
    async fn test_search_with_filter() {
        let config = VectorConfig {
            dimension: 4,
            max_vectors: 100,
            similarity_threshold: 0.0,
        };
        let storage = VectorStorage::new(config);

        let mut metadata1 = HashMap::new();
        metadata1.insert("category".to_string(), "A".to_string());

        let mut metadata2 = HashMap::new();
        metadata2.insert("category".to_string(), "B".to_string());

        storage
            .store(
                "Doc A".to_string(),
                vec![1.0, 0.0, 0.0, 0.0],
                Some(metadata1),
            )
            .await
            .unwrap();
        storage
            .store(
                "Doc B".to_string(),
                vec![0.9, 0.1, 0.0, 0.0],
                Some(metadata2),
            )
            .await
            .unwrap();

        let query = vec![1.0, 0.0, 0.0, 0.0];
        let mut filter = HashMap::new();
        filter.insert("category".to_string(), "B".to_string());

        let results = storage
            .search_with_filter(&query, 10, &filter)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry.content, "Doc B");
    }

    #[tokio::test]
    async fn test_delete() {
        let config = VectorConfig {
            dimension: 4,
            max_vectors: 100,
            similarity_threshold: 0.5,
        };
        let storage = VectorStorage::new(config);

        let id = storage
            .store("Test".to_string(), vec![1.0, 0.0, 0.0, 0.0], None)
            .await
            .unwrap();

        assert_eq!(storage.count().await, 1);

        let deleted = storage.delete(&id).await.unwrap();
        assert!(deleted);
        assert_eq!(storage.count().await, 0);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &c).abs() < 0.001); // Orthogonal

        let d = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &d) + 1.0).abs() < 0.001); // Opposite
    }

    #[test]
    fn test_normalize_vector() {
        let mut v = vec![3.0, 4.0];
        normalize_vector(&mut v);
        let magnitude: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_euclidean_distance() {
        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        assert!((euclidean_distance(&a, &b) - 5.0).abs() < 0.001);
    }
}
