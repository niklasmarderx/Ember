//! Chat command implementation for Ember CLI.
//!
//! This module powers the `ember chat` command and supports two modes:
//!
//! 1. **Simple Chat Mode**
//!    - Direct interaction with an AI model
//!    - Supports streaming responses
//!
//! 2. **Agent Mode (with tools)**
//!    - Enables AI to execute tools automatically
//!    - Available tools:
//!      - shell: run shell commands
//!      - filesystem: read/write files
//!      - web: fetch web pages
//!
//! ## Examples
//!
//! Basic chat:
//! ```bash
//! ember chat "Explain Rust ownership"
//! ```
//!
//! Interactive chat:
//! ```bash
//! ember chat
//! ```
//!
//! Using tools:
//! ```bash
//! ember chat --tools shell,filesystem
//! ```
//!
//! Custom model:
//! ```bash
//! ember chat --model gpt-4
//! ```

use crate::config::AppConfig;
use crate::ChatFormat;
use anyhow::{Context, Result};
use colored::Colorize;
use ember_llm::{CompletionRequest, LLMProvider, Message, OllamaProvider, OpenAIProvider};
use ember_tools::{FilesystemTool, ShellTool, ToolRegistry, WebTool};
use futures::StreamExt;
use serde_json;
use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Maximum iterations for tool execution loop to prevent infinite loops.
const MAX_TOOL_ITERATIONS: usize = 10;

/// Default timeout for LLM requests in seconds.
#[allow(dead_code)]
const LLM_TIMEOUT_SECS: u64 = 120;

/// Spinner frames for progress indicator.
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Response statistics for token counting and timing.
#[derive(Debug, Default)]
struct ResponseStats {
    tokens: usize,
    duration: Duration,
}

impl ResponseStats {
    fn tokens_per_second(&self) -> f64 {
        if self.duration.as_secs_f64() > 0.0 {
            self.tokens as f64 / self.duration.as_secs_f64()
        } else {
            0.0
        }
    }

    fn format(&self) -> String {
        format!(
            "[{} tokens, {:.1}s, {:.1} tok/s]",
            self.tokens,
            self.duration.as_secs_f64(),
            self.tokens_per_second()
        )
    }
}

/// Progress indicator that shows a spinner while waiting.
struct ProgressIndicator {
    message: String,
    stop_tx: Option<mpsc::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl ProgressIndicator {
    fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
            stop_tx: None,
            handle: None,
        }
    }

    fn start(&mut self) {
        let (tx, mut rx) = mpsc::channel::<()>(1);
        self.stop_tx = Some(tx);

        let message = self.message.clone();
        let handle = tokio::spawn(async move {
            let mut frame = 0;
            let start = Instant::now();

            loop {
                // Check if we should stop
                tokio::select! {
                    _ = rx.recv() => {
                        // Clear the spinner line
                        print!("\r{}\r", " ".repeat(60));
                        let _ = io::stdout().flush();
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(80)) => {
                        let elapsed = start.elapsed().as_secs();
                        let spinner = SPINNER_FRAMES[frame % SPINNER_FRAMES.len()];
                        print!(
                            "\r{} {} {} ({}s)",
                            spinner.bright_cyan(),
                            message.bright_yellow(),
                            ".".repeat((frame / 3) % 4).dimmed(),
                            elapsed
                        );
                        let _ = io::stdout().flush();
                        frame += 1;
                    }
                }
            }
        });

        self.handle = Some(handle);
    }

    async fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(()).await;
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }
}

