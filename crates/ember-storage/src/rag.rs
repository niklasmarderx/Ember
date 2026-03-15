//! RAG (Retrieval-Augmented Generation) Pipeline
//!
//! Complete implementation for document ingestion, chunking,
//! semantic search, and context augmentation.

use crate::{
    embeddings::{cosine_similarity, Embedder, LocalEmbedder},
    memory::{Document, SearchResult, VectorMemory, VectorMemoryConfig},
    Error, Result,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// =============================================================================
// Chunking Strategies
// =============================================================================

/// Strategy for splitting documents into chunks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChunkingStrategy {
    /// Fixed-size chunks with overlap
    FixedSize {
        /// Chunk size in characters
        chunk_size: usize,
        /// Overlap between chunks in characters
        overlap: usize,
    },
    /// Semantic chunking based on paragraphs
    Paragraph {
        /// Maximum chunk size
        max_size: usize,
        /// Minimum chunk size
        min_size: usize,
    },
    /// Sentence-based chunking
    Sentence {
        /// Number of sentences per chunk
        sentences_per_chunk: usize,
        /// Overlap in sentences
        overlap_sentences: usize,
    },
    /// Recursive chunking that respects document structure
    Recursive {
        /// Target chunk size
        target_size: usize,
        /// Separators in order of preference
        separators: Vec<String>,
    },
}

impl Default for ChunkingStrategy {
    fn default() -> Self {
        Self::FixedSize {
            chunk_size: 1000,
            overlap: 200,
        }
    }
}

/// A chunk of text with metadata about its position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextChunk {
    /// Chunk content
    pub content: String,
    /// Start position in original document
    pub start_pos: usize,
    /// End position in original document
    pub end_pos: usize,
    /// Chunk index within the document
    pub chunk_index: usize,
    /// Total number of chunks in document
    pub total_chunks: usize,
    /// Source document reference
    pub source_doc: Option<String>,
}

/// Document chunker implementation
pub struct Chunker {
    strategy: ChunkingStrategy,
}

impl Chunker {
    /// Create a new chunker with the given strategy
    pub fn new(strategy: ChunkingStrategy) -> Self {
        Self { strategy }
    }

    /// Create a chunker with default fixed-size strategy
    pub fn fixed_size(chunk_size: usize, overlap: usize) -> Self {
        Self::new(ChunkingStrategy::FixedSize {
            chunk_size,
            overlap,
        })
    }

    /// Split text into chunks
    pub fn chunk(&self, text: &str, source: Option<&str>) -> Vec<TextChunk> {
        match &self.strategy {
            ChunkingStrategy::FixedSize {
                chunk_size,
                overlap,
            } => self.chunk_fixed_size(text, *chunk_size, *overlap, source),
            ChunkingStrategy::Paragraph { max_size, min_size } => {
                self.chunk_paragraphs(text, *max_size, *min_size, source)
            }
            ChunkingStrategy::Sentence {
                sentences_per_chunk,
                overlap_sentences,
            } => self.chunk_sentences(text, *sentences_per_chunk, *overlap_sentences, source),
            ChunkingStrategy::Recursive {
                target_size,
                separators,
            } => self.chunk_recursive(text, *target_size, separators, source),
        }
    }

    fn chunk_fixed_size(
        &self,
        text: &str,
        chunk_size: usize,
        overlap: usize,
        source: Option<&str>,
    ) -> Vec<TextChunk> {
        let mut chunks = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let text_len = chars.len();

        if text_len == 0 {
            return chunks;
        }

        let step = chunk_size.saturating_sub(overlap).max(1);
        let mut start = 0;
        let mut index = 0;

        while start < text_len {
            let end = (start + chunk_size).min(text_len);
            let content: String = chars[start..end].iter().collect();

            // Try to break at word boundary
            let adjusted_content = if end < text_len {
                if let Some(last_space) = content.rfind(char::is_whitespace) {
                    if last_space > chunk_size / 2 {
                        content[..last_space].to_string()
                    } else {
                        content
                    }
                } else {
                    content
                }
            } else {
                content
            };

            let actual_end = start + adjusted_content.chars().count();

            chunks.push(TextChunk {
                content: adjusted_content.trim().to_string(),
                start_pos: start,
                end_pos: actual_end,
                chunk_index: index,
                total_chunks: 0, // Will be updated later
                source_doc: source.map(String::from),
            });

            start += step;
            index += 1;
        }

        // Update total counts
        let total = chunks.len();
        for chunk in &mut chunks {
            chunk.total_chunks = total;
        }

        chunks
    }

