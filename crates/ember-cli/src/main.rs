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
use clap::{Parser, Subcommand};
use colored::Colorize;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod commands;
mod config;
mod error_display;

#[cfg(feature = "tui")]
mod tui;

use commands::{chat, config as config_cmd, serve};
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

    #[command(subcommand)]
    command: Commands,
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

    init_logging(cli.verbose)?;

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
    }

    Ok(())
}

fn init_logging(verbose: bool) -> Result<()> {
    let filter = if verbose {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"))
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

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
    println!("  • OpenAI (GPT-4, GPT-3.5)");
    println!("  • Ollama (Local models)");
    println!();
    println!("{}", "Configuration:".bright_blue());
    println!(
        "  {}",
        config::AppConfig::config_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "Not found".to_string())
    );

    Ok(())
}

fn rustc_version() -> &'static str {
    env!("CARGO_PKG_RUST_VERSION")
}
