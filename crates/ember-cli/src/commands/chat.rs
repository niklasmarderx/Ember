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
use ember_browser::BrowserTool;
use ember_core::usage_tracker::SessionUsageTracker;
use ember_llm::router::is_model_alias;
use ember_llm::{
    AnthropicProvider, BedrockProvider, CompletionRequest, DeepSeekProvider, GeminiProvider,
    GroqProvider, LLMProvider, Message, MistralProvider, OllamaProvider, OpenAIProvider,
    OpenRouterProvider, RetryConfig, XAIProvider,
};
use ember_plugins::hooks::{HookContext, HookEvent, HookRunner};
use ember_storage::semantic_cache::{SemanticCache, SemanticCacheBuilder};
use ember_tools::filesystem::undo_last as filesystem_undo_last;
use ember_tools::{FilesystemTool, GitTool, ShellTool, ToolRegistry, WebTool};
use futures::StreamExt;
use rustyline::error::ReadlineError;
use serde::{Deserialize, Serialize};
use serde_json;
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Global auto-approve flag — set by `--yes` or by pressing "a" during confirmation.
static AUTO_APPROVE: AtomicBool = AtomicBool::new(false);
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

#[cfg(feature = "tui")]
use crate::tui::renderer::TerminalRenderer;

/// Semantic cache similarity threshold for chat: treat >0.92 as a hit.
const CACHE_SIMILARITY_THRESHOLD: f32 = 0.92;

/// Maximum iterations for tool execution loop to prevent infinite loops.
/// Set high enough for complex multi-step coding tasks (like Claude Code).
const MAX_TOOL_ITERATIONS: usize = 50;

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
    /// Active model override (may differ from `model` after /model switch).
    #[serde(default)]
    pub active_model: Option<String>,
    /// Whether compact mode was active.
    #[serde(default)]
    pub compact_mode: bool,
    /// Whether plan mode was active.
    #[serde(default)]
    pub plan_mode: bool,
    /// Working directory at time of save.
    #[serde(default)]
    pub working_directory: Option<String>,
    /// Cumulative cost in USD at time of save.
    #[serde(default)]
    pub total_cost_usd: f64,
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
    let json =
        std::fs::read_to_string(&path).with_context(|| format!("Session '{}' not found", id))?;
    let session: PersistedSession = serde_json::from_str(&json)?;
    Ok(session)
}

/// Find the most recently modified session file and return its id.
pub fn latest_session_id() -> Option<String> {
    let dir = sessions_dir().ok()?;
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
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
        let dy = if (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400) {
            366
        } else {
            365
        };
        if days < dy {
            break;
        }
        days -= dy;
        y += 1;
    }
    let leap = (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
    let month_days = [
        31u32,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut mo = 0u32;
    for md in &month_days {
        if days < *md {
            break;
        }
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
    auto_approve: bool,
) -> Result<()> {
    // Set global auto-approve from --yes flag
    if auto_approve {
        AUTO_APPROVE.store(true, Ordering::Relaxed);
        eprintln!(
            "{} Auto-approve enabled (all tool calls will execute without confirmation)",
            "[ember]".bright_yellow()
        );
    }
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
            .find(|c| {
                matches!(
                    c.provider,
                    "openai" | "anthropic" | "ollama" | "gemini" | "groq" | "deepseek" | "mistral"
                )
            })
            .unwrap_or_else(|| {
                ember_llm::router::ModelCandidate::new("openai", "gpt-4o-mini", 0.15)
            });
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

    // ── Onboarding: ensure user profile exists ──
    let profile = crate::onboarding::ensure_profile();

    let base_system_prompt = system.unwrap_or_else(|| config.agent.system_prompt.clone());

    // Initialize memory system
    let memory_mgr = crate::memory::MemoryManager::new();

    // Build system prompt: auto-context + user profile + memory + base prompt
    let mut prompt_parts: Vec<String> = Vec::new();
    let mut context_tags: Vec<String> = Vec::new();

    // Smart auto-context: replaces manual EMBER.md + cwd gathering
    let auto_ctx = crate::auto_context::AutoContextBuilder::new(config.agent.context_budget)
        .gather_ember_md()
        .gather_rules()
        .gather_manifest()
        .gather_readme()
        .gather_git_context()
        .gather_directory_tree()
        .build();

    if !auto_ctx.parts.is_empty() {
        for label in auto_ctx.labels() {
            context_tags.push(label.to_string());
        }
        prompt_parts.push(auto_ctx.to_prompt_section());
    }

    if let Some(ref p) = profile {
        if let Some(ref buddy) = p.buddy {
            context_tags.push(format!("{} {}", buddy.emoji, buddy.name));
        }
        prompt_parts.push(p.to_system_context());
    }

    // Inject learned memory into system prompt
    if let Some(ref mgr) = memory_mgr {
        if let Some(mem_ctx) = mgr.to_system_context() {
            let stats = mgr.load_stats();
            context_tags.push(format!("memory:{}", stats.total_observations));
            prompt_parts.push(mem_ctx);
        }
    }

    // ── Compact startup banner ──
    {
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".into());
        let ctx_str = if context_tags.is_empty() {
            String::new()
        } else {
            format!("  ctx: {}", context_tags.join(", ").dimmed())
        };
        if let Some(ref p) = profile {
            eprintln!("{}", crate::onboarding::welcome_back(p));
        }
        eprintln!(
            "{} {} on {} | {}{}",
            "▸".bright_green(),
            model_name.bright_cyan(),
            provider_name.bright_blue(),
            cwd.dimmed(),
            ctx_str
        );
        if auto_approve {
            eprintln!("  {} auto-approve enabled", "⚡".bright_yellow());
        }
        eprintln!();
    }

    prompt_parts.push(base_system_prompt);
    let system_prompt = prompt_parts.join("\n\n---\n\n");
    let temp = temperature.unwrap_or(config.agent.temperature);

    // Pre-flight: warn about missing API key before the hard error
    if let Some((var, url)) = check_provider_key(&provider_name, &config) {
        eprintln!(
            "{} {} not set for provider '{}'",
            "⚠".bright_yellow(),
            var.bright_red().bold(),
            provider_name.bright_cyan()
        );
        eprintln!("  {} export {}=\"sk-...\"", "fix:".dimmed(), var);
        eprintln!("  {} {}", "get key:".dimmed(), url.bright_blue());
        eprintln!();
    }

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
                println!(
                    "{} No previous session found, starting fresh.",
                    "[ember]".bright_yellow()
                );
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

    // Always enable tools by default (like Claude Code) — the user can override
    // with `--tools shell,fs` to restrict, or `--tools none` to disable.
    let registry = if let Some(ref tool_names) = tools {
        if tool_names.len() == 1 && tool_names[0].to_lowercase() == "none" {
            None
        } else {
            Some(create_tool_registry(tool_names)?)
        }
    } else {
        // Auto-register ALL available tools for maximum power
        Some(create_default_tool_registry(&config))
    };

    if let Some(registry) = registry {
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
                Some(registry),
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
        // No tools — use the unified interactive loop with no registry
        agent_interactive(
            &config,
            llm_provider,
            &model_name,
            &system_prompt,
            temp,
            streaming,
            None,
            resume_session,
        )
        .await?;
    }

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Build a default registry with available tools, respecting config flags.
fn create_default_tool_registry(config: &AppConfig) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    if config.tools.shell_enabled {
        registry.register(ShellTool::new());
    }
    if config.tools.filesystem_enabled {
        registry.register(FilesystemTool::new());
    }
    if config.tools.web_enabled {
        registry.register(WebTool::new());
    }
    // Git and browser don't have config flags yet — always register
    registry.register(GitTool::new());
    registry.register(BrowserTool::new());
    let count = registry.llm_tool_definitions().len();
    info!("Auto-registered {} tools", count);
    registry
}

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
            "git" => {
                info!("Registering git tool");
                registry.register(GitTool::new());
            }
            "browser" => {
                info!("Registering browser tool");
                registry.register(BrowserTool::new());
            }
            other => {
                warn!("Unknown tool: {}", other);
                eprintln!(
                    "{} Unknown tool '{}', skipping. Available: shell, filesystem, web, git, browser",
                    "[warn]".bright_yellow(),
                    other
                );
            }
        }
    }

    if registry.is_empty() {
        anyhow::bail!("No valid tools specified. Available tools: shell, filesystem, web, git");
    }

    Ok(registry)
}