    fn chunk_paragraphs(
        &self,
        text: &str,
        max_size: usize,
        min_size: usize,
        source: Option<&str>,
    ) -> Vec<TextChunk> {
        let mut chunks = Vec::new();
        let paragraphs: Vec<&str> = text.split("\n\n").collect();

        let mut current_chunk = String::new();
        let mut chunk_start = 0;
        let mut pos = 0;
        let mut index = 0;

        for para in paragraphs {
            let para = para.trim();
            if para.is_empty() {
                pos += 2;
                continue;
            }

            if current_chunk.len() + para.len() > max_size && current_chunk.len() >= min_size {
                // Save current chunk
                chunks.push(TextChunk {
                    content: current_chunk.trim().to_string(),
                    start_pos: chunk_start,
                    end_pos: pos,
                    chunk_index: index,
                    total_chunks: 0,
                    source_doc: source.map(String::from),
                });
                index += 1;
                current_chunk = String::new();
                chunk_start = pos;
            }

            if !current_chunk.is_empty() {
                current_chunk.push_str("\n\n");
            }
            current_chunk.push_str(para);
            pos += para.len() + 2;
        }

        // Don't forget the last chunk
        if !current_chunk.is_empty() {
            chunks.push(TextChunk {
                content: current_chunk.trim().to_string(),
                start_pos: chunk_start,
                end_pos: pos,
                chunk_index: index,
                total_chunks: 0,
                source_doc: source.map(String::from),
            });
        }

        let total = chunks.len();
        for chunk in &mut chunks {
            chunk.total_chunks = total;
        }

        chunks
    }

    fn chunk_sentences(
        &self,
        text: &str,
        sentences_per_chunk: usize,
        overlap: usize,
        source: Option<&str>,
    ) -> Vec<TextChunk> {
        // Simple sentence splitting (could be improved with NLP)
        let sentence_endings = [". ", "! ", "? ", ".\n", "!\n", "?\n"];
        let mut sentences: Vec<(usize, usize)> = Vec::new();
        let mut last_end = 0;

        for (i, _) in text.char_indices() {
            for ending in &sentence_endings {
                if text[i..].starts_with(ending) {
                    let end = i + ending.len();
                    sentences.push((last_end, end));
                    last_end = end;
                    break;
                }
            }
        }

        if last_end < text.len() {
            sentences.push((last_end, text.len()));
        }

        let mut chunks = Vec::new();
        let step = sentences_per_chunk.saturating_sub(overlap).max(1);
        let mut i = 0;
        let mut index = 0;

        while i < sentences.len() {
            let end_idx = (i + sentences_per_chunk).min(sentences.len());
            let start_pos = sentences[i].0;
            let end_pos = sentences[end_idx - 1].1;

            chunks.push(TextChunk {
                content: text[start_pos..end_pos].trim().to_string(),
                start_pos,
                end_pos,
                chunk_index: index,
                total_chunks: 0,
                source_doc: source.map(String::from),
            });

            i += step;
            index += 1;
        }

        let total = chunks.len();
        for chunk in &mut chunks {
            chunk.total_chunks = total;
        }

        chunks
    }

