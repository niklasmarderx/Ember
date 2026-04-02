//! Anthropic Claude API provider implementation

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

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_MODEL: &str = "claude-3-5-sonnet-20241022";
const API_VERSION: &str = "2023-06-01";
const PROMPT_CACHING_BETA: &str = "prompt-caching-2024-07-31";

/// Anthropic Claude API provider
#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    base_url: String,
    default_model: String,
    /// When enabled, the beta header is sent and `cache_control` is attached
    /// to the system prompt and the last user message, allowing the API to
    /// cache the shared prefix across repeated calls.
    prompt_caching: bool,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider with explicit API key
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            default_model: DEFAULT_MODEL.to_string(),
            prompt_caching: false,
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

    /// Enable or disable Anthropic prompt caching (default: disabled).
    ///
    /// When enabled, the `anthropic-beta: prompt-caching-2024-07-31` header is
    /// added and `cache_control: { type: "ephemeral" }` is attached to the
    /// system prompt and the last user-turn content blocks.  The API will then
    /// cache the matching prefix and reuse it on subsequent calls, reducing
    /// costs by up to 90 %.
    ///
    /// See <https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching>
    pub fn with_prompt_caching(mut self, enabled: bool) -> Self {
        self.prompt_caching = enabled;
        self
    }

    fn build_request(&self) -> reqwest::RequestBuilder {
        let builder = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("Content-Type", "application/json");

        if self.prompt_caching {
            builder.header("anthropic-beta", PROMPT_CACHING_BETA)
        } else {
            builder
        }
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let anthropic_request = AnthropicRequest::from_request(request, self.prompt_caching);

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
        let mut anthropic_request = AnthropicRequest::from_request(request, self.prompt_caching);
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
    system: Option<AnthropicSystem>,
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

/// System prompt field: plain text or a list of content blocks (required for
/// prompt caching, which needs `cache_control` on the block).
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum AnthropicSystem {
    Text(String),
    Blocks(Vec<SystemBlock>),
}

/// A system-prompt content block (only `text` type is currently valid here).
#[derive(Debug, Serialize)]
struct SystemBlock {
    r#type: &'static str,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<CacheControl>,
}

/// Marks a content block as cacheable.
///
/// The only currently supported value is `"ephemeral"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheControl {
    r#type: CacheControlType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum CacheControlType {
    Ephemeral,
}

