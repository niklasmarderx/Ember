//! DeepSeek API provider implementation
//!
//! Supports DeepSeek V3, DeepSeek R1, DeepSeek Coder, and other DeepSeek models.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use tracing::{debug, instrument};

use crate::{
    provider::StreamResponse, CompletionRequest, CompletionResponse, ContentPart, Error,
    FinishReason, ImageSource, LLMProvider, ModelInfo, Result, StreamChunk, TokenUsage, ToolCall,
    ToolCallDelta,
};

use tokio_stream::wrappers::ReceiverStream;

const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
const DEFAULT_MODEL: &str = "deepseek-chat";

/// DeepSeek API provider
///
/// Supports the latest DeepSeek models including:
/// - deepseek-chat (V3, default, most capable)
/// - deepseek-reasoner (R1, reasoning model with chain-of-thought)
/// - deepseek-coder (optimized for code generation)
///
/// DeepSeek offers competitive pricing and strong performance,
/// especially for coding tasks.
///
/// # Example
///
/// ```rust,no_run
/// use ember_llm::{DeepSeekProvider, LLMProvider, CompletionRequest, Message};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let provider = DeepSeekProvider::from_env()?;
///     let request = CompletionRequest::new("deepseek-chat")
///         .with_message(Message::user("Hello!"));
///     let response = provider.complete(request).await?;
///     println!("{}", response.content);
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct DeepSeekProvider {
    client: Client,
    api_key: String,
    base_url: String,
    default_model: String,
}

impl DeepSeekProvider {
    /// Create a new DeepSeek provider with explicit API key
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            default_model: DEFAULT_MODEL.to_string(),
        }
    }

    /// Create a provider from environment variables
    ///
    /// Looks for `DEEPSEEK_API_KEY` environment variable.
    pub fn from_env() -> Result<Self> {
        let api_key =
            env::var("DEEPSEEK_API_KEY").map_err(|_| Error::api_key_missing("deepseek"))?;

        let mut provider = Self::new(api_key);

        if let Ok(base_url) = env::var("DEEPSEEK_BASE_URL") {
            provider.base_url = base_url;
        }

        if let Ok(model) = env::var("DEEPSEEK_MODEL") {
            provider.default_model = model;
        }

        Ok(provider)
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

    fn build_request(&self) -> reqwest::RequestBuilder {
        self.client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
    }
}

#[async_trait]
impl LLMProvider for DeepSeekProvider {
    fn name(&self) -> &str {
        "deepseek"
    }

    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let deepseek_request = DeepSeekRequest::from(request);

        debug!("Sending request to DeepSeek API");

