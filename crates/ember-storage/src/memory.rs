//! Vector memory store for RAG (Retrieval-Augmented Generation).
//!
//! This module provides a semantic memory system that stores text documents
//! with their embeddings for efficient similarity search.

use crate::{
    embeddings::{cosine_similarity, Embedder, LocalEmbedder},
    Error, Result,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Unique identifier for a document
pub type DocId = Uuid;

/// A document stored in vector memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Unique identifier
    pub id: DocId,
    /// Document content
    pub content: String,
    /// Pre-computed embedding
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,
    /// Metadata associated with the document
    pub metadata: HashMap<String, serde_json::Value>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Source of the document (e.g., file path, URL)
    pub source: Option<String>,
    /// Document type/category
    pub doc_type: Option<String>,
}

impl Document {
    /// Create a new document.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            content: content.into(),
            embedding: None,
            metadata: HashMap::new(),
            created_at: Utc::now(),
            source: None,
            doc_type: None,
        }
    }

    /// Set the source.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Set the document type.
    pub fn with_type(mut self, doc_type: impl Into<String>) -> Self {
        self.doc_type = Some(doc_type.into());
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Set the embedding.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

/// Search result from vector memory.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The document
    pub document: Document,
    /// Similarity score (0.0 - 1.0)
    pub score: f32,
    /// Rank in results (1 = best match)
    pub rank: usize,
}

/// Configuration for vector memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorMemoryConfig {
    /// Maximum number of documents to store
    pub max_documents: usize,
    /// Similarity threshold for search results
    pub similarity_threshold: f32,
    /// Whether to auto-embed documents
    pub auto_embed: bool,
}

impl Default for VectorMemoryConfig {
    fn default() -> Self {
        Self {
            max_documents: 10000,
            similarity_threshold: 0.5,
            auto_embed: true,
        }
    }
}

/// In-memory vector store for semantic search.
pub struct VectorMemory {
    /// Configuration
    config: VectorMemoryConfig,
    /// Embedder for generating embeddings
    embedder: Arc<dyn Embedder>,
    /// Stored documents
    documents: HashMap<DocId, Document>,
    /// Index for fast lookup by metadata
    metadata_index: HashMap<String, Vec<DocId>>,
}

impl VectorMemory {
    /// Create a new vector memory with local embedder.
    pub fn new() -> Self {
        Self::with_embedder(Arc::new(LocalEmbedder::new()))
    }

    /// Create a new vector memory with custom embedder.
    pub fn with_embedder(embedder: Arc<dyn Embedder>) -> Self {
        Self {
            config: VectorMemoryConfig::default(),
            embedder,
            documents: HashMap::new(),
            metadata_index: HashMap::new(),
        }
    }

    /// Create a new vector memory with config and embedder.
    pub fn with_config(config: VectorMemoryConfig, embedder: Arc<dyn Embedder>) -> Self {
        Self {
            config,
            embedder,
            documents: HashMap::new(),
            metadata_index: HashMap::new(),
        }
    }

    /// Get the embedder.
    pub fn embedder(&self) -> &dyn Embedder {
        &*self.embedder
    }

    /// Get the number of documents.
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Add a document to memory.
    pub async fn add(&mut self, mut document: Document) -> Result<DocId> {
        // Check capacity
        if self.documents.len() >= self.config.max_documents {
            return Err(Error::Storage(format!(
                "Memory full: max {} documents",
                self.config.max_documents
            )));
        }

        // Generate embedding if needed
        if document.embedding.is_none() && self.config.auto_embed {
            let embedding = self.embedder.embed(&document.content).await?;
            document.embedding = Some(embedding);
        }

        let id = document.id;

        // Update metadata index
        if let Some(ref doc_type) = document.doc_type {
            self.metadata_index
                .entry(format!("type:{}", doc_type))
                .or_default()
                .push(id);
        }
        if let Some(ref source) = document.source {
            self.metadata_index
                .entry(format!("source:{}", source))
                .or_default()
                .push(id);
        }

        self.documents.insert(id, document);
        Ok(id)
    }

    /// Add text content directly.
    pub async fn add_text(
        &mut self,
        content: impl Into<String>,
        metadata: Option<serde_json::Value>,
    ) -> Result<DocId> {
        let mut doc = Document::new(content);
        if let Some(meta) = metadata {
            if let serde_json::Value::Object(map) = meta {
                for (k, v) in map {
                    doc = doc.with_metadata(k, v);
                }
            }
        }
        self.add(doc).await
    }

    /// Get a document by ID.
    pub fn get(&self, id: DocId) -> Option<&Document> {
        self.documents.get(&id)
    }

    /// Delete a document.
    pub fn delete(&mut self, id: DocId) -> Option<Document> {
        if let Some(doc) = self.documents.remove(&id) {
            // Clean up metadata index
            for values in self.metadata_index.values_mut() {
                values.retain(|&doc_id| doc_id != id);
            }
            Some(doc)
        } else {
            None
        }
    }

