//! Groq API provider implementation.
//!
//! Groq provides fast inference using their LPU (Language Processing Unit).
//! The API is OpenAI-compatible, making integration straightforward.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use tracing::{debug, instrument};

use crate::{
    provider::StreamResponse, CompletionRequest, CompletionResponse, Error, FinishReason,
    LLMProvider, ModelInfo, Result, StreamChunk, TokenUsage, ToolCall, ToolCallDelta,
};

use tokio_stream::wrappers::ReceiverStream;

const DEFAULT_BASE_URL: &str = "https://api.groq.com/openai/v1";
const DEFAULT_MODEL: &str = "llama-3.3-70b-versatile";

/// Available Groq models with their context windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroqModel {
    /// Llama 3.3 70B - Versatile, high quality
    Llama3_3_70B,
    /// Llama 3.1 70B - Versatile
    Llama3_1_70B,
    /// Llama 3.1 8B - Fast, instant responses
    Llama3_1_8B,
    /// Llama 3 70B - Versatile
    Llama3_70B,
    /// Llama 3 8B - Fast, instant responses  
    Llama3_8B,
    /// Llama Guard 3 8B - Content safety
    LlamaGuard3_8B,
    /// Mixtral 8x7B - Fast, good for code
    Mixtral8x7B,
    /// Gemma 2 9B - Google's model
    Gemma2_9B,
}

impl GroqModel {
    /// Get the model ID string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Llama3_3_70B => "llama-3.3-70b-versatile",
            Self::Llama3_1_70B => "llama-3.1-70b-versatile",
            Self::Llama3_1_8B => "llama-3.1-8b-instant",
            Self::Llama3_70B => "llama3-70b-8192",
            Self::Llama3_8B => "llama3-8b-8192",
            Self::LlamaGuard3_8B => "llama-guard-3-8b",
            Self::Mixtral8x7B => "mixtral-8x7b-32768",
            Self::Gemma2_9B => "gemma2-9b-it",
        }
    }

    /// Get the context window size.
    pub fn context_window(&self) -> usize {
        match self {
            Self::Llama3_3_70B => 128_000,
            Self::Llama3_1_70B => 128_000,
            Self::Llama3_1_8B => 128_000,
            Self::Llama3_70B => 8_192,
            Self::Llama3_8B => 8_192,
            Self::LlamaGuard3_8B => 8_192,
            Self::Mixtral8x7B => 32_768,
            Self::Gemma2_9B => 8_192,
        }
    }

    /// Check if the model supports tool use.
    pub fn supports_tools(&self) -> bool {
        matches!(
            self,
            Self::Llama3_3_70B
                | Self::Llama3_1_70B
                | Self::Llama3_1_8B
                | Self::Llama3_70B
                | Self::Llama3_8B
                | Self::Mixtral8x7B
                | Self::Gemma2_9B
        )
    }
}

/// Groq API provider.
///
/// Groq provides extremely fast LLM inference using their custom LPU hardware.
/// The API is fully OpenAI-compatible.
///
/// # Example
///
/// ```rust,no_run
/// use ember_llm::{GroqProvider, LLMProvider, CompletionRequest, Message};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let provider = GroqProvider::from_env()?;
///     
///     let request = CompletionRequest::new("llama-3.3-70b-versatile")
///         .with_message(Message::user("Hello!"));
///     
///     let response = provider.complete(request).await?;
///     println!("{}", response.content);
///     
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct GroqProvider {
    client: Client,
    api_key: String,
    base_url: String,
    default_model: String,
}

impl GroqProvider {
    /// Create a new Groq provider with explicit API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            default_model: DEFAULT_MODEL.to_string(),
        }
    }

    /// Create a provider from environment variables.
    ///
    /// Looks for `GROQ_API_KEY` environment variable.
    pub fn from_env() -> Result<Self> {
        let api_key = env::var("GROQ_API_KEY").map_err(|_| Error::api_key_missing("groq"))?;

        let mut provider = Self::new(api_key);

        if let Ok(base_url) = env::var("GROQ_BASE_URL") {
            provider.base_url = base_url;
        }

        if let Ok(model) = env::var("GROQ_DEFAULT_MODEL") {
            provider.default_model = model;
        }

        Ok(provider)
    }

    /// Set a custom base URL.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the default model.
    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }

    /// Set the default model using the enum.
    pub fn with_model(mut self, model: GroqModel) -> Self {
        self.default_model = model.as_str().to_string();
        self
    }

    fn build_request(&self) -> reqwest::RequestBuilder {
        self.client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
    }
}

#[async_trait]
impl LLMProvider for GroqProvider {
    fn name(&self) -> &str {
        "groq"
    }

    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let groq_request = GroqRequest::from(request);

        debug!("Sending request to Groq");

        let response = self.build_request().json(&groq_request).send().await?;

        let status = response.status();

