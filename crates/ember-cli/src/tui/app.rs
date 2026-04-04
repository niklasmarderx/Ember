//! TUI Application state and event handling.
//!
//! Supports full agent mode: the LLM can request tool calls (shell, filesystem,
//! git, web) and the TUI executes them automatically, showing results inline.

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

use super::ui;
use crate::config::AppConfig;
use ember_llm::{CompletionRequest, LLMProvider, Message, ToolDefinition};
use ember_tools::ToolRegistry;

/// Maximum tool-call iterations per user message (safety limit).
const MAX_TOOL_ITERATIONS: usize = 25;

/// Application state
#[allow(dead_code)]
pub enum AppState {
    /// Normal input mode
    Normal,
    /// Typing a message
    Input,
    /// Waiting for AI response
    Waiting,
    /// AI is executing tool calls
    ExecutingTool,
    /// Showing help
    Help,
}

/// A chat message entry displayed in the TUI.
///
/// The `role` field determines rendering:
/// - `"user"` — user input (>> marker)
/// - `"assistant"` — AI response (<< marker)
/// - `"tool_call"` — tool invocation (shows tool name + args)
/// - `"tool_result"` — tool output (shows success/error + output)
pub struct ChatMessage {
    /// Role: "user", "assistant", "tool_call", or "tool_result"
    pub role: String,
    /// Message content
    pub content: String,
    /// Token count (if known)
    pub tokens: Option<u32>,
    /// Timestamp when the message was created
    pub timestamp: Instant,
    /// For tool_call: the tool name
    pub tool_name: Option<String>,
    /// For tool_result: whether execution succeeded
    pub tool_success: Option<bool>,
}

impl ChatMessage {
    /// Create a standard chat message.
    fn chat(role: &str, content: String) -> Self {
        Self {
            role: role.to_string(),
            content,
            tokens: None,
            timestamp: Instant::now(),
            tool_name: None,
            tool_success: None,
        }
    }

    /// Create a tool call display message.
    fn tool_call(name: &str, args_summary: &str) -> Self {
        Self {
            role: "tool_call".to_string(),
            content: args_summary.to_string(),
            tokens: None,
            timestamp: Instant::now(),
            tool_name: Some(name.to_string()),
            tool_success: None,
        }
    }

    /// Create a tool result display message.
    fn tool_result(name: &str, output: &str, success: bool) -> Self {
        Self {
            role: "tool_result".to_string(),
            content: output.to_string(),
            tokens: None,
            timestamp: Instant::now(),
            tool_name: Some(name.to_string()),
            tool_success: Some(success),
        }
    }
}

/// Main application struct
pub struct App {
    /// Current state
    pub state: AppState,
    /// Chat messages (display-only, includes tool call/result entries)
    pub messages: Vec<ChatMessage>,
    /// Current input buffer
    pub input: String,
    /// Cursor position in input
    pub cursor: usize,
    /// Scroll position in chat history
    pub scroll: usize,
    /// Model being used
    pub model: String,
    /// Total tokens used
    pub total_tokens: u32,
    /// Status message
    pub status: String,
    /// Should quit
    pub should_quit: bool,
    /// LLM provider
    provider: Arc<dyn LLMProvider>,
    /// System prompt
    system_prompt: String,
    /// Animation tick counter (incremented every frame for animations)
    pub tick: u64,
    /// Time the app was started
    pub start_time: Instant,
    /// Number of messages sent by user
    pub user_message_count: u32,
    /// Streaming response buffer (partial response being received)
    #[allow(dead_code)]
    pub streaming_content: Option<String>,
    /// Whether agent mode (tools) is enabled
    pub agent_mode: bool,
    /// Number of tool calls executed in this session
    pub tool_call_count: u32,
    /// Tool names available
    pub tool_names: Vec<String>,
    /// Currently executing tool name (for status display)
    pub current_tool: Option<String>,
}

