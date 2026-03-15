//! Application configuration management.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// LLM provider configuration
    #[serde(default)]
    pub provider: ProviderConfig,

    /// Agent configuration
    #[serde(default)]
    pub agent: AgentSettings,

    /// Tool configuration
    #[serde(default)]
    pub tools: ToolSettings,
}

/// LLM provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Default provider (openai, ollama)
    #[serde(default = "default_provider")]
    pub default: String,

    /// OpenAI settings
    #[serde(default)]
    pub openai: OpenAISettings,

    /// Ollama settings
    #[serde(default)]
    pub ollama: OllamaSettings,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            default: default_provider(),
            openai: OpenAISettings::default(),
            ollama: OllamaSettings::default(),
        }
    }
}

fn default_provider() -> String {
    "openai".to_string()
}

/// OpenAI provider settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAISettings {
    /// API key (can also use OPENAI_API_KEY env var)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Default model
    #[serde(default = "default_openai_model")]
    pub model: String,

    /// API base URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

impl Default for OpenAISettings {
    fn default() -> Self {
        Self {
            api_key: None,
            model: default_openai_model(),
            base_url: None,
        }
    }
}

fn default_openai_model() -> String {
    "gpt-4o".to_string()
}

/// Ollama provider settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaSettings {
    /// Ollama server URL
    #[serde(default = "default_ollama_url")]
    pub url: String,

    /// Default model
    #[serde(default = "default_ollama_model")]
    pub model: String,
}

impl Default for OllamaSettings {
    fn default() -> Self {
        Self {
            url: default_ollama_url(),
            model: default_ollama_model(),
        }
    }
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_ollama_model() -> String {
    "llama3.2".to_string()
}

/// Agent settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettings {
    /// System prompt
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,

    /// Temperature (0.0 - 2.0)
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Maximum iterations in agent loop
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,

    /// Enable streaming responses
    #[serde(default = "default_true")]
    pub streaming: bool,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            system_prompt: default_system_prompt(),
            temperature: default_temperature(),
            max_iterations: default_max_iterations(),
            streaming: true,
        }
    }
}

fn default_system_prompt() -> String {
    "You are Ember, a helpful AI assistant. You are concise, accurate, and friendly.".to_string()
}

fn default_temperature() -> f32 {
    0.7
}

fn default_max_iterations() -> usize {
    10
}

fn default_true() -> bool {
    true
}

/// Tool settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSettings {
    /// Enable shell tool
    #[serde(default = "default_true")]
    pub shell_enabled: bool,

    /// Enable filesystem tool
    #[serde(default = "default_true")]
    pub filesystem_enabled: bool,

    /// Enable web tool
    #[serde(default = "default_true")]
    pub web_enabled: bool,

    /// Allowed paths for filesystem tool
    #[serde(default)]
    pub allowed_paths: Vec<String>,

    /// Shell command timeout in seconds
    #[serde(default = "default_shell_timeout")]
    pub shell_timeout: u64,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            shell_enabled: true,
            filesystem_enabled: true,
            web_enabled: true,
            allowed_paths: Vec::new(),
            shell_timeout: default_shell_timeout(),
        }
    }
}

fn default_shell_timeout() -> u64 {
    30
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: ProviderConfig::default(),
            agent: AgentSettings::default(),
            tools: ToolSettings::default(),
        }
    }
}

impl AppConfig {
    /// Get the configuration file path.
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Could not find config directory")?
            .join("ember");
        Ok(config_dir.join("config.toml"))
    }

    /// Load configuration from file or use defaults.
    pub fn load(path: Option<&str>) -> Result<Self> {
        let config_path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            Self::config_path()?
        };

        if config_path.exists() {
            let content =
                std::fs::read_to_string(&config_path).context("Failed to read config file")?;
            let config: Self = toml::from_str(&content).context("Failed to parse config file")?;
            Ok(config)
        } else {
            // Return defaults if no config file exists
            Ok(Self::default())
        }
    }

    /// Save configuration to file.
    pub fn save(&self, path: Option<&str>) -> Result<()> {
        let config_path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            Self::config_path()?
        };

        // Create parent directories if needed
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create config directory")?;
        }

        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;
        std::fs::write(&config_path, content).context("Failed to write config file")?;

        Ok(())
    }

    /// Get a configuration value by key.
    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "provider.default" => Some(self.provider.default.clone()),
            "provider.openai.model" => Some(self.provider.openai.model.clone()),
            "provider.openai.api_key" => self.provider.openai.api_key.clone(),
            "provider.ollama.url" => Some(self.provider.ollama.url.clone()),
            "provider.ollama.model" => Some(self.provider.ollama.model.clone()),
            "agent.system_prompt" => Some(self.agent.system_prompt.clone()),
            "agent.temperature" => Some(self.agent.temperature.to_string()),
            "agent.max_iterations" => Some(self.agent.max_iterations.to_string()),
            "agent.streaming" => Some(self.agent.streaming.to_string()),
            "tools.shell_enabled" => Some(self.tools.shell_enabled.to_string()),
            "tools.filesystem_enabled" => Some(self.tools.filesystem_enabled.to_string()),
            "tools.web_enabled" => Some(self.tools.web_enabled.to_string()),
            _ => None,
        }
    }

    /// Set a configuration value by key.
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "provider.default" => self.provider.default = value.to_string(),
            "provider.openai.model" => self.provider.openai.model = value.to_string(),
            "provider.openai.api_key" => self.provider.openai.api_key = Some(value.to_string()),
            "provider.ollama.url" => self.provider.ollama.url = value.to_string(),
            "provider.ollama.model" => self.provider.ollama.model = value.to_string(),
            "agent.system_prompt" => self.agent.system_prompt = value.to_string(),
            "agent.temperature" => {
                self.agent.temperature = value.parse().context("Invalid temperature value")?;
            }
            "agent.max_iterations" => {
                self.agent.max_iterations =
                    value.parse().context("Invalid max_iterations value")?;
            }
            "agent.streaming" => {
                self.agent.streaming = value.parse().context("Invalid streaming value")?;
            }
            "tools.shell_enabled" => {
                self.tools.shell_enabled = value.parse().context("Invalid shell_enabled value")?;
            }
            "tools.filesystem_enabled" => {
                self.tools.filesystem_enabled =
                    value.parse().context("Invalid filesystem_enabled value")?;
            }
            "tools.web_enabled" => {
                self.tools.web_enabled = value.parse().context("Invalid web_enabled value")?;
            }
            _ => anyhow::bail!("Unknown configuration key: {}", key),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.provider.default, "openai");
        assert_eq!(config.agent.temperature, 0.7);
        assert!(config.tools.shell_enabled);
    }

    #[test]
    fn test_config_get_set() {
        let mut config = AppConfig::default();

        config.set("agent.temperature", "0.5").unwrap();
        assert_eq!(config.get("agent.temperature"), Some("0.5".to_string()));

        config.set("provider.openai.model", "gpt-4").unwrap();
        assert_eq!(
            config.get("provider.openai.model"),
            Some("gpt-4".to_string())
        );
    }

    #[test]
    fn test_config_serialization() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("[provider]"));
        assert!(toml_str.contains("[agent]"));
    }
}
