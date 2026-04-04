//! Ember CLI - Command-line interface for the Ember AI agent.
//!
//! Usage:
//!   ember chat "Hello, world!"           # One-shot chat
//!   ember chat                           # Interactive chat mode
//!   ember config init                    # Initialize configuration
//!   ember config show                    # Show current configuration

#![allow(clippy::too_many_arguments)]
#![allow(clippy::wildcard_in_or_patterns)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::manual_div_ceil)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::module_name_repetitions)]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use colored::Colorize;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

mod auto_context;
mod commands;
mod config;
mod error_display;
pub mod memory;
pub mod onboarding;

#[cfg(feature = "tui")]
mod tui;

use commands::{
    chat, code, completions, config as config_cmd, export, git, history, plugin, serve,
};
use config::AppConfig;

/// Ember CLI - AI assistant for your terminal.
///
/// Examples:
///   ember chat "Hello world"
///   ember chat --model gpt-4
///   ember run "Explain Rust ownership"
///   ember config init
///   ember serve
#[derive(Parser)]
#[command(
    name = "ember",
    author,
    version,
    about = "Blazing fast AI agent CLI written in Rust",
    long_about = "Ember is a command-line AI assistant that lets you interact with
large language models directly from the terminal.

Features:
• Interactive AI chat
• Run AI tasks from the command line
• Manage configuration easily
• Start an HTTP server for integrations",
    after_help = "Examples:
  ember chat \"Explain Rust ownership\"
  ember chat --model gpt-4
  ember run \"Write a Python script\"
  ember config init
  ember serve"
)]
#[command(propagate_version = true)]
#[command(arg_required_else_help = true)]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Configuration file path
    #[arg(short, long, global = true, env = "EMBER_CONFIG")]
    config: Option<String>,

    /// Log output format
    #[arg(
        long,
        global = true,
        value_enum,
        default_value = "pretty",
        env = "EMBER_LOG_FORMAT"
    )]
    log_format: LogFormat,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, global = true, env = "EMBER_LOG_LEVEL")]
    log_level: Option<String>,

    /// Log output file (optional, logs to stderr by default)
    #[arg(long, global = true, env = "EMBER_LOG_FILE")]
    log_file: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

/// Log output format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum LogFormat {
    /// JSON format for log aggregation tools (ELK, Datadog, Loki)
    Json,
    /// Human-readable colored output (default)
    #[default]
    Pretty,
    /// Compact single-line format
    Compact,
    /// Full format with all details
    Full,
}

/// Chat output format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum ChatFormat {
    /// Plain text output (default)
    #[default]
    Text,
    /// JSON formatted output
    Json,
    /// Markdown formatted output
    Markdown,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch the Terminal UI (interactive mode)
    #[cfg(feature = "tui")]
    Tui,

    #[command(
        about = "Chat with the AI assistant.",
        long_about = "Start a conversation with an AI model using Ember.

You can send a single message for a one-shot response or run the command
without arguments to enter interactive chat mode.

Supports multiple providers (OpenAI, Ollama), custom models,
system prompts, temperature control, and optional tool usage.

Use the --tools flag to enable agent mode, allowing the AI to execute
shell commands, access the filesystem, and fetch web content.

Sessions are persisted to ~/.ember/sessions/ automatically. Use
--continue to resume the last session or --resume <id> for a specific one.

Model aliases:
  --model fast   cheapest available (haiku / gpt-4o-mini / gemini-flash)
  --model smart  best quality       (opus / gpt-4o / gemini-pro)
  --model code   code-optimised     (sonnet / gpt-4o / deepseek-coder)
  --model local  local Ollama models only",
        after_help = "Examples:
  ember chat \"Explain Rust ownership\"
  ember chat --model fast
  ember chat --provider ollama
  ember chat --tools shell,filesystem
  ember chat --continue
  ember chat --resume abc12345"
    )]
    Chat {
        /// Message to send (omit for interactive mode)
        message: Option<String>,

        /// LLM provider to use (openai, ollama)
        #[arg(short, long)]
        provider: Option<String>,

        /// Model to use (overrides config); accepts aliases: fast, smart, code, local
        #[arg(short, long)]
        model: Option<String>,

        /// System prompt (overrides config)
        #[arg(short, long)]
        system: Option<String>,

        /// Temperature (0.0 - 2.0)
        #[arg(short, long)]
        temperature: Option<f32>,

        /// Auto-approve all tool executions (skip confirmation prompts)
        #[arg(short = 'y', long)]
        yes: bool,

        /// Disable streaming output
        #[arg(long)]
        no_stream: bool,

        /// Enable tools (comma-separated: shell,filesystem,web)
        #[arg(long, value_delimiter = ',')]
        tools: Option<Vec<String>>,

        /// Output format (text, json, markdown)
        #[arg(short, long, value_enum, default_value = "text")]
        format: ChatFormat,

        /// Resume a specific session by id (e.g. from `ember history list`)
        #[arg(long)]
        resume: Option<String>,

        /// Resume the most recent session
        #[arg(long, conflicts_with = "resume")]
        continue_session: bool,
    },

    /// Execute a task using the AI agent and exit.
    ///
    /// Examples:
    ///   ember run "Write a Python script"
    ///   ember run "Explain async Rust"
    Run {
        /// The task to execute
        task: String,

        /// Model to use
        #[arg(short, long)]
        model: Option<String>,
    },

    #[command(
        about = "Manage Ember configuration.",
        long_about = "Manage and customize Ember's configuration settings.

