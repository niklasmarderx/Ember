# Streaming Responses

Ember supports real-time streaming of LLM responses, providing immediate feedback to users.

## Basic Streaming

### CLI Streaming

```bash
ember chat --stream "Tell me a long story"
```

### Rust API Streaming

```rust
use ember_core::Agent;
use futures::StreamExt;

let agent = Agent::builder()
    .provider(provider)
    .model("gpt-4")
    .build()?;

// Stream responses
let mut stream = agent.stream("Tell me a story").await?;

while let Some(chunk) = stream.next().await {
    match chunk {
        Ok(text) => print!("{}", text),
        Err(e) => eprintln!("Error: {}", e),
    }
}
println!(); // Final newline
```

## Streaming with Events

For more control, use the event-based streaming API:

```rust
use ember_core::streaming::{StreamEvent, StreamOptions};

let options = StreamOptions {
    include_usage: true,
    include_timing: true,
};

let mut stream = agent.stream_with_options("Query", options).await?;

while let Some(event) = stream.next().await {
    match event? {
        StreamEvent::Start { model } => {
            println!("Starting with model: {}", model);
        }
        StreamEvent::Chunk { content, .. } => {
            print!("{}", content);
        }
        StreamEvent::ToolCall { name, arguments } => {
            println!("\nTool call: {} with {:?}", name, arguments);
        }
        StreamEvent::ToolResult { name, result } => {
            println!("Tool result from {}: {}", name, result);
        }
        StreamEvent::Done { usage, duration } => {
            println!("\nTokens: {:?}, Time: {:?}", usage, duration);
        }
        StreamEvent::Error { message } => {
            eprintln!("Error: {}", message);
        }
    }
}
```

## Web API Streaming

### Server-Sent Events (SSE)

```javascript
const response = await fetch('/api/v1/chat/stream', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ message: 'Hello', model: 'gpt-4' })
});

const reader = response.body.getReader();
const decoder = new TextDecoder();

while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    
    const text = decoder.decode(value);
    const lines = text.split('\n');
    
    for (const line of lines) {
        if (line.startsWith('data: ')) {
            const data = JSON.parse(line.slice(6));
            
            switch (data.event) {
                case 'chunk':
                    process.stdout.write(data.content);
                    break;
                case 'tool_call':
                    console.log('Tool:', data.name);
                    break;
                case 'done':
                    console.log('\nComplete:', data.usage);
                    break;
                case 'error':
                    console.error('Error:', data.error);
                    break;
            }
        }
    }
}
```

### WebSocket Streaming

```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

ws.onopen = () => {
    ws.send(JSON.stringify({
        type: 'chat',
        message: 'Tell me a story',
        model: 'gpt-4',
        stream: true
    }));
};

ws.onmessage = (event) => {
    const data = JSON.parse(event.data);
    
    if (data.type === 'chunk') {
        document.getElementById('output').textContent += data.content;
    } else if (data.type === 'done') {
        console.log('Complete');
    }
};
```

## Streaming with Tools

When tools are enabled, the stream includes tool execution events:

```rust
let agent = Agent::builder()
    .provider(provider)
    .model("gpt-4")
    .tools(vec![web_search, calculator])
    .build()?;

let mut stream = agent.stream("What's 2+2 and search for cats").await?;

while let Some(event) = stream.next().await {
    match event? {
        StreamEvent::Chunk { content, .. } => {
            print!("{}", content);
        }
        StreamEvent::ToolCall { name, arguments } => {
            // Tool execution starts
            println!("\n[Calling {}...]", name);
        }
        StreamEvent::ToolResult { name, result } => {
            // Tool finished
            println!("[{} returned: {}]", name, result);
        }
        _ => {}
    }
}
```

## Cancellation

Streams can be cancelled mid-response:

```rust
use tokio::time::{timeout, Duration};

let mut stream = agent.stream("Very long response").await?;

// Cancel after 5 seconds
let result = timeout(Duration::from_secs(5), async {
    while let Some(chunk) = stream.next().await {
        print!("{}", chunk?);
    }
    Ok::<_, Error>(())
}).await;

match result {
    Ok(Ok(())) => println!("\nComplete"),
    Ok(Err(e)) => eprintln!("\nError: {}", e),
    Err(_) => {
        // Timeout - stream is automatically dropped
        println!("\nCancelled due to timeout");
    }
}
```

## Backpressure Handling

For slow consumers, configure buffering:

```rust
let options = StreamOptions {
    buffer_size: 100,  // Buffer up to 100 chunks
    drop_on_overflow: false,  // Block instead of dropping
    ..Default::default()
};

let stream = agent.stream_with_options("Query", options).await?;
```

## Performance Considerations

1. **First Token Latency**: Streaming shows content immediately, reducing perceived latency
2. **Memory Usage**: Streams use less memory than buffering entire responses
3. **Network Overhead**: SSE has slight overhead per chunk; consider chunk aggregation for very fast streams
4. **Error Recovery**: Implement retry logic for stream interruptions

```rust
async fn stream_with_retry(agent: &Agent, query: &str, max_retries: u32) -> Result<String> {
    let mut retries = 0;
    let mut full_response = String::new();
    
    loop {
        match agent.stream(query).await {
            Ok(mut stream) => {
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(text) => full_response.push_str(&text),
                        Err(e) if retries < max_retries => {
                            retries += 1;
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }
                return Ok(full_response);
            }
            Err(e) if retries < max_retries => {
                retries += 1;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => return Err(e),
        }
    }
}