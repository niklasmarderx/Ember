# Memory & RAG

Ember provides powerful memory management and Retrieval-Augmented Generation (RAG) capabilities to enhance AI conversations with contextual knowledge.

## Overview

Memory and RAG in Ember enable:
- **Conversation Memory**: Remember past interactions across sessions
- **Semantic Search**: Find relevant information based on meaning
- **Vector Storage**: Efficient similarity-based retrieval
- **Knowledge Bases**: Build custom knowledge repositories

## Memory Types

### Short-term Memory

Short-term memory keeps track of the current conversation context.

```rust
use ember_core::Memory;

// Create a memory instance
let memory = Memory::new();

// Add messages to memory
memory.add_message(Message::user("What is Rust?"));
memory.add_message(Message::assistant("Rust is a systems programming language..."));

// Get recent context
let context = memory.get_recent(10);  // Last 10 messages
```

### Long-term Memory

Long-term memory persists across sessions using the storage backend.

```rust
use ember_storage::{Storage, SqliteStorage};

// Create persistent storage
let storage = SqliteStorage::new("~/.ember/memory.db")?;

// Store conversation
storage.save_conversation(&conversation)?;

// Retrieve past conversations
let history = storage.get_conversations(10)?;
```

### Semantic Memory

Semantic memory uses embeddings for meaning-based retrieval.

```rust
use ember_storage::SemanticMemory;

// Create semantic memory with embeddings
let semantic = SemanticMemory::new(embedding_provider)?;

// Store with embeddings
semantic.store("Rust is safe and fast", metadata)?;

// Query by meaning
let results = semantic.search("What programming language is memory safe?", 5)?;
```

## RAG (Retrieval-Augmented Generation)

RAG enhances LLM responses with relevant retrieved information.

### Basic RAG Setup

```rust
use ember_storage::RAG;

// Initialize RAG
let rag = RAG::builder()
    .embedding_provider(OpenAIEmbeddings::new()?)
    .vector_store(QdrantStore::new("localhost:6333")?)
    .chunk_size(512)
    .chunk_overlap(50)
    .build()?;

// Index documents
rag.index_document("path/to/document.pdf")?;
rag.index_text("Custom content to remember", metadata)?;

// Query with RAG
let results = rag.query("What does the document say about X?", 5)?;
```

### RAG with Chat

```rust
use ember_core::Agent;

// Create agent with RAG
let agent = Agent::builder()
    .provider(provider)
    .rag(rag)
    .build()?;

// Chat with RAG-enhanced context
let response = agent.chat("Based on our documents, explain Y").await?;
```

## Vector Storage

### Supported Vector Stores

| Store | Description | Use Case |
|-------|-------------|----------|
| In-Memory | Fast, ephemeral | Testing, small datasets |
| SQLite | Local, persistent | Single-user, moderate size |
| Qdrant | Scalable, distributed | Production, large datasets |
| Postgres + pgvector | SQL + vectors | Existing Postgres users |

### Configuration

```toml
# ember.toml
[storage.vector]
provider = "qdrant"
url = "http://localhost:6333"
collection = "ember_memories"

[storage.vector.options]
dimension = 1536
distance = "cosine"
```

### Using Different Vector Stores

```rust
// In-memory (default)
let store = InMemoryVectorStore::new(1536);

// SQLite with vectors
let store = SqliteVectorStore::new("vectors.db")?;

// Qdrant
let store = QdrantStore::new("http://localhost:6333")?
    .collection("my_collection")
    .create_if_not_exists(true);

// Initialize RAG with store
let rag = RAG::new(embeddings, store)?;
```

## Embedding Providers

### OpenAI Embeddings

```rust
let embeddings = OpenAIEmbeddings::new()?
    .model("text-embedding-3-small");

let vectors = embeddings.embed(&["text to embed"])?;
```

### Ollama Embeddings (Local)

```rust
let embeddings = OllamaEmbeddings::new("http://localhost:11434")?
    .model("nomic-embed-text");

let vectors = embeddings.embed(&["text to embed"])?;
```

### Cohere Embeddings

```rust
let embeddings = CohereEmbeddings::new()?
    .model("embed-english-v3.0");
```

### Embedding Comparison

| Provider | Model | Dimensions | Cost |
|----------|-------|------------|------|
| OpenAI | text-embedding-3-small | 1536 | $0.02/1M tokens |
| OpenAI | text-embedding-3-large | 3072 | $0.13/1M tokens |
| Cohere | embed-english-v3.0 | 1024 | $0.10/1M tokens |
| Ollama | nomic-embed-text | 768 | Free (local) |

## Document Processing

### Supported Formats

- **Text**: `.txt`, `.md`, `.rst`
- **Documents**: `.pdf`, `.docx`, `.doc`
- **Code**: `.rs`, `.py`, `.js`, `.ts`, etc.
- **Data**: `.json`, `.yaml`, `.csv`

