//! Application state for the web server.
//!
//! This module defines the shared state available to all handlers.

use crate::websocket::StreamManager;
use ember_core::CostPredictor;
use ember_llm::{LLMProvider, OllamaProvider, OpenAIProvider};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Server host address.
    pub host: String,
    /// Server port.
    pub port: u16,
    /// Enable CORS.
    pub cors_enabled: bool,
    /// Allowed CORS origins.
    pub cors_origins: Vec<String>,
    /// API key for authentication (optional).
    pub api_key: Option<String>,
    /// Maximum request body size in bytes.
    pub max_body_size: usize,
    /// LLM provider type.
    pub llm_provider: LLMProviderType,
    /// Default model.
    pub default_model: String,
}

/// LLM provider type configuration.
#[derive(Debug, Clone, Default)]
pub enum LLMProviderType {
    /// OpenAI provider.
    #[default]
    OpenAI,
    /// Ollama provider (local).
    Ollama,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            cors_enabled: true,
            cors_origins: vec!["*".to_string()],
            api_key: None,
            max_body_size: 10 * 1024 * 1024, // 10 MB
            llm_provider: LLMProviderType::OpenAI,
            default_model: "gpt-4".to_string(),
        }
    }
}

impl ServerConfig {
    /// Create a new server configuration.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            ..Default::default()
        }
    }

    /// Set the API key.
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Disable CORS.
    pub fn without_cors(mut self) -> Self {
        self.cors_enabled = false;
        self
    }

    /// Set the LLM provider type.
    pub fn with_llm_provider(mut self, provider: LLMProviderType) -> Self {
        self.llm_provider = provider;
        self
    }

    /// Set the default model.
    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }

    /// Get the server address.
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Server configuration.
    pub config: ServerConfig,
    /// Active conversations count.
    pub active_conversations: Arc<RwLock<usize>>,
    /// Server start time.
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// LLM provider instance.
    pub llm_provider: Arc<dyn LLMProvider>,
    /// Cost predictor for budget management.
    pub cost_predictor: Arc<CostPredictor>,
    /// Stream manager for WebSocket connections.
    pub stream_manager: Arc<StreamManager>,
}

impl AppState {
    /// Create a new application state.
    pub fn new(config: ServerConfig) -> Self {
        let llm_provider: Arc<dyn LLMProvider> = match config.llm_provider {
            LLMProviderType::OpenAI => {
                // Try to create OpenAI provider from env, fallback to Ollama if no API key
                match OpenAIProvider::from_env() {
                    Ok(provider) => Arc::new(provider),
                    Err(e) => {
                        warn!(
                            error = %e,
                            "OpenAI provider unavailable, falling back to Ollama"
                        );
                        Arc::new(OllamaProvider::from_env())
                    }
                }
            }
            LLMProviderType::Ollama => Arc::new(OllamaProvider::from_env()),
        };

        Self {
            config,
            active_conversations: Arc::new(RwLock::new(0)),
            started_at: chrono::Utc::now(),
            llm_provider,
            cost_predictor: Arc::new(CostPredictor::default()),
            stream_manager: Arc::new(StreamManager::new()),
        }
    }

    /// Create application state with a custom LLM provider.
    pub fn with_llm_provider(config: ServerConfig, provider: Arc<dyn LLMProvider>) -> Self {
        Self {
            config,
            active_conversations: Arc::new(RwLock::new(0)),
            started_at: chrono::Utc::now(),
            llm_provider: provider,
            cost_predictor: Arc::new(CostPredictor::default()),
            stream_manager: Arc::new(StreamManager::new()),
        }
    }

    /// Increment active conversation count.
    pub async fn increment_conversations(&self) {
        let mut count = self.active_conversations.write().await;
        *count += 1;
    }

    /// Decrement active conversation count.
    pub async fn decrement_conversations(&self) {
        let mut count = self.active_conversations.write().await;
        if *count > 0 {
            *count -= 1;
        }
    }

    /// Get active conversation count.
    pub async fn conversation_count(&self) -> usize {
        *self.active_conversations.read().await
    }

    /// Get server uptime.
    pub fn uptime(&self) -> chrono::Duration {
        chrono::Utc::now() - self.started_at
    }

    /// Check if API key authentication is required.
    pub fn requires_auth(&self) -> bool {
        self.config.api_key.is_some()
    }

    /// Validate an API key.
    pub fn validate_api_key(&self, key: &str) -> bool {
        match &self.config.api_key {
            Some(expected) => expected == key,
            None => true,
        }
    }

    /// Get the default model.
    pub fn default_model(&self) -> &str {
        &self.config.default_model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 3000);
        assert!(config.cors_enabled);
    }

    #[test]
    fn test_server_config_address() {
        let config = ServerConfig::new("0.0.0.0", 8080);
        assert_eq!(config.address(), "0.0.0.0:8080");
    }

    #[test]
    fn test_server_config_with_api_key() {
        let config = ServerConfig::default().with_api_key("secret123");
        assert_eq!(config.api_key, Some("secret123".to_string()));
    }

    #[test]
    fn test_server_config_with_llm_provider() {
        let config = ServerConfig::default()
            .with_llm_provider(LLMProviderType::Ollama)
            .with_default_model("llama3");
        assert!(matches!(config.llm_provider, LLMProviderType::Ollama));
        assert_eq!(config.default_model, "llama3");
    }

    #[tokio::test]
    async fn test_app_state_conversations() {
        let state =
            AppState::new(ServerConfig::default().with_llm_provider(LLMProviderType::Ollama));

        assert_eq!(state.conversation_count().await, 0);

        state.increment_conversations().await;
        assert_eq!(state.conversation_count().await, 1);

        state.decrement_conversations().await;
        assert_eq!(state.conversation_count().await, 0);
    }

    #[test]
    fn test_app_state_auth() {
        let state_no_auth =
            AppState::new(ServerConfig::default().with_llm_provider(LLMProviderType::Ollama));
        assert!(!state_no_auth.requires_auth());
        assert!(state_no_auth.validate_api_key("anything"));

        let state_with_auth = AppState::new(
            ServerConfig::default()
                .with_api_key("secret")
                .with_llm_provider(LLMProviderType::Ollama),
        );
        assert!(state_with_auth.requires_auth());
        assert!(state_with_auth.validate_api_key("secret"));
        assert!(!state_with_auth.validate_api_key("wrong"));
    }
}
