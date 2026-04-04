//! Application configuration management.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::path::PathBuf;

/// Configuration validation error with helpful suggestions.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ConfigValidationError {
    /// The field that has an invalid value
    pub field: String,
    /// Description of what's wrong
    pub message: String,
    /// The current value (if displayable)
    pub current_value: Option<String>,
    /// Suggestion for fixing the issue
    pub suggestion: String,
    /// Line number in config file (if available)
    pub line: Option<usize>,
}

impl fmt::Display for ConfigValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  Field: {}", self.field)?;
        if let Some(line) = self.line {
            writeln!(f, "  Line:  {}", line)?;
        }
        writeln!(f, "  Error: {}", self.message)?;
        if let Some(ref value) = self.current_value {
            writeln!(f, "  Got:   {}", value)?;
        }
        writeln!(f, "  Fix:   {}", self.suggestion)
    }
}

/// Configuration validation warning (non-fatal).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ConfigValidationWarning {
    /// Description of the warning
    pub message: String,
    /// Suggestion for addressing the warning
    pub suggestion: String,
}

impl fmt::Display for ConfigValidationWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  Warning: {}", self.message)?;
        writeln!(f, "  Suggestion: {}", self.suggestion)
    }
}

/// Result of configuration validation.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct ValidationResult {
    /// List of validation errors
    pub errors: Vec<ConfigValidationError>,
    /// List of validation warnings
    pub warnings: Vec<ConfigValidationWarning>,
}

#[allow(dead_code)]
impl ValidationResult {
    /// Create a new empty validation result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if validation passed (no errors).
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Check if there are any warnings.
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Add an error.
    pub fn add_error(&mut self, error: ConfigValidationError) {
        self.errors.push(error);
    }

    /// Add a warning.
    pub fn add_warning(&mut self, warning: ConfigValidationWarning) {
        self.warnings.push(warning);
    }

    /// Format errors and warnings for display.
    pub fn format_report(&self, config_path: Option<&str>) -> String {
        let mut report = String::new();

        if !self.errors.is_empty() {
            if let Some(path) = config_path {
                report.push_str(&format!("Configuration Errors in {}\n", path));
            } else {
                report.push_str("Configuration Errors\n");
            }
            report.push_str(&"=".repeat(50));
            report.push('\n');

            for (i, error) in self.errors.iter().enumerate() {
                report.push_str(&format!("\n[Error {}]\n{}", i + 1, error));
            }
        }

        if !self.warnings.is_empty() {
            if !report.is_empty() {
                report.push('\n');
            }
            report.push_str("Configuration Warnings\n");
            report.push_str(&"-".repeat(50));
            report.push('\n');

            for warning in &self.warnings {
                report.push_str(&format!("\n{}", warning));
            }
        }

        report
    }
}

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

    /// Maximum LLM retry attempts on transient failures (default: 3)
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Enable parallel tool execution (default: true)
    #[serde(default = "default_true")]
    pub parallel_tools: bool,

    /// Start in compact response mode (default: false)
    #[serde(default)]
    pub compact_mode: bool,

    /// Token budget for auto-context injection (default: 4000)
    #[serde(default = "default_context_budget")]
    pub context_budget: usize,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            system_prompt: default_system_prompt(),
            temperature: default_temperature(),
            max_iterations: default_max_iterations(),
            streaming: true,
            max_retries: default_max_retries(),
            parallel_tools: true,
            compact_mode: false,
            context_budget: default_context_budget(),
        }
    }
}

fn default_system_prompt() -> String {
    r#"You are Ember, a powerful AI coding assistant running in the user's terminal.

You have access to tools that let you execute shell commands, read and write files, search codebases, and interact with git. Use these tools proactively to accomplish tasks.

## How to work:
1. When asked to build, fix, or modify code, USE your tools — don't just describe what to do.
2. Read files to understand the codebase before making changes.
3. Write complete file contents when creating or modifying files.
4. Run shell commands to install dependencies, build, test, and verify your changes work.
5. Use git to track changes when appropriate.
6. After making changes, verify they work by running the project's build/test commands.

## Tool usage patterns:
- **Read first**: Before editing a file, read it to understand the current state.
- **Write completely**: When writing files, provide the complete file content.
- **Verify changes**: After writing, run builds or tests to confirm correctness.
- **Be precise**: Use exact file paths relative to the working directory.

## Style:
- Be concise and direct. Show code, not explanations of code.
- When you encounter errors, fix them immediately rather than just reporting them.
- Provide working solutions, not theoretical advice."#.to_string()
}