/// Execute the `ember chat` command.
///
/// This function determines whether to run:
/// - **Simple chat mode** (no tools)
/// - **Agent mode** (tools enabled)
///
/// Behavior:
/// - If a message is provided → one-shot response
/// - If no message is provided → interactive session
///
/// # Arguments
///
/// - `config` – Loaded application configuration
/// - `message` – Optional message for one-shot chat
/// - `provider` – LLM provider override
/// - `model` – Model override
/// - `system` – Custom system prompt
/// - `temperature` – Sampling temperature
/// - `streaming` – Enable streaming output
/// - `tools` – Optional list of tools to enable
pub async fn run(
    config: AppConfig,
    message: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    system: Option<String>,
    temperature: Option<f32>,
    streaming: bool,
    tools: Option<Vec<String>>,
    format: ChatFormat,
) -> Result<()> {
    // Determine the provider to use (CLI override or config default)
    let provider_name = provider.unwrap_or_else(|| config.provider.default.clone());

    // Determine the model to use
    let model_name = model.unwrap_or_else(|| match provider_name.as_str() {
        "ollama" => config.provider.ollama.model.clone(),
        _ => config.provider.openai.model.clone(),
    });

    // Determine system prompt
    let system_prompt = system.unwrap_or_else(|| config.agent.system_prompt.clone());

    // Determine temperature
    let temp = temperature.unwrap_or(config.agent.temperature);

    // Create the LLM provider
    let llm_provider = create_provider(&config, &provider_name)?;

    // Check if tools are enabled
    if let Some(ref tool_names) = tools {
        // Agent mode with tools
        let registry = create_tool_registry(tool_names)?;

        if let Some(msg) = message {
            // One-shot agent mode
            agent_one_shot(
                llm_provider,
                &model_name,
                &system_prompt,
                temp,
                &msg,
                streaming,
                registry,
                format,
            )
            .await?;
        } else {
            // Interactive agent mode
            agent_interactive(
                llm_provider,
                &model_name,
                &system_prompt,
                temp,
                streaming,
                registry,
            )
            .await?;
        }
    } else {
        // Simple chat mode (no tools)
        if let Some(msg) = message {
            one_shot_chat(
                llm_provider,
                &model_name,
                &system_prompt,
                temp,
                &msg,
                streaming,
                format,
            )
            .await?;
        } else {
            interactive_chat(llm_provider, &model_name, &system_prompt, temp, streaming).await?;
        }
    }

    Ok(())
}

/// Build a registry of enabled tools.
///
/// The registry is used by the agent to discover and execute tools.
/// Supported tools:
///
/// - `shell` → run shell commands
/// - `filesystem` → file operations
/// - `web` → fetch web content
///
/// Invalid tools are ignored with a warning.
fn create_tool_registry(tool_names: &[String]) -> Result<ToolRegistry> {
    let mut registry = ToolRegistry::new();

    for name in tool_names {
        match name.to_lowercase().as_str() {
            "shell" => {
                info!("Registering shell tool");
                registry.register(ShellTool::new());
            }
            "filesystem" | "fs" => {
                info!("Registering filesystem tool");
                registry.register(FilesystemTool::new());
            }
            "web" | "http" => {
                info!("Registering web tool");
                registry.register(WebTool::new());
            }
            other => {
                warn!("Unknown tool: {}", other);
                eprintln!(
                    "{} Unknown tool '{}', skipping. Available: shell, filesystem, web",
                    "[warn]".bright_yellow(),
                    other
                );
            }
        }
    }

    if registry.is_empty() {
        anyhow::bail!("No valid tools specified. Available tools: shell, filesystem, web");
    }

    Ok(registry)
}

/// Create an LLM provider based on configuration and provider name.
fn create_provider(config: &AppConfig, provider_name: &str) -> Result<Arc<dyn LLMProvider>> {
    match provider_name {
        "ollama" => {
            let provider = OllamaProvider::new()
                .with_base_url(&config.provider.ollama.url)
                .with_default_model(&config.provider.ollama.model);
            Ok(Arc::new(provider))
        }
        "openai" | _ => {
            let api_key = config
                .provider
                .openai
                .api_key
                .clone()
                .or_else(|| std::env::var("OPENAI_API_KEY").ok())
                .context(
                    "OpenAI API key not found. Set OPENAI_API_KEY or configure in config file.",
                )?;

            let mut provider =
                OpenAIProvider::new(api_key).with_default_model(&config.provider.openai.model);

            if let Some(ref base_url) = config.provider.openai.base_url {
                provider = provider.with_base_url(base_url);
            }

            Ok(Arc::new(provider))
        }
    }
}