    fn chunk_recursive(
        &self,
        text: &str,
        target_size: usize,
        separators: &[String],
        source: Option<&str>,
    ) -> Vec<TextChunk> {
        fn split_recursive(
            text: &str,
            target_size: usize,
            separators: &[String],
            depth: usize,
        ) -> Vec<String> {
            if text.len() <= target_size || separators.is_empty() || depth > 10 {
                return vec![text.to_string()];
            }

            let sep = &separators[0];
            let parts: Vec<&str> = text.split(sep.as_str()).collect();

            let mut result = Vec::new();
            let mut current = String::new();

            for part in parts {
                let would_be = if current.is_empty() {
                    part.len()
                } else {
                    current.len() + sep.len() + part.len()
                };

                if would_be <= target_size {
                    if !current.is_empty() {
                        current.push_str(sep);
                    }
                    current.push_str(part);
                } else {
                    if !current.is_empty() {
                        if current.len() > target_size && separators.len() > 1 {
                            result.extend(split_recursive(
                                &current,
                                target_size,
                                &separators[1..],
                                depth + 1,
                            ));
                        } else {
                            result.push(current);
                        }
                    }
                    current = part.to_string();
                }
            }

            if !current.is_empty() {
                if current.len() > target_size && separators.len() > 1 {
                    result.extend(split_recursive(
                        &current,
                        target_size,
                        &separators[1..],
                        depth + 1,
                    ));
                } else {
                    result.push(current);
                }
            }

            result
        }

        let parts = split_recursive(text, target_size, separators, 0);
        let mut chunks = Vec::new();
        let mut pos = 0;

        for (index, content) in parts.into_iter().enumerate() {
            let content = content.trim().to_string();
            let len = content.len();
            chunks.push(TextChunk {
                content,
                start_pos: pos,
                end_pos: pos + len,
                chunk_index: index,
                total_chunks: 0,
                source_doc: source.map(String::from),
            });
            pos += len;
        }

        let total = chunks.len();
        for chunk in &mut chunks {
            chunk.total_chunks = total;
        }

        chunks
    }
}

impl Default for Chunker {
    fn default() -> Self {
        Self::new(ChunkingStrategy::default())
    }
}

// =============================================================================
// RAG Pipeline
// =============================================================================

/// Configuration for the RAG pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagConfig {
    /// Chunking strategy
    pub chunking: ChunkingStrategy,
    /// Number of results to retrieve
    pub top_k: usize,
    /// Similarity threshold
    pub similarity_threshold: f32,
    /// Whether to include chunk metadata in context
    pub include_metadata: bool,
    /// Context template for augmentation
    pub context_template: String,
    /// Maximum context length in characters
    pub max_context_length: usize,
    /// Whether to deduplicate similar chunks
    pub deduplicate: bool,
    /// Deduplication threshold
    pub dedup_threshold: f32,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            chunking: ChunkingStrategy::default(),
            top_k: 5,
            similarity_threshold: 0.5,
            include_metadata: true,
            context_template: "Context:\n{context}\n\nQuestion: {query}\n\nAnswer:".to_string(),
            max_context_length: 4000,
            deduplicate: true,
            dedup_threshold: 0.95,
        }
    }
}

/// Retrieved context for augmentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedContext {
    /// Retrieved chunks
    pub chunks: Vec<RetrievedChunk>,
    /// Formatted context string
    pub formatted_context: String,
    /// Total tokens (estimated)
    pub estimated_tokens: usize,
    /// Search latency in milliseconds
    pub search_latency_ms: u64,
}

/// A retrieved chunk with score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedChunk {
    /// Chunk content
    pub content: String,
    /// Similarity score
    pub score: f32,
    /// Source document
    pub source: Option<String>,
    /// Chunk index
    pub chunk_index: usize,
    /// Metadata
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

/// RAG Pipeline for document retrieval and context augmentation
pub struct RagPipeline {
    /// Configuration
    config: RagConfig,
    /// Chunker
    chunker: Chunker,
    /// Vector memory store
    memory: Arc<RwLock<VectorMemory>>,
    /// Total documents ingested
    documents_ingested: std::sync::atomic::AtomicUsize,
    /// Total chunks stored
    chunks_stored: std::sync::atomic::AtomicUsize,
}

impl RagPipeline {
    /// Create a new RAG pipeline with default settings
    pub fn new() -> Self {
        Self::with_config(RagConfig::default())
    }