The configuration file stores preferences such as:
- default LLM provider (OpenAI or Ollama)
- model settings
- API keys
- agent behavior and tools

You can initialize a new config file, view current settings,
or update individual configuration values.",
        after_help = "Examples:
  ember config init
  ember config show
  ember config set provider.default ollama
  ember config get provider.default"
    )]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Show version and system information
    Info,

    /// Start Ember's HTTP server for API access.
    Serve(serve::ServeArgs),

    /// Export a conversation to JSON, Markdown, or HTML.
    ///
    /// Export chat conversations in various formats for sharing, documentation, or backup.
    ///
    /// Examples:
    ///   ember export --format json --output conversation.json
    ///   ember export --format markdown --output chat.md
    ///   ember export --format html --output chat.html
    #[command(
        about = "Export a conversation to JSON, Markdown, or HTML.",
        long_about = "Export chat conversations in various formats for sharing, documentation, or backup.

Supported formats:
  - json      - Structured data format for programmatic use
  - markdown  - Human-readable format for documentation
  - html      - Styled format for web sharing

If no output file is specified, a timestamped filename will be generated automatically.",
        after_help = "Examples:
  ember export --format json
  ember export --format markdown --output my_chat.md
  ember export --format html --conversation abc123"
    )]
    Export(export::ExportArgs),

    /// Search and manage conversation history.
    ///
    /// Search through past conversations, list recent chats, view statistics,
    /// or prune old conversations.
    ///
    /// Examples:
    ///   ember history search "rust"
    ///   ember history list
    ///   ember history stats
    ///   ember history prune --older-than 2024-01-01
    #[command(
        about = "Search and manage conversation history.",
        long_about = "Search and manage your conversation history.

Features:
  - Search through conversations and messages
  - List recent conversations
  - View statistics about your chat history
  - Delete old conversations

Search supports filtering by date range and various sorting options.",
        after_help = "Examples:
  ember history search \"rust ownership\"
  ember history search \"error\" --messages-only
  ember history list --limit 20
  ember history stats
  ember history prune --older-than 2024-01-01 --yes"
    )]
    History(history::HistoryArgs),

    /// Manage Ember plugins.
    ///
    /// Search, install, update, and manage plugins from the Ember marketplace.
    ///
    /// Examples:
    ///   ember plugin search weather
    ///   ember plugin install weather
    ///   ember plugin list
    ///   ember plugin update --all
    #[command(
        about = "Manage Ember plugins.",
        long_about = "Search, install, update, and manage Ember plugins.

The plugin system allows you to extend Ember with additional tools and capabilities.
Plugins are distributed as WebAssembly modules and run in a secure sandbox.

Features:
  - Search the plugin marketplace
  - Install plugins with version pinning
  - Update plugins individually or all at once
  - View plugin details and ratings
  - Manage plugin cache",
        after_help = "Examples:
  ember plugin search \"weather\"
  ember plugin install slack
  ember plugin install github@1.5.0
  ember plugin list
  ember plugin update --all
  ember plugin info weather"
    )]
    Plugin(plugin::PluginArgs),

    /// AI-powered code intelligence.
    ///
    /// Analyze code, generate refactoring suggestions, and create tests automatically.
    ///
    /// Examples:
    ///   ember code analyze src/
    ///   ember code refactor src/main.rs
    ///   ember code testgen src/lib.rs
    ///   ember code stats .
    #[command(
        about = "AI-powered code intelligence.",
        long_about = "AI-powered code analysis, refactoring, and test generation.

Features:
  - Analyze code complexity and detect code smells
  - Generate refactoring suggestions with confidence levels
  - Auto-generate unit tests, edge case tests, and error handling tests
  - Calculate code statistics by language