/// Execute a single AI task and exit.
///
/// This command is optimized for scripting and automation.
///
/// Example:
/// ```bash
/// ember run "Write a bash script that backs up files"
/// ```
pub async fn run_task(config: AppConfig, task: String, model: Option<String>) -> Result<()> {
    let provider_name = config.provider.default.clone();

    let model_name = model.unwrap_or_else(|| match provider_name.as_str() {
        "ollama" => config.provider.ollama.model.clone(),
        _ => config.provider.openai.model.clone(),
    });

    let system_prompt = format!(
        "{}\n\nYou are in task execution mode. Complete the following task and provide a clear, actionable response.",
        config.agent.system_prompt
    );

    let llm_provider = create_provider(&config, &provider_name)?;
    one_shot_chat(
        llm_provider,
        &model_name,
        &system_prompt,
        config.agent.temperature,
        &task,
        true,
        ChatFormat::Text,
    )
    .await
}

/// Agent one-shot mode: execute a single message with tool support.
async fn agent_one_shot(
    provider: Arc<dyn LLMProvider>,
    model: &str,
    system_prompt: &str,
    temperature: f32,
    message: &str,
    streaming: bool,
    registry: ToolRegistry,
    format: ChatFormat,
) -> Result<()> {
    // Only show progress info in text mode
    if format == ChatFormat::Text {
        println!(
            "{} Agent mode with {} tool(s): {}",
            "[ember]".bright_yellow(),
            registry.len().to_string().bright_green(),
            registry.tool_names().join(", ").bright_cyan()
        );
        println!(
            "   Using {} with {}",
            provider.name().bright_blue(),
            model.bright_green()
        );
        println!();
    }

    // Get tool definitions for the LLM
    let tools = registry.llm_tool_definitions();

    // Build initial conversation
    let mut history: Vec<Message> = vec![Message::system(system_prompt), Message::user(message)];

    // Tool execution loop
    for iteration in 0..MAX_TOOL_ITERATIONS {
        debug!("Tool iteration {}", iteration + 1);

        // Build request with tools
        let mut request = CompletionRequest::new(model).with_temperature(temperature);

        for msg in &history {
            request = request.with_message(msg.clone());
        }

        // Add tool definitions
        request = request.with_tools(tools.clone());

        // Get LLM response
        let response = provider
            .complete(request)
            .await
            .context("Failed to get response from LLM")?;

        // Check for tool calls
        if !response.tool_calls.is_empty() {
            // Add assistant message with tool calls to history
            let mut assistant_msg = Message::assistant(&response.content);
            assistant_msg.tool_calls = response.tool_calls.clone();
            history.push(assistant_msg);

            // Execute each tool call
            for call in &response.tool_calls {
                println!(
                    "{} Executing tool: {} {}",
                    "[tool]".bright_magenta(),
                    call.name.bright_cyan(),
                    format!("({})", truncate_json(&call.arguments, 50)).dimmed()
                );

                let result = registry.execute_tool_call(call).await;

                match &result {
                    Ok(tool_result) => {
                        let preview = truncate_str(&tool_result.output, 100);
                        if tool_result.success {
                            println!("{} {}", "[result]".bright_green(), preview);
                        } else {
                            println!("{} {}", "[error]".bright_red(), preview);
                        }
                        // Add tool result to history
                        history.push(Message::tool_result(&call.id, &tool_result.output));
                    }
                    Err(e) => {
                        let error_msg = format!("Tool execution failed: {}", e);
                        println!("{} {}", "[error]".bright_red(), &error_msg);
                        history.push(Message::tool_result(&call.id, &error_msg));
                    }
                }
            }

            // Continue loop to get next response
            continue;
        }

        // No tool calls - this is the final response
        match format {
            ChatFormat::Text => {
                println!();
                if streaming {
                    // For final response, use streaming if available
                    print_final_response(&response.content);
                } else {
                    println!("{}", response.content);
                }
            }
            ChatFormat::Json => {
                let output = serde_json::json!({
                    "response": response.content,
                    "model": model,
                    "provider": provider.name(),
                    "tools_used": response.tool_calls.iter().map(|tc| &tc.name).collect::<Vec<_>>(),
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            }
            ChatFormat::Markdown => {
                println!("## Response\n\n{}", response.content);
                println!("\n---\n*Model: {} | Provider: {}*", model, provider.name());
            }
        }

        return Ok(());
    }

    // Reached max iterations
    eprintln!(
        "{} Reached maximum tool iterations ({}). Stopping.",
        "[warn]".bright_yellow(),
        MAX_TOOL_ITERATIONS
    );

    Ok(())
}

/// Interactive agent mode with tool execution.
///
/// In this mode the AI can automatically:
///
/// - run shell commands
/// - read/write files
/// - fetch web content
///
/// Tool usage is displayed inline in the terminal.
async fn agent_interactive(
    provider: Arc<dyn LLMProvider>,
    model: &str,
    system_prompt: &str,
    temperature: f32,
    streaming: bool,
    registry: ToolRegistry,
) -> Result<()> {
    println!(
        "{} {} agent mode",
        "[ember]".bright_yellow(),
        "Ember".bright_yellow().bold()
    );
    println!(
        "   Using {} with {}",
        provider.name().bright_blue(),
        model.bright_green()
    );
    println!(
        "   {} tool(s) enabled: {}",
        registry.len().to_string().bright_green(),
        registry.tool_names().join(", ").bright_cyan()
    );
    println!(
        "   Type {} to exit, {} for help",
        "exit".bright_red(),
        "/help".bright_cyan()
    );
    if streaming {
        println!("   {} enabled", "Streaming".bright_green());
    }
    println!();

    // Get tool definitions
    let tools = registry.llm_tool_definitions();

    let mut history: Vec<Message> = vec![Message::system(system_prompt)];

    loop {
        // Print prompt
        print!("{} ", "You:".bright_green().bold());
        io::stdout().flush()?;

        // Read input
        let input = read_line()?;
        let input = input.trim();

        // Handle empty input
        if input.is_empty() {
            continue;
        }

        // Handle special commands
        if input.starts_with('/') {
            match input {
                "/help" | "/h" => {
                    print_agent_help(&registry);
                    continue;
                }
                "/clear" | "/c" => {
                    history = vec![Message::system(system_prompt)];
                    println!("{}", "Conversation cleared.".bright_yellow());
                    continue;
                }
                "/tools" => {
                    print_tools(&registry);
                    continue;
                }
                "/history" => {
                    print_history(&history);
                    continue;
                }
                "/model" => {
                    println!("Current model: {}", model.bright_green());
                    continue;
                }
                "/exit" | "/quit" | "/q" => {
                    println!("{}", "Goodbye!".bright_yellow());
                    break;
                }
                _ => {
                    println!(
                        "{} Unknown command: {}",
                        "[warn]".bright_yellow(),
                        input.bright_red()
                    );
                    continue;
                }
            }
        }

        // Handle exit commands
        if input == "exit" || input == "quit" {
            println!("{}", "Goodbye!".bright_yellow());
            break;
        }

        // Add user message to history
        history.push(Message::user(input));

        // Agent loop for this turn
        for iteration in 0..MAX_TOOL_ITERATIONS {
            debug!("Interactive tool iteration {}", iteration + 1);

            // Build request with tools
            let mut request = CompletionRequest::new(model).with_temperature(temperature);

            for msg in &history {
                request = request.with_message(msg.clone());
            }

            request = request.with_tools(tools.clone());

            // Print thinking indicator on first iteration
            if iteration == 0 {
                print!("{} ", "Ember:".bright_blue().bold());
                io::stdout().flush()?;
            }

            // Get response
            let response = match provider.complete(request).await {
                Ok(r) => r,
                Err(e) => {
                    println!("{}", format!("Error: {}", e).bright_red());
                    break;
                }
            };

            // Check for tool calls
            if !response.tool_calls.is_empty() {
                // Clear the thinking line if this is the first iteration
                if iteration == 0 {
                    println!();
                }

                // Add assistant message to history
                let mut assistant_msg = Message::assistant(&response.content);
                assistant_msg.tool_calls = response.tool_calls.clone();
                history.push(assistant_msg);

                // Execute tools
                for call in &response.tool_calls {
                    println!(
                        "  {} {} {}",
                        "[tool]".bright_magenta(),
                        call.name.bright_cyan(),
                        format!("({})", truncate_json(&call.arguments, 40)).dimmed()
                    );

                    let result = registry.execute_tool_call(call).await;

                    match &result {
                        Ok(tool_result) => {
                            let preview = truncate_str(&tool_result.output, 80);
                            if tool_result.success {
                                println!("  {} {}", "[ok]".bright_green(), preview.dimmed());
                            } else {
                                println!("  {} {}", "[fail]".bright_red(), preview);
                            }
                            history.push(Message::tool_result(&call.id, &tool_result.output));
                        }
                        Err(e) => {
                            let error_msg = format!("Tool error: {}", e);
                            println!("  {} {}", "[error]".bright_red(), &error_msg);
                            history.push(Message::tool_result(&call.id, &error_msg));
                        }
                    }
                }

                // Continue to next iteration
                continue;
            }

            // Final response (no tool calls)
            if iteration == 0 {
                // Response on same line
                if streaming {
                    println!("{}", response.content);
                } else {
                    println!("{}", response.content);
                }
            } else {
                // Response after tool calls
                print!("{} ", "Ember:".bright_blue().bold());
                println!("{}", response.content);
            }

            history.push(Message::assistant(&response.content));
            break;
        }

        println!();
    }

    Ok(())
}

/// One-shot chat: send a single message and print the response (no tools).
async fn one_shot_chat(
    provider: Arc<dyn LLMProvider>,
    model: &str,
    system_prompt: &str,
    temperature: f32,
    message: &str,
    streaming: bool,
    format: ChatFormat,
) -> Result<()> {
    // Build the request
    let request = CompletionRequest::new(model)
        .with_message(Message::system(system_prompt))
        .with_message(Message::user(message))
        .with_temperature(temperature);

    if streaming && format == ChatFormat::Text {
        // Only show progress indicator and stats in text mode
        println!(
            "{} Using {} with {}",
            "[ember]".bright_yellow(),
            provider.name().bright_blue(),
            model.bright_green()
        );
        println!();

        // Start progress indicator
        let mut progress = ProgressIndicator::new("Thinking");
        progress.start();

        // Streaming mode
        let stream_result = provider.complete_stream(request).await;

        // Stop progress indicator
        progress.stop().await;

        let mut stream = stream_result.context("Failed to start streaming response")?;

        let start_time = Instant::now();
        let mut token_count = 0usize;
        let mut full_response = String::new();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    if let Some(content) = chunk.content {
                        print!("{}", content);
                        io::stdout().flush()?;
                        full_response.push_str(&content);
                        // Approximate token count (rough estimate: ~4 chars per token)
                        token_count += (content.len() + 3) / 4;
                    }
                    if chunk.done {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("\n{} Stream error: {}", "[error]".bright_red(), e);
                    break;
                }
            }
        }

        // Print stats
        let stats = ResponseStats {
            tokens: token_count,
            duration: start_time.elapsed(),
        };
        println!();
        println!("{}", stats.format().dimmed());
    } else {
        // Non-streaming mode (or streaming with non-text format)
        let start_time = Instant::now();
        let result = provider.complete(request).await;

        let response = result.context("Failed to get response from LLM")?;
        let token_count = (response.content.len() + 3) / 4;

        match format {
            ChatFormat::Text => {
                println!(
                    "{} Using {} with {}",
                    "[ember]".bright_yellow(),
                    provider.name().bright_blue(),
                    model.bright_green()
                );
                println!();
                println!("{}", response.content);

                let stats = ResponseStats {
                    tokens: token_count,
                    duration: start_time.elapsed(),
                };
                println!("{}", stats.format().dimmed());
            }
            ChatFormat::Json => {
                let output = serde_json::json!({
                    "response": response.content,
                    "model": model,
                    "provider": provider.name(),
                    "tokens": token_count,
                    "duration_ms": start_time.elapsed().as_millis(),
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            }
            ChatFormat::Markdown => {
                println!("## Response\n\n{}", response.content);
                println!("\n---\n*Model: {} | Provider: {}*", model, provider.name());
            }
        }
    }

    Ok(())
}

/// Start interactive chat mode.
///
/// Users can continuously send prompts and receive responses.
/// Special commands available during chat:
///
/// - `/help`    – show commands
/// - `/clear`   – reset conversation
/// - `/history` – show conversation history
/// - `/model`   – show active model
/// - `/exit`    – exit chat
async fn interactive_chat(
    provider: Arc<dyn LLMProvider>,
    model: &str,
    system_prompt: &str,
    temperature: f32,
    streaming: bool,
) -> Result<()> {
    println!(
        "{} {} interactive mode",
        "[ember]".bright_yellow(),
        "Ember".bright_yellow().bold()
    );
    println!(
        "   Using {} with {}",
        provider.name().bright_blue(),
        model.bright_green()
    );
    println!(
        "   Type {} to exit, {} for help",
        "exit".bright_red(),
        "/help".bright_cyan()
    );
    if streaming {
        println!("   {} enabled", "Streaming".bright_green());
    }
    println!();

    let mut history: Vec<Message> = vec![Message::system(system_prompt)];

    loop {
        // Print prompt
        print!("{} ", "You:".bright_green().bold());
        io::stdout().flush()?;

        // Read input
        let input = read_line()?;
        let input = input.trim();

        // Handle empty input
        if input.is_empty() {
            continue;
        }

        // Handle special commands
        if input.starts_with('/') {
            match input {
                "/help" | "/h" => {
                    print_help();
                    continue;
                }
                "/clear" | "/c" => {
                    history = vec![Message::system(system_prompt)];
                    println!("{}", "Conversation cleared.".bright_yellow());
                    continue;
                }
                "/history" => {
                    print_history(&history);
                    continue;
                }
                "/model" => {
                    println!("Current model: {}", model.bright_green());
                    continue;
                }
                "/exit" | "/quit" | "/q" => {
                    println!("{}", "Goodbye!".bright_yellow());
                    break;
                }
                _ => {
                    println!(
                        "{} Unknown command: {}",
                        "[warn]".bright_yellow(),
                        input.bright_red()
                    );
                    continue;
                }
            }
        }

        // Handle exit commands
        if input == "exit" || input == "quit" {
            println!("{}", "Goodbye!".bright_yellow());
            break;
        }

        // Add user message to history
        history.push(Message::user(input));

        // Build request with full history
        let mut request = CompletionRequest::new(model).with_temperature(temperature);

        for msg in &history {
            request = request.with_message(msg.clone());
        }

        // Print thinking indicator
        print!("{} ", "Ember:".bright_blue().bold());
        io::stdout().flush()?;

        if streaming {
            // Streaming response
            match provider.complete_stream(request).await {
                Ok(mut stream) => {
                    let mut full_response = String::new();

                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(chunk) => {
                                if let Some(content) = chunk.content {
                                    print!("{}", content);
                                    io::stdout().flush()?;
                                    full_response.push_str(&content);
                                }
                                if chunk.done {
                                    break;
                                }
                            }
                            Err(e) => {
                                eprintln!("\n{} Stream error: {}", "[error]".bright_red(), e);
                                break;
                            }
                        }
                    }
                    println!();

                    // Add assistant response to history
                    if !full_response.is_empty() {
                        history.push(Message::assistant(&full_response));
                    }
                }
                Err(e) => {
                    println!("{}", format!("Error: {}", e).bright_red());
                }
            }
        } else {
            // Non-streaming response
            match provider.complete(request).await {
                Ok(response) => {
                    println!("{}", response.content);
                    history.push(Message::assistant(&response.content));
                }
                Err(e) => {
                    println!("{}", format!("Error: {}", e).bright_red());
                }
            }
        }

        println!();
    }

    Ok(())
}

