//! Ollama local LLM provider implementation

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use tracing::{debug, instrument};

use crate::{
    provider::StreamResponse, CompletionRequest, CompletionResponse, Error, FinishReason,
    LLMProvider, ModelInfo, Result, StreamChunk, TokenUsage,
};

use tokio_stream::wrappers::ReceiverStream;

const DEFAULT_BASE_URL: &str = "http://localhost:11434";
const DEFAULT_MODEL: &str = "llama3.2";

/// Ollama local LLM provider
#[derive(Debug, Clone)]
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    default_model: String,
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl OllamaProvider {
    /// Create a new Ollama provider with default settings
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
            default_model: DEFAULT_MODEL.to_string(),
        }
    }

    /// Create a provider from environment variables
    pub fn from_env() -> Self {
        let mut provider = Self::new();

        if let Ok(base_url) = env::var("OLLAMA_HOST") {
            provider.base_url = base_url;
        }

        if let Ok(model) = env::var("OLLAMA_MODEL") {
            provider.default_model = model;
        }

        provider
    }

    /// Set a custom base URL
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the default model
    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }

    /// Check if Ollama is running
    pub async fn is_available(&self) -> bool {
        self.health_check().await.is_ok()
    }
}

#[async_trait]
impl LLMProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let ollama_request = OllamaRequest::from(request);

        debug!("Sending request to Ollama");

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&ollama_request)
            .send()
            .await
            .map_err(|e| {
                Error::provider_unavailable("ollama", format!("Failed to connect: {}", e))
            })?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::api_error("ollama", status.as_u16(), error_text));
        }

        let ollama_response: OllamaResponse = response.json().await?;

        Ok(ollama_response.into())
    }

    async fn complete_stream(&self, request: CompletionRequest) -> Result<StreamResponse> {
        let mut ollama_request = OllamaRequest::from(request);
        ollama_request.stream = true;

        debug!("Starting streaming request to Ollama");

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&ollama_request)
            .send()
            .await
            .map_err(|e| {
                Error::provider_unavailable("ollama", format!("Failed to connect: {}", e))
            })?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::api_error("ollama", status.as_u16(), error_text));
        }

        // Create channel for streaming chunks
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<StreamChunk>>(32);

        // Spawn task to process NDJSON stream
        let byte_stream = response.bytes_stream();
        tokio::spawn(async move {
            use futures::TryStreamExt;
            let mut stream = byte_stream;
            let mut buffer = String::new();

            while let Ok(Some(chunk)) = stream.try_next().await {
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete lines (NDJSON format - one JSON per line)
                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].to_string();
                    buffer = buffer[pos + 1..].to_string();

                    if line.trim().is_empty() {
                        continue;
                    }

                    if let Ok(chunk_response) = serde_json::from_str::<OllamaStreamResponse>(&line)
                    {
                        let stream_chunk = StreamChunk {
                            content: if chunk_response.message.content.is_empty() {
                                None
                            } else {
                                Some(chunk_response.message.content)
                            },
                            tool_calls: None,
                            done: chunk_response.done,
                            finish_reason: if chunk_response.done {
                                Some(FinishReason::Stop)
                            } else {
                                None
                            },
                        };

                        if tx.send(Ok(stream_chunk)).await.is_err() {
                            return; // Receiver dropped
                        }

                        if chunk_response.done {
                            return;
                        }
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(|e| {
                Error::provider_unavailable("ollama", format!("Failed to connect: {}", e))
            })?;

        if !response.status().is_success() {
            return Err(Error::api_error(
                "ollama",
                response.status().as_u16(),
                "Failed to list models",
            ));
        }

        let tags_response: OllamaTagsResponse = response.json().await?;

        Ok(tags_response
            .models
            .into_iter()
            .map(|m| ModelInfo {
                id: m.name.clone(),
                name: m.name,
                description: None,
                context_window: None,
                max_output_tokens: None,
                supports_tools: true,
                supports_vision: false,
                provider: "ollama".to_string(),
            })
            .collect())
    }

    async fn health_check(&self) -> Result<()> {
        self.client.get(&self.base_url).send().await.map_err(|e| {
            Error::provider_unavailable("ollama", format!("Ollama is not running: {}", e))
        })?;
        Ok(())
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn supports_vision(&self) -> bool {
        false
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }
}

// Ollama API types

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    model: String,
    message: OllamaMessage,
    done: bool,
    #[serde(default)]
    prompt_eval_count: u32,
    #[serde(default)]
    eval_count: u32,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamResponse {
    message: OllamaMessage,
    done: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
}

// Conversion implementations

impl From<CompletionRequest> for OllamaRequest {
    fn from(req: CompletionRequest) -> Self {
        let options =
            if req.temperature.is_some() || req.max_tokens.is_some() || req.top_p.is_some() {
                Some(OllamaOptions {
                    temperature: req.temperature,
                    num_predict: req.max_tokens,
                    top_p: req.top_p,
                })
            } else {
                None
            };

        Self {
            model: req.model,
            messages: req
                .messages
                .into_iter()
                .map(|m| OllamaMessage {
                    role: match m.role {
                        crate::Role::System => "system".to_string(),
                        crate::Role::User => "user".to_string(),
                        crate::Role::Assistant => "assistant".to_string(),
                        crate::Role::Tool => "assistant".to_string(),
                    },
                    content: m.content,
                })
                .collect(),
            stream: false,
            options,
        }
    }
}

impl From<OllamaResponse> for CompletionResponse {
    fn from(resp: OllamaResponse) -> Self {
        Self {
            content: resp.message.content,
            tool_calls: vec![],
            finish_reason: if resp.done {
                Some(FinishReason::Stop)
            } else {
                None
            },
            usage: TokenUsage {
                prompt_tokens: resp.prompt_eval_count,
                completion_tokens: resp.eval_count,
                total_tokens: resp.prompt_eval_count + resp.eval_count,
                ..Default::default()
            },
            model: resp.model,
            id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_provider_creation() {
        let provider = OllamaProvider::new();
        assert_eq!(provider.name(), "ollama");
        assert_eq!(provider.default_model(), "llama3.2");
    }
}