impl App {
    /// Create a new app instance
    pub fn new(config: &AppConfig) -> Result<Self> {
        // Reuse the same provider creation logic as chat to respect config-file keys
        let provider =
            crate::commands::provider_factory::create_provider(config, &config.provider.default)?;

        let model = match config.provider.default.to_lowercase().as_str() {
            "ollama" => config.provider.ollama.model.clone(),
            _ => config.provider.openai.model.clone(),
        };

        // Create default tool registry (same as `ember chat`)
        let registry = crate::commands::provider_factory::create_default_tool_registry(config);
        let tool_names = registry.tool_names();
        let agent_mode = !tool_names.is_empty();

        Ok(Self {
            state: AppState::Input,
            messages: Vec::new(),
            input: String::new(),
            cursor: 0,
            scroll: 0,
            model,
            total_tokens: 0,
            status: if agent_mode {
                format!("Agent ready -- {} tools", tool_names.len())
            } else {
                "Ready".to_string()
            },
            should_quit: false,
            provider,
            system_prompt: config.agent.system_prompt.clone(),
            tick: 0,
            start_time: Instant::now(),
            user_message_count: 0,
            streaming_content: None,
            agent_mode,
            tool_call_count: 0,
            tool_names,
            current_tool: None,
        })
    }

    /// Advance the animation tick
    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Handle a key event
    pub fn handle_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        match self.state {
            AppState::Help => {
                // Any key exits help
                self.state = AppState::Input;
            }
            AppState::Waiting | AppState::ExecutingTool => {
                // Only Ctrl+C to cancel
                if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                    self.status = "Cancelled".to_string();
                    self.current_tool = None;
                    self.state = AppState::Input;
                }
            }
            AppState::Input | AppState::Normal => {
                match key {
                    KeyCode::Enter => {
                        if !self.input.is_empty() {
                            // Submit message
                            let msg = self.input.clone();
                            self.messages.push(ChatMessage::chat("user", msg));
                            self.input.clear();
                            self.cursor = 0;
                            self.state = AppState::Waiting;
                            self.status = "Thinking...".to_string();
                            self.user_message_count += 1;
                        }
                    }
                    KeyCode::Char(c) => {
                        if modifiers.contains(KeyModifiers::CONTROL) {
                            match c {
                                'c' => self.should_quit = true,
                                'l' => {
                                    // Clear screen
                                    self.messages.clear();
                                    self.total_tokens = 0;
                                    self.tool_call_count = 0;
                                    self.status = "Cleared".to_string();
                                }
                                'h' => self.state = AppState::Help,
                                _ => {}
                            }
                        } else {
                            self.input.insert(self.cursor, c);
                            self.cursor += 1;
                        }
                    }
                    KeyCode::Backspace => {
                        if self.cursor > 0 {
                            self.cursor -= 1;
                            self.input.remove(self.cursor);
                        }
                    }
                    KeyCode::Delete => {
                        if self.cursor < self.input.len() {
                            self.input.remove(self.cursor);
                        }
                    }
                    KeyCode::Left => {
                        if self.cursor > 0 {
                            self.cursor -= 1;
                        }
                    }
                    KeyCode::Right => {
                        if self.cursor < self.input.len() {
                            self.cursor += 1;
                        }
                    }
                    KeyCode::Home => self.cursor = 0,
                    KeyCode::End => self.cursor = self.input.len(),
                    KeyCode::Up => {
                        // Scroll up in history
                        if self.scroll < self.messages.len().saturating_sub(1) {
                            self.scroll += 1;
                        }
                    }
                    KeyCode::Down => {
                        // Scroll down in history
                        if self.scroll > 0 {
                            self.scroll -= 1;
                        }
                    }
                    KeyCode::Esc => {
                        if !self.input.is_empty() {
                            self.input.clear();
                            self.cursor = 0;
                        } else {
                            self.should_quit = true;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Handle pasted text
    pub fn handle_paste(&mut self, text: String) {
        if matches!(self.state, AppState::Input | AppState::Normal) {
            // Filter out control chars, keep printable + newlines→spaces
            let clean: String = text
                .chars()
                .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
                .filter(|c| !c.is_control())
                .collect();
            for c in clean.chars() {
                self.input.insert(self.cursor, c);
                self.cursor += 1;
            }
        }
    }

    /// Get the last user message for LLM request
    #[allow(dead_code)]
    pub fn last_user_message(&self) -> Option<&str> {
        self.messages.iter().rev().find_map(|m| {
            if m.role == "user" {
                Some(m.content.as_str())
            } else {
                None
            }
        })
    }

    /// Build LLM message history from display messages.
    ///
    /// Translates display messages to the format the LLM expects,
    /// skipping tool_call/tool_result display entries (those are added
    /// during the agent loop directly to the LLM history).
    pub fn build_messages(&self) -> Vec<Message> {
        let mut msgs = vec![Message::system(&self.system_prompt)];
        for msg in &self.messages {
            match msg.role.as_str() {
                "user" => msgs.push(Message::user(&msg.content)),
                "assistant" => msgs.push(Message::assistant(&msg.content)),
                // tool_call and tool_result are managed by the agent loop
                _ => {}
            }
        }
        msgs
    }

    /// Add assistant response
    pub fn add_response(&mut self, content: String, tokens: u32) {
        self.messages.push(ChatMessage {
            role: "assistant".to_string(),
            content,
            tokens: Some(tokens),
            timestamp: Instant::now(),
            tool_name: None,
            tool_success: None,
        });
        self.total_tokens += tokens;
        self.state = AppState::Input;
        self.current_tool = None;
        self.status = if self.agent_mode {
            format!("Agent ready -- {} tools used", self.tool_call_count)
        } else {
            "Ready".to_string()
        };
    }

    /// Add a tool call display message
    pub fn add_tool_call(&mut self, name: &str, args_summary: &str) {
        self.messages
            .push(ChatMessage::tool_call(name, args_summary));
        self.tool_call_count += 1;
        self.current_tool = Some(name.to_string());
        self.state = AppState::ExecutingTool;
        self.status = format!("Executing: {}", name);
    }

    /// Add a tool result display message
    pub fn add_tool_result(&mut self, name: &str, output: &str, success: bool) {
        // Truncate long output for display
        let display_output = if output.len() > 500 {
            format!("{}... [{} chars]", &output[..500], output.len())
        } else {
            output.to_string()
        };
        self.messages
            .push(ChatMessage::tool_result(name, &display_output, success));
        self.current_tool = None;
    }

    /// Set error status
    pub fn set_error(&mut self, error: String) {
        self.status = format!("Error: {}", error);
        self.current_tool = None;
        self.state = AppState::Input;
    }

    /// Get uptime in a human-readable format
    pub fn uptime(&self) -> String {
        let elapsed = self.start_time.elapsed();
        let secs = elapsed.as_secs();
        if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        }
    }
}

/// Message sent from the agent background task to the UI.
enum AgentEvent {
    /// A tool call is about to be executed
    ToolCall { name: String, args_summary: String },
    /// A tool call completed
    ToolResult {
        name: String,
        output: String,
        success: bool,
    },
    /// The agent produced its final text response
    FinalResponse { content: String, tokens: u32 },
    /// An error occurred
    Error(String),
    /// Status update (e.g. "Iteration 3/25...")
    Status(String),
}

/// Summarize tool call arguments for display (truncated JSON).
fn summarize_args(args: &serde_json::Value) -> String {
    let s = args.to_string();
    if s.len() > 120 {
        format!("{}...", &s[..120])
    } else {
        s
    }
}

/// Run the TUI application
pub async fn run(config: AppConfig) -> Result<()> {
    // Verify we have a real terminal
    use std::io::IsTerminal;
    if !io::stdout().is_terminal() {
        anyhow::bail!("ember tui requires an interactive terminal. Run it directly in your terminal (not piped or in a subshell).");
    }

    // Create app
    let mut app = App::new(&config)?;

    // Create tool registry for the agent loop
    let registry =
        Arc::new(crate::commands::provider_factory::create_default_tool_registry(&config));

    // Setup terminal
    enable_raw_mode().map_err(|e| {
        anyhow::anyhow!(
            "Failed to initialize terminal: {}. Make sure you're running in a real terminal emulator.",
            e
        )
    })?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create channel for agent events
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(32);
    let mut request_sent = false;

    // Main loop
    loop {
        // Advance animation tick
        app.tick();

        // Draw UI
        terminal.draw(|f| ui::render(f, &app))?;

        // Check for agent events (tool calls, results, final response)
        while let Ok(agent_event) = event_rx.try_recv() {
            match agent_event {
                AgentEvent::ToolCall { name, args_summary } => {
                    app.add_tool_call(&name, &args_summary);
                }
                AgentEvent::ToolResult {
                    name,
                    output,
                    success,
                } => {
                    app.add_tool_result(&name, &output, success);
                    app.state = AppState::Waiting;
                    app.status = "Thinking...".to_string();
                }
                AgentEvent::FinalResponse { content, tokens } => {
                    app.add_response(content, tokens);
                    request_sent = false;
                }
                AgentEvent::Error(e) => {
                    app.set_error(e);
                    request_sent = false;
                }
                AgentEvent::Status(s) => {
                    app.status = s;
                }
            }
        }

        // If we're in Waiting state and haven't sent a request yet, send one
        if matches!(app.state, AppState::Waiting) && !request_sent {
            let provider = app.provider.clone();
            let model = app.model.clone();
            let messages = app.build_messages();
            let tx = event_tx.clone();
            let agent_mode = app.agent_mode;
            let tool_registry = registry.clone();

            request_sent = true;

            if agent_mode {
                // Spawn the full agent loop (with tool execution)
                let tool_defs = tool_registry.llm_tool_definitions();
                tokio::spawn(async move {
                    agent_loop(provider, model, messages, tool_defs, tool_registry, tx).await;
                });
            } else {
                // Simple chat (no tools)
                tokio::spawn(async move {
                    let request = CompletionRequest::from_messages(messages).with_model(&model);
                    let result = provider.complete(request).await;
                    match result {
                        Ok(r) => {
                            let _ = tx
                                .send(AgentEvent::FinalResponse {
                                    content: r.content,
                                    tokens: r.usage.total_tokens,
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = tx.send(AgentEvent::Error(e.to_string())).await;
                        }
                    }
                });
            }
        }

        // Handle input — drain ALL pending events to handle fast paste
        while event::poll(std::time::Duration::from_millis(0))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.handle_key(key.code, key.modifiers);
                }
                Event::Paste(text) => {
                    app.handle_paste(text);
                }
                _ => {} // ignore Release/Repeat/Mouse/Resize
            }
        }

        // Wait briefly if no events were ready (avoid busy-loop)
        if event::poll(std::time::Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.handle_key(key.code, key.modifiers);
                }
                Event::Paste(text) => {
                    app.handle_paste(text);
                }
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )?;
    terminal.show_cursor()?;

    Ok(())
}

/// Background agent loop: sends LLM requests, executes tool calls, feeds
/// results back to the LLM, and finally sends the text response to the UI.
async fn agent_loop(
    provider: Arc<dyn LLMProvider>,
    model: String,
    initial_messages: Vec<Message>,
    tool_defs: Vec<ToolDefinition>,
    registry: Arc<ToolRegistry>,
    tx: mpsc::Sender<AgentEvent>,
) {
    let mut history = initial_messages;

    for iteration in 0..MAX_TOOL_ITERATIONS {
        // Build request with tool definitions
        let request = CompletionRequest::new(&model)
            .with_messages(history.clone())
            .with_tools(tool_defs.clone());

        let response = match provider.complete(request).await {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(AgentEvent::Error(e.to_string())).await;
                return;
            }
        };

        // If there are tool calls, execute them
        if !response.tool_calls.is_empty() {
            // Add assistant message (with tool calls) to history
            let mut assistant_msg = Message::assistant(&response.content);
            assistant_msg.tool_calls = response.tool_calls.clone();
            history.push(assistant_msg);

            let _ = tx
                .send(AgentEvent::Status(format!(
                    "Tool iteration {}/{}",
                    iteration + 1,
                    MAX_TOOL_ITERATIONS
                )))
                .await;

            for call in &response.tool_calls {
                let args_summary = summarize_args(&call.arguments);

                // Notify UI about the tool call
                let _ = tx
                    .send(AgentEvent::ToolCall {
                        name: call.name.clone(),
                        args_summary,
                    })
                    .await;

                // Execute the tool
                let result = registry.execute(&call.name, call.arguments.clone()).await;
                match result {
                    Ok(tool_output) => {
                        // Notify UI about the result
                        let _ = tx
                            .send(AgentEvent::ToolResult {
                                name: call.name.clone(),
                                output: tool_output.output.clone(),
                                success: tool_output.success,
                            })
                            .await;
                        // Add tool result to LLM history
                        history.push(Message::tool_result(&call.id, &tool_output.output));
                    }
                    Err(e) => {
                        let error_msg = format!("Tool error: {}", e);
                        let _ = tx
                            .send(AgentEvent::ToolResult {
                                name: call.name.clone(),
                                output: error_msg.clone(),
                                success: false,
                            })
                            .await;
                        history.push(Message::tool_result(&call.id, &error_msg));
                    }
                }
            }
            // Continue the loop — LLM will see tool results and decide next action
            continue;
        }

        // No tool calls — this is the final text response
        let _ = tx
            .send(AgentEvent::FinalResponse {
                content: response.content,
                tokens: response.usage.total_tokens,
            })
            .await;
        return;
    }

    // Reached max iterations
    let _ = tx
        .send(AgentEvent::Error(format!(
            "Reached maximum tool iterations ({})",
            MAX_TOOL_ITERATIONS
        )))
        .await;
}