Supports: Rust, Python, JavaScript, TypeScript, Go, Java",
        after_help = "Examples:
  ember code analyze src/
  ember code analyze --format json --output report.json .
  ember code refactor --min-confidence high src/main.rs
  ember code testgen --framework pytest src/utils.py
  ember code stats --by-language ."
    )]
    Code(code::CodeArgs),

    /// Git-native AI integration.
    ///
    /// Smart commit messages, PR descriptions, branch naming, and code review.
    ///
    /// Examples:
    ///   ember git commit
    ///   ember git pr --template detailed
    ///   ember git review --focus security
    ///   ember git changelog --from v1.0.0
    #[command(
        about = "Git-native AI integration.",
        long_about = "AI-powered git operations for seamless developer workflow.

Features:
  - Generate smart commit messages from staged changes
  - Create PR descriptions automatically
  - Suggest branch names from descriptions
  - Help resolve merge conflicts
  - Generate code reviews
  - Create release changelogs

Supports: Conventional Commits, emoji style, detailed format",
        after_help = "Examples:
  ember git commit --style conventional
  ember git pr --base main --template detailed
  ember git branch \"add user auth\" --create
  ember git review --focus security
  ember git changelog --from v1.0.0 --to v1.1.0"
    )]
    Git(git::GitArgs),

    /// Benchmark a task across multiple providers and models.
    ///
    /// Compare response quality, speed, and cost.
    #[command(
        about = "Benchmark a task across multiple providers and models.",
        long_about = "Run the same prompt against multiple LLM providers and compare:\n\n\
  - Response quality (side-by-side output)\n\
  - Latency (time to first token, total time)\n\
  - Cost (token count × pricing)\n\
  - Token efficiency (output tokens per concept)\n\n\
Results are displayed in a formatted table."
    )]
    Bench {
        /// The task/prompt to benchmark
        #[arg(default_value = "Explain Rust ownership in 3 sentences")]
        task: String,

        /// Comma-separated list of models to test
        #[arg(long, value_delimiter = ',')]
        models: Option<Vec<String>>,

        /// Number of runs per model (for latency averaging)
        #[arg(long, default_value = "1")]
        runs: usize,

        /// Output format (table, json, csv)
        #[arg(long, default_value = "table")]
        output: String,
    },

    /// Show or manage learned coding preferences.
    ///
    /// Ember learns from your corrections and coding patterns over time.
    #[command(
        about = "Show or manage learned coding preferences.",
        long_about = "Ember tracks your coding style, preferred patterns, and corrections.\n\n\
Subcommands:\n\
  show   — Display learned preferences\n\
  reset  — Clear all learned preferences\n\
  export — Export preferences as JSON"
    )]
    Learn {
        #[command(subcommand)]
        action: LearnAction,
    },

    /// Start a hands-free voice coding session.
    ///
    /// Uses speech-to-text and text-to-speech for natural language coding.
    #[command(
        about = "[preview] Start a hands-free voice coding session.",
        long_about = "Launch an interactive voice-controlled coding session.\n\n\
Speak naturally and Ember will:\n\
  - Transcribe your speech in real-time\n\
  - Execute coding commands\n\
  - Speak responses back to you\n\n\
STT: whisper (local), openai, deepgram\n\
TTS: openai, elevenlabs, local"
    )]
    Voice {
        /// Speech-to-text provider (whisper, openai, deepgram)
        #[arg(long, default_value = "whisper")]
        stt: String,

        /// Text-to-speech provider (openai, elevenlabs, local)
        #[arg(long, default_value = "openai")]
        tts: String,

        /// Model to use for AI responses
        #[arg(short, long)]
        model: Option<String>,

        /// Wake word to activate listening
        #[arg(long, default_value = "ember")]
        wake_word: String,

        /// Enable tools in voice mode
        #[arg(long, value_delimiter = ',')]
        tools: Option<Vec<String>>,
    },

    /// Index your codebase for semantic search (RAG).
    ///
    /// Embeds source files for retrieval-augmented generation.
    #[command(
        about = "[preview] Index your codebase for semantic search (RAG).",
        long_about = "Build a local embedding index of your codebase for RAG.\n\n\
Once indexed, Ember can semantically search your code.\n\
The index is stored in .ember/index/ using local embeddings."
    )]
    Index {
        /// Paths to index (defaults to current directory)
        paths: Vec<String>,

        /// Filter by language (comma-separated)
        #[arg(long, value_delimiter = ',')]
        language: Option<Vec<String>>,

        /// Show index status and statistics
        #[arg(long)]
        status: bool,

        /// Clear the existing index
        #[arg(long)]
        clear: bool,

        /// Chunking strategy (fixed, paragraph, sentence, recursive)
        #[arg(long, default_value = "recursive")]
        strategy: String,
    },

    /// Run a multi-agent orchestrated task.
    ///
    /// Orchestrates multiple specialized AI agents working in parallel.
    #[command(
        about = "[preview] Run a multi-agent orchestrated task.",
        long_about = "Orchestrate multiple specialized AI agents on complex tasks.\n\n\
Roles: coder, reviewer, tester, architect, documenter\n\
Agents work in parallel and results are aggregated."
    )]
    Agents {
        #[command(subcommand)]
        action: AgentsAction,
    },

    /// Generate shell completion scripts.
    ///
    /// Generates shell completions that enable tab-completion for Ember commands.
    ///
    /// Examples:
    ///   ember completions bash > ~/.local/share/bash-completion/completions/ember
    ///   ember completions zsh > ~/.zfunc/_ember
    ///   ember completions fish > ~/.config/fish/completions/ember.fish
    #[command(
        about = "Generate shell completion scripts.",
        long_about = "Generate shell completion scripts for bash, zsh, fish, PowerShell, or elvish.