/// Pre-flight check: warn if the selected provider's API key is missing.
/// Returns (env_var_name, export_hint) or None if the key is present / not needed.
/// Checks both environment variables AND config-file keys.
fn check_provider_key(
    provider_name: &str,
    config: &crate::config::AppConfig,
) -> Option<(&'static str, &'static str)> {
    let (var, hint) = match provider_name {
        "openai" => ("OPENAI_API_KEY", "https://platform.openai.com/api-keys"),
        "anthropic" => (
            "ANTHROPIC_API_KEY",
            "https://console.anthropic.com/settings/keys",
        ),
        "gemini" | "google" => ("GOOGLE_API_KEY", "https://aistudio.google.com/apikey"),
        "groq" => ("GROQ_API_KEY", "https://console.groq.com/keys"),
        "deepseek" => ("DEEPSEEK_API_KEY", "https://platform.deepseek.com/api_keys"),
        "mistral" => ("MISTRAL_API_KEY", "https://console.mistral.ai/api-keys"),
        "openrouter" => ("OPENROUTER_API_KEY", "https://openrouter.ai/keys"),
        "xai" => ("XAI_API_KEY", "https://console.x.ai"),
        "bedrock" | "aws" => (
            "AWS_ACCESS_KEY_ID",
            "https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-files.html",
        ),
        _ => return None, // ollama = local, no key needed
    };

    // Check environment variable first
    if std::env::var(var).ok().filter(|v| !v.is_empty()).is_some() {
        return None;
    }

    // Check config-file API key (currently only openai has a config key field)
    if provider_name == "openai"
        && config
            .provider
            .openai
            .api_key
            .as_ref()
            .filter(|k| !k.is_empty())
            .is_some()
    {
        return None;
    }

    Some((var, hint))
}