/// Read a line from stdin.
fn read_line() -> Result<String> {
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input)
}

/// Print help information for simple chat mode.
fn print_help() {
    println!();
    println!("{}", "Commands:".bright_yellow().bold());
    println!("  {}  - Show this help", "/help".bright_cyan());
    println!("  {} - Clear conversation history", "/clear".bright_cyan());
    println!("  {} - Show conversation history", "/history".bright_cyan());
    println!("  {} - Show current model", "/model".bright_cyan());
    println!("  {}  - Exit the chat", "/exit".bright_cyan());
    println!();
    println!("{}", "Tips:".bright_yellow().bold());
    println!("  - Press Ctrl+C to cancel a request");
    println!("  - Type 'exit' or 'quit' to leave");
    println!("  - Streaming shows responses in real-time");
    println!("  - Use --tools flag to enable agent mode with tools");
    println!();
}

/// Print help information for agent mode.
fn print_agent_help(registry: &ToolRegistry) {
    println!();
    println!("{}", "Commands:".bright_yellow().bold());
    println!("  {}  - Show this help", "/help".bright_cyan());
    println!("  {} - Clear conversation history", "/clear".bright_cyan());
    println!("  {} - Show available tools", "/tools".bright_cyan());
    println!("  {} - Show conversation history", "/history".bright_cyan());
    println!("  {} - Show current model", "/model".bright_cyan());
    println!("  {}  - Exit the chat", "/exit".bright_cyan());
    println!();
    println!(
        "{} ({} enabled)",
        "Available Tools:".bright_yellow().bold(),
        registry.len()
    );
    for tool in registry.tool_definitions() {
        println!("  {} - {}", tool.name.bright_cyan(), tool.description);
    }
    println!();
    println!("{}", "Tips:".bright_yellow().bold());
    println!("  - The agent will automatically use tools when needed");
    println!("  - You can ask to run shell commands, read/write files, or fetch web pages");
    println!("  - Tool execution results are shown inline");
    println!();
}

