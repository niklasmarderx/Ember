//! Configuration command implementations for Ember CLI.
//!
//! This module provides commands to manage the Ember configuration file.
//!
//! The configuration stores settings such as:
//! - default LLM provider
//! - model settings
//! - API keys
//! - agent behavior
//! - enabled tools
//!
//! Configuration is stored in a TOML file in the user's config directory.
//!
//! Examples:
//!
//! Initialize configuration:
//! ```bash
//! ember config init
//! ```
//!
//! Show configuration:
//! ```bash
//! ember config show
//! ```
//!
//! Set a value:
//! ```bash
//! ember config set provider.default openai
//! ```
//!
//! Get a value:
//! ```bash
//! ember config get provider.default
//! ```

use crate::config::AppConfig;
use anyhow::{Context, Result};
use colored::Colorize;

/// Initialize a new Ember configuration file.
///
/// This creates a default configuration file if one does not exist.
///
/// Example:
/// ```bash
/// ember config init
/// ```
///
/// Use `--force` to overwrite an existing configuration:
///
/// ```bash
/// ember config init --force
/// ```
pub fn init(force: bool) -> Result<()> {
    let config_path = AppConfig::config_path()?;

    if config_path.exists() && !force {
        println!(
            "{} Configuration file already exists at:",
            "[!]".bright_yellow()
        );
        println!("   {}", config_path.display().to_string().bright_blue());
        println!();
        println!("Use {} to overwrite.", "--force".bright_cyan());
        return Ok(());
    }

    let config = AppConfig::default();
    config.save(None)?;

    println!("{} Configuration file created at:", "[OK]".bright_green());
    println!("   {}", config_path.display().to_string().bright_blue());
    println!();
    println!("{}", "Default settings:".bright_yellow());
    println!("   Provider: {}", config.provider.default.bright_green());
    println!("   Model: {}", config.provider.openai.model.bright_green());
    println!(
        "   Temperature: {}",
        config.agent.temperature.to_string().bright_green()
    );
    println!();
    println!("{}", "Next steps:".bright_yellow().bold());
    println!("   1. Set your OpenAI API key:");
    println!(
        "      {} provider.openai.api_key YOUR_API_KEY",
        "ember config set".bright_cyan()
    );
    println!("   2. Or use Ollama (local models):");
    println!(
        "      {} provider.default ollama",
        "ember config set".bright_cyan()
    );
    println!();

    Ok(())
}

/// Display the current configuration.
///
/// Shows provider settings, agent configuration,
/// and enabled tools in a readable format.
///
/// Example:
/// ```bash
/// ember config show
/// ```
pub fn show(config: &AppConfig, json: bool) -> Result<()> {
    if json {
        println!("{}", format_json(config)?);
        return Ok(());
    }

    println!("{}", "Ember Configuration".bright_yellow().bold());
    println!();

    // Provider section
    println!("{}", "[provider]".bright_blue().bold());
    println!(
        "  default = {}",
        format!("\"{}\"", config.provider.default).bright_green()
    );
    println!();

    println!("{}", "[provider.openai]".bright_blue());
    println!(
        "  model = {}",
        format!("\"{}\"", config.provider.openai.model).bright_green()
    );
    if config.provider.openai.api_key.is_some() {
        println!("  api_key = {}", "\"***\"".bright_green());
    } else {
        println!(
            "  api_key = {} (using env var OPENAI_API_KEY)",
            "not set".bright_yellow()
        );
    }
    if let Some(ref url) = config.provider.openai.base_url {
        println!("  base_url = {}", format!("\"{}\"", url).bright_green());
    }
    println!();

    println!("{}", "[provider.ollama]".bright_blue());
    println!(
        "  url = {}",
        format!("\"{}\"", config.provider.ollama.url).bright_green()
    );
    println!(
        "  model = {}",
        format!("\"{}\"", config.provider.ollama.model).bright_green()
    );
    println!();

    // Agent section
    println!("{}", "[agent]".bright_blue().bold());
    println!(
        "  system_prompt = {}",
        format!(
            "\"{}...\"",
            config
                .agent
                .system_prompt
                .chars()
                .take(50)
                .collect::<String>()
        )
        .bright_green()
    );
    println!(
        "  temperature = {}",
        config.agent.temperature.to_string().bright_green()
    );
    println!(
        "  max_iterations = {}",
        config.agent.max_iterations.to_string().bright_green()
    );
    println!(
        "  streaming = {}",
        config.agent.streaming.to_string().bright_green()
    );
    println!();

    // Tools section
    println!("{}", "[tools]".bright_blue().bold());
    println!(
        "  shell_enabled = {}",
        config.tools.shell_enabled.to_string().bright_green()
    );
    println!(
        "  filesystem_enabled = {}",
        config.tools.filesystem_enabled.to_string().bright_green()
    );
    println!(
        "  web_enabled = {}",
        config.tools.web_enabled.to_string().bright_green()
    );
    println!(
        "  shell_timeout = {}",
        config.tools.shell_timeout.to_string().bright_green()
    );

    Ok(())
}

