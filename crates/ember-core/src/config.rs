//! Agent configuration types.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// System prompt for the agent
    pub system_prompt: String,

    /// Maximum tokens for context window
    pub max_context_tokens: usize,

    /// Maximum iterations in the agent loop
    pub max_iterations: usize,

    /// Timeout for LLM requests
    #[serde(with = "humantime_serde")]
    pub request_timeout: Duration,

    /// Temperature for LLM responses (0.0 - 2.0)
    pub temperature: f32,

    /// Whether to enable streaming responses
    pub streaming: bool,

    /// Whether to enable tool use
    pub tools_enabled: bool,

    /// Maximum number of tool calls per turn
    pub max_tool_calls_per_turn: usize,

    /// Whether to automatically retry on recoverable errors
    pub auto_retry: bool,

    /// Maximum retry attempts
    pub max_retries: usize,

    /// Memory configuration
    pub memory: MemoryConfig,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            system_prompt: String::from("You are a helpful AI assistant."),
            max_context_tokens: 8192,
            max_iterations: 10,
            request_timeout: Duration::from_secs(60),
            temperature: 0.7,
            streaming: true,
            tools_enabled: true,
            max_tool_calls_per_turn: 5,
            auto_retry: true,
            max_retries: 3,
            memory: MemoryConfig::default(),
        }
    }
}

/// Memory configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Maximum number of messages to keep in short-term memory
    pub max_short_term_messages: usize,

    /// Whether to enable long-term memory persistence
    pub long_term_enabled: bool,

    /// Maximum entries in long-term memory
    pub max_long_term_entries: usize,

    /// Whether to automatically summarize old conversations
    pub auto_summarize: bool,

    /// Threshold for triggering summarization (number of messages)
    pub summarization_threshold: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_short_term_messages: 50,
            long_term_enabled: false,
            max_long_term_entries: 1000,
            auto_summarize: false,
            summarization_threshold: 20,
        }
    }
}

/// Builder for AgentConfig.
#[derive(Debug, Default)]
pub struct AgentConfigBuilder {
    config: AgentConfig,
}

impl AgentConfigBuilder {
    /// Create a new builder with default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the system prompt.
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.config.system_prompt = prompt.into();
        self
    }

    /// Set maximum context tokens.
    pub fn max_context_tokens(mut self, tokens: usize) -> Self {
        self.config.max_context_tokens = tokens;
        self
    }

    /// Set maximum iterations.
    pub fn max_iterations(mut self, iterations: usize) -> Self {
        self.config.max_iterations = iterations;
        self
    }

    /// Set request timeout.
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.config.request_timeout = timeout;
        self
    }

    /// Set temperature.
    pub fn temperature(mut self, temp: f32) -> Self {
        self.config.temperature = temp.clamp(0.0, 2.0);
        self
    }

    /// Enable or disable streaming.
    pub fn streaming(mut self, enabled: bool) -> Self {
        self.config.streaming = enabled;
        self
    }

    /// Enable or disable tools.
    pub fn tools_enabled(mut self, enabled: bool) -> Self {
        self.config.tools_enabled = enabled;
        self
    }

    /// Set maximum tool calls per turn.
    pub fn max_tool_calls_per_turn(mut self, max: usize) -> Self {
        self.config.max_tool_calls_per_turn = max;
        self
    }

    /// Enable or disable auto retry.
    pub fn auto_retry(mut self, enabled: bool) -> Self {
        self.config.auto_retry = enabled;
        self
    }

    /// Set maximum retries.
    pub fn max_retries(mut self, retries: usize) -> Self {
        self.config.max_retries = retries;
        self
    }

    /// Set memory configuration.
    pub fn memory(mut self, memory: MemoryConfig) -> Self {
        self.config.memory = memory;
        self
    }

    /// Enable long-term memory.
    pub fn with_long_term_memory(mut self) -> Self {
        self.config.memory.long_term_enabled = true;
        self
    }

    /// Enable auto summarization.
    pub fn with_auto_summarize(mut self, threshold: usize) -> Self {
        self.config.memory.auto_summarize = true;
        self.config.memory.summarization_threshold = threshold;
        self
    }

    /// Build the configuration.
    pub fn build(self) -> AgentConfig {
        self.config
    }
}

impl AgentConfig {
    /// Create a new configuration builder.
    pub fn builder() -> AgentConfigBuilder {
        AgentConfigBuilder::new()
    }

    /// Validate the configuration.
    pub fn validate(&self) -> crate::Result<()> {
        if self.system_prompt.is_empty() {
            return Err(crate::Error::config("System prompt cannot be empty"));
        }
        if self.max_context_tokens == 0 {
            return Err(crate::Error::config("Max context tokens must be > 0"));
        }
        if self.max_iterations == 0 {
            return Err(crate::Error::config("Max iterations must be > 0"));
        }
        if self.temperature < 0.0 || self.temperature > 2.0 {
            return Err(crate::Error::config(
                "Temperature must be between 0.0 and 2.0",
            ));
        }
        Ok(())
    }
}

/// Serde support for Duration using humantime format.
mod humantime_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = humantime::format_duration(*duration).to_string();
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        humantime::parse_duration(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AgentConfig::default();
        assert!(!config.system_prompt.is_empty());
        assert!(config.max_context_tokens > 0);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_builder() {
        let config = AgentConfig::builder()
            .system_prompt("Custom prompt")
            .max_context_tokens(4096)
            .temperature(0.5)
            .streaming(false)
            .build();

        assert_eq!(config.system_prompt, "Custom prompt");
        assert_eq!(config.max_context_tokens, 4096);
        assert_eq!(config.temperature, 0.5);
        assert!(!config.streaming);
    }

    #[test]
    fn test_temperature_clamping() {
        let config = AgentConfig::builder().temperature(3.0).build();
        assert_eq!(config.temperature, 2.0);

        let config = AgentConfig::builder().temperature(-1.0).build();
        assert_eq!(config.temperature, 0.0);
    }

    #[test]
    fn test_invalid_config() {
        let mut config = AgentConfig::default();
        config.system_prompt = String::new();
        assert!(config.validate().is_err());

        let mut config = AgentConfig::default();
        config.max_context_tokens = 0;
        assert!(config.validate().is_err());
    }
}
