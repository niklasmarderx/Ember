//! Anthropic Claude API provider implementation

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

use futures::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_MODEL: &str = "claude-3-5-sonnet-20241022";
const API_VERSION: &str = "2023-06-01";

/// Anthropic Claude API provider
#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    base_url: String,
    default_model: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider with explicit API key
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
    /// Looks for `ANTHROPIC_API_KEY` environment variable.
    pub fn from_env() -> Result<Self> {
        let api_key =
            env::var("ANTHROPIC_API_KEY").map_err(|_| Error::api_key_missing("anthropic"))?;

        let mut provider = Self::new(api_key);

        if let Ok(base_url) = env::var("ANTHROPIC_BASE_URL") {
            provider.base_url = base_url;
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
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("Content-Type", "application/json")
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let anthropic_request = AnthropicRequest::from(request);

        debug!("Sending request to Anthropic");

        let response = self.build_request().json(&anthropic_request).send().await?;

        let status = response.status();

        if !status.is_success() {
            let error_body: AnthropicError =
                response.json().await.unwrap_or_else(|_| AnthropicError {
                    r#type: "error".to_string(),
                    error: AnthropicErrorDetail {
                        r#type: "unknown".to_string(),
                        message: "Unknown error".to_string(),
                    },
                });

            return match status.as_u16() {
                401 => Err(Error::api_key_missing("anthropic")),
                429 => Err(Error::rate_limit("anthropic", None)),
                _ => Err(Error::api_error(
                    "anthropic",
                    status.as_u16(),
                    error_body.error.message,
                )),
            };
        }

        let anthropic_response: AnthropicResponse = response.json().await?;

        Ok(anthropic_response.into())
    }

    async fn complete_stream(&self, request: CompletionRequest) -> Result<StreamResponse> {
        let mut anthropic_request = AnthropicRequest::from(request);
        anthropic_request.stream = Some(true);

        debug!("Starting streaming request to Anthropic");

        let response = self.build_request().json(&anthropic_request).send().await?;

        let status = response.status();

        if !status.is_success() {
            let error_body: AnthropicError =
                response.json().await.unwrap_or_else(|_| AnthropicError {
                    r#type: "error".to_string(),
                    error: AnthropicErrorDetail {
                        r#type: "unknown".to_string(),
                        message: "Unknown error".to_string(),
                    },
                });

            return match status.as_u16() {
                401 => Err(Error::api_key_missing("anthropic")),
                429 => Err(Error::rate_limit("anthropic", None)),
                _ => Err(Error::api_error(
                    "anthropic",
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
            let mut current_tool_index = 0;

            while let Ok(Some(chunk)) = stream.try_next().await {
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete SSE events
                while let Some(pos) = buffer.find("\n\n") {
                    let event = buffer[..pos].to_string();
                    buffer = buffer[pos + 2..].to_string();

                    // Parse SSE event
                    let mut event_type = String::new();
                    let mut event_data = String::new();

                    for line in event.lines() {
                        if let Some(t) = line.strip_prefix("event: ") {
                            event_type = t.to_string();
                        } else if let Some(d) = line.strip_prefix("data: ") {
                            event_data = d.to_string();
                        }
                    }

                    match event_type.as_str() {
                        "content_block_delta" => {
                            if let Ok(delta) =
                                serde_json::from_str::<ContentBlockDelta>(&event_data)
                            {
                                let stream_chunk = match delta.delta {
                                    DeltaContent::TextDelta { text } => StreamChunk {
                                        content: Some(text),
                                        tool_calls: None,
                                        done: false,
                                        finish_reason: None,
                                    },
                                    DeltaContent::InputJsonDelta { partial_json } => StreamChunk {
                                        content: None,
                                        tool_calls: Some(vec![ToolCallDelta {
                                            index: current_tool_index,
                                            id: None,
                                            name: None,
                                            arguments: Some(partial_json),
                                        }]),
                                        done: false,
                                        finish_reason: None,
                                    },
                                };

                                if tx.send(Ok(stream_chunk)).await.is_err() {
                                    return;
                                }
                            }
                        }
                        "content_block_start" => {
                            if let Ok(start) =
                                serde_json::from_str::<ContentBlockStart>(&event_data)
                            {
                                if let ContentBlock::ToolUse { id, name, .. } = start.content_block
                                {
                                    current_tool_index = start.index;
                                    let stream_chunk = StreamChunk {
                                        content: None,
                                        tool_calls: Some(vec![ToolCallDelta {
                                            index: current_tool_index,
                                            id: Some(id),
                                            name: Some(name),
                                            arguments: None,
                                        }]),
                                        done: false,
                                        finish_reason: None,
                                    };

                                    if tx.send(Ok(stream_chunk)).await.is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                        "message_stop" => {
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
                        "message_delta" => {
                            if let Ok(delta) = serde_json::from_str::<MessageDelta>(&event_data) {
                                if let Some(stop_reason) = delta.delta.stop_reason {
                                    let finish_reason = match stop_reason.as_str() {
                                        "end_turn" | "stop_sequence" => Some(FinishReason::Stop),
                                        "max_tokens" => Some(FinishReason::Length),
                                        "tool_use" => Some(FinishReason::ToolCalls),
                                        _ => None,
                                    };

                                    let _ = tx
                                        .send(Ok(StreamChunk {
                                            content: None,
                                            tool_calls: None,
                                            done: true,
                                            finish_reason,
                                        }))
                                        .await;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Anthropic doesn't have a models endpoint, return known models
        Ok(vec![
            ModelInfo {
                id: "claude-3-5-sonnet-20241022".to_string(),
                name: "Claude 3.5 Sonnet".to_string(),
                description: Some("Most intelligent model, best for complex tasks".to_string()),
                context_window: Some(200000),
                max_output_tokens: Some(8192),
                supports_tools: true,
                supports_vision: true,
                provider: "anthropic".to_string(),
            },
            ModelInfo {
                id: "claude-3-5-haiku-20241022".to_string(),
                name: "Claude 3.5 Haiku".to_string(),
                description: Some("Fastest model, best for simple tasks".to_string()),
                context_window: Some(200000),
                max_output_tokens: Some(8192),
                supports_tools: true,
                supports_vision: true,
                provider: "anthropic".to_string(),
            },
            ModelInfo {
                id: "claude-3-opus-20240229".to_string(),
                name: "Claude 3 Opus".to_string(),
                description: Some("Previous most capable model".to_string()),
                context_window: Some(200000),
                max_output_tokens: Some(4096),
                supports_tools: true,
                supports_vision: true,
                provider: "anthropic".to_string(),
            },
        ])
    }

    async fn health_check(&self) -> Result<()> {
        // Simple health check - try to make a minimal request
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
        true
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }
}

// Anthropic API types

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: AnthropicImageSource },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

/// Image source for Anthropic API
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum AnthropicImageSource {
    /// Base64 encoded image
    #[serde(rename = "base64")]
    Base64 {
        /// MIME type of the image
        media_type: String,
        /// Base64 encoded image data
        data: String,
    },
    /// URL to image (Anthropic may not support this directly)
    #[serde(rename = "url")]
    Url {
        /// URL to the image
        url: String,
    },
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    model: String,
    content: Vec<ContentBlock>,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct AnthropicError {
    r#type: String,
    error: AnthropicErrorDetail,
}

#[derive(Debug, Deserialize)]
struct AnthropicErrorDetail {
    r#type: String,
    message: String,
}

// Streaming types

#[derive(Debug, Deserialize)]
struct ContentBlockStart {
    index: usize,
    content_block: ContentBlock,
}

#[derive(Debug, Deserialize)]
struct ContentBlockDelta {
    index: usize,
    delta: DeltaContent,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum DeltaContent {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Deserialize)]
struct MessageDelta {
    delta: MessageDeltaContent,
}

#[derive(Debug, Deserialize)]
struct MessageDeltaContent {
    stop_reason: Option<String>,
}

// Conversion implementations

/// Convert Ember ContentPart to Anthropic ContentBlock
fn convert_content_part_to_anthropic(part: &crate::types::ContentPart) -> ContentBlock {
    match part {
        crate::types::ContentPart::Text { text } => ContentBlock::Text { text: text.clone() },
        crate::types::ContentPart::Image { source, .. } => match source {
            crate::types::ImageSource::Base64 { media_type, data } => ContentBlock::Image {
                source: AnthropicImageSource::Base64 {
                    media_type: media_type.as_mime_type().to_string(),
                    data: data.clone(),
                },
            },
            crate::types::ImageSource::Url { url } => ContentBlock::Image {
                source: AnthropicImageSource::Url { url: url.clone() },
            },
        },
    }
}

impl From<CompletionRequest> for AnthropicRequest {
    fn from(req: CompletionRequest) -> Self {
        let mut system = None;
        let mut messages = Vec::new();

        for msg in req.messages {
            match msg.role {
                crate::Role::System => {
                    // System messages might also have content_parts, but typically just text
                    system = Some(msg.content);
                }
                crate::Role::User => {
                    // Check if message has multimodal content
                    if msg.content_parts.is_empty() {
                        messages.push(AnthropicMessage {
                            role: "user".to_string(),
                            content: AnthropicContent::Text(msg.content),
                        });
                    } else {
                        // Convert content_parts to Anthropic blocks
                        let blocks: Vec<ContentBlock> = msg
                            .content_parts
                            .iter()
                            .map(convert_content_part_to_anthropic)
                            .collect();
                        messages.push(AnthropicMessage {
                            role: "user".to_string(),
                            content: AnthropicContent::Blocks(blocks),
                        });
                    }
                }
                crate::Role::Assistant => {
                    if msg.tool_calls.is_empty() {
                        messages.push(AnthropicMessage {
                            role: "assistant".to_string(),
                            content: AnthropicContent::Text(msg.content),
                        });
                    } else {
                        let mut blocks: Vec<ContentBlock> = Vec::new();
                        if !msg.content.is_empty() {
                            blocks.push(ContentBlock::Text { text: msg.content });
                        }
                        for tc in msg.tool_calls {
                            blocks.push(ContentBlock::ToolUse {
                                id: tc.id,
                                name: tc.name,
                                input: tc.arguments,
                            });
                        }
                        messages.push(AnthropicMessage {
                            role: "assistant".to_string(),
                            content: AnthropicContent::Blocks(blocks),
                        });
                    }
                }
                crate::Role::Tool => {
                    if let Some(tool_call_id) = msg.tool_call_id {
                        messages.push(AnthropicMessage {
                            role: "user".to_string(),
                            content: AnthropicContent::Blocks(vec![ContentBlock::ToolResult {
                                tool_use_id: tool_call_id,
                                content: msg.content,
                            }]),
                        });
                    }
                }
            }
        }

        Self {
            model: req.model,
            messages,
            max_tokens: req.max_tokens.unwrap_or(4096),
            system,
            temperature: req.temperature,
            top_p: req.top_p,
            stop_sequences: req.stop,
            tools: req.tools.map(|tools| {
                tools
                    .into_iter()
                    .map(|t| AnthropicTool {
                        name: t.name,
                        description: t.description,
                        input_schema: t.parameters,
                    })
                    .collect()
            }),
            stream: None,
        }
    }
}

impl From<AnthropicResponse> for CompletionResponse {
    fn from(resp: AnthropicResponse) -> Self {
        let mut content = String::new();
        let mut tool_calls = Vec::new();

        for block in resp.content {
            match block {
                ContentBlock::Text { text } => {
                    content.push_str(&text);
                }
                ContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall::new(id, name, input));
                }
                _ => {}
            }
        }

        let finish_reason = resp.stop_reason.and_then(|r| match r.as_str() {
            "end_turn" | "stop_sequence" => Some(FinishReason::Stop),
            "max_tokens" => Some(FinishReason::Length),
            "tool_use" => Some(FinishReason::ToolCalls),
            _ => None,
        });

        Self {
            content,
            tool_calls,
            finish_reason,
            usage: TokenUsage {
                prompt_tokens: resp.usage.input_tokens,
                completion_tokens: resp.usage.output_tokens,
                total_tokens: resp.usage.input_tokens + resp.usage.output_tokens,
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
    fn test_anthropic_request_conversion() {
        let request = CompletionRequest::new("claude-3-5-sonnet-20241022")
            .with_message(Message::system("You are helpful"))
            .with_message(Message::user("Hello"))
            .with_temperature(0.7);

        let anthropic_req = AnthropicRequest::from(request);

        assert_eq!(anthropic_req.model, "claude-3-5-sonnet-20241022");
        assert_eq!(anthropic_req.messages.len(), 1); // System is separate
        assert_eq!(anthropic_req.system, Some("You are helpful".to_string()));
        assert_eq!(anthropic_req.temperature, Some(0.7));
    }

    #[test]
    fn test_system_message_extraction() {
        let request = CompletionRequest::new("claude-3-5-sonnet-20241022")
            .with_message(Message::system("Be helpful"))
            .with_message(Message::user("Hi"));

        let anthropic_req = AnthropicRequest::from(request);

        assert_eq!(anthropic_req.system, Some("Be helpful".to_string()));
        assert_eq!(anthropic_req.messages.len(), 1);
    }
}
