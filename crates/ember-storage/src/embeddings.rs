//! Embedding generation for vector search.
//!
//! This module provides embedding functionality for semantic search,
//! supporting both local and API-based embedding models.

use crate::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Trait for embedding text into vectors.
#[async_trait]
pub trait Embedder: Send + Sync {
    /// Get the dimension of the embedding vectors.
    fn dimension(&self) -> usize;

    /// Get the name of the embedding model.
    fn model_name(&self) -> &str;

    /// Embed a single text string.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed multiple text strings (batch).
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }
}

/// Configuration for embedding models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedderConfig {
    /// Model name
    pub model: String,
    /// API endpoint (if applicable)
    pub api_endpoint: Option<String>,
    /// API key (if applicable)
    pub api_key: Option<String>,
    /// Batch size for embedding
    pub batch_size: usize,
    /// Normalize embeddings to unit length
    pub normalize: bool,
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self {
            model: "local-minilm".to_string(),
            api_endpoint: None,
            api_key: None,
            batch_size: 32,
            normalize: true,
        }
    }
}

/// Simple local embedding using TF-IDF-like approach.
///
/// This is a lightweight embedding solution that doesn't require
/// external APIs or heavy ML models. It uses character n-grams
/// and term frequency to create sparse-dense hybrid embeddings.
pub struct LocalEmbedder {
    dimension: usize,
    ngram_range: (usize, usize),
    normalize: bool,
}

impl LocalEmbedder {
    /// Create a new local embedder with default settings.
    pub fn new() -> Self {
        Self::with_dimension(384)
    }

    /// Create a new local embedder with specified dimension.
    pub fn with_dimension(dimension: usize) -> Self {
        Self {
            dimension,
            ngram_range: (2, 4),
            normalize: true,
        }
    }

    /// Hash a string to a dimension index.
    fn hash_to_index(&self, s: &str) -> usize {
        let mut hash: u64 = 5381;
        for byte in s.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
        }
        (hash as usize) % self.dimension
    }

    /// Generate n-grams from text.
    fn generate_ngrams(&self, text: &str) -> Vec<String> {
        let text = text.to_lowercase();
        let chars: Vec<char> = text.chars().collect();
        let mut ngrams = Vec::new();

        for n in self.ngram_range.0..=self.ngram_range.1 {
            if chars.len() >= n {
                for i in 0..=chars.len() - n {
                    let ngram: String = chars[i..i + n].iter().collect();
                    ngrams.push(ngram);
                }
            }
        }

        // Also add words
        for word in text.split_whitespace() {
            if word.len() >= 2 {
                ngrams.push(format!("w_{}", word));
            }
        }

        ngrams
    }

    /// Normalize a vector to unit length.
    fn normalize_vector(&self, vec: &mut [f32]) {
        let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for x in vec.iter_mut() {
                *x /= magnitude;
            }
        }
    }
}

impl Default for LocalEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Embedder for LocalEmbedder {
    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_name(&self) -> &str {
        "local-ngram-embedder"
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let ngrams = self.generate_ngrams(text);
        let mut embedding = vec![0.0f32; self.dimension];

        // Count n-gram frequencies
        let mut counts: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
        for ngram in &ngrams {
            let idx = self.hash_to_index(ngram);
            *counts.entry(idx).or_insert(0.0) += 1.0;
        }

        // Apply log-TF weighting
        let total = ngrams.len() as f32;
        for (idx, count) in counts {
            embedding[idx] = (1.0 + count).ln() / (1.0 + total).ln();
        }

        if self.normalize {
            self.normalize_vector(&mut embedding);
        }

        Ok(embedding)
    }
}

/// Ollama-based embedder using local Ollama instance.
pub struct OllamaEmbedder {
    client: reqwest::Client,
    base_url: String,
    model: String,
    dimension: usize,
}

impl OllamaEmbedder {
    /// Create a new Ollama embedder.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "http://localhost:11434".to_string(),
            model: model.into(),
            dimension: 768, // Default for nomic-embed-text
        }
    }

    /// Set the base URL.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the dimension (must match model).
    pub fn with_dimension(mut self, dimension: usize) -> Self {
        self.dimension = dimension;
        self
    }
}

#[async_trait]
impl Embedder for OllamaEmbedder {
    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        #[derive(Serialize)]
        struct EmbedRequest<'a> {
            model: &'a str,
            prompt: &'a str,
        }

        #[derive(Deserialize)]
        struct EmbedResponse {
            embedding: Vec<f32>,
        }

        let url = format!("{}/api/embeddings", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&EmbedRequest {
                model: &self.model,
                prompt: text,
            })
            .send()
            .await
            .map_err(|e| Error::Storage(format!("Ollama request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(Error::Storage(format!(
                "Ollama embedding failed: {}",
                response.status()
            )));
        }

        let result: EmbedResponse = response
            .json::<EmbedResponse>()
            .await
            .map_err(|e| Error::Storage(format!("Failed to parse Ollama response: {}", e)))?;

        Ok(result.embedding)
    }
}

/// Calculate cosine similarity between two vectors.
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

    #[tokio::test]
    async fn test_local_embedder() {
        let embedder = LocalEmbedder::new();
        let embedding = embedder.embed("Hello, world!").await.unwrap();

        assert_eq!(embedding.len(), 384);

        // Check normalization
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_similar_texts() {
        let embedder = LocalEmbedder::new();

        let e1 = embedder.embed("The quick brown fox").await.unwrap();
        let e2 = embedder.embed("The quick brown dog").await.unwrap();
        let e3 = embedder
            .embed("Completely different text about programming")
            .await
            .unwrap();

        let sim_12 = cosine_similarity(&e1, &e2);
        let sim_13 = cosine_similarity(&e1, &e3);

        // Similar texts should have higher similarity
        assert!(sim_12 > sim_13);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];

        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_euclidean_distance() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![3.0, 4.0, 0.0];

        assert!((euclidean_distance(&a, &b) - 5.0).abs() < 0.001);
    }
}