Shell completions enable tab-completion for Ember commands, options, and arguments,
making the CLI much easier and faster to use.

Supported shells:
  - bash
  - zsh
  - fish
  - powershell
  - elvish",
        after_help = "Installation:

  Bash:
    ember completions bash > ~/.local/share/bash-completion/completions/ember

  Zsh:
    mkdir -p ~/.zfunc
    ember completions zsh > ~/.zfunc/_ember
    # Add 'fpath+=~/.zfunc' to ~/.zshrc before compinit

  Fish:
    ember completions fish > ~/.config/fish/completions/ember.fish

  PowerShell:
    ember completions powershell >> $PROFILE"
    )]
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Initialize a new configuration file
    Init {
        /// Force overwrite existing config
        #[arg(short, long)]
        force: bool,
    },

    /// Show current configuration
    Show {
        /// Output the configuration as formatted JSON
        #[arg(long)]
        json: bool,
    },

    /// Set a configuration value
    Set {
        /// Key to set (e.g., "model", "api_key")
        key: String,
        /// Value to set
        value: String,
    },

    /// Get a configuration value
    Get {
        /// Key to get
        key: String,
    },

    /// Show configuration file path
    Path,
}

#[derive(Subcommand)]
enum LearnAction {
    /// Display learned coding preferences
    Show,
    /// Clear all learned preferences
    Reset,
    /// Export preferences as JSON
    Export {
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },
}

#[derive(Subcommand)]
enum AgentsAction {
    /// Run a multi-agent task
    Run {
        /// The task description
        task: String,

        /// Comma-separated agent roles (coder, reviewer, tester, architect, documenter)
        #[arg(long, value_delimiter = ',')]
        roles: Option<String>,

        /// Model to use for agents
        #[arg(short, long)]
        model: Option<String>,

        /// Maximum orchestration rounds
        #[arg(long, default_value = "5")]
        max_rounds: usize,
    },

    /// List available agent roles
    List,
}

#[tokio::main]
async fn main() {
    // Run the actual main function and handle errors gracefully
    if let Err(e) = run().await {
        error_display::display_error(&e);
        std::process::exit(1);
    }
}

/// Check if the first non-flag argument is a known subcommand.
/// If not, treat all positional args as a chat message (one-shot mode).
///
/// Also handles piped stdin: when stdin is not a TTY and no subcommand is given,
/// reads stdin and prepends it to the chat message.
fn maybe_rewrite_args() -> Vec<String> {
    let args: Vec<String> = std::env::args().collect();

    // Known subcommands that clap expects
    let known_subcommands = [
        "chat", "run", "config", "info", "serve", "completions", "export",
        "history", "plugin", "code", "git", "bench", "learn", "voice",
        "index", "agents", "tui", "help",
    ];

    // Find the first positional argument (skip flags like --verbose, --config foo)
    let mut i = 1; // skip binary name
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            break;
        }
        if arg.starts_with('-') {
            // Skip flag value if it's a key-value flag
            if matches!(arg.as_str(), "--config" | "-c" | "--log-format" | "--log-level" | "--log-file") {
                i += 1; // skip the value
            }
            i += 1;
            continue;
        }
        // Found a positional argument — is it a known subcommand?
        if known_subcommands.contains(&arg.to_lowercase().as_str()) {
            // Check if this is "chat" without a message and stdin has piped data
            if arg.to_lowercase() == "chat" {
                return maybe_inject_stdin(args);
            }
            return args; // Already has a subcommand, no rewrite needed
        }
        // Not a known subcommand — treat everything from here as a chat message
        let mut new_args = args[..i].to_vec();
        new_args.push("chat".to_string());
        // Join remaining args as the message
        let message = args[i..].join(" ");
        // Prepend piped stdin if available
        let piped = read_piped_stdin();
        if let Some(stdin_content) = piped {
            new_args.push(format!("{}\n\n{}", stdin_content, message));
        } else {
            new_args.push(message);
        }
        return new_args;
    }

    // No positional args at all — check for piped stdin
    if let Some(stdin_content) = read_piped_stdin() {
        let mut new_args = args.clone();
        new_args.push("chat".to_string());
        new_args.push(stdin_content);
        return new_args;
    }

    args
}

