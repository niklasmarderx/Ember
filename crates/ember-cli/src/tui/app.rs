//! TUI Application state and event handling.

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
use tokio::sync::mpsc;

use super::ui;
use crate::config::AppConfig;
use ember_llm::{LLMProvider, Message};

/// Application state
#[allow(dead_code)]
pub enum AppState {
    /// Normal input mode
    Normal,
    /// Typing a message
    Input,
    /// Waiting for AI response
    Waiting,
    /// Showing help
    Help,
}

/// A chat message entry
pub struct ChatMessage {
    /// Role: "user" or "assistant"
    pub role: String,
    /// Message content
    pub content: String,
    /// Token count (if known)
    pub tokens: Option<u32>,
}

/// Main application struct
pub struct App {
    /// Current state
    pub state: AppState,
    /// Chat messages
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
}

impl App {
    /// Create a new app instance
    pub fn new(config: &AppConfig) -> Result<Self> {
        // Reuse the same provider creation logic as chat to respect config-file keys
        let provider = crate::commands::chat::create_provider(config, &config.provider.default)?;

        let model = match config.provider.default.to_lowercase().as_str() {
            "ollama" => config.provider.ollama.model.clone(),
            _ => config.provider.openai.model.clone(),
        };

        Ok(Self {
            state: AppState::Input,
            messages: Vec::new(),
            input: String::new(),
            cursor: 0,
            scroll: 0,
            model,
            total_tokens: 0,
            status: "Ready".to_string(),
            should_quit: false,
            provider,
            system_prompt: config.agent.system_prompt.clone(),
        })
    }

    /// Handle a key event
    pub fn handle_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        match self.state {
            AppState::Help => {
                // Any key exits help
                self.state = AppState::Input;
            }
            AppState::Waiting => {
                // Only Ctrl+C to cancel
                if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                    self.status = "Cancelled".to_string();
                    self.state = AppState::Input;
                }
            }
            AppState::Input | AppState::Normal => {
                match key {
                    KeyCode::Enter => {
                        if !self.input.is_empty() {
                            // Submit message
                            let msg = self.input.clone();
                            self.messages.push(ChatMessage {
                                role: "user".to_string(),
                                content: msg,
                                tokens: None,
                            });
                            self.input.clear();
                            self.cursor = 0;
                            self.state = AppState::Waiting;
                            self.status = "Thinking...".to_string();
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

    /// Build messages for LLM
    pub fn build_messages(&self) -> Vec<Message> {
        let mut msgs = vec![Message::system(&self.system_prompt)];
        for msg in &self.messages {
            match msg.role.as_str() {
                "user" => msgs.push(Message::user(&msg.content)),
                "assistant" => msgs.push(Message::assistant(&msg.content)),
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
        });
        self.total_tokens += tokens;
        self.state = AppState::Input;
        self.status = "Ready".to_string();
    }

    /// Set error status
    pub fn set_error(&mut self, error: String) {
        self.status = format!("Error: {}", error);
        self.state = AppState::Input;
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

    // Setup terminal
    enable_raw_mode().map_err(|e| anyhow::anyhow!(
        "Failed to initialize terminal: {}. Make sure you're running in a real terminal emulator.", e
    ))?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create channel for async responses
    let (response_tx, mut response_rx) = mpsc::channel::<Result<(String, u32)>>(1);
    let mut request_sent = false;

    // Main loop
    loop {
        // Draw UI
        terminal.draw(|f| ui::render(f, &app))?;

        // Check for async response (always, not just on key events)
        if let Ok(result) = response_rx.try_recv() {
            match result {
                Ok((content, tokens)) => app.add_response(content, tokens),
                Err(e) => app.set_error(e.to_string()),
            }
            request_sent = false;
        }

        // If we're in Waiting state and haven't sent a request yet, send one
        if matches!(app.state, AppState::Waiting) && !request_sent {
            let provider = app.provider.clone();
            let model = app.model.clone();
            let messages = app.build_messages();
            let tx = response_tx.clone();

            request_sent = true;
            tokio::spawn(async move {
                let request =
                    ember_llm::CompletionRequest::from_messages(messages).with_model(&model);

                let result = provider.complete(request).await;
                let _ = tx
                    .send(
                        result
                            .map(|r| (r.content, r.usage.total_tokens))
                            .map_err(|e| anyhow::anyhow!("{}", e)),
                    )
                    .await;
            });
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