    /// Create a new RAG pipeline with custom config
    pub fn with_config(config: RagConfig) -> Self {
        let chunker = Chunker::new(config.chunking.clone());
        let memory_config = VectorMemoryConfig {
            similarity_threshold: config.similarity_threshold,
            ..Default::default()
        };
        let memory = VectorMemory::with_config(memory_config, Arc::new(LocalEmbedder::new()));

        Self {
            config,
            chunker,
            memory: Arc::new(RwLock::new(memory)),
            documents_ingested: std::sync::atomic::AtomicUsize::new(0),
            chunks_stored: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Create with custom embedder
    pub fn with_embedder(config: RagConfig, embedder: Arc<dyn Embedder>) -> Self {
        let chunker = Chunker::new(config.chunking.clone());
        let memory_config = VectorMemoryConfig {
            similarity_threshold: config.similarity_threshold,
            ..Default::default()
        };
        let memory = VectorMemory::with_config(memory_config, embedder);

        Self {
            config,
            chunker,
            memory: Arc::new(RwLock::new(memory)),
            documents_ingested: std::sync::atomic::AtomicUsize::new(0),
            chunks_stored: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Ingest a single document
    pub async fn ingest(&self, content: &str, source: Option<&str>) -> Result<usize> {
        let chunks = self.chunker.chunk(content, source);
        let chunk_count = chunks.len();

        let mut memory = self.memory.write().await;
        for chunk in chunks {
            let doc = Document::new(&chunk.content)
                .with_metadata("chunk_index", serde_json::json!(chunk.chunk_index))
                .with_metadata("total_chunks", serde_json::json!(chunk.total_chunks))
                .with_metadata("start_pos", serde_json::json!(chunk.start_pos))
                .with_metadata("end_pos", serde_json::json!(chunk.end_pos));

            let doc = if let Some(src) = &chunk.source_doc {
                doc.with_source(src)
            } else {
                doc
            };

            memory.add(doc).await?;
        }

        self.documents_ingested
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.chunks_stored
            .fetch_add(chunk_count, std::sync::atomic::Ordering::SeqCst);

        info!(
            source = source.unwrap_or("unknown"),
            chunks = chunk_count,
            "Document ingested"
        );

        Ok(chunk_count)
    }

    /// Ingest multiple documents in batch
    pub async fn ingest_batch(&self, documents: Vec<(String, Option<String>)>) -> Result<usize> {
        let mut total_chunks = 0;

        for (content, source) in documents {
            let chunks = self.ingest(&content, source.as_deref()).await?;
            total_chunks += chunks;
        }

        Ok(total_chunks)
    }

    /// Retrieve relevant context for a query
    pub async fn retrieve(&self, query: &str) -> Result<RetrievedContext> {
        let start = std::time::Instant::now();

        let memory = self.memory.read().await;
        let results = memory.search(query, self.config.top_k * 2).await?;

        // Deduplicate if enabled
        let results = if self.config.deduplicate {
            self.deduplicate_results(results)
        } else {
            results.into_iter().take(self.config.top_k).collect()
        };

        let chunks: Vec<RetrievedChunk> = results
            .into_iter()
            .take(self.config.top_k)
            .map(|r| RetrievedChunk {
                content: r.document.content.clone(),
                score: r.score,
                source: r.document.source.clone(),
                chunk_index: r
                    .document
                    .metadata
                    .get("chunk_index")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize,
                metadata: r.document.metadata.clone(),
            })
            .collect();

        // Format context
        let formatted_context = self.format_context(&chunks);
        let estimated_tokens = formatted_context.len() / 4; // Rough estimate

        let search_latency_ms = start.elapsed().as_millis() as u64;

        debug!(
            query = query,
            chunks = chunks.len(),
            latency_ms = search_latency_ms,
            "Retrieved context"
        );

        Ok(RetrievedContext {
            chunks,
            formatted_context,
            estimated_tokens,
            search_latency_ms,
        })
    }

    /// Build augmented prompt from query and retrieved context
    pub async fn augment(&self, query: &str) -> Result<String> {
        let context = self.retrieve(query).await?;

        let prompt = self
            .config
            .context_template
            .replace("{context}", &context.formatted_context)
            .replace("{query}", query);

        Ok(prompt)
    }

    /// Get pipeline statistics
    pub fn stats(&self) -> RagStats {
        RagStats {
            documents_ingested: self
                .documents_ingested
                .load(std::sync::atomic::Ordering::SeqCst),
            chunks_stored: self.chunks_stored.load(std::sync::atomic::Ordering::SeqCst),
            top_k: self.config.top_k,
            similarity_threshold: self.config.similarity_threshold,
        }
    }

    /// Clear all stored data
    pub async fn clear(&self) {
        let mut memory = self.memory.write().await;
        memory.clear();
        self.documents_ingested
            .store(0, std::sync::atomic::Ordering::SeqCst);
        self.chunks_stored
            .store(0, std::sync::atomic::Ordering::SeqCst);
    }

    fn deduplicate_results(&self, results: Vec<SearchResult>) -> Vec<SearchResult> {
        if results.is_empty() {
            return results;
        }

        let mut deduped = Vec::with_capacity(results.len());
        deduped.push(results[0].clone());

        for result in results.into_iter().skip(1) {
            let is_duplicate = deduped.iter().any(|existing| {
                if let (Some(e1), Some(e2)) =
                    (&existing.document.embedding, &result.document.embedding)
                {
                    cosine_similarity(e1, e2) > self.config.dedup_threshold
                } else {
                    false
                }
            });

            if !is_duplicate {
                deduped.push(result);
            }
        }

        deduped
    }

    fn format_context(&self, chunks: &[RetrievedChunk]) -> String {
        let mut context = String::new();
        let mut current_len = 0;

        for (i, chunk) in chunks.iter().enumerate() {
            let chunk_text = if self.config.include_metadata {
                let source = chunk.source.as_deref().unwrap_or("unknown");
                format!(
                    "[Source: {}, Score: {:.2}]\n{}\n\n",
                    source, chunk.score, chunk.content
                )
            } else {
                format!("{}\n\n", chunk.content)
            };

            if current_len + chunk_text.len() > self.config.max_context_length {
                warn!(index = i, "Context truncated due to length limit");
                break;
            }

            context.push_str(&chunk_text);
            current_len += chunk_text.len();
        }

        context.trim().to_string()
    }
}

impl Default for RagPipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// RAG pipeline statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagStats {
    /// Number of documents ingested
    pub documents_ingested: usize,
    /// Number of chunks stored
    pub chunks_stored: usize,
    /// Top-k results configured
    pub top_k: usize,
    /// Similarity threshold
    pub similarity_threshold: f32,
}

// =============================================================================
// OpenAI Embeddings
// =============================================================================

/// OpenAI embedding model
pub struct OpenAIEmbedder {
    client: reqwest::Client,
    api_key: String,
    model: String,
    dimension: usize,
}

impl OpenAIEmbedder {
    /// Create a new OpenAI embedder
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            model: "text-embedding-3-small".to_string(),
            dimension: 1536,
        }
    }

    /// Use text-embedding-3-large model
    pub fn large(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            model: "text-embedding-3-large".to_string(),
            dimension: 3072,
        }
    }