/// If stdin is piped (not a TTY), read all of it. Otherwise return None.
fn read_piped_stdin() -> Option<String> {
    use std::io::IsTerminal;
    if std::io::stdin().is_terminal() {
        return None;
    }
    let mut content = String::new();
    if std::io::Read::read_to_string(&mut std::io::stdin(), &mut content).is_ok() && !content.is_empty() {
        Some(content)
    } else {
        None
    }
}

/// Inject piped stdin into a "chat" command that has no message argument.
fn maybe_inject_stdin(args: Vec<String>) -> Vec<String> {
    // Only inject if there's no message argument after "chat"
    // and stdin is piped
    let chat_idx = args.iter().position(|a| a.to_lowercase() == "chat");
    if let Some(idx) = chat_idx {
        // Check if there's already a message (non-flag arg after "chat")
        let has_message = args[idx + 1..].iter().any(|a| !a.starts_with('-'));
        if !has_message {
            if let Some(stdin_content) = read_piped_stdin() {
                let mut new_args = args;
                new_args.push(stdin_content);
                return new_args;
            }
        }
    }
    args
}

/// The actual main function that returns a Result
async fn run() -> Result<()> {
    let rewritten = maybe_rewrite_args();
    let cli = Cli::parse_from(rewritten);

    init_logging(
        cli.log_format,
        cli.verbose,
        cli.log_level.as_deref(),
        cli.log_file.as_deref(),
    )?;

    let config = AppConfig::load(cli.config.as_deref()).context("Failed to load configuration")?;

    match cli.command {
        #[cfg(feature = "tui")]
        Commands::Tui => {
            tui::run(config).await?;
        }

        Commands::Chat {
            message,
            provider,
            model,
            system,
            temperature,
            yes,
            no_stream,
            tools,
            format,
            resume,
            continue_session,
        } => {
            chat::run(
                config,
                message,
                provider,
                model,
                system,
                temperature,
                !no_stream,
                tools,
                format,
                resume,
                continue_session,
                yes,
            )
            .await?;
        }

        Commands::Run { task, model } => {
            chat::run_task(config, task, model).await?;
        }

        Commands::Config { action } => match action {
            ConfigAction::Init { force } => {
                config_cmd::init(force)?;
            }
            ConfigAction::Show { json } => {
                config_cmd::show(&config, json)?;
            }
            ConfigAction::Set { key, value } => {
                config_cmd::set(&key, &value)?;
            }
            ConfigAction::Get { key } => {
                config_cmd::get(&config, &key)?;
            }
            ConfigAction::Path => {
                config_cmd::path()?;
            }
        },

        Commands::Info => {
            print_info()?;
        }

        Commands::Serve(args) => {
            serve::run(args).await?;
        }

        Commands::Completions { shell } => {
            completions::generate_completions::<Cli>(shell)?;
            completions::print_installation_instructions(shell);
        }

        Commands::Export(args) => {
            export::run(args)?;
        }

        Commands::History(args) => {
            history::execute(args).await?;
        }

        Commands::Plugin(args) => {
            plugin::execute(args).await?;
        }

        Commands::Code(args) => {
            code::execute(args).await?;
        }

        Commands::Git(args) => {
            git::execute(args).await?;
        }

        Commands::Bench {
            task,
            models,
            runs,
            output,
        } => {
            let model_list: Vec<String> = models
                .unwrap_or_else(|| vec!["fast".into(), "smart".into(), "code".into()]);
            println!(
                "{} Benchmarking {} model(s) × {} run(s)",
                "▸".bright_green(),
                model_list.len().to_string().bright_cyan(),
                runs.to_string().bright_cyan(),
            );
            println!("  Task: {}", task.bright_green());
            println!();
            println!(
                "  {:<30} {:>6} {:>6} {:>10} {:>8}",
                "Model".bright_yellow().bold(),
                "In".bright_yellow().bold(),
                "Out".bright_yellow().bold(),
                "Latency".bright_yellow().bold(),
                "Status".bright_yellow().bold()
            );
            println!("  {}", "─".repeat(65));

            let results = chat::bench_models(&config, &task, &model_list).await;
            for r in &results {
                if let Some(ref err) = r.error {
                    let display_model = format!("{} ({})", r.model, r.provider);
                    println!(
                        "  {:<30} {:>6} {:>6} {:>10} {}",
                        display_model,
                        "-", "-", "-",
                        format!("ERR: {}", truncate(err, 25)).bright_red()
                    );
                } else {
                    let display_model = format!("{} ({})", r.model, r.provider);
                    let latency = format!("{:.1}s", r.latency_ms as f64 / 1000.0);
                    println!(
                        "  {:<30} {:>6} {:>6} {:>10} {}",
                        display_model.bright_cyan(),
                        r.tokens_in,
                        r.tokens_out,
                        latency.bright_blue(),
                        "OK".bright_green()
                    );
                }
            }
            println!();

            if output != "text" {
                println!(
                    "  {}",
                    format!("Output format '{}' — use --output text for table (default)", output).dimmed()
                );
            }

            fn truncate(s: &str, max: usize) -> &str {
                if s.len() > max { &s[..max] } else { s }
            }
        }

        Commands::Learn { action } => match action {
            LearnAction::Show => {
                println!("{}", "Learned Preferences:".bright_yellow().bold());
                let prefs_dir = dirs::home_dir()
                    .unwrap_or_default()
                    .join(".ember")
                    .join("learn");
                if prefs_dir.exists() {
                    println!(
                        "  Profile: {}",
                        prefs_dir.display().to_string().bright_cyan()
                    );
                } else {
                    println!("  {}", "No preferences learned yet.".dimmed());
                    println!(
                        "  {}",
                        "Ember learns from your corrections and coding patterns over time."
                            .dimmed()
                    );
                }
            }
            LearnAction::Reset => {
                let prefs_dir = dirs::home_dir()
                    .unwrap_or_default()
                    .join(".ember")
                    .join("learn");
                if prefs_dir.exists() {
                    let _ = std::fs::remove_dir_all(&prefs_dir);
                    println!("{} Coding preferences cleared.", "[ember]".bright_yellow());
                } else {
                    println!("{}", "Nothing to reset.".dimmed());
                }
            }
            LearnAction::Export { output } => {
                let path = output.as_deref().unwrap_or("ember-preferences.json");
                println!(
                    "{} Exported preferences to {}",
                    "[ember]".bright_yellow(),
                    path.bright_green()
                );
                // Write empty JSON for now
                let _ = std::fs::write(path, "{\"preferences\": [], \"patterns\": []}");
            }
        },

        Commands::Voice {
            stt,
            tts,
            model,
            wake_word,
            tools,
        } => {
            println!(
                "{} {} voice mode",
                "[ember]".bright_yellow(),
                "Starting".bright_cyan(),
            );
            println!(
                "   STT: {} | TTS: {} | Wake word: {}",
                stt.bright_green(),
                tts.bright_green(),
                wake_word.bright_blue(),
            );
            if let Some(ref m) = model {
                println!("   Model: {}", m.bright_green());
            }
            if let Some(ref t) = tools {
                println!("   Tools: {}", t.join(", ").bright_cyan());
            }
            println!();
            println!(
                "{}",
                "Voice interface requires audio device access.".dimmed()
            );
            println!(
                "{}",
                "Say the wake word to start, or press Enter for push-to-talk.".dimmed()
            );
            println!(
                "\n{}",
                "Voice mode is in preview. Full audio pipeline coming soon.".bright_yellow()
            );
        }

        Commands::Index {
            paths,
            language,
            status,
            clear,
            strategy,
        } => {
            if status {
                println!("{}", "Index Status:".bright_yellow().bold());
                let index_dir = dirs::home_dir()
                    .unwrap_or_default()
                    .join(".ember")
                    .join("index");
                if index_dir.exists() {
                    let count = std::fs::read_dir(&index_dir)
                        .map(|d| d.count())
                        .unwrap_or(0);
                    println!("  Location: {}", index_dir.display());
                    println!("  Files:    {}", count.to_string().bright_green());
                } else {
                    println!(
                        "  {}",
                        "No index found. Run 'ember index .' to create one.".dimmed()
                    );
                }
            } else if clear {
                let index_dir = dirs::home_dir()
                    .unwrap_or_default()
                    .join(".ember")
                    .join("index");
                if index_dir.exists() {
                    std::fs::remove_dir_all(&index_dir)?;
                    println!("{} Index cleared.", "[ember]".bright_yellow());
                } else {
                    println!("{}", "No index to clear.".dimmed());
                }
            } else {
                let target_paths = if paths.is_empty() {
                    vec![".".to_string()]
                } else {
                    paths
                };
                println!(
                    "{} Indexing {} with {} strategy...",
                    "[ember]".bright_yellow(),
                    target_paths.join(", ").bright_cyan(),
                    strategy.bright_green(),
                );
                if let Some(ref langs) = language {
                    println!("  Languages: {}", langs.join(", ").bright_blue());
                }

                // Walk files and count
                let mut file_count = 0usize;
                for path in &target_paths {
                    for entry in walkdir::WalkDir::new(path)
                        .into_iter()
                        .filter_map(|e| e.ok())
                        .filter(|e| e.file_type().is_file())
                    {
                        let ext = entry
                            .path()
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("");
                        let path_str = entry.path().to_string_lossy();
                        if path_str.contains("/.git/")
                            || path_str.contains("/target/")
                            || path_str.contains("/node_modules/")
                        {
                            continue;
                        }
                        if let Some(ref langs) = language {
                            let lang_match = langs.iter().any(|l| match l.as_str() {
                                "rust" => ext == "rs",
                                "python" => ext == "py",
                                "javascript" | "js" => ext == "js" || ext == "jsx",
                                "typescript" | "ts" => ext == "ts" || ext == "tsx",
                                "go" => ext == "go",
                                "java" => ext == "java",
                                _ => ext == l.as_str(),
                            });
                            if !lang_match {
                                continue;
                            }
                        }
                        file_count += 1;
                    }
                }
                println!(
                    "{} Found {} files to index.",
                    "[ember]".bright_yellow(),
                    file_count.to_string().bright_green(),
                );
                println!(
                    "\n{}",
                    "Embedding pipeline in preview. File discovery works, embeddings coming soon."
                        .bright_yellow()
                );
            }
        }

        Commands::Agents { action } => match action {
            AgentsAction::Run {
                task,
                roles,
                model,
                max_rounds,
            } => {
                println!(
                    "{} {} multi-agent orchestration",
                    "[ember]".bright_yellow(),
                    "Starting".bright_cyan(),
                );
                println!("  Task:   {}", task.bright_green());
                let role_list = roles.as_deref().unwrap_or("coder,reviewer");
                println!("  Roles:  {}", role_list.bright_blue());
                if let Some(ref m) = model {
                    println!("  Model:  {}", m.bright_green());
                }
                println!("  Rounds: {}", max_rounds.to_string().bright_cyan());
                println!(
                    "\n{}",
                    "Multi-agent orchestration in preview. Framework ready, execution coming soon."
                        .bright_yellow()
                );
            }
            AgentsAction::List => {
                println!("{}", "Available Agent Roles:".bright_yellow().bold());
                println!("  {} — Writes implementation code", "coder".bright_cyan());
                println!(
                    "  {} — Reviews code quality and security",
                    "reviewer".bright_cyan()
                );
                println!("  {} — Generates and runs tests", "tester".bright_cyan());
                println!("  {} — Plans system design", "architect".bright_cyan());
                println!("  {} — Writes documentation", "documenter".bright_cyan());
                println!("\nUsage: ember agents run \"task\" --roles coder,reviewer");
            }
        },
    }

    Ok(())
}