        let response = self.build_request().json(&deepseek_request).send().await?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let error_msg = serde_json::from_str::<DeepSeekError>(&error_text)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| if error_text.is_empty() {
                    format!("HTTP {} (empty response)", status.as_u16())
                } else { error_text });

            return match status.as_u16() {
                401 => Err(Error::api_key_missing("deepseek")),
                429 => Err(Error::rate_limit("deepseek", None)),
                _ => Err(Error::api_error("deepseek", status.as_u16(), error_msg)),
            };
        }

        let deepseek_response: DeepSeekResponse = response.json().await?;

        Ok(deepseek_response.into())
    }

    async fn complete_stream(&self, request: CompletionRequest) -> Result<StreamResponse> {
        let mut deepseek_request = DeepSeekRequest::from(request);
        deepseek_request.stream = Some(true);

        debug!("Starting streaming request to DeepSeek");

        let response = self.build_request().json(&deepseek_request).send().await?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let error_msg = serde_json::from_str::<DeepSeekError>(&error_text)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| if error_text.is_empty() {
                    format!("HTTP {} (empty response)", status.as_u16())
                } else { error_text });

            return match status.as_u16() {
                401 => Err(Error::api_key_missing("deepseek")),
                429 => Err(Error::rate_limit("deepseek", None)),
                _ => Err(Error::api_error("deepseek", status.as_u16(), error_msg)),
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
                                serde_json::from_str::<DeepSeekStreamResponse>(data)
                            {
                                if let Some(choice) = chunk_response.choices.first() {
                                    // Handle reasoning_content for R1 model
                                    let content = choice
                                        .delta
                                        .content
                                        .clone()
                                        .or_else(|| choice.delta.reasoning_content.clone());

                                    let stream_chunk = StreamChunk {
                                        content,
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
        // DeepSeek doesn't have a models endpoint, return known models
        Ok(vec![
            ModelInfo {
                id: "deepseek-chat".to_string(),
                name: "DeepSeek V3".to_string(),
                description: Some(
                    "Most capable DeepSeek model, excellent for general tasks and coding"
                        .to_string(),
                ),
                context_window: Some(64000),
                max_output_tokens: Some(8192),
                supports_tools: true,
                supports_vision: false,
                provider: "deepseek".to_string(),
            },
            ModelInfo {
                id: "deepseek-reasoner".to_string(),
                name: "DeepSeek R1".to_string(),
                description: Some(
                    "Reasoning model with chain-of-thought, comparable to o1".to_string(),
                ),
                context_window: Some(64000),
                max_output_tokens: Some(8192),
                supports_tools: false, // R1 doesn't support tools yet
                supports_vision: false,
                provider: "deepseek".to_string(),
            },
            ModelInfo {
                id: "deepseek-coder".to_string(),
                name: "DeepSeek Coder".to_string(),
                description: Some("Optimized for code generation and analysis".to_string()),
                context_window: Some(16000),
                max_output_tokens: Some(4096),
                supports_tools: true,
                supports_vision: false,
                provider: "deepseek".to_string(),
            },
        ])
    }

    async fn health_check(&self) -> Result<()> {
        // Make a minimal request to check connectivity
        let request = CompletionRequest::new(&self.default_model)
            .with_message(crate::Message::user("Hi"))
            .with_max_tokens(1);

        self.complete(request).await?;
        Ok(())
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn supports_vision(&self) -> bool {
        false // DeepSeek doesn't support vision yet
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }
}

// DeepSeek API types (OpenAI-compatible)

#[derive(Debug, Serialize)]
struct DeepSeekRequest {
    model: String,
    messages: Vec<DeepSeekMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<DeepSeekTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeepSeekMessage {
    role: String,
    content: DeepSeekContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<DeepSeekToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// Content for DeepSeek messages
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum DeepSeekContent {
    Text(String),
    Parts(Vec<DeepSeekContentPart>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum DeepSeekContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: DeepSeekImageUrl },
}

#[derive(Debug, Serialize, Deserialize)]
struct DeepSeekImageUrl {
    url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeepSeekTool {
    r#type: String,
    function: DeepSeekFunction,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeepSeekFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeepSeekToolCall {
    id: String,
    r#type: String,
    function: DeepSeekFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeepSeekFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct DeepSeekResponse {
    id: String,
    model: String,
    choices: Vec<DeepSeekChoice>,
    usage: DeepSeekUsage,
}

#[derive(Debug, Deserialize)]
struct DeepSeekChoice {
    message: DeepSeekResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekResponseMessage {
    role: String,
    content: Option<String>,
    /// R1 model returns reasoning in a separate field
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<DeepSeekToolCall>>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
    /// R1 model reports reasoning tokens separately
    #[serde(default)]
    reasoning_tokens: Option<u32>,
    /// Cached tokens for prompt caching
    #[serde(default)]
    prompt_cache_hit_tokens: Option<u32>,
    #[serde(default)]
    prompt_cache_miss_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekError {
    error: DeepSeekErrorDetail,
}

#[derive(Debug, Deserialize)]
struct DeepSeekErrorDetail {
    message: String,
    r#type: String,
    #[serde(default)]
    code: Option<String>,
}

// Streaming types

#[derive(Debug, Deserialize)]
struct DeepSeekStreamResponse {
    choices: Vec<DeepSeekStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekStreamChoice {
    delta: DeepSeekStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekStreamDelta {
    content: Option<String>,
    /// R1 model streams reasoning content separately
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<DeepSeekStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekStreamToolCall {
    index: usize,
    id: Option<String>,
    function: Option<DeepSeekStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

/// Convert Ember ContentPart to DeepSeek ContentPart
fn convert_content_part(part: &crate::types::ContentPart) -> DeepSeekContentPart {
    match part {
        crate::types::ContentPart::Text { text } => {
            DeepSeekContentPart::Text { text: text.clone() }
        }
        crate::types::ContentPart::Image { source, .. } => {
            let url = match source {
                crate::types::ImageSource::Base64 { media_type, data } => {
                    format!("data:{};base64,{}", media_type.as_mime_type(), data)
                }
                crate::types::ImageSource::Url { url } => url.clone(),
            };
            DeepSeekContentPart::ImageUrl {
                image_url: DeepSeekImageUrl { url },
            }
        }
    }
}

impl From<CompletionRequest> for DeepSeekRequest {
    fn from(req: CompletionRequest) -> Self {
        Self {
            model: req.model,
            messages: req
                .messages
                .into_iter()
                .map(|m| {
                    let content = if m.content_parts.is_empty() {
                        DeepSeekContent::Text(m.content)
                    } else {
                        DeepSeekContent::Parts(
                            m.content_parts.iter().map(convert_content_part).collect(),
                        )
                    };

                    DeepSeekMessage {
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
                                    .map(|tc| DeepSeekToolCall {
                                        id: tc.id,
                                        r#type: "function".to_string(),
                                        function: DeepSeekFunctionCall {
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
                    .map(|t| DeepSeekTool {
                        r#type: "function".to_string(),
                        function: DeepSeekFunction {
                            name: t.name,
                            description: t.description,
                            parameters: t.parameters,
                        },
                    })
                    .collect()
            }),
            stream: req.stream,
            frequency_penalty: None,
            presence_penalty: None,
        }
    }
}

impl From<DeepSeekResponse> for CompletionResponse {
    fn from(resp: DeepSeekResponse) -> Self {
        let choice = resp.choices.into_iter().next();

        let (content, tool_calls, finish_reason) = if let Some(c) = choice {
            // Combine regular content with reasoning content (for R1 model)
            let mut full_content = String::new();
            if let Some(reasoning) = c.message.reasoning_content {
                full_content.push_str("<thinking>\n");
                full_content.push_str(&reasoning);
                full_content.push_str("\n</thinking>\n\n");
            }
            if let Some(content) = c.message.content {
                full_content.push_str(&content);
            }

            let tool_calls = c
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
                .unwrap_or_default();

            let finish = c.finish_reason.and_then(|r| match r.as_str() {
                "stop" => Some(FinishReason::Stop),
                "length" => Some(FinishReason::Length),
                "tool_calls" => Some(FinishReason::ToolCalls),
                _ => None,
            });

            (full_content, tool_calls, finish)
        } else {
            (String::new(), Vec::new(), None)
        };

        Self {
            content,
            tool_calls,
            finish_reason,
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
    use crate::Message;

    #[test]
    fn test_deepseek_request_conversion() {
        let request = CompletionRequest::new("deepseek-chat")
            .with_message(Message::system("You are helpful"))
            .with_message(Message::user("Hello"))
            .with_temperature(0.7);

        let deepseek_req = DeepSeekRequest::from(request);

        assert_eq!(deepseek_req.model, "deepseek-chat");
        assert_eq!(deepseek_req.messages.len(), 2);
        assert_eq!(deepseek_req.temperature, Some(0.7));
    }

    #[test]
    fn test_deepseek_models() {
        let models = vec!["deepseek-chat", "deepseek-reasoner", "deepseek-coder"];

        for model in models {
            let request = CompletionRequest::new(model).with_message(Message::user("Test"));
            let deepseek_req = DeepSeekRequest::from(request);
            assert_eq!(deepseek_req.model, model);
        }
    }
}
