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
//! ## Session Persistence
//!
//! Interactive sessions are saved to `~/.ember/sessions/<id>.json`.
//! Use `--continue` to resume the last session or `--resume <id>` for a
//! specific one.
//!
//! ## Slash Commands
//!
//! Type `/help` inside any REPL to see available slash commands.
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
//! Resume last session:
//! ```bash
//! ember chat --continue
//! ```
//!
//! Using tools:
//! ```bash
//! ember chat --tools shell,filesystem
//! ```
//!
//! Custom model alias:
//! ```bash
//! ember chat --model fast
//! ```

use crate::commands::slash::{SlashCommand, SlashCommandRegistry};
use crate::config::AppConfig;
use crate::ChatFormat;
use anyhow::{Context, Result};
use colored::Colorize;
use ember_core::usage_tracker::SessionUsageTracker;
use ember_llm::{CompletionRequest, LLMProvider, Message, OllamaProvider, OpenAIProvider};
use ember_llm::router::is_model_alias;
use ember_storage::semantic_cache::{
    CacheContext, SemanticCache, SemanticCacheBuilder,
};
use ember_tools::filesystem::undo_last as filesystem_undo_last;
use ember_tools::{FilesystemTool, GitTool, ShellTool, ToolRegistry, WebTool};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json;
use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Semantic cache similarity threshold for chat: treat >0.92 as a hit.
const CACHE_SIMILARITY_THRESHOLD: f32 = 0.92;

/// Maximum iterations for tool execution loop to prevent infinite loops.
const MAX_TOOL_ITERATIONS: usize = 10;

/// Default timeout for LLM requests in seconds.
#[allow(dead_code)]
const LLM_TIMEOUT_SECS: u64 = 120;

/// Spinner frames for progress indicator.
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

// ──────────────────────────────────────────────────────────────────────────────
// Session Persistence
// ──────────────────────────────────────────────────────────────────────────────

/// A persisted chat session stored as JSON in `~/.ember/sessions/`.
#[derive(Debug, Serialize, Deserialize)]
pub struct PersistedSession {
    /// Unique session identifier (UUIDv4-style hex string).
    pub id: String,
    /// Provider name used for this session.
    pub provider: String,
    /// Model name used for this session.
    pub model: String,
    /// ISO-8601 timestamp when the session was created.
    pub created_at: String,
    /// ISO-8601 timestamp of the last message.
    pub updated_at: String,
    /// Message history (serialised form of `ember_llm::Message`).
    pub messages: Vec<PersistedMessage>,
    /// Total turn count (system message excluded).
    pub turn_count: usize,
}

/// A single serialised message in a persisted session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedMessage {
    /// Role: "system" | "user" | "assistant" | "tool"
    pub role: String,
    /// Message text content.
    pub content: String,
    /// Tool-call id (only set for tool-result messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl PersistedMessage {
    fn from_message(msg: &Message) -> Self {
        let role = format!("{:?}", msg.role).to_lowercase();
        Self {
            role,
            content: msg.content.clone(),
            tool_call_id: msg.tool_call_id.clone(),
        }
    }

    fn to_message(&self) -> Message {
        match self.role.as_str() {
            "system" => Message::system(&self.content),
            "user" => Message::user(&self.content),
            "assistant" => Message::assistant(&self.content),
            "tool" => {
                let id = self.tool_call_id.as_deref().unwrap_or("unknown");
                Message::tool_result(id, &self.content)
            }
            _ => Message::user(&self.content),
        }
    }
}

/// Return the path to the sessions directory, creating it if needed.
fn sessions_dir() -> Result<std::path::PathBuf> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let dir = home.join(".ember").join("sessions");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Generate a short random hex session id.
fn new_session_id() -> String {
    use std::time::SystemTime;
    let t = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{:08x}", t ^ std::process::id())
}

/// Save a session to disk.
fn save_session(session: &PersistedSession) -> Result<()> {
    let dir = sessions_dir()?;
    let path = dir.join(format!("{}.json", session.id));
    let json = serde_json::to_string_pretty(session)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Load a session by id.
pub fn load_session(id: &str) -> Result<PersistedSession> {
    let dir = sessions_dir()?;
    let path = dir.join(format!("{}.json", id));
    let json = std::fs::read_to_string(&path)
        .with_context(|| format!("Session '{}' not found", id))?;
    let session: PersistedSession = serde_json::from_str(&json)?;
    Ok(session)
}

/// Find the most recently modified session file and return its id.
pub fn latest_session_id() -> Option<String> {
    let dir = sessions_dir().ok()?;
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().and_then(|s| s.to_str()) == Some("json")
        })
        .collect();

    entries.sort_by_key(|e| {
        e.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    entries
        .last()
        .and_then(|e| e.path().file_stem()?.to_str().map(str::to_owned))
}

/// Current time as a simple ISO-8601 string (without external dependencies).
fn now_iso8601() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Produce a compact UTC timestamp: YYYY-MM-DDTHH:MM:SSZ
    let s = secs;
    let (y, mo, d, h, mi, sec) = seconds_to_ymd_hms(s);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, mi, sec)
}