/// Initialize logging with the specified format and level.
///
/// # Arguments
///
/// * `format` - Output format (json, pretty, compact, full)
/// * `verbose` - Enable verbose (debug) logging
/// * `level` - Override log level (trace, debug, info, warn, error)
/// * `log_file` - Optional file path for log output
fn init_logging(
    format: LogFormat,
    verbose: bool,
    level: Option<&str>,
    log_file: Option<&str>,
) -> Result<()> {
    // Determine log level
    let default_level = if verbose { "debug" } else { "warn" };
    let level_str = level.unwrap_or(default_level);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level_str));

    // Build subscriber based on format and output target
    match (format, log_file) {
        (LogFormat::Json, Some(file_path)) => {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)
                .context("Failed to open log file")?;

            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .json()
                        .with_writer(std::sync::Mutex::new(file))
                        .with_target(true)
                        .with_thread_ids(true)
                        .with_file(true)
                        .with_line_number(true)
                        .with_span_events(FmtSpan::CLOSE),
                )
                .init();
        }
        (LogFormat::Json, None) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .json()
                        .with_target(true)
                        .with_thread_ids(true)
                        .with_file(true)
                        .with_line_number(true)
                        .with_span_events(FmtSpan::CLOSE),
                )
                .init();
        }
        (LogFormat::Pretty, Some(file_path)) => {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)
                .context("Failed to open log file")?;

            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .pretty()
                        .with_writer(std::sync::Mutex::new(file))
                        .with_target(true)
                        .with_file(true)
                        .with_line_number(true),
                )
                .init();
        }
        (LogFormat::Pretty, None) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .pretty()
                        .with_target(false)
                        .with_file(false)
                        .with_line_number(false),
                )
                .init();
        }
        (LogFormat::Compact, Some(file_path)) => {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)
                .context("Failed to open log file")?;

            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .compact()
                        .with_writer(std::sync::Mutex::new(file))
                        .with_target(true),
                )
                .init();
        }
        (LogFormat::Compact, None) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().compact().with_target(true))
                .init();
        }
        (LogFormat::Full, Some(file_path)) => {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)
                .context("Failed to open log file")?;

            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .with_writer(std::sync::Mutex::new(file))
                        .with_target(true)
                        .with_thread_ids(true)
                        .with_thread_names(true)
                        .with_file(true)
                        .with_line_number(true)
                        .with_span_events(FmtSpan::FULL),
                )
                .init();
        }
        (LogFormat::Full, None) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_thread_ids(true)
                        .with_thread_names(true)
                        .with_file(true)
                        .with_line_number(true)
                        .with_span_events(FmtSpan::FULL),
                )
                .init();
        }
    }

    Ok(())
}

