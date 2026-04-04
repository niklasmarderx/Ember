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
#[cfg(feature = "browser")]
use ember_browser::BrowserTool;
use ember_core::usage_tracker::SessionUsageTracker;
use ember_llm::router::is_model_alias;
use ember_llm::{CompletionRequest, LLMProvider, Message, RetryConfig};
#[cfg(feature = "plugins")]
use ember_plugins::hooks::{HookContext, HookEvent, HookRunner};

// No-op stubs when plugins feature is disabled
#[cfg(not(feature = "plugins"))]
#[allow(dead_code)]
mod hooks_stub {
    #[derive(Clone, Copy)]
    pub enum HookEvent {
        PreToolUse,
        PostToolUse,
        PostToolUseFailure,
    }
    pub struct HookContext {
        pub event: HookEvent,
        pub tool_name: String,
        pub tool_input: String,
        pub tool_output: Option<String>,
        pub error: Option<String>,
    }
    pub struct HookResult;
    impl HookResult {
        pub fn messages(&self) -> &[String] {
            &[]
        }
        pub fn should_block(&self) -> bool {
            false
        }
        pub fn is_denied(&self) -> bool {
            false
        }
    }
    pub struct HookRunner;
    impl HookRunner {
        pub fn new() -> Self {
            Self
        }
        pub fn run(&self, _ctx: &HookContext) -> HookResult {
            HookResult
        }
    }
}
use ember_storage::semantic_cache::{SemanticCache, SemanticCacheBuilder};
use ember_tools::filesystem::undo_last as filesystem_undo_last;
use ember_tools::ToolRegistry;
use futures::StreamExt;
#[cfg(not(feature = "plugins"))]
use hooks_stub::{HookContext, HookEvent, HookRunner};
use rustyline::error::ReadlineError;
use serde_json;
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

// Re-export extracted modules for internal use within chat.rs
use super::context_builder::build_working_directory_context;
use super::display::{
    format_tool_call_display, format_tool_result_display, print_final_response, print_history,
    truncate_json, truncate_str, ProgressIndicator, ResponseStats,
};
use super::provider_factory::{
    check_provider_key, create_default_tool_registry, create_provider, create_tool_registry,
};
use super::risk::{confirm_tool_execution, AUTO_APPROVE};
use super::session::{
    latest_session_id, load_session, new_session_id, now_iso8601, save_session, PersistedMessage,
    PersistedSession,
};
use super::terminal::{drain_stdin, read_line, suppress_echo};

