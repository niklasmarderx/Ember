# Code Examples

This section provides practical, runnable code examples demonstrating various Ember features. Each example is designed to be self-contained and well-documented.

## Getting Started Examples

### Basic Chat
**File:** `examples/basic_chat.rs`

The simplest way to interact with an AI model:

```rust
use ember_llm::{OpenAIProvider, Provider, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = OpenAIProvider::from_env()?;
    
    let messages = vec![
        Message::user("What is the capital of France?")
    ];
    
    let response = provider.chat(&messages, "gpt-4").await?;
    println!("AI: {}", response.content);
    
    Ok(())
}
```

Run with: `cargo run --example basic_chat`

### Streaming Responses
**File:** `examples/streaming_chat.rs`

Handle streaming responses for real-time output:

```rust
use ember_llm::{OpenAIProvider, Provider, Message};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = OpenAIProvider::from_env()?;
    
    let messages = vec![
        Message::user("Tell me a short story")
    ];
    
    let mut stream = provider.chat_stream(&messages, "gpt-4").await?;
    
    while let Some(chunk) = stream.next().await {
        print!("{}", chunk?);
        std::io::stdout().flush()?;
    }
    
    Ok(())
}
```

Run with: `cargo run --example streaming_chat`

## Error Handling
**File:** `examples/error_handling.rs`

Learn best practices for handling errors:

- Creating custom error types
- Retry logic with exponential backoff
- Fallback provider chains
- Graceful degradation

```rust
// Handle rate limits gracefully
match provider.chat(&messages, "gpt-4").await {
    Ok(response) => println!("{}", response.content),
    Err(LlmError::RateLimited { retry_after }) => {
        println!("Rate limited. Waiting {} seconds...", retry_after.unwrap_or(30));
        tokio::time::sleep(Duration::from_secs(retry_after.unwrap_or(30))).await;
        // Retry...
    }
    Err(e) => return Err(e.into()),
}
```

Run with: `cargo run --example error_handling`

## Agent & Tools

### Agent with Tools
**File:** `examples/agent_with_tools.rs`

Build an agent that can execute tools:

```rust
use ember_core::{Agent, AgentBuilder};
use ember_tools::{ShellTool, FileSystemTool, WebSearchTool};

let agent = AgentBuilder::new()
    .provider(provider)
    .model("gpt-4")
    .tool(ShellTool::new())
    .tool(FileSystemTool::new())
    .tool(WebSearchTool::new())
    .build()?;

let response = agent.run("List files in the current directory").await?;
```

Run with: `cargo run --example agent_with_tools`

### Custom Tool Creation
**File:** `examples/custom_tool.rs`

Create your own tools:

```rust
use ember_tools::{Tool, ToolResult, ToolDefinition};
use async_trait::async_trait;

struct WeatherTool;

#[async_trait]
impl Tool for WeatherTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::builder()
            .name("get_weather")
            .description("Get current weather for a city")
            .parameter("city", "string", "City name", true)
            .build()
    }
    
    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let city = params["city"].as_str().unwrap();
        // Fetch weather data...
        Ok(ToolResult::text(format!("Weather in {}: Sunny, 22C", city)))
    }
}
```

Run with: `cargo run --example custom_tool`

## Memory & RAG

### Memory and RAG
**File:** `examples/memory_and_rag.rs`

Implement persistent memory and retrieval-augmented generation:

```rust
use ember_storage::{VectorStore, EmbeddingProvider, Document};
use ember_core::Memory;

// Create a vector store for RAG
let store = VectorStore::new("./data/vectors")?;

// Index documents
let doc = Document::new("Ember is an AI agent framework written in Rust.");
store.add(doc).await?;

// Query with semantic search
let results = store.search("What is Ember?", 5).await?;

// Use results as context for the AI
let context = results.iter().map(|r| r.content.clone()).collect::<Vec<_>>().join("\n");
let messages = vec![
    Message::system(&format!("Context:\n{}", context)),
    Message::user("What can you tell me about Ember?")
];
```

Run with: `cargo run --example memory_and_rag`

## Multi-Provider

### Multi-Provider Setup
**File:** `examples/multi_provider.rs`

Use multiple providers with automatic fallback:

```rust
use ember_llm::{ProviderRouter, OpenAIProvider, AnthropicProvider, OllamaProvider};

let router = ProviderRouter::builder()
    // Primary provider
    .primary(OpenAIProvider::from_env()?)
    // Fallback chain
    .fallback(AnthropicProvider::from_env()?)
    .fallback(OllamaProvider::new("http://localhost:11434"))
    // Routing rules
    .route_model("claude-*", "anthropic")
    .route_model("llama*", "ollama")
    .build();

// Automatically routes to the right provider
let response = router.chat(&messages, "claude-3-sonnet").await?;
```

Run with: `cargo run --example multi_provider`

## Checkpoints

### Checkpoints and Recovery
**File:** `examples/checkpoints.rs`

Save and restore agent state:

```rust
use ember_core::{Agent, Checkpoint};

// Enable checkpointing
let agent = AgentBuilder::new()
    .provider(provider)
    .checkpoint_dir("./checkpoints")
    .auto_checkpoint(true)
    .build()?;

// Save a checkpoint manually
let checkpoint_id = agent.save_checkpoint().await?;

// Restore from checkpoint
let restored_agent = Agent::from_checkpoint(&checkpoint_id).await?;
```

Run with: `cargo run --example checkpoints`

## Integration Examples

### Web Server Integration
**File:** `examples/web_server_integration.rs`

Build a REST API with Ember backend:

```rust
use axum::{Router, routing::post, Json};

async fn chat(Json(request): Json<ChatRequest>) -> Json<ChatResponse> {
    let response = agent.chat(&request.message).await?;
    Json(ChatResponse { response: response.content })
}

let app = Router::new()
    .route("/chat", post(chat))
    .route("/chat/stream", post(chat_stream));

axum::serve(listener, app).await?;
```

Features demonstrated:
- REST API endpoints
- Server-Sent Events (SSE) for streaming
- Conversation state management
- CORS configuration

Run with: `cargo run --example web_server_integration`

## Running Examples

All examples can be run with:

```bash
cargo run --example <example_name>
```

Make sure you have the required environment variables set:

```bash
# For OpenAI examples
export OPENAI_API_KEY=your-api-key

# For Anthropic examples
export ANTHROPIC_API_KEY=your-api-key

# For Ollama examples (local)
# Make sure Ollama is running on localhost:11434
```

## Contributing Examples

We welcome new examples! When contributing:

1. Keep examples under 100 lines when possible
2. Include comments explaining each step
3. Handle errors gracefully with helpful messages
4. Test that the example compiles and runs
5. Update this documentation page

See [CONTRIBUTING.md](https://github.com/ember-ai/ember/blob/main/CONTRIBUTING.md) for more details.