fn print_info() -> Result<()> {
    println!("{}", "Ember AI Agent".bright_yellow().bold());
    println!();
    println!(
        "{}  {}",
        "Version:".bright_blue(),
        env!("CARGO_PKG_VERSION")
    );
    println!("{} {}", "Rust:".bright_blue(), rustc_version());
    println!("{} {}", "OS:".bright_blue(), std::env::consts::OS);
    println!("{} {}", "Arch:".bright_blue(), std::env::consts::ARCH);
    println!();
    println!("{}", "Supported Providers:".bright_blue());
    println!(
        "  {} OpenAI       - GPT-4o, GPT-4o-mini, o1, o3-mini",
        "[x]".green()
    );
    println!(
        "  {} Anthropic    - Claude 3.5 Sonnet/Haiku/Opus",
        "[x]".green()
    );
    println!(
        "  {} Google       - Gemini 2.0 Flash, 1.5 Pro (2M context)",
        "[x]".green()
    );
    println!(
        "  {} Ollama       - Local models (Llama, Qwen, DeepSeek)",
        "[x]".green()
    );
    println!(
        "  {} Groq         - Ultra-fast inference (Llama 3.3 70B)",
        "[x]".green()
    );
    println!(
        "  {} DeepSeek     - V3, R1 Reasoner (cost-effective)",
        "[x]".green()
    );
    println!(
        "  {} Mistral      - Large, Codestral, Pixtral",
        "[x]".green()
    );
    println!(
        "  {} OpenRouter   - 200+ models via single API",
        "[x]".green()
    );
    println!("  {} xAI          - Grok 2, Grok Vision", "[x]".green());
    println!(
        "  {} AWS Bedrock  - Claude, Titan, Llama via AWS",
        "[x]".green()
    );
    println!();
    println!("{}", "Available Tools:".bright_blue());
    println!(
        "  {} Shell        - Execute terminal commands",
        "[x]".green()
    );
    println!(
        "  {} Filesystem   - Read, write, search files",
        "[x]".green()
    );
    println!(
        "  {} Git          - Clone, commit, push, branch",
        "[x]".green()
    );
    println!(
        "  {} Web          - HTTP requests, web scraping",
        "[x]".green()
    );
    println!(
        "  {} Browser      - Headless browser automation",
        "[x]".green()
    );
    println!(
        "  {} Code         - Execute Python, JS, Rust",
        "[x]".green()
    );
    println!();
    println!("{}", "Features:".bright_blue());
    println!("  {} Streaming responses", "[x]".green());
    println!("  {} Conversation memory", "[x]".green());
    println!("  {} Cost tracking & budgets", "[x]".green());
    println!("  {} Checkpoints (undo/redo)", "[x]".green());
    println!("  {} Multi-agent orchestration", "[x]".green());
    println!("  {} WASM plugin system", "[x]".green());
    println!("  {} Web UI & REST API", "[x]".green());
    println!("  {} Privacy shield (PII redaction)", "[x]".green());
    println!();
    println!("{}", "Configuration:".bright_blue());
    println!(
        "  Path: {}",
        config::AppConfig::config_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "Not found".to_string())
    );
    println!();
    println!("{}", "Quick Start:".bright_blue());
    println!("  ember chat \"Hello!\"              # One-shot chat");
    println!("  ember chat                        # Interactive mode");
    println!("  ember chat --provider ollama      # Use local models");
    println!("  ember serve                       # Start web UI");
    println!("  ember plugin search weather       # Find plugins");
    println!();
    println!(
        "{} {}",
        "Documentation:".bright_blue(),
        "https://ember.dev/docs".cyan()
    );
    println!(
        "{} {}",
        "Repository:".bright_blue(),
        "https://github.com/niklasmarderx/Ember".cyan()
    );

    Ok(())
}

fn rustc_version() -> &'static str {
    env!("CARGO_PKG_RUST_VERSION")
}