use tracing::{debug, warn};

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

    // ── Build system prompt using SystemPromptBuilder ──
    let cwd_path = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let project_kind = ember_core::detect_project_kind(&cwd_path);

    // Determine which tool names are active for the prompt
    let active_tool_names: Vec<&str> = if let Some(ref tool_names) = tools {
        if tool_names.len() == 1 && tool_names[0].to_lowercase() == "none" {
            vec![]
        } else {
            tool_names.iter().map(|s| s.as_str()).collect()
        }
    } else {
        // Default tools — matches create_default_tool_registry
        let mut names = Vec::new();
        if config.tools.shell_enabled {
            names.push("shell");
        }
        if config.tools.filesystem_enabled {
            names.push("file_read");
            names.push("file_edit");
            names.push("file_write");
        }
        if config.tools.web_enabled {
            names.push("web_fetch");
        }
        names.push("git");
        names.push("browser");
        names
    };

    let mut prompt_builder = ember_core::SystemPromptBuilder::new()
        .project_kind(project_kind)
        .cwd(cwd_path.display().to_string())
        .tool_names(&active_tool_names)
        .auto_approve(auto_approve);

    // Inject user profile name
    if let Some(ref p) = profile {
        prompt_builder = prompt_builder.user_name(&p.name);
    }

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
        prompt_builder =
            prompt_builder.add_context("PROJECT CONTEXT", auto_ctx.to_prompt_section());
    }

    if let Some(ref p) = profile {
        if let Some(ref buddy) = p.buddy {
            context_tags.push(format!("{} {}", buddy.emoji, buddy.name));
        }
        prompt_builder = prompt_builder.add_context("USER PROFILE", p.to_system_context());
    }

    // Inject learned memory into system prompt
    if let Some(ref mgr) = memory_mgr {
        if let Some(mem_ctx) = mgr.to_system_context() {
            let stats = mgr.load_stats();
            context_tags.push(format!("memory:{}", stats.total_observations));
            prompt_builder = prompt_builder.add_context("LEARNED MEMORY", mem_ctx);
        }
    }

    // Add project kind to context tags
    if project_kind != ember_core::ProjectKind::Unknown {
        context_tags.push(format!("lang:{}", project_kind));
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

    // If user provided a custom system prompt, add it as an extra rule section
    if base_system_prompt != config.agent.system_prompt {
        prompt_builder = prompt_builder.add_context("CUSTOM INSTRUCTIONS", base_system_prompt);
    }
    let system_prompt = prompt_builder.build();
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
    _streaming: bool,
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
                print_final_response(&response.content);
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
                print_final_response(&format!(
                    "## Response\n\n{}\n\n---\n*Model: {} | Provider: {}*",
                    response.content,
                    model,
                    provider.name()
                ));
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

    // Strategy tracker for smarter agentic loop (detects repeated failures)
    let mut strategy_tracker = ember_core::StrategyTracker::new();

    // Context budget tracker — estimates token usage and triggers compaction
    let mut ctx_budget = ember_core::ContextBudget::for_model(128_000);
    ctx_budget.set_system_tokens(ember_core::estimate_string_tokens(system_prompt));

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
                                match registry
                                    .as_ref()
                                    .expect(
                                        "tool registry must be initialized when tools are enabled",
                                    )
                                    .execute(tool_name, args)
                                    .await
                                {
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

        // ── @file expansion: inject referenced file contents ────────
        let input = expand_file_mentions(input);
        let input = input.as_str();

        history.push(Message::user(input));

        // Suppress keyboard echo while AI is responding so type-ahead
        // doesn't visually mix with the output.
        suppress_echo(true);

        // ── Auto-compaction: check token pressure before sending ─────
        {
            let conv_tokens: usize = history
                .iter()
                .map(|m| ember_core::estimate_string_tokens(&m.content))
                .sum();
            ctx_budget.set_conversation_tokens(conv_tokens);
            if ctx_budget.needs_compaction() {
                let info = ember_core::compact_message_history(&mut history, &ctx_budget);
                if let Some(ref ci) = info {
                    println!(
                        "  {} Compacted {} messages ({} → {} tokens)",
                        "[compact]".bright_yellow(),
                        ci.messages_removed,
                        ci.tokens_before,
                        ci.tokens_after,
                    );
                }
            }
        }

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

            // ── Streaming response: collect text + tool calls from stream ──
            let response = if streaming {
                match stream_response_interactive(&*provider, request).await {
                    Ok(r) => r,
                    Err(e) => {
                        println!("{}", format!("Error: {}", e).bright_red());
                        break;
                    }
                }
            } else {
                match complete_with_retry_visible(&*provider, request, config.agent.max_retries)
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        println!("{}", format!("Error: {}", e).bright_red());
                        break;
                    }
                }
            };

            // Record token usage
            tracker.record_turn(response.usage.clone());

            if !response.tool_calls.is_empty() {
                if iteration == 0 && !streaming {
                    // If streaming already printed "Ember: ", move to next line
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
                    let reg = registry
                        .as_ref()
                        .expect("tool registry must be initialized when tools are enabled");
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

                    // Phase 3: Process results (sequential — display + hooks + history + strategy tracking)
                    for (call_id, result, call) in results {
                        match result {
                            Ok(tool_output) => {
                                // Track success in strategy tracker
                                let err_hint = if tool_output.success {
                                    None
                                } else {
                                    Some(tool_output.output.as_str())
                                };
                                strategy_tracker.record(&call.name, tool_output.success, err_hint);

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
                                // Track failure in strategy tracker
                                let err_str = e.to_string();
                                strategy_tracker.record(&call.name, false, Some(&err_str));

                                let fail_ctx = HookContext {
                                    event: HookEvent::PostToolUseFailure,
                                    tool_name: call.name.clone(),
                                    tool_input: call.arguments.to_string(),
                                    tool_output: None,
                                    error: Some(err_str.clone()),
                                };
                                let _ = hook_runner.run(&fail_ctx);
                                let error_msg = format!("Tool error: {}", e);
                                println!("  {} {}", "[error]".bright_red(), &error_msg);
                                history.push(Message::tool_result(&call_id, &error_msg));

                                // If strategy tracker suggests switching, inject hint
                                if strategy_tracker.should_switch_strategy() {
                                    let reflection = strategy_tracker.reflect();
                                    let label = match reflection.severity {
                                        3 => "[CRITICAL]".bright_red().bold(),
                                        2 => "[reflect]".bright_yellow().bold(),
                                        _ => "[reflect]".bright_yellow(),
                                    };
                                    println!("  {} {}", label, reflection.reasoning.dimmed());
                                    // Inject a system hint to help the LLM recover
                                    if let Some(ref alt) = reflection.alternative_strategy {
                                        let hint = format!(
                                            "[Agent reflection — severity {}/3: {} Suggested action: {}]",
                                            reflection.severity, reflection.reasoning, alt
                                        );
                                        history.push(Message::system(&hint));
                                    }
                                }
                            }
                        }
                    }
                }
                continue;
            }

            // Print the AI's text response
            // When streaming was used, the text was already printed token-by-token
            // by stream_response_interactive(). Only render when NOT streaming.
            if !streaming && !response.content.is_empty() {
                if iteration > 0 {
                    print!("{} ", "Ember:".bright_blue().bold());
                }
                #[cfg(feature = "tui")]
                {
                    let renderer = TerminalRenderer::new();
                    let _ = renderer.render_markdown(&response.content);
                }
                #[cfg(not(feature = "tui"))]
                println!("{}", response.content);
            } else if streaming && !response.content.is_empty() {
                // Streaming already printed the content; just ensure a newline
                println!();
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
                print_final_response(&format!(
                    "## Response\n\n{}\n\n---\n*Model: {} | Provider: {}*",
                    response.content,
                    model,
                    provider.name()
                ));
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

/// Stream a response from the LLM, printing content tokens in real-time.
///
/// Collects any streamed tool-call deltas into a synthetic
/// `CompletionResponse` so the caller can handle tool calls exactly
/// like the non-streaming path.
async fn stream_response_interactive(
    provider: &dyn LLMProvider,
    request: CompletionRequest,
) -> anyhow::Result<ember_llm::CompletionResponse> {
    let mut stream = provider
        .complete_stream(request)
        .await
        .context("Failed to start streaming response")?;

    let mut content = String::new();
    let mut token_count: u32 = 0;

    // Tool-call accumulator: index → (id, name, arguments_json)
    let mut tc_map: std::collections::HashMap<usize, (String, String, String)> =
        std::collections::HashMap::new();
    let mut finish_reason = None;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                // Accumulate text content and print immediately
                if let Some(ref text) = chunk.content {
                    if !text.is_empty() {
                        print!("{}", text);
                        io::stdout().flush()?;
                        content.push_str(text);
                        token_count += ((text.len() + 3) / 4) as u32;
                    }
                }

                // Accumulate tool-call deltas
                if let Some(ref deltas) = chunk.tool_calls {
                    for delta in deltas {
                        let entry = tc_map
                            .entry(delta.index)
                            .or_insert_with(|| (String::new(), String::new(), String::new()));
                        if let Some(ref id) = delta.id {
                            entry.0 = id.clone();
                        }
                        if let Some(ref name) = delta.name {
                            entry.1 = name.clone();
                        }
                        if let Some(ref args) = delta.arguments {
                            entry.2.push_str(args);
                        }
                    }
                }

                if let Some(fr) = chunk.finish_reason {
                    finish_reason = Some(fr);
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

    // If we printed any content, add a newline before tool calls
    if !content.is_empty() && !tc_map.is_empty() {
        println!();
    }

    // Build tool calls from accumulated deltas
    let mut tool_calls: Vec<ember_llm::ToolCall> = tc_map
        .into_iter()
        .map(|(_idx, (id, name, args_str))| {
            let arguments = serde_json::from_str::<serde_json::Value>(&args_str)
                .unwrap_or(serde_json::Value::String(args_str));
            ember_llm::ToolCall::new(id, name, arguments)
        })
        .collect();
    // Sort by original index for deterministic order
    tool_calls.sort_by(|a, b| a.id.cmp(&b.id));

    let usage = ember_llm::TokenUsage::new(0, token_count);

    Ok(ember_llm::CompletionResponse {
        content,
        tool_calls,
        finish_reason,
        usage,
        model: String::new(),
        id: None,
    })
}

/// Expand `@path/to/file` mentions in user input by reading the file
/// contents and appending them as context. Supports both absolute and
/// relative paths. Multiple @file mentions in one message are supported.
fn expand_file_mentions(input: &str) -> String {
    // Quick bail — no @ in the input
    if !input.contains('@') {
        return input.to_string();
    }

    let mut result = input.to_string();
    let mut appended: Vec<String> = Vec::new();

    // Simple parser: find @ followed by path-like characters
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '@' && (i == 0 || chars[i - 1].is_whitespace()) {
            // Start of a potential @file mention
            let start = i + 1;
            let mut end = start;
            while end < chars.len()
                && (chars[end].is_alphanumeric()
                    || chars[end] == '/'
                    || chars[end] == '.'
                    || chars[end] == '-'
                    || chars[end] == '_'
                    || chars[end] == '~')
            {
                end += 1;
            }
            if end > start {
                let raw_path: String = chars[start..end].iter().collect();

                // Must look like a file path (contains / or .)
                if raw_path.contains('/') || raw_path.contains('.') {
                    // Expand ~ to home dir
                    let path = if let Some(stripped) = raw_path.strip_prefix('~') {
                        if let Some(home) = dirs::home_dir() {
                            home.join(stripped.trim_start_matches('/'))
                        } else {
                            std::path::PathBuf::from(&raw_path)
                        }
                    } else {
                        std::path::PathBuf::from(&raw_path)
                    };

                    match std::fs::read_to_string(&path) {
                        Ok(contents) => {
                            let display_path = path.display();
                            let line_count = contents.lines().count();
                            eprintln!(
                                "  {} {} ({} lines)",
                                "[file]".bright_magenta(),
                                display_path,
                                line_count
                            );
                            appended.push(format!(
                                "\n<file path=\"{}\">\n{}\n</file>",
                                display_path, contents
                            ));
                        }
                        Err(e) => {
                            eprintln!(
                                "  {} Could not read {}: {}",
                                "[warn]".bright_yellow(),
                                path.display(),
                                e
                            );
                        }
                    }
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }

    if !appended.is_empty() {
        result.push_str(&appended.join(""));
    }
    result
}
