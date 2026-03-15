# Custom Tool SDK

The Ember Tool SDK provides a simple and powerful way to create custom tools for AI agents. This guide covers everything you need to know to build, register, and use custom tools.

## Overview

Tools are functions that AI agents can call to interact with external systems, perform calculations, or access information. Ember provides a flexible SDK with multiple approaches:

1. **Builder Pattern** - Quick tool creation with `SimpleTool::builder()`
2. **Async Tools** - For I/O-bound operations with `AsyncTool`
3. **Trait Implementation** - Full control with `impl ToolHandler`

## Quick Start

Here is a minimal example of creating and using a custom tool:

```rust
use ember_tools::prelude::*;

// Create a simple greeting tool
let greet_tool = SimpleTool::builder("greet")
    .description("Greets a user by name")
    .string_param("name", "The name to greet", true)
    .handler(|args| {
        let name = args.get_string("name")?;
        Ok(ToolOutput::success(format!("Hello, {}!", name)))
    });

// Register and use
let mut registry = ToolRegistry::new();
registry.register(greet_tool);

// Execute
let result = registry.execute("greet", json!({"name": "Alice"})).await?;
println!("{}", result.output); // "Hello, Alice!"
```

## Builder Pattern

The `SimpleTool::builder()` is the recommended way to create most tools:

```rust
let tool = SimpleTool::builder("tool_name")
    .description("What the tool does")
    // Add parameters
    .string_param("text", "A text input", true)  // required
    .integer_param("count", "A number", false)    // optional
    .boolean_param_default("verbose", "Enable verbose mode", false)
    // Set the handler
    .handler(|args| {
        // Tool logic here
        Ok(ToolOutput::success("Result"))
    });
```

### Parameter Types

The SDK supports these parameter types:

| Method | Type | JSON Schema |
|--------|------|-------------|
| `string_param()` | String | `"type": "string"` |
| `string_param_default()` | String with default | `"type": "string", "default": "..."` |
| `integer_param()` | Integer | `"type": "integer"` |
| `integer_param_default()` | Integer with default | `"type": "integer", "default": N` |
| `number_param()` | Float | `"type": "number"` |
| `boolean_param()` | Boolean | `"type": "boolean"` |
| `boolean_param_default()` | Boolean with default | `"type": "boolean", "default": true/false` |
| `array_param()` | Array | `"type": "array"` |
| `object_param()` | Object | `"type": "object"` |
| `enum_param()` | String enum | `"type": "string", "enum": [...]` |

### Enum Parameters

For parameters with predefined values:

```rust
.enum_param(
    "format",
    "Output format",
    &["json", "yaml", "toml"],
    true,  // required
)
```

### Custom Parameters

For complex parameters, use `ParamDef` directly:

```rust
use ember_tools::sdk::ParamDef;

.param(ParamDef {
    name: "config".to_string(),
    description: "Configuration object".to_string(),
    param_type: ParamType::Object,
    required: false,
    default: Some(json!({})),
    enum_values: None,
})
```

## Async Tools

For tools that perform I/O operations (HTTP requests, file access, etc.):

```rust
let async_tool = AsyncTool::builder("fetch_url")
    .description("Fetches content from a URL")
    .string_param("url", "The URL to fetch", true)
    .async_handler(|args| async move {
        let url = args.get_string("url")?.to_string();
        
        // Async operations
        let response = reqwest::get(&url).await?;
        let text = response.text().await?;
        
        Ok(ToolOutput::success(text))
    });
```

## Parameter Extraction

The `ParamExtractor` trait provides convenient methods for extracting typed values:

```rust
// Required parameters (returns error if missing)
let name = args.get_string("name")?;
let count = args.get_integer("count")?;
let value = args.get_number("value")?;
let enabled = args.get_boolean("enabled")?;
let items = args.get_array("items")?;

// Optional parameters (returns None if missing)
let name = args.get_string_opt("name");
let count = args.get_integer_opt("count");
let value = args.get_number_opt("value");
let enabled = args.get_boolean_opt("enabled");
let items = args.get_array_opt("items");
```

## Validation

The SDK includes validation helpers in `ember_tools::sdk::validation`:

```rust
use ember_tools::sdk::validation;

// String validation
validation::non_empty_string(text, "text")?;

// Range validation
validation::in_range(value, 1, 100, "value")?;

// Pattern matching
validation::matches_pattern(email, r"^[\w.-]+@[\w.-]+\.\w+$", "email")?;

// Enum validation
validation::one_of(&choice, &["a", "b", "c"], "choice")?;
```

## Tool Output

Tools return `ToolOutput` with success or error status:

```rust
// Simple success
Ok(ToolOutput::success("Operation completed"))

// Success with structured data
Ok(ToolOutput::success_with_data(
    "Calculated result: 42",
    json!({ "result": 42 }),
))

// Error
Ok(ToolOutput::error("Something went wrong"))
```

## Custom Tool Implementation

For full control, implement the `ToolHandler` trait:

