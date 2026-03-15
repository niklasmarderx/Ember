//! Ember CLI - Command-line interface for the Ember AI agent.
//!
//! Usage:
//!   ember chat "Hello, world!"           # One-shot chat
//!   ember chat                           # Interactive chat mode
//!   ember config init                    # Initialize configuration
//!   ember config show                    # Show current configuration

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod commands;
mod config;

#[cfg(feature = "tui")]
mod tui;

use commands::{chat, config as config_cmd, serve};
use config::AppConfig;

/// Ember - Blazing fast AI agent in Rust
#[derive(Parser)]
#[command(name = "ember")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
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

    /// Chat with the AI agent
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

    /// Run a single command and exit
    Run {
        /// The task to execute
        task: String,

        /// Model to use
        #[arg(short, long)]
        model: Option<String>,
    },

    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Show version and system information
    Info,

    /// Start the web server
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
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    init_logging(cli.verbose)?;

    // Load configuration
    let config = AppConfig::load(cli.config.as_deref()).context("Failed to load configuration")?;

    // Execute command
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

/// Initialize logging based on verbosity level.
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

/// Print version and system information.
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

/// Get rustc version (compile-time).
fn rustc_version() -> &'static str {
    env!("CARGO_PKG_RUST_VERSION")
}
