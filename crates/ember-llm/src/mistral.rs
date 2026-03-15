//! Mistral AI API provider implementation
//!
//! Supports Mistral Large, Mistral Small, Codestral, and other Mistral models.

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

const DEFAULT_BASE_URL: &str = "https://api.mistral.ai/v1";
const DEFAULT_MODEL: &str = "mistral-large-latest";

/// Mistral AI API provider
///
/// Supports the latest Mistral models including:
/// - mistral-large-latest (default, most capable)
/// - mistral-small-latest (fast, cost-effective)
/// - codestral-latest (optimized for code)
/// - ministral-8b-latest (lightweight)
/// - ministral-3b-latest (ultra-lightweight)
/// - pixtral-large-latest (vision)
///
/// # Example
///
/// ```rust,no_run
/// use ember_llm::{MistralProvider, LLMProvider, CompletionRequest, Message};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let provider = MistralProvider::from_env()?;
///     let request = CompletionRequest::new("mistral-large-latest")
///         .with_message(Message::user("Hello!"));
///     let response = provider.complete(request).await?;
///     println!("{}", response.content);
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct MistralProvider {
    client: Client,
    api_key: String,
    base_url: String,
    default_model: String,
}

impl MistralProvider {
    /// Create a new Mistral provider with explicit API key
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
    /// Looks for `MISTRAL_API_KEY` environment variable.
    pub fn from_env() -> Result<Self> {
        let api_key = env::var("MISTRAL_API_KEY").map_err(|_| Error::api_key_missing("mistral"))?;

        let mut provider = Self::new(api_key);

        if let Ok(base_url) = env::var("MISTRAL_BASE_URL") {
            provider.base_url = base_url;
        }

        if let Ok(model) = env::var("MISTRAL_MODEL") {
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
impl LLMProvider for MistralProvider {
    fn name(&self) -> &str {
        "mistral"
    }

    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let mistral_request = MistralRequest::from(request);

        debug!("Sending request to Mistral API");

        let response = self.build_request().json(&mistral_request).send().await?;

        let status = response.status();

        if !status.is_success() {
            let error_body: MistralError = response.json().await.unwrap_or_else(|_| MistralError {
                message: "Unknown error".to_string(),
                request_id: None,
            });

            return match status.as_u16() {
                401 => Err(Error::api_key_missing("mistral")),
                429 => Err(Error::rate_limit("mistral", None)),
                _ => Err(Error::api_error(
                    "mistral",
                    status.as_u16(),
                    error_body.message,
                )),
            };
        }

        let mistral_response: MistralResponse = response.json().await?;

        Ok(mistral_response.into())
    }

    async fn complete_stream(&self, request: CompletionRequest) -> Result<StreamResponse> {
        let mut mistral_request = MistralRequest::from(request);
        mistral_request.stream = Some(true);

        debug!("Starting streaming request to Mistral");

        let response = self.build_request().json(&mistral_request).send().await?;

        let status = response.status();

        if !status.is_success() {
            let error_body: MistralError = response.json().await.unwrap_or_else(|_| MistralError {
                message: "Unknown error".to_string(),
                request_id: None,
            });

            return match status.as_u16() {
                401 => Err(Error::api_key_missing("mistral")),
                429 => Err(Error::rate_limit("mistral", None)),
                _ => Err(Error::api_error(
                    "mistral",
                    status.as_u16(),
                    error_body.message,
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
                                serde_json::from_str::<MistralStreamResponse>(data)
                            {
                                if let Some(choice) = chunk_response.choices.first() {
                                    let stream_chunk = StreamChunk {
                                        content: choice.delta.content.clone(),
                                        tool_calls: choice.delta.tool_calls.as_ref().map(|tcs| {
                                            tcs.iter()
                                                .enumerate()
                                                .map(|(i, tc)| ToolCallDelta {
                                                    index: i,
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
                                                "model_length" => Some(FinishReason::Length),
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
                "mistral",
                response.status().as_u16(),
                "Failed to list models",
            ));
        }

        let models_response: MistralModelsResponse = response.json().await?;

        Ok(models_response
            .data
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id.clone(),
                name: m.id,
                description: m.description,
                context_window: m.max_context_length,
                max_output_tokens: None,
                supports_tools: true,
                supports_vision: m.capabilities.as_ref().map_or(false, |c| c.vision),
                provider: "mistral".to_string(),
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
        // Pixtral models support vision
        true
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }
}

// Mistral API types

#[derive(Debug, Serialize)]
struct MistralRequest {
    model: String,
    messages: Vec<MistralMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<MistralTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    safe_prompt: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MistralMessage {
    role: String,
    content: MistralContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<MistralToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// Content for Mistral messages - either simple text or multimodal parts
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum MistralContent {
    /// Simple text content
    Text(String),
    /// Array of content parts for multimodal
    Parts(Vec<MistralContentPart>),
}

/// A content part for multimodal Mistral messages
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum MistralContentPart {
    /// Text content
    #[serde(rename = "text")]
    Text {
        /// The text content
        text: String,
    },
    /// Image URL content
    #[serde(rename = "image_url")]
    ImageUrl {
        /// The image URL
        image_url: MistralImageUrl,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct MistralImageUrl {
    url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MistralTool {
    r#type: String,
    function: MistralFunction,
}

#[derive(Debug, Serialize, Deserialize)]
struct MistralFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct MistralToolCall {
    id: Option<String>,
    r#type: String,
    function: MistralFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct MistralFunctionCall {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MistralResponse {
    id: String,
    model: String,
    choices: Vec<MistralChoice>,
    usage: MistralUsage,
}

#[derive(Debug, Deserialize)]
struct MistralChoice {
    message: MistralMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MistralUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct MistralError {
    message: String,
    #[serde(default)]
    request_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MistralModelsResponse {
    data: Vec<MistralModelInfo>,
}

#[derive(Debug, Deserialize)]
struct MistralModelInfo {
    id: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    max_context_length: Option<u32>,
    #[serde(default)]
    capabilities: Option<MistralCapabilities>,
}

#[derive(Debug, Deserialize)]
struct MistralCapabilities {
    #[serde(default)]
    vision: bool,
    #[serde(default)]
    function_calling: bool,
}

// Streaming response types

#[derive(Debug, Deserialize)]
struct MistralStreamResponse {
    choices: Vec<MistralStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct MistralStreamChoice {
    delta: MistralStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MistralStreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<MistralStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct MistralStreamToolCall {
    id: Option<String>,
    function: Option<MistralStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct MistralStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

/// Convert Ember ContentPart to Mistral ContentPart
fn convert_content_part(part: &ContentPart) -> MistralContentPart {
    match part {
        ContentPart::Text { text } => MistralContentPart::Text { text: text.clone() },
        ContentPart::Image { source, .. } => {
            let url = match source {
                ImageSource::Base64 { media_type, data } => {
                    format!("data:{};base64,{}", media_type.as_mime_type(), data)
                }
                ImageSource::Url { url } => url.clone(),
            };
            MistralContentPart::ImageUrl {
                image_url: MistralImageUrl { url },
            }
        }
    }
}

impl From<CompletionRequest> for MistralRequest {
    fn from(req: CompletionRequest) -> Self {
        Self {
            model: req.model,
            messages: req
                .messages
                .into_iter()
                .map(|m| {
                    // Determine content: use multimodal parts if present
                    let content = if m.content_parts.is_empty() {
                        MistralContent::Text(m.content)
                    } else {
                        MistralContent::Parts(
                            m.content_parts.iter().map(convert_content_part).collect(),
                        )
                    };

                    MistralMessage {
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
                                    .map(|tc| MistralToolCall {
                                        id: Some(tc.id),
                                        r#type: "function".to_string(),
                                        function: MistralFunctionCall {
                                            name: Some(tc.name),
                                            arguments: Some(tc.arguments.to_string()),
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
                    .map(|t| MistralTool {
                        r#type: "function".to_string(),
                        function: MistralFunction {
                            name: t.name,
                            description: t.description,
                            parameters: t.parameters,
                        },
                    })
                    .collect()
            }),
            tool_choice: None,
            stream: req.stream,
            safe_prompt: None,
        }
    }
}

impl From<MistralResponse> for CompletionResponse {
    fn from(resp: MistralResponse) -> Self {
        let choice = resp.choices.into_iter().next().unwrap_or(MistralChoice {
            message: MistralMessage {
                role: "assistant".to_string(),
                content: MistralContent::Text(String::new()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            finish_reason: None,
        });

        // Extract text content
        let content = match choice.message.content {
            MistralContent::Text(text) => text,
            MistralContent::Parts(parts) => parts
                .into_iter()
                .filter_map(|p| match p {
                    MistralContentPart::Text { text } => Some(text),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
        };

        Self {
            content,
            tool_calls: choice
                .message
                .tool_calls
                .map(|tcs| {
                    tcs.into_iter()
                        .filter_map(|tc| {
                            let id = tc.id?;
                            let name = tc.function.name?;
                            let args = tc.function.arguments.unwrap_or_default();
                            Some(ToolCall::new(
                                id,
                                name,
                                serde_json::from_str(&args).unwrap_or(serde_json::Value::Null),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            finish_reason: choice.finish_reason.and_then(|r| match r.as_str() {
                "stop" => Some(FinishReason::Stop),
                "length" | "model_length" => Some(FinishReason::Length),
                "tool_calls" => Some(FinishReason::ToolCalls),
                _ => None,
            }),
            usage: TokenUsage {
                prompt_tokens: resp.usage.prompt_tokens,
                completion_tokens: resp.usage.completion_tokens,
                total_tokens: resp.usage.total_tokens,
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
    fn test_mistral_request_conversion() {
        let request = CompletionRequest::new("mistral-large-latest")
            .with_message(Message::system("You are helpful"))
            .with_message(Message::user("Hello"))
            .with_temperature(0.7);

        let mistral_req = MistralRequest::from(request);

        assert_eq!(mistral_req.model, "mistral-large-latest");
        assert_eq!(mistral_req.messages.len(), 2);
        assert_eq!(mistral_req.temperature, Some(0.7));
    }

    #[test]
    fn test_mistral_role_mapping() {
        let request = CompletionRequest::new("mistral-small-latest")
            .with_message(Message::system("System"))
            .with_message(Message::user("User"))
            .with_message(Message::assistant("Assistant"));

        let mistral_req = MistralRequest::from(request);

        assert_eq!(mistral_req.messages[0].role, "system");
        assert_eq!(mistral_req.messages[1].role, "user");
        assert_eq!(mistral_req.messages[2].role, "assistant");
    }
}