    /// Use Ada model (legacy)
    pub fn ada(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            model: "text-embedding-ada-002".to_string(),
            dimension: 1536,
        }
    }

    /// Set custom model
    pub fn with_model(mut self, model: impl Into<String>, dimension: usize) -> Self {
        self.model = model.into();
        self.dimension = dimension;
        self
    }
}

#[async_trait]
impl Embedder for OpenAIEmbedder {
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
            input: &'a str,
        }

        #[derive(Deserialize)]
        struct EmbedResponse {
            data: Vec<EmbedData>,
        }

        #[derive(Deserialize)]
        struct EmbedData {
            embedding: Vec<f32>,
        }

        let response = self
            .client
            .post("https://api.openai.com/v1/embeddings")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&EmbedRequest {
                model: &self.model,
                input: text,
            })
            .send()
            .await
            .map_err(|e| Error::Storage(format!("OpenAI request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Storage(format!(
                "OpenAI embedding failed: {} - {}",
                status, body
            )));
        }

        let result: EmbedResponse = response
            .json()
            .await
            .map_err(|e| Error::Storage(format!("Failed to parse OpenAI response: {}", e)))?;

        result
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| Error::Storage("No embedding returned".to_string()))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        #[derive(Serialize)]
        struct BatchEmbedRequest<'a> {
            model: &'a str,
            input: &'a [String],
        }

        #[derive(Deserialize)]
        struct EmbedResponse {
            data: Vec<EmbedData>,
        }

        #[derive(Deserialize)]
        struct EmbedData {
            embedding: Vec<f32>,
            index: usize,
        }

        let response = self
            .client
            .post("https://api.openai.com/v1/embeddings")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&BatchEmbedRequest {
                model: &self.model,
                input: texts,
            })
            .send()
            .await
            .map_err(|e| Error::Storage(format!("OpenAI batch request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Storage(format!(
                "OpenAI batch embedding failed: {} - {}",
                status, body
            )));
        }

        let mut result: EmbedResponse = response
            .json()
            .await
            .map_err(|e| Error::Storage(format!("Failed to parse OpenAI response: {}", e)))?;

        // Sort by index to maintain order
        result.data.sort_by_key(|d| d.index);

        Ok(result.data.into_iter().map(|d| d.embedding).collect())
    }
}

