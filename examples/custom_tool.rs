//! Example: Creating Custom Tools with the Ember Tool SDK
//!
//! This example demonstrates how to create custom tools for Ember AI agents
//! using the Tool SDK. It shows different patterns for tool creation:
//!
//! 1. Simple synchronous tools using the builder pattern
//! 2. Async tools for I/O operations
//! 3. Tools with complex parameter validation
//! 4. Custom tool implementations using the ToolHandler trait
//!
//! Run with: cargo run --example custom_tool

use ember_tools::prelude::*;
use ember_tools::ToolRegistry;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Example 1: Simple Calculator Tool (Synchronous)
// ---------------------------------------------------------------------------

fn create_calculator_tool() -> SimpleTool {
    SimpleTool::builder("calculator")
        .description("Performs basic arithmetic operations")
        .enum_param(
            "operation",
            "The arithmetic operation to perform",
            &["add", "subtract", "multiply", "divide"],
            true,
        )
        .number_param("a", "First operand", true)
        .number_param("b", "Second operand", true)
        .handler(|args| {
            let operation = args.get_string("operation")?;
            let a = args.get_number("a")?;
            let b = args.get_number("b")?;

            let result = match operation {
                "add" => a + b,
                "subtract" => a - b,
                "multiply" => a * b,
                "divide" => {
                    if b == 0.0 {
                        return Ok(ToolOutput::error("Division by zero"));
                    }
                    a / b
                }
                _ => return Ok(ToolOutput::error(format!("Unknown operation: {}", operation))),
            };

            Ok(ToolOutput::success_with_data(
                format!("{} {} {} = {}", a, operation, b, result),
                json!({ "result": result }),
            ))
        })
}

// ---------------------------------------------------------------------------
// Example 2: Text Formatter Tool (with validation)
// ---------------------------------------------------------------------------

