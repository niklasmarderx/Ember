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

// ─────────────────────────────────────────────────────────────────────────────
// Interactive Setup Wizard
// ─────────────────────────────────────────────────────────────────────────────

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal,
};
use std::io::{self, Write};

/// Read a line of input in raw mode, masking characters with `*`.
fn raw_input_masked(prompt: &str) -> String {
    terminal::enable_raw_mode().ok();
    print!("\r\n   {} ", prompt.bright_cyan());
    io::stdout().flush().ok();

    let mut buf = String::new();
    loop {
        if let Ok(Event::Key(KeyEvent {
            code, modifiers, ..
        })) = event::read()
        {
            if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
                terminal::disable_raw_mode().ok();
                println!();
                std::process::exit(0);
            }
            match code {
                KeyCode::Enter => {
                    print!("\r\n");
                    io::stdout().flush().ok();
                    break;
                }
                KeyCode::Backspace => {
                    if !buf.is_empty() {
                        buf.pop();
                        print!("\x08 \x08");
                        io::stdout().flush().ok();
                    }
                }
                KeyCode::Char(c) => {
                    buf.push(c);
                    print!("*");
                    io::stdout().flush().ok();
                }
                _ => {}
            }
        }
    }
    terminal::disable_raw_mode().ok();
    buf.trim().to_string()
}

