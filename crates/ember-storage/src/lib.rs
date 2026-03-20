//! Persistent storage for the Ember AI agent framework.
//!
//! This crate provides storage backends for conversations, memories, and agent state.
//!
//! # Features
//!
//! - `sqlite` (default): SQLite storage backend for persistent data
//! - `vector`: In-memory vector storage for semantic search
//! - `full`: All storage features
//!
//! # Example
//!
//! ```rust,no_run
//! use ember_storage::{SqliteConfig, SqliteStorage};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create SQLite storage
//!     let config = SqliteConfig::default();
//!     let storage = SqliteStorage::new(&config)?;
//!     
//!     // Run migrations
//!     storage.migrate().await?;
//!     
//!     // Create a conversation
//!     let conv_id = storage.create_conversation(Some("My Chat")).await?;
//!     
//!     Ok(())
//! }
//! ```

#![deny(missing_docs)]
#![deny(unsafe_code)]

pub mod embeddings;
pub mod error;
pub mod memory;
pub mod rag;
pub mod semantic_cache;

#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "vector")]
pub mod vector;

// Type alias for compatibility
pub use error::StorageError as Error;

// Re-exports
pub use embeddings::{
    cosine_similarity, euclidean_distance as embed_euclidean_distance, Embedder, EmbedderConfig,
    LocalEmbedder, OllamaEmbedder,
};
pub use error::{Result, StorageError};
pub use memory::{
    DocId, Document, MemoryStats, SearchResult as VectorSearchResult, VectorMemory,
    VectorMemoryConfig,
};
pub use rag::{
    Chunker, ChunkingStrategy, OpenAIEmbedder, RagConfig, RagPipeline, RagStats, RetrievedChunk,
    RetrievedContext, TextChunk, VoyageEmbedder,
};
pub use semantic_cache::{
    CacheContext, CacheEntry, CacheHit, CacheStats, CacheStatsSummary, EmbeddingProvider,
    SemanticCache, SemanticCacheBuilder, SemanticCacheConfig, SimpleEmbedder,
};

#[cfg(feature = "sqlite")]
pub use sqlite::{
    ConversationRecord, ConversationStats, HighlightSpan, MemoryRecord, MessageRecord,
    MessageSearchResult, SearchOptions, SearchResult as SqliteSearchResult, SearchSortBy,
    SqliteConfig, SqliteStorage,
};

#[cfg(feature = "vector")]
pub use vector::{
    euclidean_distance, normalize_vector, SearchResult, VectorConfig, VectorEntry, VectorStorage,
};

/// Prelude module for convenient imports.
pub mod prelude {
    pub use crate::embeddings::{cosine_similarity, Embedder, LocalEmbedder};
    pub use crate::error::{Result, StorageError};
    pub use crate::memory::{DocId, Document, VectorMemory, VectorMemoryConfig};
    pub use crate::rag::{
        Chunker, ChunkingStrategy, RagConfig, RagPipeline, RetrievedChunk, RetrievedContext,
    };
    pub use crate::semantic_cache::{
        CacheContext, CacheHit, SemanticCache, SemanticCacheBuilder, SemanticCacheConfig,
    };

    #[cfg(feature = "sqlite")]
    pub use crate::sqlite::{
        ConversationRecord, ConversationStats, HighlightSpan, MemoryRecord, MessageRecord,
        MessageSearchResult, SearchOptions, SearchSortBy, SqliteConfig, SqliteStorage,
    };

    #[cfg(feature = "vector")]
    pub use crate::vector::{SearchResult, VectorConfig, VectorEntry, VectorStorage};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_error_display() {
        // Basic test that error types can be created and displayed
        let err = StorageError::NotFound {
            entity: "test".to_string(),
            id: "123".to_string(),
        };
        let display = format!("{}", err);
        assert!(!display.is_empty());
    }
}
