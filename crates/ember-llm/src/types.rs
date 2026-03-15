//! Core types for LLM interactions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Vision/Multimodal Types
// ============================================================================

/// Content type for multimodal messages (text, images, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Plain text content
    Text {
        /// The text content
        text: String,
    },
    /// Image content (base64 encoded or URL)
    Image {
        /// Image source - either base64 data or URL
        source: ImageSource,
        /// Optional alt text for accessibility
        #[serde(skip_serializing_if = "Option::is_none")]
        alt_text: Option<String>,
    },
}

impl ContentPart {
    /// Create a text content part
    pub fn text(content: impl Into<String>) -> Self {
        ContentPart::Text {
            text: content.into(),
        }
    }

    /// Create an image content part from base64 data
    pub fn image_base64(data: impl Into<String>, media_type: ImageMediaType) -> Self {
        ContentPart::Image {
            source: ImageSource::Base64 {
                media_type,
                data: data.into(),
            },
            alt_text: None,
        }
    }

    /// Create an image content part from a URL
    pub fn image_url(url: impl Into<String>) -> Self {
        ContentPart::Image {
            source: ImageSource::Url { url: url.into() },
            alt_text: None,
        }
    }

    /// Add alt text to an image content part
    pub fn with_alt_text(mut self, alt: impl Into<String>) -> Self {
        if let ContentPart::Image {
            ref mut alt_text, ..
        } = self
        {
            *alt_text = Some(alt.into());
        }
        self
    }

    /// Check if this is a text part
    pub fn is_text(&self) -> bool {
        matches!(self, ContentPart::Text { .. })
    }

    /// Check if this is an image part
    pub fn is_image(&self) -> bool {
        matches!(self, ContentPart::Image { .. })
    }

    /// Get text content if this is a text part
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ContentPart::Text { text } => Some(text),
            _ => None,
        }
    }
}

/// Source of image data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    /// Base64 encoded image data
    Base64 {
        /// MIME type of the image
        media_type: ImageMediaType,
        /// Base64 encoded image data
        data: String,
    },
    /// Image URL (provider must support URL fetching)
    Url {
        /// URL to the image
        url: String,
    },
}

/// Supported image media types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageMediaType {
    /// JPEG image
    #[serde(rename = "image/jpeg")]
    Jpeg,
    /// PNG image
    #[serde(rename = "image/png")]
    Png,
    /// GIF image
    #[serde(rename = "image/gif")]
    Gif,
    /// WebP image
    #[serde(rename = "image/webp")]
    WebP,
}

impl ImageMediaType {
    /// Get the MIME type string
    pub fn as_mime_type(&self) -> &'static str {
        match self {
            ImageMediaType::Jpeg => "image/jpeg",
            ImageMediaType::Png => "image/png",
            ImageMediaType::Gif => "image/gif",
            ImageMediaType::WebP => "image/webp",
        }
    }

    /// Parse from file extension
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "jpg" | "jpeg" => Some(ImageMediaType::Jpeg),
            "png" => Some(ImageMediaType::Png),
            "gif" => Some(ImageMediaType::Gif),
            "webp" => Some(ImageMediaType::WebP),
            _ => None,
        }
    }

    /// Parse from MIME type string
    pub fn from_mime_type(mime: &str) -> Option<Self> {
        match mime.to_lowercase().as_str() {
            "image/jpeg" | "image/jpg" => Some(ImageMediaType::Jpeg),
            "image/png" => Some(ImageMediaType::Png),
            "image/gif" => Some(ImageMediaType::Gif),
            "image/webp" => Some(ImageMediaType::WebP),
            _ => None,
        }
    }
}

// ============================================================================
// Message Types
// ============================================================================

/// Message role in a conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System message (instructions)
    System,
    /// User message
    User,
    /// Assistant (LLM) response
    Assistant,
    /// Tool result message
    Tool,
}