fn seconds_to_ymd_hms(mut s: u64) -> (u32, u32, u32, u32, u32, u32) {
    let sec = (s % 60) as u32;
    s /= 60;
    let min = (s % 60) as u32;
    s /= 60;
    let hour = (s % 24) as u32;
    s /= 24;
    // Days since 1970-01-01 → Gregorian date (simplified, good until ~2100)
    let mut days = s as u32;
    let mut y = 1970u32;
    loop {
        let dy = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 366 } else { 365 };
        if days < dy { break; }
        days -= dy;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let month_days = [31u32, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut mo = 0u32;
    for md in &month_days {
        if days < *md { break; }
        days -= md;
        mo += 1;
    }
    (y, mo + 1, days + 1, hour, min, sec)
}

// ──────────────────────────────────────────────────────────────────────────────
// Response statistics
// ──────────────────────────────────────────────────────────────────────────────

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

// ──────────────────────────────────────────────────────────────────────────────
// Progress indicator
// ──────────────────────────────────────────────────────────────────────────────

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
                tokio::select! {
                    _ = rx.recv() => {
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

// ──────────────────────────────────────────────────────────────────────────────
// Public run() entry point
// ──────────────────────────────────────────────────────────────────────────────

/// Execute the `ember chat` command.
#[allow(clippy::too_many_arguments)]
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
    resume_id: Option<String>,
    continue_last: bool,
) -> Result<()> {
    let provider_name = provider.unwrap_or_else(|| config.provider.default.clone());

    // Resolve model aliases: "fast" / "smart" / "code" / "local"
    let raw_model = model.unwrap_or_else(|| match provider_name.as_str() {
        "ollama" => config.provider.ollama.model.clone(),
        _ => config.provider.openai.model.clone(),
    });

    // When the user passes an alias, pick the provider from the first candidate
    // that has a registered provider. For simplicity we just use the alias as-is
    // in the CLI path and let FallbackRouter handle it when we have one; here we
    // resolve to the first candidate's concrete model + provider names.
    let (provider_name, model_name) = if is_model_alias(&raw_model) {
        use ember_llm::router::resolve_model_alias;
        let candidates = resolve_model_alias(&raw_model);
        // Use the first candidate whose provider matches one we support in create_provider
        let candidate = candidates
            .into_iter()
            .find(|c| matches!(c.provider, "openai" | "anthropic" | "ollama" | "gemini" | "groq" | "deepseek" | "mistral"))
            .unwrap_or_else(|| ember_llm::router::ModelCandidate::new("openai", "gpt-4o-mini", 0.15));
        eprintln!(
            "{} Model alias '{}' → {} ({})",
            "[ember]".bright_yellow(),
            raw_model,
            candidate.model.bright_green(),
            candidate.provider.bright_blue()
        );
        (candidate.provider.to_owned(), candidate.model.clone())
    } else {
        (provider_name, raw_model)
    };

    let base_system_prompt = system.unwrap_or_else(|| config.agent.system_prompt.clone());
    let system_prompt = if let Some(ctx) = load_project_context() {
        println!(
            "{} Loaded project context from EMBER.md",
            "📋 [ember]".bright_yellow()
        );
        format!("{}\n\n---\n\n{}", ctx, base_system_prompt)
    } else {
        base_system_prompt
    };
    let temp = temperature.unwrap_or(config.agent.temperature);
    let llm_provider = create_provider(&config, &provider_name)?;

    // Resolve session to resume (if any)
    let resume_session: Option<PersistedSession> = if continue_last {
        match latest_session_id() {
            Some(id) => match load_session(&id) {
                Ok(s) => {
                    println!(
                        "{} Resuming last session {} ({} turns)",
                        "[ember]".bright_yellow(),
                        s.id.bright_cyan(),
                        s.turn_count
                    );
                    Some(s)
                }
                Err(e) => {
                    warn!("Could not load last session: {}", e);
                    None
                }
            },
            None => {
                println!("{} No previous session found, starting fresh.", "[ember]".bright_yellow());
                None
            }
        }
    } else if let Some(ref id) = resume_id {
        match load_session(id) {
            Ok(s) => {
                println!(
                    "{} Resuming session {} ({} turns)",
                    "[ember]".bright_yellow(),
                    s.id.bright_cyan(),
                    s.turn_count
                );
                Some(s)
            }
            Err(e) => {
                eprintln!("{} {}", "[error]".bright_red(), e);
                None
            }
        }
    } else {
        None
    };

    if let Some(ref tool_names) = tools {
        let registry = create_tool_registry(tool_names)?;
        if let Some(msg) = message {
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
            agent_interactive(
                &config,
                llm_provider,
                &model_name,
                &system_prompt,
                temp,
                streaming,
                registry,
                resume_session,
            )
            .await?;
        }
    } else if let Some(msg) = message {
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
        interactive_chat(
            &config,
            llm_provider,
            &model_name,
            &system_prompt,
            temp,
            streaming,
            resume_session,
        )
        .await?;
    }

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Build a registry of enabled tools.
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

/// Build a new semantic cache for chat sessions.
fn new_semantic_cache() -> SemanticCache {
    SemanticCacheBuilder::new()
        .similarity_threshold(CACHE_SIMILARITY_THRESHOLD)
        .context_aware(true)
        .build()
}

/// Execute a single AI task and exit.
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

// ──────────────────────────────────────────────────────────────────────────────
// Slash command handler (shared between both REPL modes)
// ──────────────────────────────────────────────────────────────────────────────

/// Result of handling a slash command inside the REPL.
enum SlashOutcome {
    /// Continue the loop normally.
    Continue,
    /// Exit the REPL.
    Exit,
    /// Switch to a new model (returns new model name).
    SwitchModel(String),
    /// Run /compare — handled asynchronously by the caller.
    RunCompare {
        provider1: Option<String>,
        provider2: Option<String>,
        prompt: String,
    },
    /// Show cache stats or clear the cache — handled by the caller.
    HandleCache { subcommand: Option<String> },
}

/// Handle a parsed slash command, writing any output to stdout.
///
/// Returns a `SlashOutcome` so the REPL loop can react appropriately.
fn handle_slash(
    cmd: &SlashCommand,
    history: &[Message],
    tracker: &SessionUsageTracker,
    current_model: &str,
    registry: Option<&ToolRegistry>,
) -> SlashOutcome {
    match cmd {
        SlashCommand::Help => {
            let reg = SlashCommandRegistry::new();
            println!();
            print!("{}", reg.format_help());
            println!("{}", "Tips:".bright_yellow().bold());
            println!("  - Press Ctrl+C to cancel a request");
            println!("  - Type 'exit' or 'quit' to leave");
            println!("  - /model fast|smart|code|local  — switch to an alias group");
            println!();
            SlashOutcome::Continue
        }

        SlashCommand::Status => {
            let turn_count = history.iter().filter(|m| {
                matches!(m.role, ember_llm::Role::User)
            }).count();
            let (inp, out) = tracker.total_tokens();
            println!();
            println!("{}", "Session Status:".bright_yellow().bold());
            println!("  Turns:         {}", turn_count.to_string().bright_green());
            println!("  Input tokens:  {}", inp.to_string().bright_green());
            println!("  Output tokens: {}", out.to_string().bright_green());
            println!("  Model:         {}", current_model.bright_blue());
            println!();
            SlashOutcome::Continue
        }

        SlashCommand::Cost => {
            println!();
            println!("{}", "Cost Summary:".bright_yellow().bold());
            println!("  {}", tracker.format_summary().bright_green());
            let cost = tracker.total_cost();
            println!("  Input:  ${:.4}", cost.input_cost_usd);
            println!("  Output: ${:.4}", cost.output_cost_usd);
            if cost.cache_read_cost_usd > 0.0 || cost.cache_creation_cost_usd > 0.0 {
                println!("  Cache:  ${:.4}", cost.cache_read_cost_usd + cost.cache_creation_cost_usd);
            }
            println!("  Total:  ${:.4}", cost.total_cost_usd());
            println!();
            SlashOutcome::Continue
        }

        SlashCommand::Model { model } => {
            match model {
                None => {
                    println!("Current model: {}", current_model.bright_green());
                    println!("Aliases: fast, smart, code, local");
                    println!("Usage: /model <name or alias>");
                    SlashOutcome::Continue
                }
                Some(new_model) => {
                    println!(
                        "{} Switching model to {}",
                        "[ember]".bright_yellow(),
                        new_model.bright_green()
                    );
                    SlashOutcome::SwitchModel(new_model.clone())
                }
            }
        }

        SlashCommand::Memory => {
            let msg_count = history.len();
            // Rough token estimate: 4 chars per token
            let approx_tokens: usize = history.iter()
                .map(|m| (m.content.len() + 3) / 4)
                .sum();
            println!();
            println!("{}", "Context Window:".bright_yellow().bold());
            println!("  Messages: {}", msg_count.to_string().bright_green());
            println!("  ~Tokens:  {} (estimate)", approx_tokens.to_string().bright_green());
            println!();
            SlashOutcome::Continue
        }

        SlashCommand::Clear { confirm } => {
            if *confirm {
                println!("{}", "Conversation cleared.".bright_yellow());
                // Caller must handle resetting history; we signal via Continue
                // and the caller checks this specially. For now just signal.
                SlashOutcome::Continue
            } else {
                print!(
                    "{} Clear conversation history? ({}/{}) ",
                    "[confirm]".bright_yellow(),
                    "y".bright_green(),
                    "n".bright_red()
                );
                let _ = io::stdout().flush();
                let mut line = String::new();
                let _ = io::stdin().read_line(&mut line);
                if matches!(line.trim().to_lowercase().as_str(), "y" | "yes") {
                    println!("{}", "Conversation cleared.".bright_yellow());
                }
                SlashOutcome::Continue
            }
        }

        SlashCommand::Config { section } => {
            println!();
            match section {
                None => println!("{}", "Run 'ember config show' for full config.".dimmed()),
                Some(s) => println!("{}", format!("Config section '{}' — run 'ember config show'.", s).dimmed()),
            }
            println!();
            SlashOutcome::Continue
        }

        SlashCommand::Compact => {
            println!("{}", "Context compaction not available in CLI mode.".dimmed());
            SlashOutcome::Continue
        }

        SlashCommand::Permissions { .. } => {
            println!("{}", "Permission management not available in CLI mode.".dimmed());
            SlashOutcome::Continue
        }

        SlashCommand::Fork { name } => {
            let label = name.as_deref().unwrap_or("unnamed");
            println!("{} Fork '{}' — session forks require the TUI.", "[info]".bright_blue(), label);
            SlashOutcome::Continue
        }

        SlashCommand::Forks => {
            println!("{}", "Session forks require the TUI mode.".dimmed());
            SlashOutcome::Continue
        }

        SlashCommand::Restore { fork_id } => {
            println!("{} Restore '{}' — session forks require the TUI.", "[info]".bright_blue(), fork_id);
            SlashOutcome::Continue
        }

        SlashCommand::Compare { provider1, provider2, prompt } => {
            // Signal to caller: run the async compare handler.
            SlashOutcome::RunCompare {
                provider1: provider1.clone(),
                provider2: provider2.clone(),
                prompt: prompt.clone(),
            }
        }

        SlashCommand::Cache { subcommand } => {
            // Signal to caller: handle cache stats/clear.
            SlashOutcome::HandleCache {
                subcommand: subcommand.clone(),
            }
        }

        SlashCommand::Undo => {
            match filesystem_undo_last() {
                Ok(Some(path)) => {
                    println!(
                        "{} Restored {}",
                        "[undo]".bright_yellow(),
                        path.bright_cyan()
                    );
                }
                Ok(None) => {
                    println!("{}", "Nothing to undo.".dimmed());
                }
                Err(e) => {
                    println!(
                        "{} Undo failed: {}",
                        "[error]".bright_red(),
                        e
                    );
                }
            }
            SlashOutcome::Continue
        }

        SlashCommand::Commit { message } => {
            // Run `git add -A && git commit -m "..."`
            let msg = message.as_deref().unwrap_or("ember: auto-commit changes");
            match std::process::Command::new("git")
                .args(["add", "-A"])
                .output()
            {
                Ok(_) => {
                    match std::process::Command::new("git")
                        .args(["commit", "-m", msg])
                        .output()
                    {
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            if out.status.success() {
                                println!(
                                    "{} {}",
                                    "[commit]".bright_green(),
                                    stdout.trim()
                                );
                            } else {
                                let stderr = String::from_utf8_lossy(&out.stderr);
                                println!(
                                    "{} {}",
                                    "[info]".bright_blue(),
                                    if stderr.contains("nothing to commit") {
                                        "Nothing to commit."
                                    } else {
                                        stderr.trim().lines().next().unwrap_or("commit failed")
                                    }
                                );
                            }
                        }
                        Err(e) => {
                            println!("{} git commit failed: {}", "[error]".bright_red(), e);
                        }
                    }
                }
                Err(e) => {
                    println!("{} git add failed: {}", "[error]".bright_red(), e);
                }
            }
            SlashOutcome::Continue
        }

        SlashCommand::Diff { staged } => {
            let mut args = vec!["diff"];
            if *staged {
                args.push("--cached");
            }
            args.push("--stat");
            match std::process::Command::new("git").args(&args).output() {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    if stdout.trim().is_empty() {
                        println!("{}", "No changes.".dimmed());
                    } else {
                        println!();
                        println!("{}", "Git Diff:".bright_yellow().bold());
                        print!("{}", stdout);
                        println!();
                    }
                }
                Err(e) => {
                    println!("{} git diff failed: {}", "[error]".bright_red(), e);
                }
            }
            SlashOutcome::Continue
        }

        SlashCommand::Unknown(name) => {
            // Legacy aliases kept for muscle memory
            match name.as_str() {
                "tools" => {
                    if let Some(reg) = registry {
                        println!();
                        println!("{}", "Available Tools:".bright_yellow().bold());
                        for tool in reg.tool_definitions() {
                            println!("  {} - {}", tool.name.bright_cyan(), tool.description);
                        }
                        println!();
                    } else {
                        println!("{}", "No tools enabled.".dimmed());
                    }
                }
                "history" => {
                    print_history(history);
                }
                "exit" | "quit" | "q" => return SlashOutcome::Exit,
                _ => {
                    println!(
                        "{} Unknown command '{}'. Type /help for available commands.",
                        "[warn]".bright_yellow(),
                        name.bright_red()
                    );
                }
            }
            SlashOutcome::Continue
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// /compare — side-by-side provider comparison
// ──────────────────────────────────────────────────────────────────────────────

/// Run a side-by-side comparison of two providers on the same prompt.
///
/// When `p1` / `p2` are `None` the function falls back to the current config
/// provider and `"ollama"` respectively (simple heuristic; in a real build we
/// would query a cost-sorted list).
async fn compare_providers(
    config: &AppConfig,
    current_provider: &str,
    current_model: &str,
    temperature: f32,
    p1_name: Option<&str>,
    p2_name: Option<&str>,
    prompt: &str,
) -> Result<()> {
    if prompt.trim().is_empty() {
        eprintln!(
            "{} Usage: /compare [provider1] [provider2] <prompt>",
            "[error]".bright_red()
        );
        return Ok(());
    }

    // Resolve providers.
    let name1 = p1_name.unwrap_or(current_provider);
    // Default second provider: pick something different from the first.
    let name2 = p2_name.unwrap_or_else(|| {
        if name1 == "ollama" { "openai" } else { "ollama" }
    });

    // Resolve models: use configured defaults per provider.
    let model1 = if name1 == current_provider {
        current_model.to_owned()
    } else {
        match name1 {
            "ollama" => config.provider.ollama.model.clone(),
            _ => config.provider.openai.model.clone(),
        }
    };
    let model2 = match name2 {
        "ollama" => config.provider.ollama.model.clone(),
        _ => config.provider.openai.model.clone(),
    };

    let provider1 = match create_provider(config, name1) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{} Provider '{}' unavailable: {}", "[error]".bright_red(), name1, e);
            return Ok(());
        }
    };
    let provider2 = match create_provider(config, name2) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{} Provider '{}' unavailable: {}", "[error]".bright_red(), name2, e);
            return Ok(());
        }
    };

    println!();
    println!(
        "{} Comparing {} ({}) vs {} ({})",
        "[compare]".bright_magenta().bold(),
        name1.bright_blue(),
        model1.dimmed(),
        name2.bright_blue(),
        model2.dimmed(),
    );
    println!("  Prompt: {}", prompt.bright_white());
    println!();

    let req1 = CompletionRequest::new(&model1)
        .with_message(Message::user(prompt))
        .with_temperature(temperature);
    let req2 = CompletionRequest::new(&model2)
        .with_message(Message::user(prompt))
        .with_temperature(temperature);

    // Send both requests concurrently.
    let start = Instant::now();
    let (res1, res2) = tokio::join!(provider1.complete(req1), provider2.complete(req2));
    let elapsed = start.elapsed();

    let content1 = match res1 {
        Ok(r) => r.content,
        Err(e) => format!("[error: {}]", e),
    };
    let content2 = match res2 {
        Ok(r) => r.content,
        Err(e) => format!("[error: {}]", e),
    };

    // Display side-by-side (sequential, labelled).
    let divider = "─".repeat(60);
    println!("{}", divider.dimmed());
    println!(
        "{} {} ({})",
        "[1]".bright_cyan().bold(),
        name1.bright_blue(),
        model1.dimmed()
    );
    println!("{}", divider.dimmed());
    println!("{}", content1);
    println!();
    println!("{}", divider.dimmed());
    println!(
        "{} {} ({})",
        "[2]".bright_cyan().bold(),
        name2.bright_blue(),
        model2.dimmed()
    );
    println!("{}", divider.dimmed());
    println!("{}", content2);
    println!();
    println!(
        "  {} Total round-trip: {:.1}s",
        "⏱".dimmed(),
        elapsed.as_secs_f64()
    );
    println!();

    // Prompt the user to pick a response.
    print!(
        "{} Keep which response? ({}/{}/{}) ",
        "[compare]".bright_magenta(),
        "1".bright_cyan(),
        "2".bright_cyan(),
        "n".dimmed()
    );
    io::stdout().flush()?;

    let choice = read_line()?;
    match choice.trim() {
        "1" => {
            println!("{} Keeping response from {}.", "[compare]".bright_magenta(), name1.bright_blue());
        }
        "2" => {
            println!("{} Keeping response from {}.", "[compare]".bright_magenta(), name2.bright_blue());
        }
        _ => {
            println!("{} Neither response kept.", "[compare]".bright_magenta());
        }
    }
    println!();

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// /cache — show stats or clear
// ──────────────────────────────────────────────────────────────────────────────

/// Display cache stats or clear the cache.
fn handle_cache_command(cache: &SemanticCache, subcommand: Option<&str>) {
    match subcommand {
        Some("clear") => {
            cache.clear();
            println!("{} Semantic cache cleared.", "[cache]".bright_cyan().bold());
        }
        _ => {
            let stats = cache.stats();
            let total = stats.hits + stats.misses;
            println!();
            println!("{}", "Semantic Cache:".bright_yellow().bold());
            println!("  Entries:    {}", cache.len().to_string().bright_green());
            println!("  Hits:       {}", stats.hits.to_string().bright_green());
            println!("  Misses:     {}", stats.misses.to_string().bright_yellow());
            println!(
                "  Hit rate:   {:.1}%",
                if total > 0 { stats.hit_rate * 100.0 } else { 0.0 }
            );
            println!(
                "  Avg sim:    {:.3}",
                stats.average_similarity
            );
            println!(
                "  Tokens saved: {}",
                stats.tokens_saved.to_string().bright_green()
            );
            println!(
                "  Est. savings: ${:.4}",
                stats.estimated_savings_usd
            );
            println!();
            println!("  Use {} to clear the cache.", "/cache clear".bright_cyan());
            println!();
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Agent one-shot
// ──────────────────────────────────────────────────────────────────────────────

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

    let tools = registry.llm_tool_definitions();
    let mut history: Vec<Message> = vec![Message::system(system_prompt), Message::user(message)];

    for iteration in 0..MAX_TOOL_ITERATIONS {
        debug!("Tool iteration {}", iteration + 1);

        let mut request = CompletionRequest::new(model).with_temperature(temperature);
        for msg in &history {
            request = request.with_message(msg.clone());
        }
        request = request.with_tools(tools.clone());

        let response = provider
            .complete(request)
            .await
            .context("Failed to get response from LLM")?;

        if !response.tool_calls.is_empty() {
            let mut assistant_msg = Message::assistant(&response.content);
            assistant_msg.tool_calls = response.tool_calls.clone();
            history.push(assistant_msg);

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
                        history.push(Message::tool_result(&call.id, &tool_result.output));
                    }
                    Err(e) => {
                        let error_msg = format!("Tool execution failed: {}", e);
                        println!("{} {}", "[error]".bright_red(), &error_msg);
                        history.push(Message::tool_result(&call.id, &error_msg));
                    }
                }
            }
            continue;
        }

        match format {
            ChatFormat::Text => {
                println!();
                if streaming {
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

    eprintln!(
        "{} Reached maximum tool iterations ({}). Stopping.",
        "[warn]".bright_yellow(),
        MAX_TOOL_ITERATIONS
    );
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// Agent interactive (with tools + slash commands + session persistence)
// ──────────────────────────────────────────────────────────────────────────────

async fn agent_interactive(
    config: &AppConfig,
    provider: Arc<dyn LLMProvider>,
    model: &str,
    system_prompt: &str,
    temperature: f32,
    streaming: bool,
    registry: ToolRegistry,
    resume: Option<PersistedSession>,
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

    let tools = registry.llm_tool_definitions();

    // Restore history from persisted session or start fresh
    let (session_id, mut history) = if let Some(ref s) = resume {
        let msgs: Vec<Message> = s.messages.iter().map(|m| m.to_message()).collect();
        (s.id.clone(), msgs)
    } else {
        (new_session_id(), vec![Message::system(system_prompt)])
    };

    // Active model (can be changed mid-session via /model)
    let mut active_model = model.to_owned();

    // Usage tracker (approximation: we record by token count from response)
    let mut tracker = SessionUsageTracker::new(&active_model);

    // Semantic cache for agent REPL.
    let sem_cache = new_semantic_cache();

    loop {
        print!("{} ", "You:".bright_green().bold());
        io::stdout().flush()?;

        let input = read_line()?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        // Handle slash commands
        if input.starts_with('/') {
            if let Some(cmd) = SlashCommand::parse(input) {
                match handle_slash(&cmd, &history, &tracker, &active_model, Some(&registry)) {
                    SlashOutcome::Exit => {
                        println!("{}", "Goodbye!".bright_yellow());
                        break;
                    }
                    SlashOutcome::SwitchModel(new_model) => {
                        active_model = new_model;
                        tracker = SessionUsageTracker::new(&active_model);
                        continue;
                    }
                    SlashOutcome::RunCompare { provider1, provider2, prompt } => {
                        compare_providers(
                            config,
                            provider.name(),
                            &active_model,
                            temperature,
                            provider1.as_deref(),
                            provider2.as_deref(),
                            &prompt,
                        )
                        .await?;
                        continue;
                    }
                    SlashOutcome::HandleCache { subcommand } => {
                        handle_cache_command(&sem_cache, subcommand.as_deref());
                        continue;
                    }
                    SlashOutcome::Continue => {
                        // Handle /clear specially: reset history
                        if matches!(cmd, SlashCommand::Clear { .. }) {
                            history = vec![Message::system(system_prompt)];
                        }
                        continue;
                    }
                }
            }
            continue;
        }

        // Bare "exit" / "quit"
        if matches!(input, "exit" | "quit") {
            println!("{}", "Goodbye!".bright_yellow());
            break;
        }

        history.push(Message::user(input));

        for iteration in 0..MAX_TOOL_ITERATIONS {
            debug!("Interactive tool iteration {}", iteration + 1);

            let mut request = CompletionRequest::new(&active_model).with_temperature(temperature);
            for msg in &history {
                request = request.with_message(msg.clone());
            }
            request = request.with_tools(tools.clone());

            if iteration == 0 {
                print!("{} ", "Ember:".bright_blue().bold());
                io::stdout().flush()?;
            }

            let response = match provider.complete(request).await {
                Ok(r) => r,
                Err(e) => {
                    println!("{}", format!("Error: {}", e).bright_red());
                    break;
                }
            };

            // Record token usage
            tracker.record_turn(response.usage.clone());

            if !response.tool_calls.is_empty() {
                if iteration == 0 {
                    println!();
                }

                let mut assistant_msg = Message::assistant(&response.content);
                assistant_msg.tool_calls = response.tool_calls.clone();
                history.push(assistant_msg);

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
                continue;
            }

            if iteration == 0 {
                println!("{}", response.content);
            } else {
                print!("{} ", "Ember:".bright_blue().bold());
                println!("{}", response.content);
            }

            history.push(Message::assistant(&response.content));
            break;
        }

        println!();

        // Persist session after each turn
        let turn_count = history.iter().filter(|m| matches!(m.role, ember_llm::Role::User)).count();
        let session = PersistedSession {
            id: session_id.clone(),
            provider: provider.name().to_owned(),
            model: active_model.clone(),
            created_at: now_iso8601(),
            updated_at: now_iso8601(),
            messages: history.iter().map(PersistedMessage::from_message).collect(),
            turn_count,
        };
        if let Err(e) = save_session(&session) {
            warn!("Could not save session: {}", e);
        }
    }

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// One-shot chat
// ──────────────────────────────────────────────────────────────────────────────

async fn one_shot_chat(
    provider: Arc<dyn LLMProvider>,
    model: &str,
    system_prompt: &str,
    temperature: f32,
    message: &str,
    streaming: bool,
    format: ChatFormat,
) -> Result<()> {
    let request = CompletionRequest::new(model)
        .with_message(Message::system(system_prompt))
        .with_message(Message::user(message))
        .with_temperature(temperature);

    if streaming && format == ChatFormat::Text {
        println!(
            "{} Using {} with {}",
            "[ember]".bright_yellow(),
            provider.name().bright_blue(),
            model.bright_green()
        );
        println!();

        let mut progress = ProgressIndicator::new("Thinking");
        progress.start();

        let stream_result = provider.complete_stream(request).await;
        progress.stop().await;

        let mut stream = stream_result.context("Failed to start streaming response")?;

        let start_time = Instant::now();
        let mut token_count = 0usize;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    if let Some(content) = chunk.content {
                        print!("{}", content);
                        io::stdout().flush()?;
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

        let stats = ResponseStats {
            tokens: token_count,
            duration: start_time.elapsed(),
        };
        println!();
        println!("{}", stats.format().dimmed());
    } else {
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

// ──────────────────────────────────────────────────────────────────────────────
// Interactive chat (no tools, with slash commands + session persistence)
// ──────────────────────────────────────────────────────────────────────────────

async fn interactive_chat(
    config: &AppConfig,
    provider: Arc<dyn LLMProvider>,
    model: &str,
    system_prompt: &str,
    temperature: f32,
    streaming: bool,
    resume: Option<PersistedSession>,
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

    let (session_id, mut history) = if let Some(ref s) = resume {
        let msgs: Vec<Message> = s.messages.iter().map(|m| m.to_message()).collect();
        (s.id.clone(), msgs)
    } else {
        (new_session_id(), vec![Message::system(system_prompt)])
    };

    let mut active_model = model.to_owned();
    let mut tracker = SessionUsageTracker::new(&active_model);

    // Semantic cache shared across turns.
    let sem_cache = new_semantic_cache();

    loop {
        print!("{} ", "You:".bright_green().bold());
        io::stdout().flush()?;

        let input = read_line()?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        // Slash commands
        if input.starts_with('/') {
            if let Some(cmd) = SlashCommand::parse(input) {
                match handle_slash(&cmd, &history, &tracker, &active_model, None) {
                    SlashOutcome::Exit => {
                        println!("{}", "Goodbye!".bright_yellow());
                        break;
                    }
                    SlashOutcome::SwitchModel(new_model) => {
                        active_model = new_model;
                        tracker = SessionUsageTracker::new(&active_model);
                        continue;
                    }
                    SlashOutcome::RunCompare { provider1, provider2, prompt } => {
                        compare_providers(
                            config,
                            provider.name(),
                            &active_model,
                            temperature,
                            provider1.as_deref(),
                            provider2.as_deref(),
                            &prompt,
                        )
                        .await?;
                        continue;
                    }
                    SlashOutcome::HandleCache { subcommand } => {
                        handle_cache_command(&sem_cache, subcommand.as_deref());
                        continue;
                    }
                    SlashOutcome::Continue => {
                        if matches!(cmd, SlashCommand::Clear { .. }) {
                            history = vec![Message::system(system_prompt)];
                        }
                        continue;
                    }
                }
            }
            continue;
        }

        if matches!(input, "exit" | "quit") {
            println!("{}", "Goodbye!".bright_yellow());
            break;
        }

        // ── Semantic cache lookup ──────────────────────────────────────────
        let cache_ctx = CacheContext {
            system_prompt: Some(system_prompt.to_owned()),
            model: Some(active_model.clone()),
            temperature: Some(temperature),
            ..Default::default()
        };

        if let Some(hit) = sem_cache.get(input, &cache_ctx) {
            print!("{} ", "Ember:".bright_blue().bold());
            println!(
                "{} {}",
                hit.response,
                format!("(cached {:.0}%)", hit.similarity * 100.0).dimmed()
            );
            history.push(Message::user(input));
            history.push(Message::assistant(&hit.response));
            println!();

            // Persist session
            let turn_count = history.iter().filter(|m| matches!(m.role, ember_llm::Role::User)).count();
            let session = PersistedSession {
                id: session_id.clone(),
                provider: provider.name().to_owned(),
                model: active_model.clone(),
                created_at: now_iso8601(),
                updated_at: now_iso8601(),
                messages: history.iter().map(PersistedMessage::from_message).collect(),
                turn_count,
            };
            if let Err(e) = save_session(&session) {
                warn!("Could not save session: {}", e);
            }
            continue;
        }

        history.push(Message::user(input));

        let mut request = CompletionRequest::new(&active_model).with_temperature(temperature);
        for msg in &history {
            request = request.with_message(msg.clone());
        }

        print!("{} ", "Ember:".bright_blue().bold());
        io::stdout().flush()?;

        if streaming {
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

                    if !full_response.is_empty() {
                        // Store in semantic cache.
                        if let Err(e) = sem_cache.put(input, &full_response, &cache_ctx, &active_model) {
                            debug!("Cache put failed: {}", e);
                        }
                        // Approximate usage for streaming responses
                        let approx_tokens = (full_response.len() + 3) / 4;
                        let approx_input = history.iter().map(|m| (m.content.len() + 3) / 4).sum::<usize>();
                        tracker.record_turn(ember_llm::TokenUsage::new(
                            approx_input as u32,
                            approx_tokens as u32,
                        ));
                        history.push(Message::assistant(&full_response));
                    }
                }
                Err(e) => {
                    println!("{}", format!("Error: {}", e).bright_red());
                }
            }
        } else {
            match provider.complete(request).await {
                Ok(response) => {
                    println!("{}", response.content);
                    // Store in semantic cache.
                    if let Err(e) = sem_cache.put(input, &response.content, &cache_ctx, &active_model) {
                        debug!("Cache put failed: {}", e);
                    }
                    tracker.record_turn(response.usage.clone());
                    history.push(Message::assistant(&response.content));
                }
                Err(e) => {
                    println!("{}", format!("Error: {}", e).bright_red());
                }
            }
        }

        println!();

        // Persist session
        let turn_count = history.iter().filter(|m| matches!(m.role, ember_llm::Role::User)).count();
        let session = PersistedSession {
            id: session_id.clone(),
            provider: provider.name().to_owned(),
            model: active_model.clone(),
            created_at: now_iso8601(),
            updated_at: now_iso8601(),
            messages: history.iter().map(PersistedMessage::from_message).collect(),
            turn_count,
        };
        if let Err(e) = save_session(&session) {
            warn!("Could not save session: {}", e);
        }
    }

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// Small helpers
// ──────────────────────────────────────────────────────────────────────────────

fn read_line() -> Result<String> {
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input)
}

fn print_history(history: &[Message]) {
    if history.len() <= 1 {
        println!("{}", "No conversation history.".bright_yellow());
        return;
    }

    println!();
    println!("{}", "Conversation History:".bright_yellow().bold());

    let mut turn = 0;
    for msg in history.iter().skip(1) {
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

fn print_final_response(content: &str) {
    println!("{}", content);
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

fn truncate_json(value: &serde_json::Value, max_len: usize) -> String {
    let s = value.to_string();
    truncate_str(&s, max_len)
}

// ──────────────────────────────────────────────────────────────────────────────
// Project context (EMBER.md)
// ──────────────────────────────────────────────────────────────────────────────

/// Walk up from the current working directory looking for an `EMBER.md` file,
/// mirroring how tools like Claude Code discover `CLAUDE.md` and how Git
/// discovers `.git/`.
///
/// Returns the file contents when found, or `None` when no `EMBER.md` exists
/// anywhere in the directory hierarchy.
fn load_project_context() -> Option<String> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join("EMBER.md");
        if candidate.exists() {
            return std::fs::read_to_string(&candidate).ok();
        }
        if !dir.pop() {
            break;
        }
    }
    None
}
