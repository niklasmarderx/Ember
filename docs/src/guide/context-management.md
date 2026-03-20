# Context Window Management

Ember includes an advanced context manager that intelligently handles conversation history, optimizes token usage, and implements various pruning strategies to make the most of LLM context windows.

## Overview

The Context Manager helps you:

- Track and manage conversation history efficiently
- Stay within model token limits automatically
- Apply intelligent pruning strategies
- Preserve important context while removing less relevant content
- Estimate token costs before sending requests

## Basic Usage

```rust
use ember_core::context_manager::{ContextManagerV2, ContextMessage, MessageRole};

// Create a context manager with default settings
let mut manager = ContextManagerV2::builder()
    .max_tokens(8000)      // Maximum tokens to use
    .reserve_tokens(1000)  // Reserve tokens for response
    .build();

// Add messages
manager.add_message(ContextMessage {
    role: MessageRole::System,
    content: "You are a helpful assistant.".to_string(),
    timestamp: chrono::Utc::now(),
    priority: 1.0,  // Higher priority = less likely to be pruned
    metadata: None,
});

manager.add_message(ContextMessage {
    role: MessageRole::User,
    content: "What is Rust?".to_string(),
    timestamp: chrono::Utc::now(),
    priority: 0.8,
    metadata: None,
});

// Get messages optimized for context window
let messages = manager.get_messages()?;
```

## Builder Configuration

```rust
use ember_core::context_manager::{
    ContextManagerBuilder, 
    PruningStrategy, 
    PriorityWeights
};

let manager = ContextManagerV2::builder()
    // Token limits
    .max_tokens(16000)
    .reserve_tokens(2000)
    
    // Pruning strategy
    .pruning_strategy(PruningStrategy::Hybrid {
        summarize_threshold: 0.3,
        window_size: 10,
    })
    
    // Priority weights for content scoring
    .priority_weights(PriorityWeights {
        recency: 0.3,
        user_messages: 0.3,
        tool_results: 0.2,
        system_messages: 0.2,
    })
    
    // System message handling
    .preserve_system_messages(true)
    
    // Compression settings
    .enable_compression(true)
    .compression_ratio(0.5)
    
    .build();
```

## Pruning Strategies

### Sliding Window

Keeps only the most recent N messages:

```rust
let manager = ContextManagerV2::builder()
    .pruning_strategy(PruningStrategy::SlidingWindow { window_size: 20 })
    .build();
```

Best for: Simple conversations where recent context is most relevant.

### Priority-Based

Removes messages based on calculated priority scores:

```rust
let manager = ContextManagerV2::builder()
    .pruning_strategy(PruningStrategy::PriorityBased)
    .priority_weights(PriorityWeights {
        recency: 0.4,        // How recent the message is
        user_messages: 0.3,  // Boost for user messages
        tool_results: 0.2,   // Boost for tool execution results
        system_messages: 0.1, // Boost for system messages
    })
    .build();
```

Best for: Complex conversations where message importance varies.

### Summarize

Summarizes older content to save tokens:

```rust
let manager = ContextManagerV2::builder()
    .pruning_strategy(PruningStrategy::Summarize { 
        summarize_after: 10,  // Summarize messages older than this
    })
    .build();
```

Best for: Long conversations where historical context matters.

### Hybrid

Combines multiple strategies for optimal results:

```rust
let manager = ContextManagerV2::builder()
    .pruning_strategy(PruningStrategy::Hybrid {
        summarize_threshold: 0.3,  // Summarize when below this priority
        window_size: 15,           // Keep at least this many recent messages
    })
    .build();
```

Best for: Production applications requiring balanced performance.

## Message Priorities

Assign priorities to control which messages are preserved:

```rust
// System messages - always keep
manager.add_message(ContextMessage {
    role: MessageRole::System,
    content: system_prompt,
    priority: 1.0,  // Maximum priority
    ..Default::default()
});

// Important user input
manager.add_message(ContextMessage {
    role: MessageRole::User,
    content: user_question,
    priority: 0.9,  // High priority
    ..Default::default()
});

// Tool execution result
manager.add_message(ContextMessage {
    role: MessageRole::Tool,
    content: tool_output,
    priority: 0.7,  // Medium-high priority
    metadata: Some(serde_json::json!({
        "tool_name": "shell",
        "success": true
    })),
    ..Default::default()
});

// Informational assistant message
manager.add_message(ContextMessage {
    role: MessageRole::Assistant,
    content: thinking_output,
    priority: 0.3,  // Lower priority - can be pruned
    ..Default::default()
});
```