/// A single message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// The role of the message sender
    pub role: Role,
    /// The content of the message (text-only for backwards compatibility)
    pub content: String,
    /// Multimodal content parts (for vision-enabled messages)
    /// When present, this takes precedence over `content` for providers
    /// that support multimodal input.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content_parts: Vec<ContentPart>,
    /// Optional name for the sender
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tool calls made by the assistant
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Tool call ID (for tool result messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    /// Create a new system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            content_parts: Vec::new(),
            name: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// Create a new user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            content_parts: Vec::new(),
            name: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// Create a new assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            content_parts: Vec::new(),
            name: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// Create a new tool result message
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            content_parts: Vec::new(),
            name: None,
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    /// Create a user message with text and an image (base64)
    ///
    /// # Example
    /// ```rust
    /// use ember_llm::{Message, ImageMediaType};
    ///
    /// let base64_data = "iVBORw0KGgoAAAANSUhEUg..."; // base64 image data
    /// let msg = Message::user_with_image(
    ///     "What do you see in this image?",
    ///     base64_data,
    ///     ImageMediaType::Png,
    /// );
    /// ```
    pub fn user_with_image(
        text: impl Into<String>,
        image_base64: impl Into<String>,
        media_type: ImageMediaType,
    ) -> Self {
        let text_str = text.into();
        Self {
            role: Role::User,
            content: text_str.clone(),
            content_parts: vec![
                ContentPart::text(text_str),
                ContentPart::image_base64(image_base64, media_type),
            ],
            name: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// Create a user message with text and an image URL
    ///
    /// # Example
    /// ```rust
    /// use ember_llm::Message;
    ///
    /// let msg = Message::user_with_image_url(
    ///     "What do you see in this image?",
    ///     "https://example.com/image.jpg",
    /// );
    /// ```
    pub fn user_with_image_url(text: impl Into<String>, url: impl Into<String>) -> Self {
        let text_str = text.into();
        Self {
            role: Role::User,
            content: text_str.clone(),
            content_parts: vec![ContentPart::text(text_str), ContentPart::image_url(url)],
            name: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// Create a user message with multiple images
    ///
    /// # Example
    /// ```rust
    /// use ember_llm::{Message, ContentPart, ImageMediaType};
    ///
    /// let img1_data = "base64data1";
    /// let img2_data = "base64data2";
    /// let msg = Message::user_with_content_parts(vec![
    ///     ContentPart::text("Compare these two images:"),
    ///     ContentPart::image_base64(img1_data, ImageMediaType::Jpeg),
    ///     ContentPart::image_base64(img2_data, ImageMediaType::Jpeg),
    /// ]);
    /// ```
    pub fn user_with_content_parts(parts: Vec<ContentPart>) -> Self {
        // Extract text content for backwards compatibility
        let text_content: String = parts
            .iter()
            .filter_map(|p| p.as_text())
            .collect::<Vec<_>>()
            .join(" ");

        Self {
            role: Role::User,
            content: text_content,
            content_parts: parts,
            name: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// Set a name for this message
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Add tool calls to this message
    pub fn with_tool_calls(mut self, tool_calls: Vec<ToolCall>) -> Self {
        self.tool_calls = tool_calls;
        self
    }

    /// Add an image to this message (converts to multimodal)
    pub fn with_image(
        mut self,
        image_base64: impl Into<String>,
        media_type: ImageMediaType,
    ) -> Self {
        // If content_parts is empty, add the text content first
        if self.content_parts.is_empty() && !self.content.is_empty() {
            self.content_parts.push(ContentPart::text(&self.content));
        }
        self.content_parts
            .push(ContentPart::image_base64(image_base64, media_type));
        self
    }

    /// Add an image URL to this message (converts to multimodal)
    pub fn with_image_url(mut self, url: impl Into<String>) -> Self {
        // If content_parts is empty, add the text content first
        if self.content_parts.is_empty() && !self.content.is_empty() {
            self.content_parts.push(ContentPart::text(&self.content));
        }
        self.content_parts.push(ContentPart::image_url(url));
        self
    }

    /// Check if this message contains images
    pub fn has_images(&self) -> bool {
        self.content_parts.iter().any(|p| p.is_image())
    }

    /// Check if this message is multimodal (has content_parts)
    pub fn is_multimodal(&self) -> bool {
        !self.content_parts.is_empty()
    }

    /// Get all text from the message (combining content_parts if present)
    pub fn text_content(&self) -> &str {
        &self.content
    }

    /// Get image count in this message
    pub fn image_count(&self) -> usize {
        self.content_parts.iter().filter(|p| p.is_image()).count()
    }
}

/// A tool call made by the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID for this tool call
    pub id: String,
    /// Name of the tool to call
    pub name: String,
    /// Arguments for the tool (JSON)
    pub arguments: serde_json::Value,
}

impl ToolCall {
    /// Create a new tool call
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }
}

/// Result from executing a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// ID of the tool call this result is for
    pub tool_call_id: String,
    /// Output from the tool execution
    pub output: String,
    /// Whether the tool execution was successful
    pub success: bool,
    /// Optional error message if execution failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    /// Create a successful tool result
    pub fn success(tool_call_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            output: output.into(),
            success: true,
            error: None,
        }
    }

    /// Create a failed tool result
    pub fn failure(tool_call_id: impl Into<String>, error: impl Into<String>) -> Self {
        let error_msg = error.into();
        Self {
            tool_call_id: tool_call_id.into(),
            output: format!("Error: {}", error_msg),
            success: false,
            error: Some(error_msg),
        }
    }
}

/// Definition of a tool for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// JSON Schema for parameters
    pub parameters: serde_json::Value,
}

impl ToolDefinition {
    /// Create a new tool definition
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }
}