// =============================================================================
// Anthropic / Voyage Embeddings
// =============================================================================

/// Voyage AI embedder (recommended for Anthropic)
pub struct VoyageEmbedder {
    client: reqwest::Client,
    api_key: String,
    model: String,
    dimension: usize,
}

impl VoyageEmbedder {
    /// Create a new Voyage embedder
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            model: "voyage-2".to_string(),
            dimension: 1024,
        }
    }

    /// Use voyage-large-2 model
    pub fn large(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            model: "voyage-large-2".to_string(),
            dimension: 1536,
        }
    }

    /// Use voyage-code-2 model (optimized for code)
    pub fn code(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            model: "voyage-code-2".to_string(),
            dimension: 1536,
        }
    }
}

#[async_trait]
impl Embedder for VoyageEmbedder {
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
            input: Vec<&'a str>,
        }

        #[derive(Deserialize)]
        struct EmbedResponse {
            data: Vec<EmbedData>,
        }

        #[derive(Deserialize)]
        struct EmbedData {
            embedding: Vec<f32>,
        }

        let response = self
            .client
            .post("https://api.voyageai.com/v1/embeddings")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&EmbedRequest {
                model: &self.model,
                input: vec![text],
            })
            .send()
            .await
            .map_err(|e| Error::Storage(format!("Voyage request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Storage(format!(
                "Voyage embedding failed: {} - {}",
                status, body
            )));
        }

        let result: EmbedResponse = response
            .json()
            .await
            .map_err(|e| Error::Storage(format!("Failed to parse Voyage response: {}", e)))?;

        result
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| Error::Storage("No embedding returned".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_size_chunking() {
        let chunker = Chunker::fixed_size(100, 20);
        let text = "This is a test. ".repeat(20);
        let chunks = chunker.chunk(&text, Some("test.txt"));

        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.content.len() <= 110); // Some flexibility for word boundaries
        }
    }

    #[test]
    fn test_paragraph_chunking() {
        let chunker = Chunker::new(ChunkingStrategy::Paragraph {
            max_size: 100,
            min_size: 20,
        });
        let text = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
        let chunks = chunker.chunk(text, None);

        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_sentence_chunking() {
        let chunker = Chunker::new(ChunkingStrategy::Sentence {
            sentences_per_chunk: 2,
            overlap_sentences: 1,
        });
        let text = "First sentence. Second sentence. Third sentence. Fourth sentence.";
        let chunks = chunker.chunk(text, None);

        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_recursive_chunking() {
        let chunker = Chunker::new(ChunkingStrategy::Recursive {
            target_size: 50,
            separators: vec!["\n\n".to_string(), "\n".to_string(), ". ".to_string()],
        });
        let text = "Section 1\n\nParagraph 1. Sentence 1. Sentence 2.\n\nSection 2\n\nParagraph 2.";
        let chunks = chunker.chunk(text, None);

        assert!(!chunks.is_empty());
    }

    #[tokio::test]
    async fn test_rag_pipeline() {
        // Use a low similarity threshold for local embedder (hash-based, not semantic)
        let config = RagConfig {
            similarity_threshold: 0.0,
            ..Default::default()
        };
        let pipeline = RagPipeline::with_config(config);

        // Ingest documents
        pipeline
            .ingest(
                "Rust is a systems programming language focused on safety.",
                None,
            )
            .await
            .unwrap();
        pipeline
            .ingest(
                "Python is great for data science and machine learning.",
                None,
            )
            .await
            .unwrap();

        // Retrieve - should return results (may not be semantically accurate with local embedder)
        let _context = pipeline.retrieve("programming language").await.unwrap();

        // With local embedder, we just verify the pipeline works
        // Semantic accuracy requires real embedding models
        let stats = pipeline.stats();
        assert_eq!(stats.documents_ingested, 2);
        assert!(stats.chunks_stored >= 2);
    }

    #[tokio::test]
    async fn test_augmented_prompt() {
        let pipeline = RagPipeline::new();
        pipeline
            .ingest("Ember is a Rust-powered AI agent framework.", None)
            .await
            .unwrap();

        let prompt = pipeline.augment("What is Ember?").await.unwrap();
        assert!(prompt.contains("Ember"));
        assert!(prompt.contains("Context"));
    }
}