```rust
use ember_tools::{ToolHandler, ToolDefinition, ToolOutput, Result};
use async_trait::async_trait;
use serde_json::Value;

struct MyCustomTool {
    // Tool state
}

#[async_trait]
impl ToolHandler for MyCustomTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new("my_tool", "Description of my tool")
            .add_string_param("input", "Input parameter", true)
    }

    async fn execute(&self, arguments: Value) -> Result<ToolOutput> {
        let input = arguments
            .get("input")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ember_tools::Error::MissingParameter("input".into()))?;
        
        // Tool logic
        Ok(ToolOutput::success(format!("Processed: {}", input)))
    }

    fn is_enabled(&self) -> bool {
        true  // Override to dynamically enable/disable
    }
}
```

## Tool Registry

The `ToolRegistry` manages tool registration and execution:

```rust
let mut registry = ToolRegistry::new();

// Register tools
registry.register(my_tool);
registry.register_arc(Arc::new(shared_tool));

// Check if tool exists
if registry.has("my_tool") {
    // Execute tool
    let result = registry.execute("my_tool", json!({"input": "data"})).await?;
}

// Get tool definitions for LLM
let definitions = registry.tool_definitions();
let llm_definitions = registry.llm_tool_definitions();

// List tools
for name in registry.tool_names() {
    println!("Tool: {}", name);
}
```

## LLM Integration

Tool definitions are automatically formatted for LLM providers:

```rust
// Get definitions in LLM format
let tools = registry.llm_tool_definitions();

// Execute LLM tool calls
let tool_call = LLMToolCall {
    id: "call_123".to_string(),
    name: "calculator".to_string(),
    arguments: json!({"a": 1, "b": 2, "operation": "add"}),
};

let result = registry.execute_tool_call(&tool_call).await?;
// Returns LLMToolResult for sending back to the LLM
```

## Best Practices

### 1. Descriptive Names and Descriptions

```rust
// Good
SimpleTool::builder("search_files")
    .description("Searches for files matching a pattern in a directory")

// Bad
SimpleTool::builder("sf")
    .description("search")
```

### 2. Validate Input Early

```rust
.handler(|args| {
    let path = args.get_string("path")?;
    
    // Validate before processing
    validation::non_empty_string(path, "path")?;
    validation::matches_pattern(path, r"^[a-zA-Z0-9/_.-]+$", "path")?;
    
    // Now proceed with validated input
    // ...
})
```

### 3. Return Structured Data

```rust
// Include both human-readable and machine-readable output
Ok(ToolOutput::success_with_data(
    "Found 5 files matching pattern",
    json!({
        "count": 5,
        "files": ["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"]
    }),
))
```

### 4. Handle Errors Gracefully

```rust
.handler(|args| {
    match do_operation() {
        Ok(result) => Ok(ToolOutput::success(result)),
        Err(e) => Ok(ToolOutput::error(format!("Operation failed: {}", e))),
    }
})
```

### 5. Document Parameters Clearly

```rust
.string_param(
    "format",
    "Output format. Supported values: 'json' (default), 'yaml', 'toml'",
    false,
)
```

## Example: Complete Tool

Here is a complete example of a file search tool:

```rust
use ember_tools::prelude::*;
use std::path::Path;

fn create_file_search_tool() -> AsyncTool {
    AsyncTool::builder("file_search")
        .description("Searches for files matching a glob pattern")
        .string_param("pattern", "Glob pattern (e.g., '*.rs', 'src/**/*.txt')", true)
        .string_param_default("directory", "Starting directory", ".")
        .boolean_param_default("recursive", "Search recursively", true)
        .integer_param_default("max_results", "Maximum results to return", 100)
        .async_handler(|args| async move {
            let pattern = args.get_string("pattern")?.to_string();
            let directory = args.get_string_opt("directory").unwrap_or(".");
            let recursive = args.get_boolean_opt("recursive").unwrap_or(true);
            let max_results = args.get_integer_opt("max_results").unwrap_or(100) as usize;

            // Validate
            validation::non_empty_string(&pattern, "pattern")?;
            validation::in_range(max_results as i64, 1, 1000, "max_results")?;

            // Search (simplified)
            let full_pattern = if recursive {
                format!("{}/**/{}", directory, pattern)
            } else {
                format!("{}/{}", directory, pattern)
            };

            let files: Vec<String> = glob::glob(&full_pattern)
                .map_err(|e| Error::InvalidParameter {
                    name: "pattern".to_string(),
                    reason: e.to_string(),
                })?
                .filter_map(|r| r.ok())
                .take(max_results)
                .map(|p| p.display().to_string())
                .collect();

            Ok(ToolOutput::success_with_data(
                format!("Found {} files", files.len()),
                json!({
                    "count": files.len(),
                    "files": files,
                    "pattern": pattern,
                    "directory": directory,
                }),
            ))
        })
}
```

## Next Steps

- See the [examples/custom_tool.rs](https://github.com/ember-ai/ember/blob/main/examples/custom_tool.rs) for more examples
- Check out the built-in tools in `ember_tools::shell`, `ember_tools::filesystem`, etc.
- Read about [Tool Security](./tool-security.md) for best practices