/// Request for a completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Model to use
    pub model: String,
    /// Messages in the conversation
    pub messages: Vec<Message>,
    /// Temperature for sampling (0.0 - 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Top-p sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Available tools for function calling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    /// Whether to stream the response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Additional provider-specific options
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl CompletionRequest {
    /// Create a new completion request with a model name
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            messages: Vec::new(),
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            tools: None,
            stream: None,
            extra: HashMap::new(),
        }
    }

    /// Create a new completion request from messages
    ///
    /// The model will be set to an empty string and should be filled in
    /// by the provider or via `with_model()`.
    pub fn from_messages(messages: Vec<Message>) -> Self {
        Self {
            model: String::new(),
            messages,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            tools: None,
            stream: None,
            extra: HashMap::new(),
        }
    }

    /// Set the model for this request
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Add a message to the request
    pub fn with_message(mut self, message: Message) -> Self {
        self.messages.push(message);
        self
    }

    /// Add multiple messages to the request
    pub fn with_messages(mut self, messages: impl IntoIterator<Item = Message>) -> Self {
        self.messages.extend(messages);
        self
    }

    /// Set the temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set top-p sampling
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    /// Add stop sequences
    pub fn with_stop(mut self, stop: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.stop = Some(stop.into_iter().map(Into::into).collect());
        self
    }

    /// Add tools for function calling
    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Enable streaming
    pub fn with_streaming(mut self, stream: bool) -> Self {
        self.stream = Some(stream);
        self
    }

    /// Add an extra option
    pub fn with_extra(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.extra.insert(key.into(), value);
        self
    }
}

/// Response from a completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// The generated content
    pub content: String,
    /// Tool calls made by the assistant
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    /// The reason the generation stopped
    pub finish_reason: Option<FinishReason>,
    /// Token usage statistics
    pub usage: TokenUsage,
    /// Model that was used
    pub model: String,
    /// Response ID
    pub id: Option<String>,
}

impl CompletionResponse {
    /// Check if there are tool calls
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }

    /// Convert response to an assistant message
    pub fn to_message(&self) -> Message {
        Message::assistant(&self.content).with_tool_calls(self.tool_calls.clone())
    }
}

/// Reason for stopping generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// Model reached a natural stopping point
    Stop,
    /// Maximum token limit reached
    Length,
    /// Model made a tool call
    ToolCalls,
    /// Content was filtered
    ContentFilter,
}

/// Token usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Tokens in the prompt
    pub prompt_tokens: u32,
    /// Tokens in the completion
    pub completion_tokens: u32,
    /// Total tokens used
    pub total_tokens: u32,
}

impl TokenUsage {
    /// Create new token usage stats
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        }
    }
}

/// A chunk from streaming response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Delta content
    pub content: Option<String>,
    /// Tool call deltas
    pub tool_calls: Option<Vec<ToolCallDelta>>,
    /// Whether this is the final chunk
    pub done: bool,
    /// Finish reason (in final chunk)
    pub finish_reason: Option<FinishReason>,
}

/// Delta for a tool call during streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDelta {
    /// Index of the tool call
    pub index: usize,
    /// Tool call ID (first chunk)
    pub id: Option<String>,
    /// Tool name (first chunk)
    pub name: Option<String>,
    /// Arguments delta (JSON string fragment)
    pub arguments: Option<String>,
}

/// Information about an available model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model ID
    pub id: String,
    /// Model name (display name)
    pub name: String,
    /// Model description
    pub description: Option<String>,
    /// Context window size
    pub context_window: Option<u32>,
    /// Maximum output tokens
    pub max_output_tokens: Option<u32>,
    /// Whether the model supports tool calling
    pub supports_tools: bool,
    /// Whether the model supports vision (images)
    pub supports_vision: bool,
    /// Provider name
    pub provider: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content, "Hello");
    }

    #[test]
    fn test_completion_request_builder() {
        let req = CompletionRequest::new("gpt-4")
            .with_message(Message::system("You are helpful"))
            .with_message(Message::user("Hello"))
            .with_temperature(0.7)
            .with_max_tokens(100);

        assert_eq!(req.model, "gpt-4");
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.temperature, Some(0.7));
        assert_eq!(req.max_tokens, Some(100));
    }

    #[test]
    fn test_token_usage() {
        let usage = TokenUsage::new(100, 50);
        assert_eq!(usage.total_tokens, 150);
    }
}