fn create_text_formatter_tool() -> SimpleTool {
    SimpleTool::builder("text_formatter")
        .description("Formats text in various styles")
        .string_param("text", "The text to format", true)
        .enum_param(
            "style",
            "The formatting style",
            &["uppercase", "lowercase", "titlecase", "reverse", "leetspeak"],
            true,
        )
        .boolean_param_default("trim", "Whether to trim whitespace", true)
        .handler(|args| {
            let text = args.get_string("text")?;
            let style = args.get_string("style")?;
            let trim = args.get_boolean_opt("trim").unwrap_or(true);

            // Validate non-empty text
            validation::non_empty_string(text, "text")?;

            let text = if trim { text.trim() } else { text };

            let formatted = match style {
                "uppercase" => text.to_uppercase(),
                "lowercase" => text.to_lowercase(),
                "titlecase" => text
                    .split_whitespace()
                    .map(|word| {
                        let mut chars = word.chars();
                        match chars.next() {
                            Some(first) => {
                                first.to_uppercase().to_string() + chars.as_str().to_lowercase().as_str()
                            }
                            None => String::new(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
                "reverse" => text.chars().rev().collect(),
                "leetspeak" => text
                    .chars()
                    .map(|c| match c.to_ascii_lowercase() {
                        'a' => '4',
                        'e' => '3',
                        'i' => '1',
                        'o' => '0',
                        's' => '5',
                        't' => '7',
                        _ => c,
                    })
                    .collect(),
                _ => return Ok(ToolOutput::error(format!("Unknown style: {}", style))),
            };

            Ok(ToolOutput::success(formatted))
        })
}

// ---------------------------------------------------------------------------
// Example 3: Async HTTP Tool
// ---------------------------------------------------------------------------

fn create_http_status_tool() -> AsyncTool {
    AsyncTool::builder("http_status")
        .description("Checks the HTTP status of a URL (simulated)")
        .string_param("url", "The URL to check", true)
        .integer_param_default("timeout_ms", "Timeout in milliseconds", 5000)
        .async_handler(|args| async move {
            let url = args.get_string("url")?.to_string();
            let timeout = args.get_integer_opt("timeout_ms").unwrap_or(5000);

            // Validate URL format
            validation::matches_pattern(&url, r"^https?://", "url")?;
            validation::in_range(timeout, 100, 30000, "timeout_ms")?;

            // Simulate HTTP request with delay
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // For demonstration, return simulated results
            let status = if url.contains("example.com") {
                200
            } else if url.contains("error") {
                500
            } else if url.contains("notfound") {
                404
            } else {
                200
            };

            Ok(ToolOutput::success_with_data(
                format!("URL {} returned status {}", url, status),
                json!({
                    "url": url,
                    "status": status,
                    "success": status >= 200 && status < 400
                }),
            ))
        })
}

// ---------------------------------------------------------------------------
// Example 4: Custom ToolHandler Implementation
// ---------------------------------------------------------------------------

/// A counter tool that maintains state between calls.
/// This demonstrates implementing ToolHandler directly.
struct CounterTool {
    initial_value: i64,
}

impl CounterTool {
    fn new(initial_value: i64) -> Self {
        Self { initial_value }
    }
}

#[async_trait::async_trait]
impl ToolHandler for CounterTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new("counter", "A simple counter that tracks a value")
            .add_string_param("action", "Action: 'get', 'increment', 'decrement', 'reset'", true)
            .add_integer_param("amount", "Amount to increment/decrement by", false)
    }

    async fn execute(&self, arguments: Value) -> Result<ToolOutput> {
        // In a real implementation, this would use shared state
        // For this example, we just demonstrate the pattern
        let action = arguments
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("get");
        
        let amount = arguments
            .get("amount")
            .and_then(|v| v.as_i64())
            .unwrap_or(1);

        // Simulated counter value (in reality, would be stored in Arc<Mutex<i64>>)
        let current = self.initial_value;

        let (new_value, message) = match action {
            "get" => (current, format!("Current value: {}", current)),
            "increment" => {
                let new = current + amount;
                (new, format!("Incremented by {}: {} -> {}", amount, current, new))
            }
            "decrement" => {
                let new = current - amount;
                (new, format!("Decremented by {}: {} -> {}", amount, current, new))
            }
            "reset" => (0, format!("Reset: {} -> 0", current)),
            _ => return Ok(ToolOutput::error(format!("Unknown action: {}", action))),
        };

        Ok(ToolOutput::success_with_data(
            message,
            json!({ "value": new_value }),
        ))
    }
}

// ---------------------------------------------------------------------------
// Example 5: JSON Path Query Tool
// ---------------------------------------------------------------------------

fn create_json_query_tool() -> SimpleTool {
    SimpleTool::builder("json_query")
        .description("Extracts values from JSON using simple path notation")
        .string_param("json", "JSON string to query", true)
        .string_param("path", "Path to extract (dot notation, e.g., 'user.name')", true)
        .handler(|args| {
            let json_str = args.get_string("json")?;
            let path = args.get_string("path")?;

            // Parse JSON
            let json: Value = serde_json::from_str(json_str)
                .map_err(|e| Error::InvalidParameter {
                    name: "json".to_string(),
                    reason: format!("Invalid JSON: {}", e),
                })?;

            // Navigate path
            let parts: Vec<&str> = path.split('.').collect();
            let mut current = &json;

            for part in &parts {
                // Handle array indexing
                if let Some(idx) = part.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                    let index: usize = idx.parse().map_err(|_| Error::InvalidParameter {
                        name: "path".to_string(),
                        reason: format!("Invalid array index: {}", idx),
                    })?;
                    current = current.get(index).ok_or_else(|| Error::InvalidParameter {
                        name: "path".to_string(),
                        reason: format!("Array index out of bounds: {}", index),
                    })?;
                } else {
                    current = current.get(*part).ok_or_else(|| Error::InvalidParameter {
                        name: "path".to_string(),
                        reason: format!("Path not found: {}", part),
                    })?;
                }
            }

            Ok(ToolOutput::success_with_data(
                current.to_string(),
                json!({ "value": current }),
            ))
        })
}

// ---------------------------------------------------------------------------
// Main: Demonstrate all tools
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create a registry and register all tools
    let mut registry = ToolRegistry::new();
    
    registry.register(create_calculator_tool());
    registry.register(create_text_formatter_tool());
    registry.register(create_http_status_tool());
    registry.register(CounterTool::new(10));
    registry.register(create_json_query_tool());

    println!("Ember Custom Tool SDK Examples");
    println!("==============================\n");

    // List all registered tools
    println!("Registered Tools:");
    for def in registry.tool_definitions() {
        println!("  - {}: {}", def.name, def.description);
    }
    println!();

    // Test Calculator Tool
    println!("1. Calculator Tool");
    println!("-------------------");
    let result = registry
        .execute("calculator", json!({
            "operation": "multiply",
            "a": 7,
            "b": 6
        }))
        .await?;
    println!("   7 x 6 = {}", result.output);
    println!();

    // Test Text Formatter Tool
    println!("2. Text Formatter Tool");
    println!("-----------------------");
    let result = registry
        .execute("text_formatter", json!({
            "text": "hello world",
            "style": "leetspeak"
        }))
        .await?;
    println!("   'hello world' in leetspeak: {}", result.output);
    println!();

    // Test HTTP Status Tool
    println!("3. HTTP Status Tool (Async)");
    println!("---------------------------");
    let result = registry
        .execute("http_status", json!({
            "url": "https://example.com"
        }))
        .await?;
    println!("   {}", result.output);
    println!();

    // Test Counter Tool
    println!("4. Counter Tool (Custom Implementation)");
    println!("---------------------------------------");
    let result = registry
        .execute("counter", json!({
            "action": "increment",
            "amount": 5
        }))
        .await?;
    println!("   {}", result.output);
    println!();

    // Test JSON Query Tool
    println!("5. JSON Query Tool");
    println!("------------------");
    let result = registry
        .execute("json_query", json!({
            "json": r#"{"user": {"name": "Alice", "age": 30}}"#,
            "path": "user.name"
        }))
        .await?;
    println!("   user.name = {}", result.output);
    println!();

    // Show tool definitions (as would be sent to LLM)
    println!("Tool Definitions (for LLM integration):");
    println!("---------------------------------------");
    for def in registry.tool_definitions().iter().take(2) {
        println!("  Name: {}", def.name);
        println!("  Description: {}", def.description);
        println!("  Parameters: {}", serde_json::to_string_pretty(&def.parameters)?);
        println!();
    }

    Ok(())
}