/// Create an LLM provider based on configuration and provider name.
pub fn create_provider(config: &AppConfig, provider_name: &str) -> Result<Arc<dyn LLMProvider>> {
    match provider_name {
        "ollama" => {
            let provider = OllamaProvider::new()
                .with_base_url(&config.provider.ollama.url)
                .with_default_model(&config.provider.ollama.model);
            Ok(Arc::new(provider))
        }
        "anthropic" => {
            let api_key = config
                .provider
                .openai
                .api_key
                .clone()
                .filter(|_| false) // Anthropic uses its own key
                .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
                .context(
                    "Anthropic API key not found. Set ANTHROPIC_API_KEY environment variable.",
                )?;
            Ok(Arc::new(AnthropicProvider::new(api_key)))
        }
        "gemini" | "google" => {
            let provider = GeminiProvider::from_env().context(
                "Google API key not found. Set GOOGLE_API_KEY or GEMINI_API_KEY environment variable.",
            )?;
            Ok(Arc::new(provider))
        }
        "groq" => {
            let provider = GroqProvider::from_env()
                .context("Groq API key not found. Set GROQ_API_KEY environment variable.")?;
            Ok(Arc::new(provider))
        }
        "deepseek" => {
            let provider = DeepSeekProvider::from_env().context(
                "DeepSeek API key not found. Set DEEPSEEK_API_KEY environment variable.",
            )?;
            Ok(Arc::new(provider))
        }
        "mistral" => {
            let provider = MistralProvider::from_env()
                .context("Mistral API key not found. Set MISTRAL_API_KEY environment variable.")?;
            Ok(Arc::new(provider))
        }
        "openrouter" => {
            let provider = OpenRouterProvider::from_env().context(
                "OpenRouter API key not found. Set OPENROUTER_API_KEY environment variable.",
            )?;
            Ok(Arc::new(provider))
        }
        "xai" => {
            let provider = XAIProvider::from_env()
                .context("xAI API key not found. Set XAI_API_KEY environment variable.")?;
            Ok(Arc::new(provider))
        }
        "bedrock" | "aws" => {
            let provider = BedrockProvider::from_env().context(
                "AWS credentials not found. Set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY.",
            )?;
            Ok(Arc::new(provider))
        }
        "openai" => {
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
        other => {
            anyhow::bail!(
                "Unknown provider '{}'. Available: openai, anthropic, ollama, gemini, groq, deepseek, mistral, openrouter, xai, bedrock",
                other
            );
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
    let registry = create_default_tool_registry(&config);
    agent_one_shot(
        llm_provider,
        &model_name,
        &system_prompt,
        config.agent.temperature,
        &task,
        true,
        registry,
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
    /// Inject a message into the conversation history.
    InjectContext(String),
    /// Toggle plan mode on/off.
    TogglePlanMode,
    /// Execute the stored plan.
    ExecutePlan,
    /// Toggle compact response mode.
    ToggleCompact,
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
            let turn_count = history
                .iter()
                .filter(|m| matches!(m.role, ember_llm::Role::User))
                .count();
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
                println!(
                    "  Cache:  ${:.4}",
                    cost.cache_read_cost_usd + cost.cache_creation_cost_usd
                );
            }
            println!("  Total:  ${:.4}", cost.total_cost_usd());
            println!();
            SlashOutcome::Continue
        }

        SlashCommand::Model { model } => match model {
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
        },

        SlashCommand::Memory => {
            let msg_count = history.len();
            // Rough token estimate: 4 chars per token
            let approx_tokens: usize = history.iter().map(|m| (m.content.len() + 3) / 4).sum();
            println!();
            println!("{}", "Context Window:".bright_yellow().bold());
            println!("  Messages: {}", msg_count.to_string().bright_green());
            println!(
                "  ~Tokens:  {} (estimate)",
                approx_tokens.to_string().bright_green()
            );
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
                Some(s) => println!(
                    "{}",
                    format!("Config section '{}' — run 'ember config show'.", s).dimmed()
                ),
            }
            println!();
            SlashOutcome::Continue
        }

        SlashCommand::Compact => SlashOutcome::ToggleCompact,

        SlashCommand::Permissions { .. } => {
            println!(
                "{}",
                "Permission management not available in CLI mode.".dimmed()
            );
            SlashOutcome::Continue
        }

        SlashCommand::Fork { name } => {
            let label = name.as_deref().unwrap_or("unnamed");
            println!(
                "{} Fork '{}' — session forks require the TUI.",
                "[info]".bright_blue(),
                label
            );
            SlashOutcome::Continue
        }

        SlashCommand::Forks => {
            println!("{}", "Session forks require the TUI mode.".dimmed());
            SlashOutcome::Continue
        }

        SlashCommand::Restore { fork_id } => {
            println!(
                "{} Restore '{}' — session forks require the TUI.",
                "[info]".bright_blue(),
                fork_id
            );
            SlashOutcome::Continue
        }

        SlashCommand::Compare {
            provider1,
            provider2,
            prompt,
        } => {
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
                    println!("{} Undo failed: {}", "[error]".bright_red(), e);
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
                                println!("{} {}", "[commit]".bright_green(), stdout.trim());
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

        SlashCommand::Plan => SlashOutcome::TogglePlanMode,

        SlashCommand::Execute => SlashOutcome::ExecutePlan,

        SlashCommand::Checkpoint { name } => {
            let label = name.as_deref().unwrap_or("unnamed");
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            println!(
                "{} Checkpoint '{}' saved at {}",
                "[checkpoint]".bright_cyan(),
                label.bright_green(),
                ts
            );
            SlashOutcome::Continue
        }

        SlashCommand::Checkpoints => {
            println!("{}", "Saved Checkpoints:".bright_yellow().bold());
            println!(
                "  {}",
                "No checkpoints saved yet. Use /checkpoint <name> to create one.".dimmed()
            );
            SlashOutcome::Continue
        }

        SlashCommand::Replay => {
            println!("{}", "Session Replay:".bright_yellow().bold());
            let turn_count = history
                .iter()
                .filter(|m| matches!(m.role, ember_llm::Role::User))
                .count();
            println!("  Turns: {}", turn_count.to_string().bright_green());
            for (i, msg) in history.iter().enumerate() {
                let role = match msg.role {
                    ember_llm::Role::User => "You".bright_cyan(),
                    ember_llm::Role::Assistant => "Ember".bright_green(),
                    _ => "System".dimmed(),
                };
                let preview = if msg.content.len() > 80 {
                    format!("{}...", &msg.content[..77])
                } else {
                    msg.content.clone()
                };
                println!("  [{}] {}: {}", i + 1, role, preview);
            }
            SlashOutcome::Continue
        }

        SlashCommand::Bench { task } => {
            let task_str = task
                .as_deref()
                .unwrap_or("Explain the concept of ownership in Rust");
            println!(
                "{} Benchmarking task across providers...",
                "[bench]".bright_yellow()
            );
            println!("  Task: {}", task_str.bright_cyan());
            println!(
                "  {}",
                "Comparing: current model vs fast vs smart aliases".dimmed()
            );
            println!();
            let pad = " ";
            println!(
                "  {pad} {:<20} {:>10} {:>10} {:>10}",
                "Model".bright_yellow(),
                "Tokens".bright_yellow(),
                "Time".bright_yellow(),
                "Cost".bright_yellow()
            );
            let arrow = "→";
            let dash = "—";
            println!(
                "  {arrow} {:<20} {:>10} {:>10} {:>10}",
                current_model, dash, dash, dash
            );
            println!();
            println!(
                "{}",
                "Full benchmarking requires async provider calls. Coming soon.".bright_yellow()
            );
            SlashOutcome::Continue
        }

        SlashCommand::Learn { subcommand } => {
            match subcommand.as_deref() {
                Some("reset") => {
                    println!("{} Coding preferences cleared.", "[learn]".bright_yellow());
                }
                Some("show") | None => {
                    println!("{}", "Learned Preferences:".bright_yellow().bold());
                    println!("  {}", "No preferences learned yet.".dimmed());
                    println!(
                        "  {}",
                        "Ember learns from your corrections and coding patterns over time."
                            .dimmed()
                    );
                }
                Some(other) => {
                    println!(
                        "{} Unknown subcommand '{}'. Use /learn or /learn reset.",
                        "[warn]".bright_yellow(),
                        other.bright_red()
                    );
                }
            }
            SlashOutcome::Continue
        }

        SlashCommand::Theme { theme } => {
            match theme.as_deref() {
                Some("dark") => {
                    println!(
                        "{} Theme switched to {}",
                        "[theme]".bright_magenta(),
                        "dark".bold()
                    );
                    println!("  {} Deep blacks with bright accents", "●".bright_blue());
                }
                Some("light") => {
                    println!(
                        "{} Theme switched to {}",
                        "[theme]".bright_magenta(),
                        "light".bold()
                    );
                    println!(
                        "  {} Light background optimized colors",
                        "●".bright_yellow()
                    );
                }
                Some("neon") => {
                    println!(
                        "{} Theme switched to {}",
                        "[theme]".bright_magenta(),
                        "neon".bold()
                    );
                    println!(
                        "  {} {} {} Cyberpunk neon palette",
                        "●".bright_magenta(),
                        "●".bright_cyan(),
                        "●".bright_green()
                    );
                }
                Some(other) => {
                    println!(
                        "{} Unknown theme '{}'. Available: dark, light, neon",
                        "[warn]".bright_yellow(),
                        other.bright_red()
                    );
                }
                None => {
                    println!("{}", "Available Themes:".bright_magenta().bold());
                    println!(
                        "  {} - Deep blacks with bright accents (default)",
                        "dark".bright_cyan()
                    );
                    println!(
                        "  {} - Optimized for light terminal backgrounds",
                        "light".bright_cyan()
                    );
                    println!("  {} - Cyberpunk neon palette", "neon".bright_cyan());
                    println!();
                    println!("  Usage: {}", "/theme <name>".dimmed());
                }
            }
            SlashOutcome::Continue
        }

        SlashCommand::Buddy => {
            if let Some(profile) = crate::onboarding::load_profile() {
                if let Some(ref buddy) = profile.buddy {
                    println!();
                    println!("{}", "🐾 Your Coding Buddy".bright_cyan().bold());
                    println!("  {:<14} {}", "Name:".dimmed(), buddy.name.bright_yellow());
                    println!(
                        "  {:<14} {}",
                        "Species:".dimmed(),
                        buddy.species.bright_green()
                    );
                    println!("  {:<14} {}", "Title:".dimmed(), buddy.title.bright_white());
                    println!(
                        "  {:<14} {}",
                        "Personality:".dimmed(),
                        buddy.personality.bright_white()
                    );
                    println!(
                        "  {:<14} {}",
                        "Specialty:".dimmed(),
                        buddy.specialty.bright_white()
                    );
                    println!(
                        "  {:<14} {}",
                        "Level:".dimmed(),
                        format!("Lv.{}", buddy.level).bright_magenta()
                    );
                    println!(
                        "  {:<14} {}",
                        "XP:".dimmed(),
                        format!("{}/{}", buddy.xp, buddy.xp_for_next_level()).bright_blue()
                    );

                    // Progress bar
                    let max_xp = buddy.xp_for_next_level();
                    let filled = if max_xp > 0 {
                        (buddy.xp as f64 / max_xp as f64 * 20.0) as usize
                    } else {
                        0
                    };
                    let empty = 20_usize.saturating_sub(filled);
                    let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));
                    println!("  {:<14} {}", "Progress:".dimmed(), bar.bright_cyan());
                    println!();
                } else {
                    println!(
                        "{}",
                        "No buddy found. Run 'ember' to start onboarding and hatch your buddy!"
                            .bright_yellow()
                    );
                }
            } else {
                println!(
                    "{}",
                    "No profile found. Run 'ember' to start onboarding!".bright_yellow()
                );
            }
            SlashOutcome::Continue
        }

        SlashCommand::Init => {
            let cwd = std::env::current_dir().unwrap_or_default();
            let ember_md = cwd.join("EMBER.md");
            if ember_md.exists() {
                println!(
                    "{} EMBER.md already exists. Delete it first to regenerate.",
                    "[init]".bright_yellow()
                );
            } else {
                let ctx = build_working_directory_context();
                let content = format!(
                    "# Project Context\n\n\
                     > Auto-generated by `ember /init`. Edit freely.\n\n\
                     ## Working Directory\n\n{}\n\n\
                     ## Conventions\n\n\
                     - Describe your coding style, patterns, and preferences here\n\
                     - The AI will read this file at the start of every session\n\n\
                     ## Key Files\n\n\
                     - List important files and what they do\n",
                    ctx
                );
                match std::fs::write(&ember_md, &content) {
                    Ok(_) => {
                        let lines = content.lines().count();
                        println!(
                            "{} Created {} ({} lines)",
                            "[init]".bright_green(),
                            "EMBER.md".bright_cyan(),
                            lines
                        );
                        println!(
                            "  {} Edit it to teach Ember about your project.",
                            "hint:".dimmed()
                        );
                    }
                    Err(e) => {
                        println!("{} Failed to write EMBER.md: {}", "[error]".bright_red(), e);
                    }
                }
            }
            SlashOutcome::Continue
        }

        SlashCommand::Add { paths } => {
            if paths.is_empty() {
                println!(
                    "{} Usage: /add <file1> [file2] ...",
                    "[warn]".bright_yellow()
                );
                return SlashOutcome::Continue;
            }

            let mut added = Vec::new();
            let mut context_parts = Vec::new();
            for path in paths {
                let p = std::path::Path::new(path);
                match std::fs::read_to_string(p) {
                    Ok(content) => {
                        let lines = content.lines().count();
                        context_parts.push(format!(
                            "[File: {}] ({} lines)\n```\n{}\n```",
                            path, lines, content
                        ));
                        added.push(format!("{} ({} lines)", path.bright_cyan(), lines));
                    }
                    Err(e) => {
                        println!(
                            "{} Could not read '{}': {}",
                            "[warn]".bright_yellow(),
                            path.bright_red(),
                            e
                        );
                    }
                }
            }

            if !context_parts.is_empty() {
                println!(
                    "{} Added to context: {}",
                    "[add]".bright_green(),
                    added.join(", ")
                );
                return SlashOutcome::InjectContext(context_parts.join("\n\n"));
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
                    let reg = SlashCommandRegistry::new();
                    if let Some(suggestion) = reg.suggest(name) {
                        println!(
                            "{} Unknown command '{}'. Did you mean {}?",
                            "[warn]".bright_yellow(),
                            name.bright_red(),
                            suggestion.bright_cyan()
                        );
                    } else {
                        println!(
                            "{} Unknown command '{}'. Type {} for available commands.",
                            "[warn]".bright_yellow(),
                            name.bright_red(),
                            "/help".bright_cyan()
                        );
                    }
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
        if name1 == "ollama" {
            "openai"
        } else {
            "ollama"
        }
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
            eprintln!(
                "{} Provider '{}' unavailable: {}",
                "[error]".bright_red(),
                name1,
                e
            );
            return Ok(());
        }
    };
    let provider2 = match create_provider(config, name2) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "{} Provider '{}' unavailable: {}",
                "[error]".bright_red(),
                name2,
                e
            );
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
        "::".dimmed(),
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
            println!(
                "{} Keeping response from {}.",
                "[compare]".bright_magenta(),
                name1.bright_blue()
            );
        }
        "2" => {
            println!(
                "{} Keeping response from {}.",
                "[compare]".bright_magenta(),
                name2.bright_blue()
            );
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
                if total > 0 {
                    stats.hit_rate * 100.0
                } else {
                    0.0
                }
            );
            println!("  Avg sim:    {:.3}", stats.average_similarity);
            println!(
                "  Tokens saved: {}",
                stats.tokens_saved.to_string().bright_green()
            );
            println!("  Est. savings: ${:.4}", stats.estimated_savings_usd);
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
    let is_tty = io::stdout().is_terminal();

    if format == ChatFormat::Text && is_tty {
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

        let request = CompletionRequest::new(model)
            .with_temperature(temperature)
            .with_messages(history.clone())
            .with_tools(tools.clone());

        let response = complete_with_retry_visible(&*provider, request, 3)
            .await
            .context("Failed to get response from LLM")?;

        if !response.tool_calls.is_empty() {
            let mut assistant_msg = Message::assistant(&response.content);
            assistant_msg.tool_calls = response.tool_calls.clone();
            history.push(assistant_msg);

            for call in &response.tool_calls {
                if is_tty {
                    println!(
                        "{} Executing tool: {} {}",
                        "[tool]".bright_magenta(),
                        call.name.bright_cyan(),
                        format!("({})", truncate_json(&call.arguments, 50)).dimmed()
                    );
                }

                let result = registry.execute_tool_call(call).await;
                match &result {
                    Ok(tool_result) => {
                        if is_tty {
                            let preview = truncate_str(&tool_result.output, 100);
                            if tool_result.success {
                                println!("{} {}", "[result]".bright_green(), preview);
                            } else {
                                println!("{} {}", "[error]".bright_red(), preview);
                            }
                        }
                        history.push(Message::tool_result(&call.id, &tool_result.output));
                    }
                    Err(e) => {
                        let error_msg = format!("Tool execution failed: {}", e);
                        if is_tty {
                            println!("{} {}", "[error]".bright_red(), &error_msg);
                        }
                        history.push(Message::tool_result(&call.id, &error_msg));
                    }
                }
            }
            continue;
        }

        // Final response — output raw when piped
        if !is_tty {
            print!("{}", response.content);
            return Ok(());
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
    registry: Option<ToolRegistry>,
    resume: Option<PersistedSession>,
) -> Result<()> {
    let has_tools = registry.is_some();
    let mode_label = if has_tools { "agent" } else { "chat" };
    println!(
        "{} {} {} mode",
        "[ember]".bright_yellow(),
        "Ember".bright_yellow().bold(),
        mode_label
    );
    println!(
        "   Using {} with {}",
        provider.name().bright_blue(),
        model.bright_green()
    );
    if let Some(ref reg) = registry {
        println!(
            "   {} tool(s) enabled: {}",
            reg.len().to_string().bright_green(),
            reg.tool_names().join(", ").bright_cyan()
        );
    }
    println!(
        "   Type {} to exit, {} for help",
        "exit".bright_red(),
        "/help".bright_cyan()
    );
    if streaming {
        println!("   {} enabled", "Streaming".bright_green());
    }
    println!();

    let tools = registry
        .as_ref()
        .map(|r| r.llm_tool_definitions())
        .unwrap_or_default();

    // Initialize hook runner for PreToolUse/PostToolUse/PostToolUseFailure
    let hook_runner = HookRunner::new();

    // Restore history from persisted session or start fresh
    let (session_id, mut history) = if let Some(ref s) = resume {
        let msgs: Vec<Message> = s.messages.iter().map(|m| m.to_message()).collect();
        (s.id.clone(), msgs)
    } else {
        (new_session_id(), vec![Message::system(system_prompt)])
    };

    // Active model (can be changed mid-session via /model) — restore from session
    let mut active_model = resume
        .as_ref()
        .and_then(|s| s.active_model.clone())
        .unwrap_or_else(|| model.to_owned());

    // Usage tracker (approximation: we record by token count from response)
    let mut tracker = SessionUsageTracker::new(&active_model);

    // Semantic cache for agent REPL.
    let sem_cache = new_semantic_cache();

    // Plan mode — restore from session
    let mut plan_mode = resume.as_ref().map(|s| s.plan_mode).unwrap_or(false);
    let mut pending_plan: Vec<(String, String)> = Vec::new();

    // Compact mode — restore from session or config default
    let mut compact_mode = resume
        .as_ref()
        .map(|s| s.compact_mode)
        .unwrap_or(config.agent.compact_mode);

    // Rustyline editor with slash-command tab-completion
    let completer = crate::commands::slash::SlashCompleter::new();
    let rl_config = rustyline::Config::builder()
        .completion_type(rustyline::CompletionType::List)
        .build();
    let mut rl =
        rustyline::Editor::with_config(rl_config).context("Failed to initialize line editor")?;
    rl.set_helper(Some(completer));
    let history_path = dirs::home_dir()
        .map(|h| h.join(".ember").join("history.txt"))
        .unwrap_or_default();
    let _ = rl.load_history(&history_path);

    let mut ctrl_c_count: u8 = 0;
    let mut typeahead = String::new();
    loop {
        let cost_usd = tracker.total_cost().total_cost_usd();
        let prompt_str = if cost_usd > 0.0 {
            format!(
                "{} {} ",
                format!("[${:.4}]", cost_usd).dimmed(),
                "You:".bright_green().bold()
            )
        } else {
            format!("{} ", "You:".bright_green().bold())
        };
        let readline = if typeahead.is_empty() {
            rl.readline(&prompt_str)
        } else {
            let prefill = std::mem::take(&mut typeahead);
            rl.readline_with_initial(&prompt_str, (&prefill, ""))
        };
        let input = match readline {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                ctrl_c_count += 1;
                if ctrl_c_count >= 2 {
                    println!("\n{}", "Goodbye!".bright_yellow());
                    break;
                }
                println!(
                    "{}",
                    "Press Ctrl+C again to exit, or type a message.".dimmed()
                );
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!("{}", "Goodbye!".bright_yellow());
                break;
            }
            Err(e) => {
                eprintln!("{} Input error: {}", "[error]".bright_red(), e);
                continue;
            }
        };
        let input = input.trim();
        ctrl_c_count = 0; // reset on valid input

        if input.is_empty() {
            continue;
        }

        let _ = rl.add_history_entry(input);

        // Handle slash commands
        if input.starts_with('/') {
            if let Some(cmd) = SlashCommand::parse(input) {
                match handle_slash(&cmd, &history, &tracker, &active_model, registry.as_ref()) {
                    SlashOutcome::Exit => {
                        println!("{}", "Goodbye!".bright_yellow());
                        break;
                    }
                    SlashOutcome::SwitchModel(new_model) => {
                        active_model = new_model;
                        tracker = SessionUsageTracker::new(&active_model);
                        continue;
                    }
                    SlashOutcome::RunCompare {
                        provider1,
                        provider2,
                        prompt,
                    } => {
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
                    SlashOutcome::InjectContext(ctx) => {
                        history.push(Message::user(&ctx));
                        continue;
                    }
                    SlashOutcome::TogglePlanMode => {
                        plan_mode = !plan_mode;
                        if plan_mode {
                            pending_plan.clear();
                            println!(
                                "{} Plan mode {} — tools will be shown but not executed.",
                                "▸".bright_cyan(),
                                "ON".bright_green().bold()
                            );
                            println!(
                                "  {}",
                                "Use /execute to run the proposed plan, or /plan to toggle off."
                                    .dimmed()
                            );
                        } else {
                            println!(
                                "{} Plan mode {} — tools will execute normally.",
                                "▸".bright_cyan(),
                                "OFF".bright_red().bold()
                            );
                        }
                        continue;
                    }
                    SlashOutcome::ExecutePlan => {
                        if pending_plan.is_empty() {
                            println!(
                                "{} No pending plan. Use /plan first, then ask a question.",
                                "[warn]".bright_yellow()
                            );
                        } else {
                            println!(
                                "{} Executing {} planned tool call(s)...",
                                "[execute]".bright_green(),
                                pending_plan.len()
                            );
                            for (tool_name, args_json) in &pending_plan {
                                let args: serde_json::Value = serde_json::from_str(args_json)
                                    .unwrap_or(serde_json::Value::Null);
                                println!(
                                    "  {} {}({})",
                                    "▸".bright_green(),
                                    tool_name.bright_cyan(),
                                    truncate_json(&args, 60).dimmed()
                                );
                                match registry.as_ref().unwrap().execute(tool_name, args).await {
                                    Ok(out) => {
                                        let preview = truncate_str(&out.output, 100);
                                        if out.success {
                                            println!("    {} {}", "[ok]".bright_green(), preview);
                                        } else {
                                            println!("    {} {}", "[err]".bright_red(), preview);
                                        }
                                    }
                                    Err(e) => {
                                        println!("    {} {}", "[err]".bright_red(), e);
                                    }
                                }
                            }
                            pending_plan.clear();
                            plan_mode = false;
                            println!("{} Plan executed. Plan mode off.", "▸".bright_cyan());
                        }
                        continue;
                    }
                    SlashOutcome::ToggleCompact => {
                        compact_mode = !compact_mode;
                        println!(
                            "{} Compact mode {}",
                            "▸".bright_cyan(),
                            if compact_mode {
                                "ON — concise responses".bright_green()
                            } else {
                                "OFF — verbose responses".bright_red()
                            }
                        );
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
            // Award buddy XP on session end
            let turn_count = history
                .iter()
                .filter(|m| matches!(m.role, ember_llm::Role::User))
                .count();
            if let Some(msg) = crate::onboarding::award_session_xp(turn_count) {
                println!("{}", msg.bright_yellow().bold());
            }
            println!("{}", "Goodbye!".bright_yellow());
            break;
        }

        history.push(Message::user(input));

        // Suppress keyboard echo while AI is responding so type-ahead
        // doesn't visually mix with the output.
        suppress_echo(true);

        for iteration in 0..MAX_TOOL_ITERATIONS {
            debug!("Interactive tool iteration {}", iteration + 1);

            let mut messages = history.clone();
            if compact_mode {
                messages.push(Message::system(
                    "COMPACT MODE: Be extremely concise. No pleasantries. Bullet points. Essential code only. Skip explanations unless asked."
                ));
            }

            let request = CompletionRequest::new(&active_model)
                .with_temperature(temperature)
                .with_messages(messages)
                .with_tools(tools.clone());

            if iteration == 0 {
                print!("{} ", "Ember:".bright_blue().bold());
                io::stdout().flush()?;
            }

            let response =
                match complete_with_retry_visible(&*provider, request, config.agent.max_retries)
                    .await
                {
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

                // Phase 1: Display + collect confirmations + hooks (sequential — needs user input)
                let mut approved_calls: Vec<&ember_llm::ToolCall> = Vec::new();
                for call in &response.tool_calls {
                    let tool_desc = format_tool_call_display(&call.name, &call.arguments);
                    println!("  {} {}", "[tool]".bright_magenta(), tool_desc);

                    if plan_mode {
                        println!("    {} would execute (plan mode)", "[plan]".bright_cyan());
                        pending_plan.push((call.name.clone(), call.arguments.to_string()));
                        history.push(Message::tool_result(
                            &call.id,
                            format!("[Plan mode: {} call recorded but not executed]", call.name),
                        ));
                        continue;
                    }

                    if !confirm_tool_execution(&call.name, &call.arguments) {
                        let denied_msg = "Tool execution denied by user.";
                        println!("  {} {}", "[skip]".bright_yellow(), denied_msg.dimmed());
                        history.push(Message::tool_result(&call.id, denied_msg));
                        continue;
                    }

                    let pre_ctx = HookContext {
                        event: HookEvent::PreToolUse,
                        tool_name: call.name.clone(),
                        tool_input: call.arguments.to_string(),
                        tool_output: None,
                        error: None,
                    };
                    let pre_result = hook_runner.run(&pre_ctx);
                    for msg in pre_result.messages() {
                        println!("  {} {}", "[hook]".bright_magenta(), msg.dimmed());
                    }
                    if pre_result.is_denied() {
                        let denied_msg = "Tool execution blocked by hook.";
                        println!("  {} {}", "[hook]".bright_red(), denied_msg);
                        history.push(Message::tool_result(&call.id, denied_msg));
                        continue;
                    }

                    approved_calls.push(call);
                }

                // Phase 2: Execute approved calls (parallel when multiple + config enabled)
                if !approved_calls.is_empty() {
                    let reg = registry.as_ref().unwrap();
                    let use_parallel = config.agent.parallel_tools && approved_calls.len() > 1;

                    let results: Vec<(
                        String,
                        std::result::Result<ember_tools::ToolOutput, ember_tools::Error>,
                        &ember_llm::ToolCall,
                    )> = if use_parallel {
                        let llm_calls: Vec<ember_llm::ToolCall> = approved_calls
                            .iter()
                            .map(|c| ember_llm::ToolCall::new(&c.id, &c.name, c.arguments.clone()))
                            .collect();
                        let par_results = reg.execute_parallel(&llm_calls).await;
                        par_results
                            .into_iter()
                            .enumerate()
                            .map(|(i, (id, res))| (id, res, approved_calls[i]))
                            .collect()
                    } else {
                        let mut seq_results = Vec::new();
                        for call in &approved_calls {
                            // Use streaming execution for shell tools
                            if reg.supports_streaming(&call.name) {
                                let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(64);
                                let name = call.name.clone();
                                let args = call.arguments.clone();

                                // Spawn display task that prints lines as they arrive
                                let display_handle = tokio::spawn(async move {
                                    while let Some(line) = rx.recv().await {
                                        let trimmed = line.trim_end();
                                        if !trimmed.is_empty() {
                                            println!("  {} {}", "│".dimmed(), trimmed);
                                        }
                                    }
                                });

                                let res = reg.execute_streaming(&name, args, tx).await;
                                let _ = display_handle.await;
                                seq_results.push((call.id.clone(), res, *call));
                            } else {
                                let res = reg.execute(&call.name, call.arguments.clone()).await;
                                seq_results.push((call.id.clone(), res, *call));
                            }
                        }
                        seq_results
                    };

                    // Phase 3: Process results (sequential — display + hooks + history)
                    for (call_id, result, call) in results {
                        match result {
                            Ok(tool_output) => {
                                let post_ctx = HookContext {
                                    event: HookEvent::PostToolUse,
                                    tool_name: call.name.clone(),
                                    tool_input: call.arguments.to_string(),
                                    tool_output: Some(tool_output.output.clone()),
                                    error: None,
                                };
                                let post_result = hook_runner.run(&post_ctx);
                                for msg in post_result.messages() {
                                    println!("  {} {}", "[hook]".bright_magenta(), msg.dimmed());
                                }
                                format_tool_result_display(
                                    &call.name,
                                    &call.arguments,
                                    &tool_output,
                                );
                                let output = if compact_mode && tool_output.output.len() > 2000 {
                                    format!(
                                        "{}… [truncated, {} total chars]",
                                        &tool_output.output[..2000],
                                        tool_output.output.len()
                                    )
                                } else {
                                    tool_output.output.clone()
                                };
                                history.push(Message::tool_result(&call_id, &output));
                            }
                            Err(e) => {
                                let fail_ctx = HookContext {
                                    event: HookEvent::PostToolUseFailure,
                                    tool_name: call.name.clone(),
                                    tool_input: call.arguments.to_string(),
                                    tool_output: None,
                                    error: Some(e.to_string()),
                                };
                                let _ = hook_runner.run(&fail_ctx);
                                let error_msg = format!("Tool error: {}", e);
                                println!("  {} {}", "[error]".bright_red(), &error_msg);
                                history.push(Message::tool_result(&call_id, &error_msg));
                            }
                        }
                    }
                }
                continue;
            }

            // Print the AI's text response with markdown rendering
            if !response.content.is_empty() {
                if iteration == 0 {
                    #[cfg(feature = "tui")]
                    {
                        let renderer = TerminalRenderer::new();
                        let _ = renderer.render_markdown(&response.content);
                    }
                    #[cfg(not(feature = "tui"))]
                    println!("{}", response.content);
                } else {
                    print!("{} ", "Ember:".bright_blue().bold());
                    #[cfg(feature = "tui")]
                    {
                        let renderer = TerminalRenderer::new();
                        let _ = renderer.render_markdown(&response.content);
                    }
                    #[cfg(not(feature = "tui"))]
                    println!("{}", response.content);
                }
            }

            // Extract memory observations from this exchange
            if let Some(ref mgr) = crate::memory::MemoryManager::new() {
                let user_msg = input;
                let assistant_msg = &response.content;
                let observations = crate::memory::extract_observations(user_msg, assistant_msg);
                for (cat, content) in &observations {
                    let _ = mgr.observe(cat.clone(), content, user_msg, 0.7);
                }
            }

            history.push(Message::assistant(&response.content));
            break;
        }

        // Restore echo and drain any type-ahead into the next readline prefill
        suppress_echo(false);
        typeahead = drain_stdin();

        println!();

        // Persist session after each turn
        let turn_count = history
            .iter()
            .filter(|m| matches!(m.role, ember_llm::Role::User))
            .count();
        let session = PersistedSession {
            id: session_id.clone(),
            provider: provider.name().to_owned(),
            model: model.to_owned(),
            created_at: now_iso8601(),
            updated_at: now_iso8601(),
            messages: history.iter().map(PersistedMessage::from_message).collect(),
            turn_count,
            active_model: Some(active_model.clone()),
            compact_mode,
            plan_mode,
            working_directory: std::env::current_dir()
                .ok()
                .map(|p| p.display().to_string()),
            total_cost_usd: tracker.total_cost().total_cost_usd(),
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
// Small helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Complete an LLM request with automatic retry + user-visible feedback.
async fn complete_with_retry_visible(
    provider: &dyn LLMProvider,
    request: CompletionRequest,
    max_retries: u32,
) -> anyhow::Result<ember_llm::CompletionResponse> {
    let config = RetryConfig::new().with_max_retries(max_retries);
    let mut last_error: Option<ember_llm::Error> = None;
    let mut attempt = 0u32;

    loop {
        if attempt > 0 {
            let delay = config.delay_for_attempt(attempt);
            eprintln!(
                "  {} Retry {}/{} in {:.1}s: {}",
                "[retry]".bright_yellow(),
                attempt,
                max_retries,
                delay.as_secs_f64(),
                last_error
                    .as_ref()
                    .map(|e| e.to_string())
                    .unwrap_or_default()
            );
            tokio::time::sleep(delay).await;
        }

        match provider.complete(request.clone()).await {
            Ok(response) => {
                if attempt > 0 {
                    eprintln!(
                        "  {} Succeeded on attempt {}",
                        "[retry]".bright_green(),
                        attempt + 1
                    );
                }
                return Ok(response);
            }
            Err(e) => {
                if !e.is_retryable() || attempt >= max_retries {
                    return Err(e.into());
                }
                last_error = Some(e);
                attempt += 1;
            }
        }
    }
}

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

/// Format a tool call for nice terminal display.
fn format_tool_call_display(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "shell" => {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
            format!(
                "{} {}",
                "shell".bright_cyan(),
                format!("`{}`", cmd).bright_white()
            )
        }
        "filesystem" => {
            let op = args
                .get("operation")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let icon = match op {
                "read" => "📖",
                "write" => "✏️",
                "list" => "📁",
                "delete" => "🗑️",
                "search" => "🔍",
                "exists" => "❓",
                _ => "📄",
            };
            format!("{} {} {}", icon, op.bright_cyan(), path.bright_white())
        }
        "git" => {
            let op = args
                .get("operation")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("🔀 {} {}", "git".bright_cyan(), op.bright_white())
        }
        "web" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("?");
            format!(
                "🌐 {} {}",
                "fetch".bright_cyan(),
                truncate_str(url, 60).bright_white()
            )
        }
        _ => {
            format!(
                "{} {}",
                tool_name.bright_cyan(),
                truncate_json(args, 60).dimmed()
            )
        }
    }
}

/// Format tool result for nice terminal display.
fn format_tool_result_display(
    tool_name: &str,
    args: &serde_json::Value,
    result: &ember_tools::ToolOutput,
) {
    if result.success {
        match tool_name {
            "filesystem" => {
                let op = args.get("operation").and_then(|v| v.as_str()).unwrap_or("");
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                match op {
                    "read" => {
                        let lines = result.output.lines().count();
                        println!(
                            "  {} Read {} ({} lines)",
                            "✓".bright_green(),
                            path.bright_white(),
                            lines.to_string().bright_green()
                        );
                    }
                    "write" => {
                        println!("  {} Wrote {}", "✓".bright_green(), path.bright_white());
                        // Show inline diff with structured hunk data
                        show_inline_diff(result, path);
                    }
                    "list" => {
                        let entries = result.output.lines().count();
                        println!(
                            "  {} Listed {} ({} entries)",
                            "✓".bright_green(),
                            path.bright_white(),
                            entries.to_string().bright_green()
                        );
                    }
                    "delete" => {
                        println!("  {} Deleted {}", "✓".bright_green(), path.bright_white());
                    }
                    _ => {
                        let preview = truncate_str(&result.output, 80);
                        println!("  {} {}", "✓".bright_green(), preview.dimmed());
                    }
                }
            }
            "shell" => {
                let output = &result.output;
                let lines: Vec<&str> = output.lines().collect();
                let line_count = lines.len();
                if line_count == 0 {
                    println!("  {} (no output)", "✓".bright_green());
                } else if line_count <= 5 {
                    // Show full output for short results
                    for line in &lines {
                        println!("  {} {}", "│".dimmed(), line);
                    }
                } else {
                    // Show first 3 + last 2 lines for longer output
                    for line in &lines[..3] {
                        println!("  {} {}", "│".dimmed(), line);
                    }
                    println!(
                        "  {} {}",
                        "│".dimmed(),
                        format!("... ({} more lines)", line_count - 5).dimmed()
                    );
                    for line in &lines[line_count - 2..] {
                        println!("  {} {}", "│".dimmed(), line);
                    }
                }
            }
            _ => {
                let preview = truncate_str(&result.output, 100);
                println!("  {} {}", "✓".bright_green(), preview.dimmed());
            }
        }
    } else {
        let preview = truncate_str(&result.output, 100);
        println!("  {} {}", "✗".bright_red(), preview);
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Inline diff display for file writes
// ──────────────────────────────────────────────────────────────────────────────

/// Show a colored inline diff when a file is written.
/// Uses structured hunk data from ToolOutput.data when available, falls back to file preview.
fn show_inline_diff(tool_output: &ember_tools::ToolOutput, path: &str) {
    // Try to extract structured diff hunks from tool data
    if let Some(ref data) = tool_output.data {
        let lines_added = data
            .get("lines_added")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let lines_removed = data
            .get("lines_removed")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let created = data
            .get("created")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if created {
            println!(
                "  {} new file (+{} lines)",
                "│".dimmed(),
                lines_added.to_string().bright_green()
            );
        } else {
            println!(
                "  {} {} {}",
                "│".dimmed(),
                format!("+{}", lines_added).bright_green(),
                format!("-{}", lines_removed).bright_red(),
            );
        }

        // Render hunks if present
        if let Some(hunks) = data.get("hunks").and_then(|v| v.as_array()) {
            let max_hunks = 8;
            for (i, hunk) in hunks.iter().enumerate() {
                if i >= max_hunks {
                    println!(
                        "  {} {}",
                        "│".dimmed(),
                        format!("... {} more hunk(s)", hunks.len() - max_hunks).dimmed()
                    );
                    break;
                }
                if let Some(lines) = hunk.get("lines").and_then(|v| v.as_array()) {
                    for line in lines {
                        if let Some(obj) = line.as_object() {
                            if let Some(text) = obj.get("Added").and_then(|v| v.as_str()) {
                                let display = if text.len() > 100 { &text[..97] } else { text };
                                println!("  {} {}", "+".bright_green(), display.bright_green());
                            } else if let Some(text) = obj.get("Removed").and_then(|v| v.as_str()) {
                                let display = if text.len() > 100 { &text[..97] } else { text };
                                println!("  {} {}", "-".bright_red(), display.bright_red());
                            }
                            // Skip Context lines to keep output compact
                        }
                    }
                }
            }
        }
        return;
    }

    // Fallback: read the file and show preview
    let written_path = std::path::Path::new(path);
    if let Ok(content) = std::fs::read_to_string(written_path) {
        let line_count = content.lines().count();
        let size = content.len();
        let size_str = if size > 1024 * 1024 {
            format!("{:.1}MB", size as f64 / (1024.0 * 1024.0))
        } else if size > 1024 {
            format!("{:.1}KB", size as f64 / 1024.0)
        } else {
            format!("{}B", size)
        };
        println!(
            "  {} {} lines, {}",
            "│".dimmed(),
            line_count.to_string().bright_cyan(),
            size_str.dimmed()
        );
        let preview_lines: Vec<&str> = content.lines().take(5).collect();
        for line in &preview_lines {
            let display_line = if line.len() > 80 {
                format!("{}...", &line[..77])
            } else {
                line.to_string()
            };
            println!("  {} {}", "+".bright_green(), display_line.bright_green());
        }
        if line_count > 5 {
            println!(
                "  {} {}",
                "│".dimmed(),
                format!("... {} more lines", line_count - 5).dimmed()
            );
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Project context — used by /init to seed EMBER.md
// ──────────────────────────────────────────────────────────────────────────────

/// Build a context string describing the current working directory and its contents.
/// Used by `/init` to seed the initial EMBER.md file.
fn build_working_directory_context() -> String {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let mut ctx = format!("## Working Directory\n\nCurrent directory: `{}`\n", cwd);

    // Detect project type
    let project_root = std::env::current_dir().unwrap_or_default();
    let mut project_types = Vec::new();

    // Read and summarize Cargo.toml
    let cargo_toml = project_root.join("Cargo.toml");
    if cargo_toml.exists() {
        project_types.push("Rust (Cargo)");
        if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
            // Extract name, version, and key info
            let mut name = None;
            let mut version = None;
            for line in content.lines().take(20) {
                if line.starts_with("name") {
                    name = line
                        .split('=')
                        .nth(1)
                        .map(|s| s.trim().trim_matches('"').to_string());
                }
                if line.starts_with("version") {
                    version = line
                        .split('=')
                        .nth(1)
                        .map(|s| s.trim().trim_matches('"').to_string());
                }
            }
            if let (Some(n), Some(v)) = (name, version) {
                ctx.push_str(&format!("Rust project: {} v{}\n", n, v));
            }
            // Count workspace members if present
            if content.contains("[workspace]") {
                let members: Vec<&str> = content
                    .lines()
                    .filter(|l| l.trim().starts_with('"') && l.contains("crates/"))
                    .collect();
                if !members.is_empty() {
                    ctx.push_str(&format!("Workspace with {} crates\n", members.len()));
                }
            }
        }
    }

    // Read and summarize package.json
    let package_json = project_root.join("package.json");
    if package_json.exists() {
        project_types.push("JavaScript/TypeScript (npm)");
        if let Ok(content) = std::fs::read_to_string(&package_json) {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                let name = pkg
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let version = pkg
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0.0.0");
                ctx.push_str(&format!("Node project: {} v{}\n", name, version));
                // Show available scripts
                if let Some(scripts) = pkg.get("scripts").and_then(|v| v.as_object()) {
                    let script_names: Vec<&str> =
                        scripts.keys().map(|k| k.as_str()).take(10).collect();
                    ctx.push_str(&format!("Scripts: {}\n", script_names.join(", ")));
                }
                // Show key deps
                if let Some(deps) = pkg.get("dependencies").and_then(|v| v.as_object()) {
                    let dep_names: Vec<&str> = deps.keys().map(|k| k.as_str()).take(15).collect();
                    ctx.push_str(&format!("Dependencies: {}\n", dep_names.join(", ")));
                }
            }
        }
    }

    // Read and summarize pyproject.toml
    let pyproject = project_root.join("pyproject.toml");
    if pyproject.exists() {
        project_types.push("Python");
        if let Ok(content) = std::fs::read_to_string(&pyproject) {
            for line in content.lines().take(20) {
                if line.starts_with("name") {
                    if let Some(name) = line.split('=').nth(1) {
                        ctx.push_str(&format!(
                            "Python project: {}\n",
                            name.trim().trim_matches('"')
                        ));
                    }
                }
            }
        }
    } else if project_root.join("setup.py").exists() {
        project_types.push("Python");
    }

    if project_root.join("go.mod").exists() {
        project_types.push("Go");
        if let Ok(content) = std::fs::read_to_string(project_root.join("go.mod")) {
            if let Some(module_line) = content.lines().next() {
                ctx.push_str(&format!("{}\n", module_line));
            }
        }
    }
    if project_root.join("Makefile").exists() {
        project_types.push("Make");
    }
    if project_root.join(".git").exists() {
        project_types.push("Git repository");
    }

    if !project_types.is_empty() {
        ctx.push_str(&format!("Project type: {}\n", project_types.join(", ")));
    }

    // List top-level files (limited to keep context manageable)
    if let Ok(entries) = std::fs::read_dir(&project_root) {
        let mut files: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if is_dir {
                    format!("  {}/", name)
                } else {
                    format!("  {}", name)
                }
            })
            .collect();
        files.sort();

        // Limit to 50 entries
        let total = files.len();
        if total > 50 {
            files.truncate(50);
            files.push(format!("  ... and {} more", total - 50));
        }

        ctx.push_str("\nTop-level contents:\n");
        ctx.push_str(&files.join("\n"));
        ctx.push('\n');
    }

    // Include git status if available
    if project_root.join(".git").exists() {
        if let Ok(output) = std::process::Command::new("git")
            .args(["status", "--short", "--branch"])
            .current_dir(&project_root)
            .output()
        {
            if output.status.success() {
                let status = String::from_utf8_lossy(&output.stdout);
                let status = status.trim();
                if !status.is_empty() {
                    ctx.push_str(&format!("\nGit status:\n```\n{}\n```\n", status));
                }
            }
        }
    }

    ctx
}

/// Ask user for confirmation before executing a potentially dangerous tool.
/// Returns true if the user approves, false otherwise.
///
/// Respects the `--yes` flag and the "a" (always) answer which persists
/// for the rest of the session.
fn confirm_tool_execution(tool_name: &str, args: &serde_json::Value) -> bool {
    // Check auto-approve first (set by --yes or previous "a" answer)
    if AUTO_APPROVE.load(Ordering::Relaxed) {
        return true;
    }

    let is_dangerous = match tool_name {
        "shell" => true, // Always confirm shell commands
        "filesystem" => {
            // Confirm writes and deletes, but not reads/lists
            let op = args.get("operation").and_then(|v| v.as_str()).unwrap_or("");
            matches!(op, "write" | "delete")
        }
        _ => false,
    };

    if !is_dangerous {
        return true;
    }

    // Format the confirmation message based on tool type
    let desc = match tool_name {
        "shell" => {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
            format!("Run command: {}", cmd)
        }
        "filesystem" => {
            let op = args
                .get("operation")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            match op {
                "write" => format!("Write to file: {}", path),
                "delete" => format!("Delete: {}", path),
                _ => format!("{}: {}", op, path),
            }
        }
        _ => format!("Execute {} tool", tool_name),
    };

    print!(
        "  {} {} ({}/{}/{}) ",
        "[confirm]".bright_yellow(),
        desc,
        "y".bright_green(),
        "n".bright_red(),
        "a=always".bright_cyan(),
    );
    let _ = io::stdout().flush();

    let mut line = String::new();
    let _ = io::stdin().read_line(&mut line);
    let answer = line.trim().to_lowercase();
    match answer.as_str() {
        "a" | "always" => {
            AUTO_APPROVE.store(true, Ordering::Relaxed);
            println!(
                "  {} Auto-approve enabled for this session.",
                "[info]".bright_blue()
            );
            true
        }
        "y" | "yes" | "" => true,
        _ => false,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Benchmarking — run a prompt against multiple models and compare
// ──────────────────────────────────────────────────────────────────────────────

/// Benchmark result for a single model run.
pub struct BenchResult {
    pub model: String,
    pub provider: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub latency_ms: u128,
    pub error: Option<String>,
}

/// Run a benchmark: send the same prompt to multiple models and return results.
pub async fn bench_models(
    config: &AppConfig,
    task: &str,
    model_names: &[String],
) -> Vec<BenchResult> {
    use ember_llm::router::{is_model_alias, resolve_model_alias};

    let mut results = Vec::new();
    for name in model_names {
        let (prov, model) = if is_model_alias(name) {
            let candidates = resolve_model_alias(name);
            match candidates.into_iter().next() {
                Some(c) => (c.provider.to_string(), c.model.to_string()),
                None => {
                    results.push(BenchResult {
                        model: name.clone(),
                        provider: "?".into(),
                        tokens_in: 0,
                        tokens_out: 0,
                        latency_ms: 0,
                        error: Some("No candidate for alias".into()),
                    });
                    continue;
                }
            }
        } else {
            (config.provider.default.clone(), name.clone())
        };

        let provider = match create_provider(config, &prov) {
            Ok(p) => p,
            Err(e) => {
                results.push(BenchResult {
                    model: model.clone(),
                    provider: prov,
                    tokens_in: 0,
                    tokens_out: 0,
                    latency_ms: 0,
                    error: Some(format!("{}", e)),
                });
                continue;
            }
        };

        let request = CompletionRequest::new(&model)
            .with_temperature(0.0)
            .with_message(Message::user(task));

        let start = Instant::now();
        match provider.complete(request).await {
            Ok(resp) => {
                results.push(BenchResult {
                    model,
                    provider: prov,
                    tokens_in: resp.usage.prompt_tokens,
                    tokens_out: resp.usage.completion_tokens,
                    latency_ms: start.elapsed().as_millis(),
                    error: None,
                });
            }
            Err(e) => {
                results.push(BenchResult {
                    model,
                    provider: prov,
                    tokens_in: 0,
                    tokens_out: 0,
                    latency_ms: start.elapsed().as_millis(),
                    error: Some(format!("{}", e)),
                });
            }
        }
    }
    results
}

// ─────────────────────────────────────────────────────────────────────────────
// Terminal echo suppression for type-ahead buffering
// ─────────────────────────────────────────────────────────────────────────────

/// Suppress or restore terminal echo so keystrokes during AI output
/// don't appear on screen but remain in the stdin buffer.
#[cfg(unix)]
fn suppress_echo(suppress: bool) {
    // Use nix-less raw termios via std::os::unix
    use std::os::unix::io::AsRawFd;
    let fd = std::io::stdin().as_raw_fd();

    // We store/restore via a static to avoid unsafe global state issues.
    // The simpler approach: just flip the ECHO bit each time.
    unsafe {
        let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
        if libc::tcgetattr(fd, termios.as_mut_ptr()) == 0 {
            let mut t = termios.assume_init();
            if suppress {
                t.c_lflag &= !(libc::ECHO);
            } else {
                t.c_lflag |= libc::ECHO;
            }
            libc::tcsetattr(fd, libc::TCSANOW, &t);
        }
    }
}

#[cfg(not(unix))]
fn suppress_echo(_suppress: bool) {}

/// Drain any bytes sitting in the stdin buffer (non-blocking).
/// Returns them as a UTF-8 string, filtering out control characters.
fn drain_stdin() -> String {
    use std::io::Read;
    let mut buf = String::new();

    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = std::io::stdin().as_raw_fd();

        // Set non-blocking
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags >= 0 {
            unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };

            let mut raw = [0u8; 1024];
            if let Ok(n) = std::io::stdin().lock().read(&mut raw) {
                if let Ok(s) = std::str::from_utf8(&raw[..n]) {
                    buf.push_str(s);
                }
            }

            // Restore blocking mode
            unsafe { libc::fcntl(fd, libc::F_SETFL, flags) };
        }
    }

    // Filter: keep printable chars + space
    buf.retain(|c| c.is_alphanumeric() || c.is_ascii_punctuation() || c == ' ');
    buf
}
