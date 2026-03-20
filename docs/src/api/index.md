# API Reference

This section provides comprehensive API documentation for all Ember crates.

## Core API

### ember-core

The core crate provides fundamental types and the agent implementation.

#### Agent

```rust
use ember_core::{Agent, AgentBuilder, AgentConfig};

// Create an agent with builder pattern
let agent = Agent::builder()
    .provider(provider)
    .model("gpt-4")
    .system_prompt("You are a helpful assistant.")
    .tools(vec![shell_tool, fs_tool])
    .memory(memory_backend)
    .build()?;

// Chat with the agent
let response = agent.chat("Hello!").await?;

// Stream responses
let stream = agent.stream("Tell me a story").await?;
while let Some(chunk) = stream.next().await {
    print!("{}", chunk?);
}
```

#### Configuration

```rust
use ember_core::AgentConfig;

let config = AgentConfig {
    model: "gpt-4".to_string(),
    temperature: 0.7,
    max_tokens: Some(4096),
    system_prompt: Some("You are helpful.".to_string()),
    ..Default::default()
};
```

#### Memory

```rust
use ember_core::memory::{Memory, MemoryEntry};

// Add to memory
memory.add(MemoryEntry {
    content: "User prefers concise answers".to_string(),
    importance: 0.8,
    timestamp: Utc::now(),
}).await?;

// Search memory
let relevant = memory.search("preferences", 5).await?;
```

### ember-llm

The LLM crate provides provider implementations and routing.

#### Providers

```rust
use ember_llm::{OpenAIProvider, AnthropicProvider, Provider};

// OpenAI
let openai = OpenAIProvider::new(api_key)?;
let response = openai.complete(request).await?;

// Anthropic
let anthropic = AnthropicProvider::new(api_key)?;
let response = anthropic.complete(request).await?;

// With configuration
let provider = OpenAIProvider::builder()
    .api_key(key)
    .base_url("https://custom.openai.com")
    .timeout(Duration::from_secs(60))
    .build()?;
```

#### Model Registry

```rust
use ember_llm::ModelRegistry;

let registry = ModelRegistry::new();

// Get model info
let info = registry.get("gpt-4")?;
println!("Context window: {}", info.context_window);
println!("Cost per 1K input: ${}", info.input_cost_per_1k);

// Find models by capability
let vision_models = registry.find_by_capability("vision");
```

#### Smart Router

```rust
use ember_llm::SmartRouter;

let router = SmartRouter::new(vec![
    ("openai", openai_provider),
    ("anthropic", anthropic_provider),
]);

// Route based on task
let provider = router.route_for_task(
    "complex coding task",
    &RouteOptions {
        prefer_cost: false,
        prefer_speed: false,
        required_capabilities: vec!["code"],
    }
)?;
```

### ember-tools

The tools crate provides built-in tools and the tool trait.

#### Tool Trait

```rust
use ember_tools::{Tool, ToolResult};
use async_trait::async_trait;

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str { "my_tool" }
    
    fn description(&self) -> &str {
        "Description for the LLM"
    }
    
    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "input": { "type": "string" }
            },
            "required": ["input"]
        })
    }
    
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let input = params["input"].as_str().unwrap();
        Ok(ToolResult::success(format!("Processed: {}", input)))
    }
}
```

#### Built-in Tools

```rust
use ember_tools::{ShellTool, FilesystemTool, WebSearchTool};

// Shell tool with restrictions
let shell = ShellTool::new()
    .allowed_commands(vec!["ls", "cat", "grep"])
    .working_directory("/safe/path")
    .timeout(Duration::from_secs(30));

// Filesystem tool with sandbox
let fs = FilesystemTool::new()
    .allowed_paths(vec!["/project"])
    .read_only(false);

// Web search
let web = WebSearchTool::new(api_key);
```

### ember-storage

The storage crate provides persistence and RAG capabilities.

#### Vector Storage

```rust
use ember_storage::{VectorStore, EmbeddingProvider};

let store = VectorStore::new(
    SqliteBackend::new("embeddings.db")?,
    OpenAIEmbeddings::new(api_key),
)?;

// Add documents
store.add_documents(vec![
    Document::new("content", metadata),
]).await?;

// Search
let results = store.search("query", SearchOptions {
    top_k: 10,
    threshold: 0.7,
    filter: Some(json!({"type": "code"})),
}).await?;
```

#### RAG Pipeline

```rust
use ember_storage::rag::{RagPipeline, Chunker};

let rag = RagPipeline::new(vector_store)
    .chunker(Chunker::semantic(512))
    .retriever(HybridRetriever::new())
    .reranker(CrossEncoderReranker::new());

// Index documents
rag.index_file("document.pdf").await?;

// Query with context
let context = rag.retrieve("question", 5).await?;
```

## CLI API

### Commands

```bash
# Chat
ember chat "Your message"
ember chat -m claude-3-opus "Your message"
ember chat --stream "Tell me a story"

# Interactive mode
ember interactive
ember interactive --model gpt-4 --tools shell,fs

# Configuration
ember config set default_model gpt-4
ember config get default_model
ember config list

# History
ember history list
ember history show <id>
ember history export --format json

# Serve
ember serve --port 8080
ember serve --host 0.0.0.0 --cors
```

## Web API

### REST Endpoints

#### Chat

```http
POST /api/v1/chat
Content-Type: application/json

{
  "message": "Hello",
  "model": "gpt-4",
  "stream": false
}
```

#### Streaming Chat

```http
POST /api/v1/chat/stream
Content-Type: application/json

{
  "message": "Tell me a story",
  "model": "gpt-4"
}
```

Response (Server-Sent Events):
```
data: {"event": "chunk", "content": "Once"}
data: {"event": "chunk", "content": " upon"}
data: {"event": "chunk", "content": " a time"}
data: {"event": "done", "usage": {"prompt_tokens": 10, "completion_tokens": 50}}
```

#### Models

```http
GET /api/v1/models

Response:
{
  "models": [
    {"id": "gpt-4", "name": "GPT-4", "provider": "openai"},
    {"id": "claude-3-opus", "name": "Claude 3 Opus", "provider": "anthropic"}
  ]
}
```

#### Info

```http
GET /api/v1/info

Response:
{
  "name": "Ember",
  "version": "1.1.0",
  "llm_provider": "openai",
  "default_model": "gpt-4"
}
```

## WebSocket API

### Connection

```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  switch (data.type) {
    case 'chunk':
      console.log(data.content);
      break;
    case 'tool_call':
      console.log('Tool:', data.name, data.params);
      break;
    case 'done':
      console.log('Complete');
      break;
  }
};

ws.send(JSON.stringify({
  type: 'chat',
  message: 'Hello',
  model: 'gpt-4'
}));
```

## Error Types

```rust
use ember_core::Error;

match result {
    Err(Error::Provider(e)) => {
        // LLM provider error
    }
    Err(Error::RateLimit { retry_after }) => {
        // Rate limited, wait and retry
    }
    Err(Error::InvalidConfig(msg)) => {
        // Configuration error
    }
    Err(Error::Tool { name, cause }) => {
        // Tool execution error
    }
    _ => {}
}
```

## Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `openai` | OpenAI provider | Yes |
| `anthropic` | Anthropic provider | Yes |
| `ollama` | Ollama provider | Yes |
| `all-providers` | All LLM providers | No |
| `tools` | Built-in tools | Yes |
| `browser` | Browser automation | No |
| `rag` | RAG capabilities | No |
| `mcp` | MCP protocol | No |

```toml
[dependencies]
ember = { version = "1.1", features = ["all-providers", "rag", "mcp"] }