//! `ember providers` — list available LLM providers and their status.

use colored::Colorize;

struct ProviderInfo {
    name: &'static str,
    description: &'static str,
    env_var: Option<&'static str>,
    local: bool,
    default_model: &'static str,
    docs_url: &'static str,
}

const PROVIDERS: &[ProviderInfo] = &[
    ProviderInfo {
        name: "openai",
        description: "GPT-4o, GPT-4 Turbo, o1, and more",
        env_var: Some("OPENAI_API_KEY"),
        local: false,
        default_model: "gpt-4o",
        docs_url: "https://platform.openai.com/api-keys",
    },
    ProviderInfo {
        name: "anthropic",
        description: "Claude 3.5, Claude 3 Opus, Haiku",
        env_var: Some("ANTHROPIC_API_KEY"),
        local: false,
        default_model: "claude-3-5-sonnet-20241022",
        docs_url: "https://console.anthropic.com/settings/keys",
    },
    ProviderInfo {
        name: "ollama",
        description: "Local models — Llama, Mistral, DeepSeek, Qwen",
        env_var: None,
        local: true,
        default_model: "llama3.2",
        docs_url: "https://ollama.com",
    },
    ProviderInfo {
        name: "gemini",
        description: "Gemini 1.5 Pro, Flash — Google AI",
        env_var: Some("GOOGLE_API_KEY"),
        local: false,
        default_model: "gemini-1.5-pro",
        docs_url: "https://aistudio.google.com/apikey",
    },
    ProviderInfo {
        name: "groq",
        description: "LPU inference — Llama 3, Mixtral at high speed",
        env_var: Some("GROQ_API_KEY"),
        local: false,
        default_model: "llama-3.3-70b-versatile",
        docs_url: "https://console.groq.com/keys",
    },
    ProviderInfo {
        name: "deepseek",
        description: "DeepSeek Chat and Coder models",
        env_var: Some("DEEPSEEK_API_KEY"),
        local: false,
        default_model: "deepseek-chat",
        docs_url: "https://platform.deepseek.com/api_keys",
    },
    ProviderInfo {
        name: "mistral",
        description: "Mistral Large, Medium, and Mixtral",
        env_var: Some("MISTRAL_API_KEY"),
        local: false,
        default_model: "mistral-large-latest",
        docs_url: "https://console.mistral.ai/api-keys",
    },
    ProviderInfo {
        name: "openrouter",
        description: "200+ models via a single API key",
        env_var: Some("OPENROUTER_API_KEY"),
        local: false,
        default_model: "anthropic/claude-3.5-sonnet",
        docs_url: "https://openrouter.ai/keys",
    },
    ProviderInfo {
        name: "xai",
        description: "Grok — xAI's language model",
        env_var: Some("XAI_API_KEY"),
        local: false,
        default_model: "grok-beta",
        docs_url: "https://console.x.ai",
    },
    ProviderInfo {
        name: "bedrock",
        description: "AWS Bedrock — Claude, Llama, Titan via AWS",
        env_var: Some("AWS_ACCESS_KEY_ID"),
        local: false,
        default_model: "anthropic.claude-3-5-sonnet-20241022-v2:0",
        docs_url: "https://docs.aws.amazon.com/bedrock",
    },
];

fn key_status(env_var: Option<&'static str>) -> (&'static str, &'static str) {
    match env_var {
        None => ("local", "no key needed"),
        Some(var) => {
            if std::env::var(var).ok().filter(|v| !v.is_empty()).is_some() {
                ("ready", var)
            } else {
                ("no key", var)
            }
        }
    }
}

pub fn list_providers() {
    println!();
    println!("{}", "Available providers".bright_white().bold());
    println!();

    let ready_count = PROVIDERS
        .iter()
        .filter(|p| {
            p.local
                || p.env_var
                    .map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()).is_some())
                    .unwrap_or(false)
        })
        .count();

    for provider in PROVIDERS {
        let (status_label, status_detail) = key_status(provider.env_var);

        let status_display = match status_label {
            "ready" | "local" => format!(" {} ", status_label)
                .bright_white()
                .on_green()
                .bold(),
            _ => format!(" {} ", status_label).bright_white().on_red().bold(),
        };

        println!(
            "  {} {}",
            status_display,
            provider.name.bright_white().bold()
        );
        println!("         {}", provider.description.dimmed());
        println!(
            "         default model: {}",
            provider.default_model.bright_cyan()
        );

        if provider.local {
            println!(
                "         {} no API key needed — runs on your machine",
                "○".bright_green()
            );
        } else if status_label == "ready" {
            println!("         {} {} is set", "●".bright_green(), status_detail);
        } else {
            println!(
                "         {} {} not found — get a key at {}",
                "○".bright_red(),
                status_detail,
                provider.docs_url.bright_blue()
            );
        }
        println!();
    }

    println!(
        "  {}/{} providers ready",
        ready_count.to_string().bright_green().bold(),
        PROVIDERS.len()
    );
    println!();
    println!(
        "  Use with:  {}",
        "ember chat --provider <name>".bright_cyan()
    );
    println!(
        "  Configure: {}",
        "ember config set provider.default <name>".bright_cyan()
    );
    println!();
}