    /// Search for similar documents.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        if self.documents.is_empty() {
            return Ok(Vec::new());
        }

        // Embed the query
        let query_embedding = self.embedder.embed(query).await?;

        // Calculate similarities
        let mut scored: Vec<(DocId, f32)> = self
            .documents
            .iter()
            .filter_map(|(id, doc)| {
                doc.embedding.as_ref().map(|emb| {
                    let score = cosine_similarity(&query_embedding, emb);
                    (*id, score)
                })
            })
            .filter(|(_, score)| *score >= self.config.similarity_threshold)
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top results
        let results: Vec<SearchResult> = scored
            .into_iter()
            .take(limit)
            .enumerate()
            .map(|(idx, (id, score))| SearchResult {
                document: self.documents.get(&id).cloned().unwrap(),
                score,
                rank: idx + 1,
            })
            .collect();

        Ok(results)
    }

    /// Search with filters.
    pub async fn search_with_filter(
        &self,
        query: &str,
        limit: usize,
        filter: impl Fn(&Document) -> bool,
    ) -> Result<Vec<SearchResult>> {
        if self.documents.is_empty() {
            return Ok(Vec::new());
        }

        let query_embedding = self.embedder.embed(query).await?;

        let mut scored: Vec<(DocId, f32)> = self
            .documents
            .iter()
            .filter(|(_, doc)| filter(doc))
            .filter_map(|(id, doc)| {
                doc.embedding.as_ref().map(|emb| {
                    let score = cosine_similarity(&query_embedding, emb);
                    (*id, score)
                })
            })
            .filter(|(_, score)| *score >= self.config.similarity_threshold)
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let results: Vec<SearchResult> = scored
            .into_iter()
            .take(limit)
            .enumerate()
            .map(|(idx, (id, score))| SearchResult {
                document: self.documents.get(&id).cloned().unwrap(),
                score,
                rank: idx + 1,
            })
            .collect();

        Ok(results)
    }

    /// Find documents by type.
    pub fn find_by_type(&self, doc_type: &str) -> Vec<&Document> {
        let key = format!("type:{}", doc_type);
        self.metadata_index
            .get(&key)
            .map(|ids| ids.iter().filter_map(|id| self.documents.get(id)).collect())
            .unwrap_or_default()
    }

    /// Clear all documents.
    pub fn clear(&mut self) {
        self.documents.clear();
        self.metadata_index.clear();
    }

    /// Get all documents.
    pub fn all_documents(&self) -> Vec<&Document> {
        self.documents.values().collect()
    }

    /// Get statistics.
    pub fn stats(&self) -> MemoryStats {
        let total_chars: usize = self.documents.values().map(|d| d.content.len()).sum();
        let with_embeddings = self
            .documents
            .values()
            .filter(|d| d.embedding.is_some())
            .count();

        MemoryStats {
            document_count: self.documents.len(),
            total_characters: total_chars,
            documents_with_embeddings: with_embeddings,
            embedding_dimension: self.embedder.dimension(),
            embedder_model: self.embedder.model_name().to_string(),
        }
    }
}

impl Default for VectorMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about vector memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    /// Number of documents
    pub document_count: usize,
    /// Total characters stored
    pub total_characters: usize,
    /// Documents with embeddings
    pub documents_with_embeddings: usize,
    /// Embedding dimension
    pub embedding_dimension: usize,
    /// Embedder model name
    pub embedder_model: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_document() {
        let mut memory = VectorMemory::new();
        let doc = Document::new("Hello, world!")
            .with_type("greeting")
            .with_source("test");

        let id = memory.add(doc).await.unwrap();
        assert_eq!(memory.len(), 1);
        assert!(memory.get(id).is_some());
    }

    #[tokio::test]
    async fn test_search() {
        let mut memory = VectorMemory::new();

        memory
            .add_text("Rust is a systems programming language", None)
            .await
            .unwrap();
        memory
            .add_text("Python is great for data science", None)
            .await
            .unwrap();
        memory
            .add_text("JavaScript runs in the browser", None)
            .await
            .unwrap();

        let results = memory
            .search("programming languages like Rust", 2)
            .await
            .unwrap();

        assert!(!results.is_empty());
        // First result should be about Rust
        assert!(results[0].document.content.contains("Rust"));
    }

    #[tokio::test]
    async fn test_search_with_filter() {
        let mut memory = VectorMemory::new();

        memory
            .add(Document::new("Rust systems programming").with_type("tech"))
            .await
            .unwrap();
        memory
            .add(Document::new("Python data science").with_type("tech"))
            .await
            .unwrap();
        memory
            .add(Document::new("Cooking recipes").with_type("food"))
            .await
            .unwrap();

        let results = memory
            .search_with_filter("programming", 5, |doc| {
                doc.doc_type.as_ref().map(|t| t == "tech").unwrap_or(false)
            })
            .await
            .unwrap();

        assert!(!results.is_empty());
        for result in &results {
            assert_eq!(result.document.doc_type.as_deref(), Some("tech"));
        }
    }

    #[tokio::test]
    async fn test_delete() {
        let mut memory = VectorMemory::new();
        let id = memory.add_text("Test document", None).await.unwrap();

        assert_eq!(memory.len(), 1);
        memory.delete(id);
        assert_eq!(memory.len(), 0);
    }

    #[tokio::test]
    async fn test_stats() {
        let mut memory = VectorMemory::new();
        memory.add_text("Hello world", None).await.unwrap();

        let stats = memory.stats();
        assert_eq!(stats.document_count, 1);
        assert_eq!(stats.documents_with_embeddings, 1);
    }
}