fn format_json(config: &AppConfig) -> Result<String> {
    let mut sanitized = config.clone();
    if sanitized.provider.openai.api_key.is_some() {
        sanitized.provider.openai.api_key = Some("***".to_string());
    }

    Ok(serde_json::to_string_pretty(&sanitized)?)
}

/// Set a configuration key to a new value.
///
/// Example:
/// ```bash
/// ember config set provider.default ollama
/// ember config set agent.temperature 0.7
/// ```
pub fn set(key: &str, value: &str) -> Result<()> {
    let mut config = AppConfig::load(None)?;

    config
        .set(key, value)
        .context(format!("Failed to set configuration key: {}", key))?;

    config.save(None)?;

    println!(
        "{} Set {} = {}",
        "[OK]".bright_green(),
        key.bright_blue(),
        value.bright_green()
    );

    Ok(())
}

/// Retrieve a configuration value.
///
/// Example:
/// ```bash
/// ember config get provider.default
/// ```
pub fn get(config: &AppConfig, key: &str) -> Result<()> {
    match config.get(key) {
        Some(value) => {
            println!("{} = {}", key.bright_blue(), value.bright_green());
        }
        None => {
            println!(
                "{} Unknown configuration key: {}",
                "[!]".bright_yellow(),
                key.bright_red()
            );
            println!();
            println!("{}", "Available keys:".bright_yellow());
            print_available_keys();
        }
    }

    Ok(())
}

/// Display the location of the Ember configuration file.
///
/// Example:
/// ```bash
/// ember config path
/// ```
pub fn path() -> Result<()> {
    let config_path = AppConfig::config_path()?;

    println!("{}", "Configuration file path:".bright_yellow());
    println!("  {}", config_path.display().to_string().bright_blue());

    if config_path.exists() {
        println!("  Status: {}", "exists".bright_green());
    } else {
        println!("  Status: {}", "not created".bright_yellow());
        println!();
        println!("Run {} to create it.", "ember config init".bright_cyan());
    }

    Ok(())
}

/// Print all supported configuration keys.
///
/// This helps users discover valid keys when using
/// `ember config set` or `ember config get`.
fn print_available_keys() {
    let keys = [
        ("provider.default", "Default LLM provider (openai, ollama)"),
        ("provider.openai.model", "OpenAI model name"),
        ("provider.openai.api_key", "OpenAI API key"),
        ("provider.ollama.url", "Ollama server URL"),
        ("provider.ollama.model", "Ollama model name"),
        ("agent.system_prompt", "System prompt"),
        ("agent.temperature", "Temperature (0.0 - 2.0)"),
        ("agent.max_iterations", "Max iterations in agent loop"),
        ("agent.streaming", "Enable streaming responses"),
        ("tools.shell_enabled", "Enable shell tool"),
        ("tools.filesystem_enabled", "Enable filesystem tool"),
        ("tools.web_enabled", "Enable web tool"),
    ];

    for (key, description) in keys {
        println!("  {} - {}", key.bright_cyan(), description.bright_white());
    }
}
