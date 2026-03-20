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

mod commands;
mod config;
mod error_display;

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
shell commands, access the filesystem, and fetch web content.",
        after_help = "Examples:
  ember chat \"Explain Rust ownership\"
  ember chat --model gpt-4
  ember chat --provider ollama
  ember chat --tools shell,filesystem"
    )]
    Chat {
        /// Message to send (omit for interactive mode)
        message: Option<String>,

        /// LLM provider to use (openai, ollama)
        #[arg(short, long)]
        provider: Option<String>,

        /// Model to use (overrides config)
        #[arg(short, long)]
        model: Option<String>,

        /// System prompt (overrides config)
        #[arg(short, long)]
        system: Option<String>,

        /// Temperature (0.0 - 2.0)
        #[arg(short, long)]
        temperature: Option<f32>,

        /// Disable streaming output
        #[arg(long)]
        no_stream: bool,

        /// Enable tools (comma-separated: shell,filesystem,web)
        #[arg(long, value_delimiter = ',')]
        tools: Option<Vec<String>>,
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
    Show,

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

#[tokio::main]
async fn main() {
    // Run the actual main function and handle errors gracefully
    if let Err(e) = run().await {
        error_display::display_error(&e);
        std::process::exit(1);
    }
}

/// The actual main function that returns a Result
async fn run() -> Result<()> {
    let cli = Cli::parse();

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
            no_stream,
            tools,
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
            ConfigAction::Show => {
                config_cmd::show(&config)?;
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