### Chunking Strategies

```rust
use ember_storage::Chunker;

// Fixed-size chunks
let chunker = Chunker::fixed_size(512, 50);  // size, overlap

// Semantic chunks (sentence-based)
let chunker = Chunker::semantic();

// Code-aware chunks
let chunker = Chunker::code("rust");

// Process document
let chunks = chunker.chunk(&document)?;
```

### Processing Pipeline

```rust
let pipeline = DocumentPipeline::new()
    .add_loader(PdfLoader::new())
    .add_loader(MarkdownLoader::new())
    .add_transformer(TextCleaner::new())
    .add_transformer(MetadataExtractor::new())
    .add_chunker(Chunker::semantic())
    .add_embedder(embeddings);

// Process multiple documents
for path in document_paths {
    pipeline.process(path, &rag)?;
}
```

## Memory Management

### Context Window Management

```rust
use ember_core::ContextManager;

let manager = ContextManager::new()
    .max_tokens(8000)
    .strategy(PruningStrategy::Summarize)
    .preserve_system_message(true)
    .preserve_recent(5);

// Automatically manage context
let pruned_messages = manager.prune(&messages)?;
```

### Pruning Strategies

- **Truncate**: Remove oldest messages first
- **Summarize**: Compress old messages into summaries
- **Selective**: Keep important messages based on relevance
- **Sliding Window**: Maintain a fixed recent window

### Memory Limits

```rust
// Configure memory limits
let memory = Memory::builder()
    .max_messages(100)
    .max_tokens(50000)
    .auto_summarize(true)
    .summarize_threshold(80)  // Summarize at 80% capacity
    .build()?;
```

## Semantic Cache

Cache LLM responses based on semantic similarity to avoid redundant API calls.

```rust
use ember_storage::SemanticCache;

let cache = SemanticCache::new(embeddings, store)?
    .similarity_threshold(0.95)
    .ttl(Duration::from_hours(24));

// Check cache before calling LLM
if let Some(cached) = cache.get(&query)? {
    return Ok(cached);
}

// Get response from LLM
let response = llm.chat(&query).await?;

// Cache the response
cache.set(&query, &response)?;
```

## Best Practices

### 1. Choose the Right Embedding Model

```rust
// For English-only, high accuracy
let embeddings = OpenAIEmbeddings::new()?.model("text-embedding-3-large");

// For multilingual support
let embeddings = CohereEmbeddings::new()?.model("embed-multilingual-v3.0");

// For local/private data
let embeddings = OllamaEmbeddings::new()?.model("nomic-embed-text");
```

### 2. Optimize Chunk Size

```rust
// For Q&A: Smaller chunks for precision
let chunker = Chunker::fixed_size(256, 25);

// For summarization: Larger chunks for context
let chunker = Chunker::fixed_size(1024, 100);

// For code: Use syntax-aware chunking
let chunker = Chunker::code("rust");
```

### 3. Use Metadata Effectively

```rust
let metadata = Metadata::new()
    .set("source", "documentation")
    .set("date", "2024-01-15")
    .set("category", "getting-started");

rag.index_text(content, metadata)?;

// Filter by metadata in queries
let results = rag.query(query, 5)?
    .filter("category", "getting-started");
```

### 4. Monitor and Optimize

```rust
// Track RAG performance
let stats = rag.stats()?;
println!("Total documents: {}", stats.document_count);
println!("Average query time: {}ms", stats.avg_query_ms);
println!("Cache hit rate: {}%", stats.cache_hit_rate);
```

## CLI Commands

```bash
# Index documents
ember rag index ./documents --recursive

# Query the knowledge base
ember rag query "What is X?"

# List indexed documents
ember rag list

# Remove documents
ember rag remove --source "old-docs"

# Clear all data
ember rag clear --confirm

# Export embeddings
ember rag export embeddings.json

# Import embeddings
ember rag import embeddings.json
```

## Configuration Reference

```toml
# ember.toml

[memory]
# Maximum messages to keep in memory
max_messages = 100

# Maximum tokens before pruning
max_tokens = 50000

# Auto-summarize when reaching threshold
auto_summarize = true
summarize_threshold = 0.8

[rag]
# Embedding provider
embedding_provider = "openai"
embedding_model = "text-embedding-3-small"

# Chunking settings
chunk_size = 512
chunk_overlap = 50
chunking_strategy = "semantic"

# Retrieval settings
top_k = 5
similarity_threshold = 0.7
rerank = true

[rag.vector_store]
provider = "sqlite"
path = "~/.ember/vectors.db"

# Or for Qdrant
# provider = "qdrant"
# url = "http://localhost:6333"
# collection = "ember"
```

## See Also

- [Streaming](streaming.md) - Real-time response streaming
- [Tool Selection](../guide/tool-selection.md) - AI-powered tool selection
- [Context Management](../guide/context-management.md) - Managing conversation context