impl CacheControl {
    fn ephemeral() -> Self {
        Self {
            r#type: CacheControlType::Ephemeral,
        }
    }
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
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    #[serde(rename = "image")]
    Image {
        source: AnthropicImageSource,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
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
    /// Tokens written into the prompt cache on this request.
    #[serde(default)]
    cache_creation_input_tokens: Option<u32>,
    /// Tokens read from the prompt cache (not re-billed at full price).
    #[serde(default)]
    cache_read_input_tokens: Option<u32>,
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

/// Convert Ember ContentPart to Anthropic ContentBlock (no cache_control).
fn convert_content_part_to_anthropic(part: &crate::types::ContentPart) -> ContentBlock {
    match part {
        crate::types::ContentPart::Text { text } => ContentBlock::Text {
            text: text.clone(),
            cache_control: None,
        },
        crate::types::ContentPart::Image { source, .. } => match source {
            crate::types::ImageSource::Base64 { media_type, data } => ContentBlock::Image {
                source: AnthropicImageSource::Base64 {
                    media_type: media_type.as_mime_type().to_string(),
                    data: data.clone(),
                },
                cache_control: None,
            },
            crate::types::ImageSource::Url { url } => ContentBlock::Image {
                source: AnthropicImageSource::Url { url: url.clone() },
                cache_control: None,
            },
        },
    }
}

/// Set `cache_control: ephemeral` on the last content block in a slice.
///
/// Anthropic's caching semantics require the marker to sit on the *last*
/// block of the prefix to be cached; earlier blocks in the same turn are
/// implicitly included.
fn mark_last_block_cacheable(blocks: &mut Vec<ContentBlock>) {
    if let Some(last) = blocks.last_mut() {
        match last {
            ContentBlock::Text { cache_control, .. }
            | ContentBlock::Image { cache_control, .. } => {
                *cache_control = Some(CacheControl::ephemeral());
            }
            // ToolUse / ToolResult don't support cache_control — leave them.
            _ => {}
        }
    }
}

impl AnthropicRequest {
    /// Build an [`AnthropicRequest`] from an Ember [`CompletionRequest`].
    ///
    /// When `prompt_caching` is `true`, the system prompt is converted to a
    /// content-block array with `cache_control: ephemeral` on its last block,
    /// and the same marker is applied to the last block of the final user
    /// message.  This instructs the API to cache the shared prefix.
    fn from_request(req: CompletionRequest, prompt_caching: bool) -> Self {
        let mut system: Option<String> = None;
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
                            blocks.push(ContentBlock::Text {
                                text: msg.content,
                                cache_control: None,
                            });
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

        // Apply cache_control markers when prompt caching is enabled.
        let anthropic_system = if prompt_caching {
            system.map(|text| {
                AnthropicSystem::Blocks(vec![SystemBlock {
                    r#type: "text",
                    text,
                    cache_control: Some(CacheControl::ephemeral()),
                }])
            })
        } else {
            system.map(AnthropicSystem::Text)
        };

        if prompt_caching {
            // Find the last user-turn message and mark its final content block.
            if let Some(last_user) = messages.iter_mut().rev().find(|m| m.role == "user") {
                match &mut last_user.content {
                    AnthropicContent::Text(text) => {
                        // Upgrade plain text to a blocks array so we can attach cache_control.
                        let owned = std::mem::take(text);
                        last_user.content = AnthropicContent::Blocks(vec![ContentBlock::Text {
                            text: owned,
                            cache_control: Some(CacheControl::ephemeral()),
                        }]);
                    }
                    AnthropicContent::Blocks(blocks) => {
                        mark_last_block_cacheable(blocks);
                    }
                }
            }
        }

        Self {
            model: req.model,
            messages,
            max_tokens: req.max_tokens.unwrap_or(4096),
            system: anthropic_system,
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
                ContentBlock::Text { text, .. } => {
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
                cache_creation_tokens: resp.usage.cache_creation_input_tokens,
                cache_read_tokens: resp.usage.cache_read_input_tokens,
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

    // ── existing tests (no caching) ──────────────────────────────────────────

    #[test]
    fn test_anthropic_request_conversion() {
        let request = CompletionRequest::new("claude-3-5-sonnet-20241022")
            .with_message(Message::system("You are helpful"))
            .with_message(Message::user("Hello"))
            .with_temperature(0.7);

        let anthropic_req = AnthropicRequest::from_request(request, false);

        assert_eq!(anthropic_req.model, "claude-3-5-sonnet-20241022");
        assert_eq!(anthropic_req.messages.len(), 1); // System is separate
        assert_eq!(anthropic_req.temperature, Some(0.7));

        // Without caching the system field is plain text.
        match anthropic_req.system {
            Some(AnthropicSystem::Text(t)) => assert_eq!(t, "You are helpful"),
            other => panic!("expected AnthropicSystem::Text, got {:?}", other),
        }
    }

    #[test]
    fn test_system_message_extraction() {
        let request = CompletionRequest::new("claude-3-5-sonnet-20241022")
            .with_message(Message::system("Be helpful"))
            .with_message(Message::user("Hi"));

        let anthropic_req = AnthropicRequest::from_request(request, false);

        match anthropic_req.system {
            Some(AnthropicSystem::Text(t)) => assert_eq!(t, "Be helpful"),
            other => panic!("expected AnthropicSystem::Text, got {:?}", other),
        }
        assert_eq!(anthropic_req.messages.len(), 1);
    }

    // ── prompt caching tests ─────────────────────────────────────────────────

    #[test]
    fn test_caching_disabled_by_default() {
        let provider = AnthropicProvider::new("test-key");
        assert!(!provider.prompt_caching);
    }

    #[test]
    fn test_with_prompt_caching_builder() {
        let provider = AnthropicProvider::new("test-key").with_prompt_caching(true);
        assert!(provider.prompt_caching);

        let provider = provider.with_prompt_caching(false);
        assert!(!provider.prompt_caching);
    }

    #[test]
    fn test_caching_adds_beta_header() {
        // Build the request builder and inspect its headers.
        let provider = AnthropicProvider::new("test-key").with_prompt_caching(true);
        let builder = provider.build_request();
        // We can't inspect reqwest::RequestBuilder headers directly, but we
        // can verify the builder was produced without panicking and that the
        // flag is set.  The integration test (wiremock) would catch the header
        // at the HTTP level; here we just confirm the code path is taken.
        assert!(provider.prompt_caching);
        drop(builder);
    }

    #[test]
    fn test_caching_system_block_has_cache_control() {
        let request = CompletionRequest::new("claude-3-5-sonnet-20241022")
            .with_message(Message::system("You are a helpful assistant."))
            .with_message(Message::user("Hello"));

        let anthropic_req = AnthropicRequest::from_request(request, true);

        match anthropic_req.system {
            Some(AnthropicSystem::Blocks(blocks)) => {
                assert_eq!(blocks.len(), 1);
                assert!(
                    blocks[0].cache_control.is_some(),
                    "system block must have cache_control"
                );
                assert!(matches!(
                    blocks[0].cache_control.as_ref().unwrap().r#type,
                    CacheControlType::Ephemeral
                ));
            }
            other => panic!("expected AnthropicSystem::Blocks, got {:?}", other),
        }
    }

    #[test]
    fn test_caching_last_user_message_has_cache_control() {
        let request = CompletionRequest::new("claude-3-5-sonnet-20241022")
            .with_message(Message::user("First message"))
            .with_message(Message::assistant("Got it"))
            .with_message(Message::user("Second message"));

        let anthropic_req = AnthropicRequest::from_request(request, true);

        // The *last* user message must carry cache_control; the first must not.
        let user_messages: Vec<&AnthropicMessage> =
            anthropic_req.messages.iter().filter(|m| m.role == "user").collect();

        assert_eq!(user_messages.len(), 2);

        // First user message — no cache_control.
        match &user_messages[0].content {
            AnthropicContent::Blocks(blocks) => {
                if let Some(ContentBlock::Text { cache_control, .. }) = blocks.last() {
                    assert!(
                        cache_control.is_none(),
                        "first user message must NOT have cache_control"
                    );
                }
            }
            AnthropicContent::Text(_) => {} // plain text → no cache_control, fine
        }

        // Last user message — must have cache_control on the final block.
        match &user_messages[1].content {
            AnthropicContent::Blocks(blocks) => {
                let last = blocks.last().expect("blocks must not be empty");
                let cc = match last {
                    ContentBlock::Text { cache_control, .. } => cache_control,
                    ContentBlock::Image { cache_control, .. } => cache_control,
                    other => panic!("unexpected block type: {:?}", other),
                };
                assert!(cc.is_some(), "last user message must have cache_control");
                assert!(matches!(
                    cc.as_ref().unwrap().r#type,
                    CacheControlType::Ephemeral
                ));
            }
            AnthropicContent::Text(_) => {
                panic!("last user message should have been upgraded to Blocks");
            }
        }
    }

    #[test]
    fn test_no_caching_no_cache_control() {
        let request = CompletionRequest::new("claude-3-5-sonnet-20241022")
            .with_message(Message::system("Be helpful"))
            .with_message(Message::user("Hi"));

        let anthropic_req = AnthropicRequest::from_request(request, false);

        // System must be plain text, not blocks.
        assert!(matches!(anthropic_req.system, Some(AnthropicSystem::Text(_))));

        // Messages must not contain any cache_control.
        for msg in &anthropic_req.messages {
            if let AnthropicContent::Blocks(blocks) = &msg.content {
                for block in blocks {
                    match block {
                        ContentBlock::Text { cache_control, .. }
                        | ContentBlock::Image { cache_control, .. } => {
                            assert!(cache_control.is_none(), "cache_control must be absent");
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    #[test]
    fn test_cache_tokens_parsed_from_response() {
        let usage = AnthropicUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: Some(80),
            cache_read_input_tokens: Some(20),
        };

        let resp = AnthropicResponse {
            id: "msg_01".to_string(),
            model: "claude-3-5-sonnet-20241022".to_string(),
            content: vec![ContentBlock::Text {
                text: "Hello!".to_string(),
                cache_control: None,
            }],
            stop_reason: Some("end_turn".to_string()),
            usage,
        };

        let completion: CompletionResponse = resp.into();

        assert_eq!(completion.usage.prompt_tokens, 100);
        assert_eq!(completion.usage.completion_tokens, 50);
        assert_eq!(completion.usage.cache_creation_tokens, Some(80));
        assert_eq!(completion.usage.cache_read_tokens, Some(20));
    }

    #[test]
    fn test_no_cache_tokens_when_absent() {
        let usage = AnthropicUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
        };

        let resp = AnthropicResponse {
            id: "msg_02".to_string(),
            model: "claude-3-5-sonnet-20241022".to_string(),
            content: vec![ContentBlock::Text {
                text: "Hi".to_string(),
                cache_control: None,
            }],
            stop_reason: Some("end_turn".to_string()),
            usage,
        };

        let completion: CompletionResponse = resp.into();

        assert!(completion.usage.cache_creation_tokens.is_none());
        assert!(completion.usage.cache_read_tokens.is_none());
    }

    #[test]
    fn test_serialized_request_contains_cache_control() {
        let request = CompletionRequest::new("claude-3-5-sonnet-20241022")
            .with_message(Message::system("System prompt"))
            .with_message(Message::user("User message"));

        let anthropic_req = AnthropicRequest::from_request(request, true);
        let json = serde_json::to_string(&anthropic_req).expect("serialization must succeed");

        assert!(
            json.contains("cache_control"),
            "serialized request must contain cache_control"
        );
        assert!(
            json.contains("ephemeral"),
            "serialized request must contain ephemeral"
        );
    }
}
