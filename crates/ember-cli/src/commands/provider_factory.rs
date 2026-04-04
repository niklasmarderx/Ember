//! Provider and tool registry factory functions.
//!
//! Centralises the creation of LLM providers and tool registries so that
//! both `chat.rs` and `config.rs` can share the same logic.

use anyhow::{Context, Result};
use colored::Colorize;
use ember_llm::{
    AnthropicProvider, BedrockProvider, DeepSeekProvider, GeminiProvider,
    GroqProvider, LLMProvider, MistralProvider, OllamaProvider, OpenAIProvider,
    OpenRouterProvider, XAIProvider,
};
use ember_tools::{FilesystemTool, GitTool, GlobTool, GrepTool, ShellTool, ToolRegistry, WebTool};
use std::sync::Arc;
use tracing::{info, warn};

use crate::config::AppConfig;

/// Build a default registry with available tools, respecting config flags.
pub fn create_default_tool_registry(config: &AppConfig) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    if config.tools.shell_enabled {
        registry.register(ShellTool::new());
    }
    if config.tools.filesystem_enabled {
        registry.register(FilesystemTool::new());
        registry.register(GrepTool::new());
        registry.register(GlobTool::new());
    }
    if config.tools.web_enabled {
        registry.register(WebTool::new());
    }
    // Git and browser don't have config flags yet — always register
    registry.register(GitTool::new());
    #[cfg(feature = "browser")]
    registry.register(ember_browser::BrowserTool::new());
    let count = registry.llm_tool_definitions().len();
    info!("Auto-registered {} tools", count);
    registry
}

/// Build a registry of enabled tools.
pub fn create_tool_registry(tool_names: &[String]) -> Result<ToolRegistry> {
    let mut registry = ToolRegistry::new();

    for name in tool_names {
        match name.to_lowercase().as_str() {
            "shell" => {
                info!("Registering shell tool");
                registry.register(ShellTool::new());
            }
            "filesystem" | "fs" => {
                info!("Registering filesystem, grep, glob tools");
                registry.register(FilesystemTool::new());
                registry.register(GrepTool::new());
                registry.register(GlobTool::new());
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
                #[cfg(feature = "browser")]
                {
                    info!("Registering browser tool");
                    registry.register(ember_browser::BrowserTool::new());
                }
                #[cfg(not(feature = "browser"))]
                {
                    warn!("Browser tool requires --features browser");
                    eprintln!(
                        "{} Browser tool not available. Recompile with: --features browser",
                        "[warn]".bright_yellow(),
                    );
                }
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
pub fn check_provider_key(
    provider_name: &str,
    config: &AppConfig,
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
