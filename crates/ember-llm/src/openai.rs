//! OpenAI API provider implementation

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

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-4o";

/// OpenAI API provider
#[derive(Debug, Clone)]
pub struct OpenAIProvider {
    client: Client,
    api_key: String,
    base_url: String,
    default_model: String,
    organization: Option<String>,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider with explicit API key
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            default_model: DEFAULT_MODEL.to_string(),
            organization: None,
        }
    }

    /// Create a provider from environment variables
    ///
    /// Looks for `OPENAI_API_KEY` environment variable.
    pub fn from_env() -> Result<Self> {
        let api_key = env::var("OPENAI_API_KEY").map_err(|_| Error::api_key_missing("openai"))?;

        let mut provider = Self::new(api_key);

        if let Ok(base_url) = env::var("OPENAI_BASE_URL") {
            provider.base_url = base_url;
        }

        if let Ok(org) = env::var("OPENAI_ORGANIZATION") {
            provider.organization = Some(org);
        }

        Ok(provider)
    }

    /// Set a custom base URL (for OpenAI-compatible APIs)
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the default model
    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }

    /// Set organization ID
    pub fn with_organization(mut self, org: impl Into<String>) -> Self {
        self.organization = Some(org.into());
        self
    }

    fn build_request(&self) -> reqwest::RequestBuilder {
        let mut builder = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json");

        if let Some(ref org) = self.organization {
            builder = builder.header("OpenAI-Organization", org);
        }

        builder
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let openai_request = OpenAIRequest::from(request);

        debug!("Sending request to OpenAI");

        let response = self.build_request().json(&openai_request).send().await?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let error_msg = serde_json::from_str::<OpenAIError>(&error_text)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| if error_text.is_empty() {
                    format!("HTTP {} (empty response)", status.as_u16())
                } else { error_text });

            return match status.as_u16() {
                401 => Err(Error::api_key_missing("openai")),
                429 => Err(Error::rate_limit("openai", None)),
                _ => Err(Error::api_error("openai", status.as_u16(), error_msg)),
            };
        }

        let openai_response: OpenAIResponse = response.json().await?;

        Ok(openai_response.into())
    }

    async fn complete_stream(&self, request: CompletionRequest) -> Result<StreamResponse> {
        let mut openai_request = OpenAIRequest::from(request);
        openai_request.stream = Some(true);
        openai_request.stream_options = Some(StreamOptions {
            include_usage: true,
        });

        debug!("Starting streaming request to OpenAI");

        let response = self.build_request().json(&openai_request).send().await?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let error_msg = serde_json::from_str::<OpenAIError>(&error_text)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| if error_text.is_empty() {
                    format!("HTTP {} (empty response)", status.as_u16())
                } else { error_text });

            return match status.as_u16() {
                401 => Err(Error::api_key_missing("openai")),
                429 => Err(Error::rate_limit("openai", None)),
                _ => Err(Error::api_error("openai", status.as_u16(), error_msg)),
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
                                serde_json::from_str::<OpenAIStreamResponse>(data)
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
                "openai",
                response.status().as_u16(),
                "Failed to list models",
            ));
        }

        let models_response: OpenAIModelsResponse = response.json().await?;

        Ok(models_response
            .data
            .into_iter()
            .filter(|m| m.id.starts_with("gpt"))
            .map(|m| ModelInfo {
                id: m.id.clone(),
                name: m.id,
                description: None,
                context_window: None,
                max_output_tokens: None,
                supports_tools: true,
                supports_vision: false,
                provider: "openai".to_string(),
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
        true
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }
}

// OpenAI API types

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    /// Content can be either a simple string or an array of content parts for multimodal
    content: OpenAIContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// Content for OpenAI messages - either simple text or multimodal parts
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum OpenAIContent {
    /// Simple text content (backwards compatible)
    Text(String),
    /// Array of content parts for multimodal (text + images)
    Parts(Vec<OpenAIContentPart>),
}

/// A content part for multimodal OpenAI messages
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum OpenAIContentPart {
    /// Text content
    #[serde(rename = "text")]
    Text {
        /// The text content
        text: String,
    },
    /// Image URL content (also used for base64 data URLs)
    #[serde(rename = "image_url")]
    ImageUrl {
        /// The image URL or data URL
        image_url: OpenAIImageUrl,
    },
}

/// Image URL structure for OpenAI
#[derive(Debug, Serialize, Deserialize)]
struct OpenAIImageUrl {
    /// URL to the image (can be a data: URL for base64)
    url: String,
    /// Optional detail level: auto, low, or high
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAITool {
    r#type: String,
    function: OpenAIFunction,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIToolCall {
    id: String,
    r#type: String,
    function: OpenAIFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    id: String,
    model: String,
    choices: Vec<OpenAIChoice>,
    usage: OpenAIUsage,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct OpenAIError {
    error: OpenAIErrorDetail,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OpenAIErrorDetail {
    message: String,
    r#type: String,
    code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIModelsResponse {
    data: Vec<OpenAIModel>,
}

// Streaming response types

#[derive(Debug, Deserialize)]
struct OpenAIStreamResponse {
    choices: Vec<OpenAIStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    delta: OpenAIStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamToolCall {
    index: usize,
    id: Option<String>,
    function: Option<OpenAIStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIModel {
    id: String,
}

// Conversion implementations

/// Convert Ember ContentPart to OpenAI ContentPart
fn convert_content_part(part: &crate::types::ContentPart) -> OpenAIContentPart {
    match part {
        crate::types::ContentPart::Text { text } => OpenAIContentPart::Text { text: text.clone() },
        crate::types::ContentPart::Image { source, .. } => {
            let url = match source {
                crate::types::ImageSource::Base64 { media_type, data } => {
                    format!("data:{};base64,{}", media_type.as_mime_type(), data)
                }
                crate::types::ImageSource::Url { url } => url.clone(),
            };
            OpenAIContentPart::ImageUrl {
                image_url: OpenAIImageUrl {
                    url,
                    detail: Some("auto".to_string()),
                },
            }
        }
    }
}

impl From<CompletionRequest> for OpenAIRequest {
    fn from(req: CompletionRequest) -> Self {
        Self {
            model: req.model,
            stream_options: None,
            messages: req
                .messages
                .into_iter()
                .map(|m| {
                    // Determine content: use multimodal parts if present, otherwise simple text
                    let content = if m.content_parts.is_empty() {
                        OpenAIContent::Text(m.content)
                    } else {
                        OpenAIContent::Parts(
                            m.content_parts.iter().map(convert_content_part).collect(),
                        )
                    };

                    OpenAIMessage {
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
                                    .map(|tc| OpenAIToolCall {
                                        id: tc.id,
                                        r#type: "function".to_string(),
                                        function: OpenAIFunctionCall {
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
                    .map(|t| OpenAITool {
                        r#type: "function".to_string(),
                        function: OpenAIFunction {
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

impl From<OpenAIResponse> for CompletionResponse {
    fn from(resp: OpenAIResponse) -> Self {
        let choice = resp.choices.into_iter().next().unwrap_or(OpenAIChoice {
            message: OpenAIMessage {
                role: "assistant".to_string(),
                content: OpenAIContent::Text(String::new()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            finish_reason: None,
        });

        // Extract text content from OpenAIContent
        let content = match choice.message.content {
            OpenAIContent::Text(text) => text,
            OpenAIContent::Parts(parts) => {
                // Concatenate all text parts
                parts
                    .into_iter()
                    .filter_map(|p| match p {
                        OpenAIContentPart::Text { text } => Some(text),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("")
            }
        };

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
    fn test_openai_request_conversion() {
        use crate::Message;

        let request = CompletionRequest::new("gpt-4")
            .with_message(Message::system("You are helpful"))
            .with_message(Message::user("Hello"))
            .with_temperature(0.7);

        let openai_req = OpenAIRequest::from(request);

        assert_eq!(openai_req.model, "gpt-4");
        assert_eq!(openai_req.messages.len(), 2);
        assert_eq!(openai_req.temperature, Some(0.7));
    }
}
