//! OpenRouter API provider implementation
//!
//! OpenRouter provides unified access to 200+ AI models from all major providers
//! through a single API. This includes OpenAI, Anthropic, Google, Meta, Mistral,
//! and many more.

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

const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
const DEFAULT_MODEL: &str = "anthropic/claude-3.5-sonnet";

/// OpenRouter API provider
///
/// Provides unified access to 200+ AI models including:
/// - OpenAI: GPT-4o, GPT-4-turbo, o1-preview, o1-mini
/// - Anthropic: Claude 3.5 Sonnet, Claude 3 Opus, Claude 3 Haiku
/// - Google: Gemini 2.0 Flash, Gemini 1.5 Pro
/// - Meta: Llama 3.3 70B, Llama 3.2 90B Vision
/// - Mistral: Mistral Large, Codestral
/// - DeepSeek: DeepSeek V3, DeepSeek R1
/// - And many more!
///
/// # Example
///
/// ```rust,no_run
/// use ember_llm::{OpenRouterProvider, LLMProvider, CompletionRequest, Message};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let provider = OpenRouterProvider::from_env()?;
///     
///     // Use any model from any provider!
///     let request = CompletionRequest::new("anthropic/claude-3.5-sonnet")
///         .with_message(Message::user("Hello!"));
///     let response = provider.complete(request).await?;
///     println!("{}", response.content);
///     
///     // Switch to a different provider's model
///     let request = CompletionRequest::new("google/gemini-2.0-flash-exp:free")
///         .with_message(Message::user("Hello from Gemini!"));
///     let response = provider.complete(request).await?;
///     println!("{}", response.content);
///     
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct OpenRouterProvider {
    client: Client,
    api_key: String,
    base_url: String,
    default_model: String,
    site_url: Option<String>,
    site_name: Option<String>,
}

impl OpenRouterProvider {
    /// Create a new OpenRouter provider with explicit API key
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            default_model: DEFAULT_MODEL.to_string(),
            site_url: None,
            site_name: None,
        }
    }

    /// Create a provider from environment variables
    ///
    /// Looks for `OPENROUTER_API_KEY` environment variable.
    /// Optionally reads `OPENROUTER_SITE_URL` and `OPENROUTER_SITE_NAME` for
    /// attribution on the OpenRouter leaderboard.
    pub fn from_env() -> Result<Self> {
        let api_key =
            env::var("OPENROUTER_API_KEY").map_err(|_| Error::api_key_missing("openrouter"))?;

        let mut provider = Self::new(api_key);

        if let Ok(base_url) = env::var("OPENROUTER_BASE_URL") {
            provider.base_url = base_url;
        }

        if let Ok(model) = env::var("OPENROUTER_MODEL") {
            provider.default_model = model;
        }

        if let Ok(site_url) = env::var("OPENROUTER_SITE_URL") {
            provider.site_url = Some(site_url);
        }

        if let Ok(site_name) = env::var("OPENROUTER_SITE_NAME") {
            provider.site_name = Some(site_name);
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

    /// Set site URL for OpenRouter leaderboard attribution
    pub fn with_site_url(mut self, url: impl Into<String>) -> Self {
        self.site_url = Some(url.into());
        self
    }

    /// Set site name for OpenRouter leaderboard attribution
    pub fn with_site_name(mut self, name: impl Into<String>) -> Self {
        self.site_name = Some(name.into());
        self
    }

    fn build_request(&self) -> reqwest::RequestBuilder {
        let mut builder = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json");

        // Add optional headers for OpenRouter attribution
        if let Some(ref site_url) = self.site_url {
            builder = builder.header("HTTP-Referer", site_url);
        }

        if let Some(ref site_name) = self.site_name {
            builder = builder.header("X-Title", site_name);
        }

        builder
    }
}

#[async_trait]
impl LLMProvider for OpenRouterProvider {
    fn name(&self) -> &str {
        "openrouter"
    }

    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let openrouter_request = OpenRouterRequest::from(request);

        debug!("Sending request to OpenRouter API");

        let response = self
            .build_request()
            .json(&openrouter_request)
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let error_body: OpenRouterError =
                response.json().await.unwrap_or_else(|_| OpenRouterError {
                    error: OpenRouterErrorDetail {
                        message: "Unknown error".to_string(),
                        code: None,
                        r#type: None,
                    },
                });

            return match status.as_u16() {
                401 => Err(Error::api_key_missing("openrouter")),
                402 => Err(Error::api_error(
                    "openrouter",
                    402,
                    "Insufficient credits. Add credits at https://openrouter.ai/credits",
                )),
                429 => Err(Error::rate_limit("openrouter", None)),
                _ => Err(Error::api_error(
                    "openrouter",
                    status.as_u16(),
                    error_body.error.message,
                )),
            };
        }

        let openrouter_response: OpenRouterResponse = response.json().await?;

        Ok(openrouter_response.into())
    }

    async fn complete_stream(&self, request: CompletionRequest) -> Result<StreamResponse> {
        let mut openrouter_request = OpenRouterRequest::from(request);
        openrouter_request.stream = Some(true);

        debug!("Starting streaming request to OpenRouter");

        let response = self
            .build_request()
            .json(&openrouter_request)
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let error_body: OpenRouterError =
                response.json().await.unwrap_or_else(|_| OpenRouterError {
                    error: OpenRouterErrorDetail {
                        message: "Unknown error".to_string(),
                        code: None,
                        r#type: None,
                    },
                });

            return match status.as_u16() {
                401 => Err(Error::api_key_missing("openrouter")),
                402 => Err(Error::api_error("openrouter", 402, "Insufficient credits")),
                429 => Err(Error::rate_limit("openrouter", None)),
                _ => Err(Error::api_error(
                    "openrouter",
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
                                serde_json::from_str::<OpenRouterStreamResponse>(data)
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
                                                "content_filter" => {
                                                    Some(FinishReason::ContentFilter)
                                                }
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
                "openrouter",
                response.status().as_u16(),
                "Failed to list models",
            ));
        }

        let models_response: OpenRouterModelsResponse = response.json().await?;

        Ok(models_response
            .data
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id.clone(),
                name: m.name.unwrap_or_else(|| m.id.clone()),
                description: m.description,
                context_window: Some(m.context_length),
                max_output_tokens: m
                    .top_provider
                    .as_ref()
                    .and_then(|p| p.max_completion_tokens),
                supports_tools: true, // Most models support tools via OpenRouter
                supports_vision: m.architecture.as_ref().map_or(false, |a| {
                    a.modality.as_ref().map_or(false, |m| m.contains("image"))
                }),
                provider: "openrouter".to_string(),
            })
            .collect())
    }

    async fn health_check(&self) -> Result<()> {
        // Use the models endpoint to check health
        self.list_models().await?;
        Ok(())
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn supports_vision(&self) -> bool {
        true
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }
}