## Token Counting

The context manager tracks token usage:

```rust
// Get current token count
let token_count = manager.token_count();
println!("Current tokens: {}", token_count.total);
println!("By role:");
println!("  System: {}", token_count.system);
println!("  User: {}", token_count.user);
println!("  Assistant: {}", token_count.assistant);
println!("  Tool: {}", token_count.tool);

// Check available space
let available = manager.available_tokens();
println!("Available for new content: {}", available);

// Estimate cost for new message
let estimate = manager.estimate_tokens("This is a new message");
println!("New message would use ~{} tokens", estimate);
```

## Context Snapshots

Save and restore context states:

```rust
// Create a snapshot
let snapshot = manager.snapshot();

// Continue conversation...
manager.add_message(msg1);
manager.add_message(msg2);

// Restore to previous state if needed
manager.restore(snapshot);
```

## Compression

Enable automatic compression for long content:

```rust
let manager = ContextManagerV2::builder()
    .enable_compression(true)
    .compression_ratio(0.5)  // Target 50% of original size
    .build();

// Long content will be automatically compressed
manager.add_message(ContextMessage {
    role: MessageRole::User,
    content: very_long_document,
    ..Default::default()
});
```

## Working with Different Models

Configure for specific model limits:

```rust
// GPT-4 (8K context)
let gpt4_manager = ContextManagerV2::builder()
    .max_tokens(8000)
    .reserve_tokens(1000)
    .build();

// Claude 3 (200K context)
let claude_manager = ContextManagerV2::builder()
    .max_tokens(180000)
    .reserve_tokens(20000)
    .build();

// Ollama local model (varies)
let ollama_manager = ContextManagerV2::builder()
    .max_tokens(4000)
    .reserve_tokens(500)
    .pruning_strategy(PruningStrategy::SlidingWindow { window_size: 10 })
    .build();
```

## Integration with Agent

The context manager integrates with the Agent:

```rust
use ember_core::{Agent, AgentConfig};

let config = AgentConfig {
    context_management: ContextManagerConfig {
        max_tokens: 16000,
        pruning_strategy: PruningStrategy::Hybrid {
            summarize_threshold: 0.3,
            window_size: 15,
        },
        ..Default::default()
    },
    ..Default::default()
};

let agent = Agent::new(config)?;

// Context is managed automatically
for message in conversation {
    let response = agent.chat(&message).await?;
    // Context manager handles token limits
}
```

## Events and Callbacks

Monitor context management events:

```rust
manager.on_prune(|pruned_messages| {
    println!("Pruned {} messages", pruned_messages.len());
    for msg in pruned_messages {
        println!("  - {} (priority: {})", msg.role, msg.priority);
    }
});

manager.on_summarize(|original, summary| {
    println!("Summarized {} messages into {} tokens", 
        original.len(), 
        summary.token_count
    );
});

manager.on_token_warning(|current, max| {
    println!("Warning: Using {}% of context window", 
        (current as f64 / max as f64 * 100.0) as u32
    );
});
```

## Configuration File

Configure in `ember.toml`:

```toml
[context]
max_tokens = 16000
reserve_tokens = 2000

[context.pruning]
strategy = "hybrid"
summarize_threshold = 0.3
window_size = 15

[context.priorities]
recency = 0.3
user_messages = 0.3
tool_results = 0.2
system_messages = 0.2

[context.compression]
enabled = true
ratio = 0.5
min_length = 1000  # Only compress content longer than this
```

## Best Practices

1. **Set Appropriate Reserves**: Always reserve tokens for the model's response
2. **Prioritize Important Content**: Use priorities to protect crucial context
3. **Choose the Right Strategy**: Match pruning strategy to your use case
4. **Monitor Token Usage**: Track usage to optimize performance
5. **Test with Real Data**: Validate settings with actual conversation patterns

## Debugging

Enable debug logging for context management:

```rust
// Enable detailed logging
manager.set_debug(true);

// Get detailed stats
let stats = manager.stats();
println!("Context Stats:");
println!("  Total messages: {}", stats.message_count);
println!("  Total tokens: {}", stats.total_tokens);
println!("  Pruned count: {}", stats.pruned_count);
println!("  Compression ratio: {:.2}", stats.actual_compression_ratio);
```

## See Also

- [Tool Selection](./tool-selection.md)
- [Agent Mode](./agent-mode.md)
- [Performance](./performance.md)