        if !status.is_success() {
            let error_body: GroqError = response.json().await.unwrap_or_else(|_| GroqError {
                error: GroqErrorDetail {
                    message: "Unknown error".to_string(),
                    r#type: "unknown".to_string(),
                    code: None,
                },
            });

            return match status.as_u16() {
                401 => Err(Error::api_key_missing("groq")),
                429 => {
                    // Groq includes retry-after in rate limit errors
                    let retry_after = error_body
                        .error
                        .message
                        .split("try again in ")
                        .nth(1)
                        .and_then(|s| s.split('s').next())
                        .and_then(|s| s.parse::<f64>().ok())
                        .map(|s| s.ceil() as u64);
                    Err(Error::rate_limit("groq", retry_after))
                }
                _ => Err(Error::api_error(
                    "groq",
                    status.as_u16(),
                    error_body.error.message,
                )),
            };
        }

        let groq_response: GroqResponse = response.json().await?;

        Ok(groq_response.into())
    }

    async fn complete_stream(&self, request: CompletionRequest) -> Result<StreamResponse> {
        let mut groq_request = GroqRequest::from(request);
        groq_request.stream = Some(true);

        debug!("Starting streaming request to Groq");

        let response = self.build_request().json(&groq_request).send().await?;

        let status = response.status();

        if !status.is_success() {
            let error_body: GroqError = response.json().await.unwrap_or_else(|_| GroqError {
                error: GroqErrorDetail {
                    message: "Unknown error".to_string(),
                    r#type: "unknown".to_string(),
                    code: None,
                },
            });

            return match status.as_u16() {
                401 => Err(Error::api_key_missing("groq")),
                429 => Err(Error::rate_limit("groq", None)),
                _ => Err(Error::api_error(
                    "groq",
                    status.as_u16(),
                    error_body.error.message,
                )),
            };
        }

        // Create channel for streaming chunks
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<StreamChunk>>(32);

        // Spawn task to process SSE stream
        let byte_stream = response.bytes_stream();
        tokio::spawn(async move {
            use futures::TryStreamExt;
            let mut stream = byte_stream;
            let mut buffer = String::new();

            while let Ok(Some(chunk)) = stream.try_next().await {
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete SSE events
                while let Some(pos) = buffer.find("\n\n") {
                    let event = buffer[..pos].to_string();
                    buffer = buffer[pos + 2..].to_string();

                    // Parse SSE event
                    for line in event.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if data == "[DONE]" {
                                let _ = tx
                                    .send(Ok(StreamChunk {
                                        content: None,
                                        tool_calls: None,
                                        done: true,
                                        finish_reason: Some(FinishReason::Stop),
                                    }))
                                    .await;
                                return;
                            }

                            if let Ok(chunk_response) =
                                serde_json::from_str::<GroqStreamResponse>(data)
                            {
                                if let Some(choice) = chunk_response.choices.first() {
                                    let stream_chunk = StreamChunk {
                                        content: choice.delta.content.clone(),
                                        tool_calls: choice.delta.tool_calls.as_ref().map(|tcs| {
                                            tcs.iter()
                                                .map(|tc| ToolCallDelta {
                                                    index: tc.index,
                                                    id: tc.id.clone(),
                                                    name: tc
                                                        .function
                                                        .as_ref()
                                                        .and_then(|f| f.name.clone()),
                                                    arguments: tc
                                                        .function
                                                        .as_ref()
                                                        .and_then(|f| f.arguments.clone()),
                                                })
                                                .collect()
                                        }),
                                        done: choice.finish_reason.is_some(),
                                        finish_reason: choice.finish_reason.as_ref().and_then(
                                            |r| match r.as_str() {
                                                "stop" => Some(FinishReason::Stop),
                                                "length" => Some(FinishReason::Length),
                                                "tool_calls" => Some(FinishReason::ToolCalls),
                                                _ => None,
                                            },
                                        ),
                                    };

                                    if tx.send(Ok(stream_chunk)).await.is_err() {
                                        return; // Receiver dropped
                                    }
                                }
                            }
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
            .get(format!("{}/models", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(Error::api_error(
                "groq",
                response.status().as_u16(),
                "Failed to list models",
            ));
        }

        let models_response: GroqModelsResponse = response.json().await?;

        Ok(models_response
            .data
            .into_iter()
            .map(|m| {
                // Determine context window based on known models
                let context_window = if m.id.contains("128k") || m.id.contains("versatile") {
                    Some(128_000)
                } else if m.id.contains("32768") {
                    Some(32_768)
                } else {
                    Some(8_192)
                };

                ModelInfo {
                    id: m.id.clone(),
                    name: m.id,
                    description: None,
                    context_window,
                    max_output_tokens: Some(8_192), // Groq default
                    supports_tools: true,
                    supports_vision: false, // Groq doesn't support vision yet
                    provider: "groq".to_string(),
                }
            })
            .collect())
    }

    async fn health_check(&self) -> Result<()> {
        self.list_models().await?;
        Ok(())
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn supports_vision(&self) -> bool {
        false // Groq doesn't support vision models yet
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }
}

// Groq API types (OpenAI-compatible)

#[derive(Debug, Serialize)]
struct GroqRequest {
    model: String,
    messages: Vec<GroqMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GroqTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GroqMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<GroqToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GroqTool {
    r#type: String,
    function: GroqFunction,
}

#[derive(Debug, Serialize, Deserialize)]
struct GroqFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GroqToolCall {
    id: String,
    r#type: String,
    function: GroqFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct GroqFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct GroqResponse {
    id: String,
    model: String,
    choices: Vec<GroqChoice>,
    usage: GroqUsage,
}

#[derive(Debug, Deserialize)]
struct GroqChoice {
    message: GroqMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GroqUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct GroqError {
    error: GroqErrorDetail,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GroqErrorDetail {
    message: String,
    r#type: String,
    code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GroqModelsResponse {
    data: Vec<GroqModelInfo>,
}

#[derive(Debug, Deserialize)]
struct GroqModelInfo {
    id: String,
}

// Streaming response types

#[derive(Debug, Deserialize)]
struct GroqStreamResponse {
    choices: Vec<GroqStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct GroqStreamChoice {
    delta: GroqStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GroqStreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<GroqStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct GroqStreamToolCall {
    index: usize,
    id: Option<String>,
    function: Option<GroqStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct GroqStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

// Conversion implementations

impl From<CompletionRequest> for GroqRequest {
    fn from(req: CompletionRequest) -> Self {
        Self {
            model: req.model,
            messages: req
                .messages
                .into_iter()
                .map(|m| {
                    // Groq uses simple text content (no multimodal support)
                    let content = if m.content_parts.is_empty() {
                        m.content
                    } else {
                        // Extract text from content parts
                        m.content_parts
                            .iter()
                            .filter_map(|p| match p {
                                crate::types::ContentPart::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    };

                    GroqMessage {
                        role: match m.role {
                            crate::Role::System => "system".to_string(),
                            crate::Role::User => "user".to_string(),
                            crate::Role::Assistant => "assistant".to_string(),
                            crate::Role::Tool => "tool".to_string(),
                        },
                        content,
                        name: m.name,
                        tool_calls: if m.tool_calls.is_empty() {
                            None
                        } else {
                            Some(
                                m.tool_calls
                                    .into_iter()
                                    .map(|tc| GroqToolCall {
                                        id: tc.id,
                                        r#type: "function".to_string(),
                                        function: GroqFunctionCall {
                                            name: tc.name,
                                            arguments: tc.arguments.to_string(),
                                        },
                                    })
                                    .collect(),
                            )
                        },
                        tool_call_id: m.tool_call_id,
                    }
                })
                .collect(),
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            top_p: req.top_p,
            stop: req.stop,
            tools: req.tools.map(|tools| {
                tools
                    .into_iter()
                    .map(|t| GroqTool {
                        r#type: "function".to_string(),
                        function: GroqFunction {
                            name: t.name,
                            description: t.description,
                            parameters: t.parameters,
                        },
                    })
                    .collect()
            }),
            stream: req.stream,
        }
    }
}

impl From<GroqResponse> for CompletionResponse {
    fn from(resp: GroqResponse) -> Self {
        let choice = resp.choices.into_iter().next().unwrap_or(GroqChoice {
            message: GroqMessage {
                role: "assistant".to_string(),
                content: String::new(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            finish_reason: None,
        });

        Self {
            content: choice.message.content,
            tool_calls: choice
                .message
                .tool_calls
                .map(|tcs| {
                    tcs.into_iter()
                        .map(|tc| {
                            ToolCall::new(
                                tc.id,
                                tc.function.name,
                                serde_json::from_str(&tc.function.arguments)
                                    .unwrap_or(serde_json::Value::Null),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            finish_reason: choice.finish_reason.and_then(|r| match r.as_str() {
                "stop" => Some(FinishReason::Stop),
                "length" => Some(FinishReason::Length),
                "tool_calls" => Some(FinishReason::ToolCalls),
                _ => None,
            }),
            usage: TokenUsage {
                prompt_tokens: resp.usage.prompt_tokens,
                completion_tokens: resp.usage.completion_tokens,
                total_tokens: resp.usage.total_tokens,
                ..Default::default()
            },
            model: resp.model,
            id: Some(resp.id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_groq_model_strings() {
        assert_eq!(GroqModel::Llama3_3_70B.as_str(), "llama-3.3-70b-versatile");
        assert_eq!(GroqModel::Mixtral8x7B.as_str(), "mixtral-8x7b-32768");
    }

    #[test]
    fn test_groq_model_context_windows() {
        assert_eq!(GroqModel::Llama3_3_70B.context_window(), 128_000);
        assert_eq!(GroqModel::Mixtral8x7B.context_window(), 32_768);
        assert_eq!(GroqModel::Llama3_8B.context_window(), 8_192);
    }

    #[test]
    fn test_groq_request_conversion() {
        use crate::Message;

        let request = CompletionRequest::new("llama-3.3-70b-versatile")
            .with_message(Message::system("You are helpful"))
            .with_message(Message::user("Hello"))
            .with_temperature(0.7);

        let groq_req = GroqRequest::from(request);

        assert_eq!(groq_req.model, "llama-3.3-70b-versatile");
        assert_eq!(groq_req.messages.len(), 2);
        assert_eq!(groq_req.temperature, Some(0.7));
    }
}