/// Print available tools.
fn print_tools(registry: &ToolRegistry) {
    println!();
    println!("{}", "Available Tools:".bright_yellow().bold());
    for tool in registry.tool_definitions() {
        println!("  {} - {}", tool.name.bright_cyan(), tool.description);
    }
    println!();
}

/// Print conversation history.
fn print_history(history: &[Message]) {
    if history.len() <= 1 {
        println!("{}", "No conversation history.".bright_yellow());
        return;
    }

    println!();
    println!("{}", "Conversation History:".bright_yellow().bold());

    let mut turn = 0;
    for msg in history.iter().skip(1) {
        // Skip system prompt
        match msg.role {
            ember_llm::Role::User => {
                turn += 1;
                println!("{}. {}: {}", turn, "You".bright_green(), msg.content);
            }
            ember_llm::Role::Assistant => {
                let preview: String = msg.content.chars().take(100).collect();
                let suffix = if msg.content.len() > 100 { "..." } else { "" };
                println!("   {}: {}{}", "Ember".bright_blue(), preview, suffix);
            }
            ember_llm::Role::Tool => {
                let preview: String = msg.content.chars().take(60).collect();
                let suffix = if msg.content.len() > 60 { "..." } else { "" };
                println!(
                    "   {}: {}{}",
                    "[tool result]".dimmed(),
                    preview.dimmed(),
                    suffix
                );
            }
            _ => {}
        }
    }
    println!();
}

/// Print final response with formatting.
fn print_final_response(content: &str) {
    println!("{}", content);
}

/// Truncate a string to a maximum length.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Truncate JSON value to a short preview.
fn truncate_json(value: &serde_json::Value, max_len: usize) -> String {
    let s = value.to_string();
    truncate_str(&s, max_len)
}