// OpenRouter API types (OpenAI-compatible with extensions)

#[derive(Debug, Serialize)]
struct OpenRouterRequest {
    model: String,
    messages: Vec<OpenRouterMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenRouterTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    /// OpenRouter-specific: Route to specific providers
    #[serde(skip_serializing_if = "Option::is_none")]
    route: Option<String>,
    /// OpenRouter-specific: Transforms for prompt handling
    #[serde(skip_serializing_if = "Option::is_none")]
    transforms: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenRouterMessage {
    role: String,
    content: OpenRouterContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenRouterToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// Content for OpenRouter messages
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum OpenRouterContent {
    Text(String),
    Parts(Vec<OpenRouterContentPart>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum OpenRouterContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: OpenRouterImageUrl },
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenRouterImageUrl {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenRouterTool {
    r#type: String,
    function: OpenRouterFunction,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenRouterFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenRouterToolCall {
    id: String,
    r#type: String,
    function: OpenRouterFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenRouterFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenRouterResponse {
    id: String,
    model: String,
    choices: Vec<OpenRouterChoice>,
    usage: Option<OpenRouterUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterChoice {
    message: OpenRouterMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct OpenRouterError {
    error: OpenRouterErrorDetail,
}

#[derive(Debug, Deserialize)]
struct OpenRouterErrorDetail {
    message: String,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModelInfo>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModelInfo {
    id: String,
    name: Option<String>,
    description: Option<String>,
    context_length: u32,
    #[serde(default)]
    top_provider: Option<OpenRouterTopProvider>,
    #[serde(default)]
    architecture: Option<OpenRouterArchitecture>,
    #[serde(default)]
    pricing: Option<OpenRouterPricing>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterTopProvider {
    #[serde(default)]
    max_completion_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterArchitecture {
    #[serde(default)]
    modality: Option<String>,
    #[serde(default)]
    tokenizer: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterPricing {
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    completion: Option<String>,
}

// Streaming types

#[derive(Debug, Deserialize)]
struct OpenRouterStreamResponse {
    choices: Vec<OpenRouterStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamChoice {
    delta: OpenRouterStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenRouterStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamToolCall {
    index: usize,
    id: Option<String>,
    function: Option<OpenRouterStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

/// Convert Ember ContentPart to OpenRouter ContentPart
fn convert_content_part(part: &crate::types::ContentPart) -> OpenRouterContentPart {
    match part {
        crate::types::ContentPart::Text { text } => {
            OpenRouterContentPart::Text { text: text.clone() }
        }
        crate::types::ContentPart::Image { source, .. } => {
            let url = match source {
                crate::types::ImageSource::Base64 { media_type, data } => {
                    format!("data:{};base64,{}", media_type.as_mime_type(), data)
                }
                crate::types::ImageSource::Url { url } => url.clone(),
            };
            OpenRouterContentPart::ImageUrl {
                image_url: OpenRouterImageUrl {
                    url,
                    detail: Some("auto".to_string()),
                },
            }
        }
    }
}

impl From<CompletionRequest> for OpenRouterRequest {
    fn from(req: CompletionRequest) -> Self {
        Self {
            model: req.model,
            messages: req
                .messages
                .into_iter()
                .map(|m| {
                    let content = if m.content_parts.is_empty() {
                        OpenRouterContent::Text(m.content)
                    } else {
                        OpenRouterContent::Parts(
                            m.content_parts.iter().map(convert_content_part).collect(),
                        )
                    };

                    OpenRouterMessage {
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
                                    .map(|tc| OpenRouterToolCall {
                                        id: tc.id,
                                        r#type: "function".to_string(),
                                        function: OpenRouterFunctionCall {
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
                    .map(|t| OpenRouterTool {
                        r#type: "function".to_string(),
                        function: OpenRouterFunction {
                            name: t.name,
                            description: t.description,
                            parameters: t.parameters,
                        },
                    })
                    .collect()
            }),
            stream: req.stream,
            route: None,
            transforms: None,
        }
    }
}

impl From<OpenRouterResponse> for CompletionResponse {
    fn from(resp: OpenRouterResponse) -> Self {
        let choice = resp.choices.into_iter().next().unwrap_or(OpenRouterChoice {
            message: OpenRouterMessage {
                role: "assistant".to_string(),
                content: OpenRouterContent::Text(String::new()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            finish_reason: None,
        });

        let content = match choice.message.content {
            OpenRouterContent::Text(text) => text,
            OpenRouterContent::Parts(parts) => parts
                .into_iter()
                .filter_map(|p| match p {
                    OpenRouterContentPart::Text { text } => Some(text),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
        };

        let usage = resp.usage.map_or(
            TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                ..Default::default()
            },
            |u| TokenUsage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
                ..Default::default()
            },
        );

        Self {
            content,
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
                "content_filter" => Some(FinishReason::ContentFilter),
                _ => None,
            }),
            usage,
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
    fn test_openrouter_request_conversion() {
        let request = CompletionRequest::new("anthropic/claude-3.5-sonnet")
            .with_message(Message::system("You are helpful"))
            .with_message(Message::user("Hello"))
            .with_temperature(0.7);

        let openrouter_req = OpenRouterRequest::from(request);

        assert_eq!(openrouter_req.model, "anthropic/claude-3.5-sonnet");
        assert_eq!(openrouter_req.messages.len(), 2);
        assert_eq!(openrouter_req.temperature, Some(0.7));
    }

    #[test]
    fn test_openrouter_free_model() {
        let request = CompletionRequest::new("google/gemini-2.0-flash-exp:free")
            .with_message(Message::user("Hello"));

        let openrouter_req = OpenRouterRequest::from(request);

        assert_eq!(openrouter_req.model, "google/gemini-2.0-flash-exp:free");
    }

    #[test]
    fn test_openrouter_model_formats() {
        // Test various model ID formats
        let models = vec![
            "openai/gpt-4o",
            "anthropic/claude-3.5-sonnet",
            "google/gemini-1.5-pro",
            "meta-llama/llama-3.3-70b-instruct",
            "mistralai/mistral-large-latest",
            "deepseek/deepseek-r1",
            "qwen/qwen-2.5-72b-instruct",
        ];

        for model in models {
            let request = CompletionRequest::new(model).with_message(Message::user("Test"));
            let openrouter_req = OpenRouterRequest::from(request);
            assert_eq!(openrouter_req.model, model);
        }
    }
}