fn default_temperature() -> f32 {
    0.7
}

fn default_max_iterations() -> usize {
    10
}

fn default_max_retries() -> u32 {
    3
}

fn default_context_budget() -> usize {
    4000
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

    /// Diff display verbosity: "full", "summary", "none" (default: "full")
    #[serde(default = "default_diff_verbosity")]
    pub diff_verbosity: String,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            shell_enabled: true,
            filesystem_enabled: true,
            web_enabled: true,
            allowed_paths: Vec::new(),
            shell_timeout: default_shell_timeout(),
            diff_verbosity: default_diff_verbosity(),
        }
    }
}

fn default_shell_timeout() -> u64 {
    30
}

fn default_diff_verbosity() -> String {
    "full".to_string()
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

/// Known valid provider names.
#[allow(dead_code)]
const VALID_PROVIDERS: &[&str] = &[
    "openai",
    "anthropic",
    "ollama",
    "groq",
    "gemini",
    "deepseek",
    "mistral",
    "openrouter",
    "xai",
    "bedrock",
];

/// Known valid configuration sections.
#[allow(dead_code)]
const VALID_SECTIONS: &[&str] = &["provider", "agent", "tools"];

#[allow(dead_code)]
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

    /// Validate the configuration and return detailed errors/warnings.
    pub fn validate(&self) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate provider configuration
        self.validate_provider(&mut result);

        // Validate agent settings
        self.validate_agent(&mut result);

        // Validate tool settings
        self.validate_tools(&mut result);

        result
    }

    /// Validate provider configuration.
    fn validate_provider(&self, result: &mut ValidationResult) {
        // Check if default provider is valid
        let provider_lower = self.provider.default.to_lowercase();
        if !VALID_PROVIDERS.contains(&provider_lower.as_str()) {
            result.add_error(ConfigValidationError {
                field: "provider.default".to_string(),
                message: format!("Unknown provider '{}'", self.provider.default),
                current_value: Some(self.provider.default.clone()),
                suggestion: format!(
                    "Use one of the supported providers: {}",
                    VALID_PROVIDERS.join(", ")
                ),
                line: None,
            });
        }

        // Validate OpenAI settings if OpenAI is the default provider
        if provider_lower == "openai" {
            // Check if API key is set (or available in environment)
            if self.provider.openai.api_key.is_none() && std::env::var("OPENAI_API_KEY").is_err() {
                result.add_warning(ConfigValidationWarning {
                    message: "OpenAI API key not configured".to_string(),
                    suggestion: "Set 'provider.openai.api_key' in config or OPENAI_API_KEY environment variable".to_string(),
                });
            }

            // Validate API key format if provided
            if let Some(ref key) = self.provider.openai.api_key {
                if key.trim().is_empty() {
                    result.add_error(ConfigValidationError {
                        field: "provider.openai.api_key".to_string(),
                        message: "API key cannot be empty".to_string(),
                        current_value: Some("(empty string)".to_string()),
                        suggestion: "Provide a valid OpenAI API key starting with 'sk-'"
                            .to_string(),
                        line: None,
                    });
                } else if !key.starts_with("sk-") && !key.starts_with("sess-") {
                    result.add_warning(ConfigValidationWarning {
                        message: "OpenAI API key has unusual format".to_string(),
                        suggestion: "OpenAI API keys typically start with 'sk-' or 'sess-'"
                            .to_string(),
                    });
                }
            }

            // Validate model name
            if self.provider.openai.model.trim().is_empty() {
                result.add_error(ConfigValidationError {
                    field: "provider.openai.model".to_string(),
                    message: "Model name cannot be empty".to_string(),
                    current_value: None,
                    suggestion: "Set a model like 'gpt-4o', 'gpt-4-turbo', or 'gpt-3.5-turbo'"
                        .to_string(),
                    line: None,
                });
            }

            // Validate base URL if provided
            if let Some(ref base_url) = self.provider.openai.base_url {
                if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
                    result.add_error(ConfigValidationError {
                        field: "provider.openai.base_url".to_string(),
                        message: "Invalid URL format".to_string(),
                        current_value: Some(base_url.clone()),
                        suggestion: "URL must start with 'http://' or 'https://'".to_string(),
                        line: None,
                    });
                }
            }
        }

        // Validate Ollama settings if Ollama is the default provider
        if provider_lower == "ollama" {
            // Validate Ollama URL
            if !self.provider.ollama.url.starts_with("http://")
                && !self.provider.ollama.url.starts_with("https://")
            {
                result.add_error(ConfigValidationError {
                    field: "provider.ollama.url".to_string(),
                    message: "Invalid URL format".to_string(),
                    current_value: Some(self.provider.ollama.url.clone()),
                    suggestion: "URL must start with 'http://' or 'https://', e.g., 'http://localhost:11434'".to_string(),
                    line: None,
                });
            }

            // Check if model is specified
            if self.provider.ollama.model.trim().is_empty() {
                result.add_error(ConfigValidationError {
                    field: "provider.ollama.model".to_string(),
                    message: "Model name cannot be empty".to_string(),
                    current_value: None,
                    suggestion: "Set a model like 'llama3.2', 'codellama', or 'mistral'"
                        .to_string(),
                    line: None,
                });
            }
        }

        // Validate Anthropic settings
        if provider_lower == "anthropic" && std::env::var("ANTHROPIC_API_KEY").is_err() {
            result.add_warning(ConfigValidationWarning {
                message: "Anthropic API key not configured".to_string(),
                suggestion: "Set ANTHROPIC_API_KEY environment variable".to_string(),
            });
        }

        // Validate Gemini settings
        if provider_lower == "gemini"
            && std::env::var("GOOGLE_API_KEY").is_err()
            && std::env::var("GEMINI_API_KEY").is_err()
        {
            result.add_warning(ConfigValidationWarning {
                message: "Google/Gemini API key not configured".to_string(),
                suggestion: "Set GOOGLE_API_KEY or GEMINI_API_KEY environment variable".to_string(),
            });
        }
    }

    /// Validate agent settings.
    fn validate_agent(&self, result: &mut ValidationResult) {
        // Validate temperature
        if self.agent.temperature < 0.0 {
            result.add_error(ConfigValidationError {
                field: "agent.temperature".to_string(),
                message: "Temperature cannot be negative".to_string(),
                current_value: Some(self.agent.temperature.to_string()),
                suggestion: "Set temperature between 0.0 and 2.0 (recommended: 0.7)".to_string(),
                line: None,
            });
        } else if self.agent.temperature > 2.0 {
            result.add_error(ConfigValidationError {
                field: "agent.temperature".to_string(),
                message: "Temperature too high".to_string(),
                current_value: Some(self.agent.temperature.to_string()),
                suggestion: "Set temperature between 0.0 and 2.0 (recommended: 0.7)".to_string(),
                line: None,
            });
        } else if self.agent.temperature > 1.5 {
            result.add_warning(ConfigValidationWarning {
                message: format!(
                    "High temperature ({}) may produce unpredictable responses",
                    self.agent.temperature
                ),
                suggestion:
                    "Consider using a lower temperature (0.5-1.0) for more consistent output"
                        .to_string(),
            });
        }

        // Validate max_iterations
        if self.agent.max_iterations == 0 {
            result.add_error(ConfigValidationError {
                field: "agent.max_iterations".to_string(),
                message: "max_iterations must be greater than 0".to_string(),
                current_value: Some("0".to_string()),
                suggestion: "Set max_iterations to at least 1 (recommended: 10)".to_string(),
                line: None,
            });
        } else if self.agent.max_iterations > 100 {
            result.add_warning(ConfigValidationWarning {
                message: format!(
                    "High max_iterations ({}) may cause long-running or expensive requests",
                    self.agent.max_iterations
                ),
                suggestion: "Consider a lower value (10-25) unless necessary".to_string(),
            });
        }

        // Validate system prompt
        if self.agent.system_prompt.trim().is_empty() {
            result.add_warning(ConfigValidationWarning {
                message: "Empty system prompt".to_string(),
                suggestion: "Consider adding a system prompt to guide the AI's behavior"
                    .to_string(),
            });
        } else if self.agent.system_prompt.len() > 10000 {
            result.add_warning(ConfigValidationWarning {
                message: "Very long system prompt may reduce available context for conversation"
                    .to_string(),
                suggestion: "Consider shortening the system prompt or using a model with larger context window".to_string(),
            });
        }
    }

    /// Validate tool settings.
    fn validate_tools(&self, result: &mut ValidationResult) {
        // Validate shell timeout
        if self.tools.shell_timeout == 0 {
            result.add_error(ConfigValidationError {
                field: "tools.shell_timeout".to_string(),
                message: "Shell timeout must be greater than 0".to_string(),
                current_value: Some("0".to_string()),
                suggestion: "Set shell_timeout to at least 1 second (recommended: 30)".to_string(),
                line: None,
            });
        } else if self.tools.shell_timeout > 3600 {
            result.add_warning(ConfigValidationWarning {
                message: format!(
                    "Very long shell timeout ({} seconds) may cause unresponsive behavior",
                    self.tools.shell_timeout
                ),
                suggestion: "Consider a shorter timeout (30-300 seconds)".to_string(),
            });
        }

        // Validate allowed paths
        for path in &self.tools.allowed_paths {
            let path_buf = PathBuf::from(path);
            if !path_buf.exists() {
                result.add_warning(ConfigValidationWarning {
                    message: format!("Allowed path does not exist: {}", path),
                    suggestion: "Ensure the path exists or remove it from allowed_paths"
                        .to_string(),
                });
            } else if !path_buf.is_dir() {
                result.add_warning(ConfigValidationWarning {
                    message: format!("Allowed path is not a directory: {}", path),
                    suggestion: "allowed_paths should contain directory paths, not files"
                        .to_string(),
                });
            }
        }

        // Warn if all tools are disabled
        if !self.tools.shell_enabled && !self.tools.filesystem_enabled && !self.tools.web_enabled {
            result.add_warning(ConfigValidationWarning {
                message: "All tools are disabled".to_string(),
                suggestion:
                    "Enable at least one tool for agent functionality, or use chat mode only"
                        .to_string(),
            });
        }
    }

    /// Validate configuration loaded from TOML and check for unknown fields.
    pub fn validate_toml(content: &str) -> Result<(Self, ValidationResult)> {
        // First try to parse the TOML
        let config: Self = toml::from_str(content).context("Failed to parse config file")?;

        // Get validation errors/warnings
        let mut result = config.validate();

        // Check for unknown fields by parsing as generic TOML value
        if let Ok(toml_value) = content.parse::<toml::Value>() {
            Self::check_unknown_fields(&toml_value, &mut result);
        }

        Ok((config, result))
    }

    /// Check for unknown fields in the TOML configuration.
    fn check_unknown_fields(value: &toml::Value, result: &mut ValidationResult) {
        if let Some(table) = value.as_table() {
            let known_sections: HashSet<&str> = VALID_SECTIONS.iter().copied().collect();

            for key in table.keys() {
                if !known_sections.contains(key.as_str()) {
                    result.add_warning(ConfigValidationWarning {
                        message: format!("Unknown configuration section: '{}'", key),
                        suggestion: format!(
                            "Valid sections are: {}. This might be a typo.",
                            VALID_SECTIONS.join(", ")
                        ),
                    });
                }
            }

            // Check provider subsections
            if let Some(provider) = table.get("provider").and_then(|v| v.as_table()) {
                let known_provider_keys: HashSet<&str> =
                    ["default", "openai", "ollama"].iter().copied().collect();
                for key in provider.keys() {
                    if !known_provider_keys.contains(key.as_str()) {
                        result.add_warning(ConfigValidationWarning {
                            message: format!("Unknown provider configuration: 'provider.{}'", key),
                            suggestion: "Valid provider keys are: default, openai, ollama"
                                .to_string(),
                        });
                    }
                }
            }

            // Check agent subsections
            if let Some(agent) = table.get("agent").and_then(|v| v.as_table()) {
                let known_agent_keys: HashSet<&str> = [
                    "system_prompt",
                    "temperature",
                    "max_iterations",
                    "streaming",
                ]
                .iter()
                .copied()
                .collect();
                for key in agent.keys() {
                    if !known_agent_keys.contains(key.as_str()) {
                        result.add_warning(ConfigValidationWarning {
                            message: format!("Unknown agent configuration: 'agent.{}'", key),
                            suggestion: format!(
                                "Valid agent keys are: {}. Did you mean one of these?",
                                known_agent_keys
                                    .iter()
                                    .copied()
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        });
                    }
                }
            }

            // Check tools subsections
            if let Some(tools) = table.get("tools").and_then(|v| v.as_table()) {
                let known_tools_keys: HashSet<&str> = [
                    "shell_enabled",
                    "filesystem_enabled",
                    "web_enabled",
                    "allowed_paths",
                    "shell_timeout",
                ]
                .iter()
                .copied()
                .collect();
                for key in tools.keys() {
                    if !known_tools_keys.contains(key.as_str()) {
                        result.add_warning(ConfigValidationWarning {
                            message: format!("Unknown tools configuration: 'tools.{}'", key),
                            suggestion: format!(
                                "Valid tools keys are: {}",
                                known_tools_keys
                                    .iter()
                                    .copied()
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        });
                    }
                }
            }
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

    // ==================== Validation Tests ====================

    #[test]
    fn test_valid_default_config() {
        // Set a mock API key for the test
        std::env::set_var("OPENAI_API_KEY", "sk-test-key-12345");
        let config = AppConfig::default();
        let result = config.validate();
        // Default config should be valid (no errors)
        assert!(
            result.is_valid(),
            "Default config should be valid: {:?}",
            result.errors
        );
        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn test_invalid_provider() {
        let mut config = AppConfig::default();
        config.provider.default = "invalid_provider".to_string();

        let result = config.validate();
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.field == "provider.default"));

        // Check that suggestion lists valid providers
        let error = result
            .errors
            .iter()
            .find(|e| e.field == "provider.default")
            .unwrap();
        assert!(error.suggestion.contains("openai"));
        assert!(error.suggestion.contains("ollama"));
    }

    #[test]
    fn test_negative_temperature() {
        let mut config = AppConfig::default();
        config.agent.temperature = -0.5;

        let result = config.validate();
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.field == "agent.temperature"));

        let error = result
            .errors
            .iter()
            .find(|e| e.field == "agent.temperature")
            .unwrap();
        assert!(error.message.contains("negative"));
    }

    #[test]
    fn test_temperature_too_high() {
        let mut config = AppConfig::default();
        config.agent.temperature = 3.0;

        let result = config.validate();
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.field == "agent.temperature"));
    }

    #[test]
    fn test_temperature_warning_high() {
        let mut config = AppConfig::default();
        config.agent.temperature = 1.8; // High but valid
        std::env::set_var("OPENAI_API_KEY", "sk-test-key-12345");

        let result = config.validate();
        assert!(result.is_valid()); // Should still be valid
        assert!(result.has_warnings()); // But should have a warning
        assert!(result
            .warnings
            .iter()
            .any(|w| w.message.contains("High temperature")));

        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn test_zero_max_iterations() {
        let mut config = AppConfig::default();
        config.agent.max_iterations = 0;

        let result = config.validate();
        assert!(!result.is_valid());
        assert!(result
            .errors
            .iter()
            .any(|e| e.field == "agent.max_iterations"));
    }

    #[test]
    fn test_high_max_iterations_warning() {
        let mut config = AppConfig::default();
        config.agent.max_iterations = 150;
        std::env::set_var("OPENAI_API_KEY", "sk-test-key-12345");

        let result = config.validate();
        assert!(result.is_valid());
        assert!(result.has_warnings());
        assert!(result
            .warnings
            .iter()
            .any(|w| w.message.contains("max_iterations")));

        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn test_zero_shell_timeout() {
        let mut config = AppConfig::default();
        config.tools.shell_timeout = 0;

        let result = config.validate();
        assert!(!result.is_valid());
        assert!(result
            .errors
            .iter()
            .any(|e| e.field == "tools.shell_timeout"));
    }

    #[test]
    fn test_empty_openai_api_key() {
        let mut config = AppConfig::default();
        config.provider.openai.api_key = Some("".to_string());
        std::env::remove_var("OPENAI_API_KEY");

        let result = config.validate();
        assert!(!result.is_valid());
        assert!(result
            .errors
            .iter()
            .any(|e| e.field == "provider.openai.api_key"));
    }

    #[test]
    fn test_empty_model_name() {
        let mut config = AppConfig::default();
        config.provider.openai.model = "".to_string();
        std::env::set_var("OPENAI_API_KEY", "sk-test-key-12345");

        let result = config.validate();
        assert!(!result.is_valid());
        assert!(result
            .errors
            .iter()
            .any(|e| e.field == "provider.openai.model"));

        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn test_invalid_openai_base_url() {
        let mut config = AppConfig::default();
        config.provider.openai.base_url = Some("not-a-url".to_string());
        std::env::set_var("OPENAI_API_KEY", "sk-test-key-12345");

        let result = config.validate();
        assert!(!result.is_valid());
        assert!(result
            .errors
            .iter()
            .any(|e| e.field == "provider.openai.base_url"));

        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn test_valid_openai_base_url() {
        let mut config = AppConfig::default();
        config.provider.openai.base_url = Some("https://api.example.com".to_string());
        std::env::set_var("OPENAI_API_KEY", "sk-test-key-12345");

        let result = config.validate();
        // Should not have an error for base_url
        assert!(!result
            .errors
            .iter()
            .any(|e| e.field == "provider.openai.base_url"));

        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn test_ollama_invalid_url() {
        let mut config = AppConfig::default();
        config.provider.default = "ollama".to_string();
        config.provider.ollama.url = "not-a-url".to_string();

        let result = config.validate();
        assert!(!result.is_valid());
        assert!(result
            .errors
            .iter()
            .any(|e| e.field == "provider.ollama.url"));
    }

    #[test]
    fn test_ollama_empty_model() {
        let mut config = AppConfig::default();
        config.provider.default = "ollama".to_string();
        config.provider.ollama.model = "".to_string();

        let result = config.validate();
        assert!(!result.is_valid());
        assert!(result
            .errors
            .iter()
            .any(|e| e.field == "provider.ollama.model"));
    }

    #[test]
    fn test_all_tools_disabled_warning() {
        let mut config = AppConfig::default();
        config.tools.shell_enabled = false;
        config.tools.filesystem_enabled = false;
        config.tools.web_enabled = false;
        std::env::set_var("OPENAI_API_KEY", "sk-test-key-12345");

        let result = config.validate();
        assert!(result.has_warnings());
        assert!(result
            .warnings
            .iter()
            .any(|w| w.message.contains("All tools are disabled")));

        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn test_unusual_api_key_format_warning() {
        let mut config = AppConfig::default();
        config.provider.openai.api_key = Some("unusual-key-format".to_string());

        let result = config.validate();
        // Should be valid but with a warning
        assert!(result.is_valid());
        assert!(result
            .warnings
            .iter()
            .any(|w| w.message.contains("unusual format")));
    }

    #[test]
    fn test_valid_api_key_format() {
        let mut config = AppConfig::default();
        config.provider.openai.api_key = Some("sk-abc123xyz".to_string());

        let result = config.validate();
        // Should not have warning about unusual format
        assert!(!result
            .warnings
            .iter()
            .any(|w| w.message.contains("unusual format")));
    }

    #[test]
    fn test_validate_toml_unknown_section() {
        let toml_content = r#"
[provider]
default = "openai"

[unknown_section]
some_key = "value"

[agent]
temperature = 0.7
"#;

        let (_, result) = AppConfig::validate_toml(toml_content).unwrap();
        assert!(result
            .warnings
            .iter()
            .any(|w| w.message.contains("Unknown configuration section")));
        assert!(result
            .warnings
            .iter()
            .any(|w| w.message.contains("unknown_section")));
    }

    #[test]
    fn test_validate_toml_unknown_agent_key() {
        let toml_content = r#"
[provider]
default = "ollama"

[agent]
temperature = 0.7
unknown_key = "value"
"#;

        let (_, result) = AppConfig::validate_toml(toml_content).unwrap();
        assert!(result
            .warnings
            .iter()
            .any(|w| w.message.contains("Unknown agent configuration")));
    }

    #[test]
    fn test_validate_toml_unknown_tools_key() {
        let toml_content = r#"
[provider]
default = "ollama"

[tools]
shell_enabled = true
typo_key = false
"#;

        let (_, result) = AppConfig::validate_toml(toml_content).unwrap();
        assert!(result
            .warnings
            .iter()
            .any(|w| w.message.contains("Unknown tools configuration")));
    }

    #[test]
    fn test_validation_result_format_report() {
        let mut result = ValidationResult::new();
        result.add_error(ConfigValidationError {
            field: "test.field".to_string(),
            message: "Test error message".to_string(),
            current_value: Some("bad_value".to_string()),
            suggestion: "Use a good value instead".to_string(),
            line: Some(5),
        });
        result.add_warning(ConfigValidationWarning {
            message: "Test warning".to_string(),
            suggestion: "Consider fixing this".to_string(),
        });

        let report = result.format_report(Some("/path/to/config.toml"));

        assert!(report.contains("Configuration Errors in /path/to/config.toml"));
        assert!(report.contains("test.field"));
        assert!(report.contains("Test error message"));
        assert!(report.contains("bad_value"));
        assert!(report.contains("Use a good value instead"));
        assert!(report.contains("Line:  5"));
        assert!(report.contains("Configuration Warnings"));
        assert!(report.contains("Test warning"));
    }

    #[test]
    fn test_validation_result_is_valid() {
        let mut result = ValidationResult::new();
        assert!(result.is_valid());

        result.add_warning(ConfigValidationWarning {
            message: "Warning".to_string(),
            suggestion: "Fix it".to_string(),
        });
        assert!(result.is_valid()); // Warnings don't affect validity

        result.add_error(ConfigValidationError {
            field: "field".to_string(),
            message: "Error".to_string(),
            current_value: None,
            suggestion: "Fix".to_string(),
            line: None,
        });
        assert!(!result.is_valid()); // Errors make it invalid
    }

    #[test]
    fn test_empty_system_prompt_warning() {
        let mut config = AppConfig::default();
        config.agent.system_prompt = "   ".to_string(); // Just whitespace
        std::env::set_var("OPENAI_API_KEY", "sk-test-key-12345");

        let result = config.validate();
        assert!(result.has_warnings());
        assert!(result
            .warnings
            .iter()
            .any(|w| w.message.contains("Empty system prompt")));

        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn test_very_long_system_prompt_warning() {
        let mut config = AppConfig::default();
        config.agent.system_prompt = "x".repeat(15000); // Very long
        std::env::set_var("OPENAI_API_KEY", "sk-test-key-12345");

        let result = config.validate();
        assert!(result.has_warnings());
        assert!(result
            .warnings
            .iter()
            .any(|w| w.message.contains("Very long system prompt")));

        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn test_shell_timeout_too_long_warning() {
        let mut config = AppConfig::default();
        config.tools.shell_timeout = 5000; // More than 1 hour
        std::env::set_var("OPENAI_API_KEY", "sk-test-key-12345");

        let result = config.validate();
        assert!(result.has_warnings());
        assert!(result
            .warnings
            .iter()
            .any(|w| w.message.contains("shell timeout")));

        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn test_multiple_validation_errors() {
        let mut config = AppConfig::default();
        config.provider.default = "invalid".to_string();
        config.agent.temperature = -1.0;
        config.agent.max_iterations = 0;
        config.tools.shell_timeout = 0;

        let result = config.validate();
        assert!(!result.is_valid());
        assert!(result.errors.len() >= 4); // Should have multiple errors
    }
}