/// Interactive setup wizard — guides users through first-time configuration.
pub async fn init_interactive(force: bool) -> Result<()> {
    use crate::onboarding::{interactive_menu, raw_input};

    let config_path = AppConfig::config_path()?;
    if config_path.exists() && !force {
        println!(
            "\n  {} Config already exists at {}",
            "!".bright_yellow(),
            config_path.display().to_string().bright_blue()
        );
        let overwrite = raw_input("Overwrite? (y/N)", "n");
        if !overwrite.eq_ignore_ascii_case("y") {
            println!("  {}", "Keeping existing config.".dimmed());
            return Ok(());
        }
    }

    // Welcome
    println!();
    println!(
        "  {}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_yellow()
    );
    println!(
        "  {}  {}",
        "Ember Setup".bright_yellow().bold(),
        "— let's get you chatting in 30 seconds".dimmed()
    );
    println!(
        "  {}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_yellow()
    );

    // Step 1: Provider selection
    let providers = &[
        "OpenAI",
        "Anthropic",
        "Ollama (local, no API key)",
        "Groq",
        "Google Gemini",
        "DeepSeek",
        "Mistral",
        "OpenRouter",
        "xAI (Grok)",
        "Custom OpenAI-compatible endpoint",
    ];
    let provider_idx = interactive_menu("Choose your AI provider:", providers, 0);

    let (provider_name, env_var, default_model) = match provider_idx {
        0 => ("openai", "OPENAI_API_KEY", "gpt-4o"),
        1 => ("anthropic", "ANTHROPIC_API_KEY", "claude-sonnet-4-20250514"),
        2 => ("ollama", "", "llama3.2"),
        3 => ("groq", "GROQ_API_KEY", "llama-3.3-70b-versatile"),
        4 => ("gemini", "GOOGLE_API_KEY", "gemini-2.0-flash"),
        5 => ("deepseek", "DEEPSEEK_API_KEY", "deepseek-chat"),
        6 => ("mistral", "MISTRAL_API_KEY", "mistral-large-latest"),
        7 => (
            "openrouter",
            "OPENROUTER_API_KEY",
            "anthropic/claude-sonnet-4-20250514",
        ),
        8 => ("xai", "XAI_API_KEY", "grok-3"),
        9 => ("openai", "OPENAI_API_KEY", "gpt-4o"), // custom = openai-compat
        _ => ("openai", "OPENAI_API_KEY", "gpt-4o"),
    };

    // Step 2: API key (skip for Ollama)
    let mut api_key: Option<String> = None;
    let mut base_url: Option<String> = None;

    if provider_idx == 2 {
        // Ollama — no key needed
        println!(
            "\n  {} Ollama detected — no API key needed.",
            "OK".bright_green()
        );
        println!(
            "  {} Make sure Ollama is running: {}",
            "tip:".dimmed(),
            "ollama serve".bright_cyan()
        );
    } else {
        // Check if env var is already set
        let env_key = std::env::var(env_var).ok().filter(|v| !v.is_empty());
        if let Some(ref key) = env_key {
            let masked = if key.len() > 8 {
                format!("{}...{}", &key[..4], &key[key.len() - 4..])
            } else {
                "****".to_string()
            };
            println!(
                "\n  {} {} already set ({})",
                "OK".bright_green(),
                env_var.bright_cyan(),
                masked.dimmed()
            );
            let use_env = raw_input("Use this key? (Y/n)", "y");
            if use_env.eq_ignore_ascii_case("n") {
                let key_input = raw_input_masked("Enter your API key:");
                if !key_input.is_empty() {
                    api_key = Some(key_input);
                }
            }
        } else {
            let key_input = raw_input_masked("Enter your API key:");
            if !key_input.is_empty() {
                api_key = Some(key_input);
            } else {
                println!(
                    "  {} No key entered. You can set {} later.",
                    "!".bright_yellow(),
                    env_var.bright_cyan()
                );
            }
        }

        // Custom endpoint
        if provider_idx == 9 {
            let url = raw_input("Base URL (e.g. http://localhost:8080/v1):", "");
            if !url.is_empty() {
                base_url = Some(url);
            }
        }
    }

    // Step 3: Model selection
    let model = raw_input(&format!("Model [{}]:", default_model), default_model);

    // Step 4: Build and save config
    let mut config = AppConfig::default();
    config.provider.default = provider_name.to_string();

    match provider_name {
        "ollama" => {
            config.provider.ollama.model = model;
        }
        _ => {
            config.provider.openai.model = model.clone();
            if let Some(key) = api_key {
                config.provider.openai.api_key = Some(key);
            }
            if let Some(url) = base_url {
                config.provider.openai.base_url = Some(url);
            }
        }
    }

    config.save(None)?;

    let saved_path = AppConfig::config_path()?;
    println!(
        "\n  {} Config saved to {}",
        "OK".bright_green(),
        saved_path.display().to_string().bright_blue()
    );

    // Step 5: Offer a test
    println!();
    let test = raw_input("Send a test message? (Y/n)", "y");
    if !test.eq_ignore_ascii_case("n") {
        println!("\n  {} Sending test message...\n", ">".bright_green());
        // Use create_provider + a simple completion
        match crate::commands::provider_factory::create_provider(&config, provider_name) {
            Ok(provider) => {
                use ember_llm::{CompletionRequest, Message, Role};
                let req = CompletionRequest {
                    model: if provider_name == "ollama" {
                        config.provider.ollama.model.clone()
                    } else {
                        config.provider.openai.model.clone()
                    },
                    messages: vec![Message {
                        role: Role::User,
                        content: "Say hello in one sentence and confirm you're working!"
                            .to_string(),
                        content_parts: vec![],
                        name: None,
                        tool_calls: vec![],
                        tool_call_id: None,
                    }],
                    temperature: Some(0.7),
                    max_tokens: Some(100),
                    tools: None,
                    top_p: None,
                    stop: None,
                    stream: Some(false),
                    extra: Default::default(),
                };
                match provider.complete(req).await {
                    Ok(resp) => {
                        println!(
                            "  {} {}",
                            "AI:".bright_green().bold(),
                            resp.content.bright_white()
                        );
                        println!(
                            "\n  {} You're all set! Run {} to start chatting.",
                            "OK".bright_green().bold(),
                            "ember chat".bright_cyan()
                        );
                    }
                    Err(e) => {
                        println!(
                            "  {} Test failed: {}",
                            "ERR".bright_red(),
                            e.to_string().bright_red()
                        );
                        println!(
                            "  {} Check your API key and try {}",
                            "tip:".dimmed(),
                            "ember init".bright_cyan()
                        );
                    }
                }
            }
            Err(e) => {
                println!(
                    "  {} Could not create provider: {}",
                    "ERR".bright_red(),
                    e.to_string().bright_red()
                );
            }
        }
    } else {
        println!(
            "\n  {} Setup complete! Run {} to start chatting.",
            "OK".bright_green().bold(),
            "ember chat".bright_cyan()
        );
    }

    println!();
    Ok(())
}
