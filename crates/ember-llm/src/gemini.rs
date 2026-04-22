//! Google Gemini API provider implementation
//!
//! Supports Gemini 2.0 Flash, Gemini 1.5 Pro, and other Google AI models.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use tracing::{debug, instrument};

use crate::{
    provider::StreamResponse, CompletionRequest, CompletionResponse, Error, FinishReason,
    LLMProvider, ModelInfo, Result, StreamChunk, TokenUsage, ToolCallDelta,
};

use tokio_stream::wrappers::ReceiverStream;

const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
const DEFAULT_MODEL: &str = "gemini-2.0-flash";

/// Google Gemini API provider
///
/// Supports the latest Gemini models including:
/// - gemini-2.5-pro (most capable, with thinking)
/// - gemini-2.0-flash (default, fast and capable)
/// - gemini-1.5-pro (high quality)
/// - gemini-1.5-flash (cost-effective)
///
/// # Example
///
/// ```rust,no_run
/// use ember_llm::{GeminiProvider, LLMProvider, CompletionRequest, Message};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let provider = GeminiProvider::from_env()?;
///     let request = CompletionRequest::new("gemini-2.0-flash")
///         .with_message(Message::user("Hello!"));
///     let response = provider.complete(request).await?;
///     println!("{}", response.content);
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct GeminiProvider {
    client: Client,
    api_key: String,
    base_url: String,
    default_model: String,
}

impl GeminiProvider {
    /// Create a new Gemini provider with explicit API key
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
    /// Looks for `GOOGLE_API_KEY` or `GEMINI_API_KEY` environment variable.
    pub fn from_env() -> Result<Self> {
        let api_key = env::var("GOOGLE_API_KEY")
            .or_else(|_| env::var("GEMINI_API_KEY"))
            .map_err(|_| Error::api_key_missing("gemini"))?;

        let mut provider = Self::new(api_key);

        if let Ok(base_url) = env::var("GEMINI_BASE_URL") {
            provider.base_url = base_url;
        }

        if let Ok(model) = env::var("GEMINI_MODEL") {
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

    fn get_endpoint(&self, model: &str, stream: bool) -> String {
        let action = if stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };
        format!(
            "{}/models/{}:{}?key={}",
            self.base_url, model, action, self.api_key
        )
    }
}

#[async_trait]
impl LLMProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let model = if request.model.is_empty() {
            &self.default_model
        } else {
            &request.model
        };

        let gemini_request = GeminiRequest::from_completion_request(request.clone());

        debug!("Sending request to Gemini API");

        let response = self
            .client
            .post(self.get_endpoint(model, false))
            .header("Content-Type", "application/json")
            .json(&gemini_request)
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let error_msg = serde_json::from_str::<GeminiError>(&error_text)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| {
                    if error_text.is_empty() {
                        format!("HTTP {} (empty response)", status.as_u16())
                    } else {
                        error_text
                    }
                });

            return match status.as_u16() {
                401 | 403 => Err(Error::api_key_missing("gemini")),
                429 => Err(Error::rate_limit("gemini", None)),
                _ => Err(Error::api_error("gemini", status.as_u16(), error_msg)),
            };
        }

        let gemini_response: GeminiResponse = response.json().await?;

        Ok(gemini_response.into_completion_response(model.to_string()))
    }

    async fn complete_stream(&self, request: CompletionRequest) -> Result<StreamResponse> {
        let model = if request.model.is_empty() {
            self.default_model.clone()
        } else {
            request.model.clone()
        };

        let gemini_request = GeminiRequest::from_completion_request(request);

        debug!("Starting streaming request to Gemini");

        let response = self
            .client
            .post(format!("{}&alt=sse", self.get_endpoint(&model, true)))
            .header("Content-Type", "application/json")
            .json(&gemini_request)
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let error_msg = serde_json::from_str::<GeminiError>(&error_text)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| {
                    if error_text.is_empty() {
                        format!("HTTP {} (empty response)", status.as_u16())
                    } else {
                        error_text
                    }
                });

            return match status.as_u16() {
                401 | 403 => Err(Error::api_key_missing("gemini")),
                429 => Err(Error::rate_limit("gemini", None)),
                _ => Err(Error::api_error("gemini", status.as_u16(), error_msg)),
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
                            if let Ok(chunk_response) =
                                serde_json::from_str::<GeminiStreamResponse>(data)
                            {
                                if let Some(candidate) = chunk_response.candidates.first() {
                                    let content = candidate
                                        .content
                                        .parts
                                        .iter()
                                        .filter_map(|p| p.text.clone())
                                        .collect::<Vec<_>>()
                                        .join("");

                                    let done = candidate.finish_reason.is_some();

                                    let stream_chunk = StreamChunk {
                                        content: if content.is_empty() {
                                            None
                                        } else {
                                            Some(content)
                                        },
                                        tool_calls: candidate.content.parts.iter().find_map(|p| {
                                            p.function_call.as_ref().map(|fc| {
                                                vec![ToolCallDelta {
                                                    index: 0,
                                                    id: Some(format!("call_{}", uuid_simple())),
                                                    name: Some(fc.name.clone()),
                                                    arguments: Some(
                                                        serde_json::to_string(&fc.args)
                                                            .unwrap_or_default(),
                                                    ),
                                                }]
                                            })
                                        }),
                                        done,
                                        finish_reason: candidate.finish_reason.as_ref().map(|r| {
                                            match r.as_str() {
                                                "STOP" => FinishReason::Stop,
                                                "MAX_TOKENS" => FinishReason::Length,
                                                "SAFETY" => FinishReason::ContentFilter,
                                                _ => FinishReason::Stop,
                                            }
                                        }),
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

            // Send final done message
            let _ = tx
                .send(Ok(StreamChunk {
                    content: None,
                    tool_calls: None,
                    done: true,
                    finish_reason: Some(FinishReason::Stop),
                }))
                .await;
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(format!("{}/models?key={}", self.base_url, self.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(Error::api_error(
                "gemini",
                response.status().as_u16(),
                "Failed to list models",
            ));
        }

        let models_response: GeminiModelsResponse = response.json().await?;

        Ok(models_response
            .models
            .into_iter()
            .filter(|m| m.name.contains("gemini"))
            .map(|m| {
                let id = m.name.replace("models/", "");
                ModelInfo {
                    id: id.clone(),
                    name: m.display_name,
                    description: m.description,
                    context_window: m.input_token_limit,
                    max_output_tokens: m.output_token_limit,
                    supports_tools: true,
                    supports_vision: id.contains("vision")
                        || id.contains("1.5")
                        || id.contains("2.0"),
                    provider: "gemini".to_string(),
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
        true
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }
}

// Helper function to generate simple UUID-like strings
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}{:x}", duration.as_secs(), duration.subsec_nanos())
}

// Gemini API types

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    safety_settings: Option<Vec<GeminiSafetySetting>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inline_data: Option<GeminiInlineData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_call: Option<GeminiFunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_response: Option<GeminiFunctionResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct GeminiInlineData {
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct GeminiTool {
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct GeminiSafetySetting {
    category: String,
    threshold: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct GeminiCandidate {
    content: GeminiContent,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct GeminiUsageMetadata {
    prompt_token_count: u32,
    candidates_token_count: u32,
    total_token_count: u32,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GeminiStreamResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GeminiError {
    error: GeminiErrorDetail,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GeminiErrorDetail {
    message: String,
    code: u16,
    status: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GeminiModelsResponse {
    models: Vec<GeminiModelInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct GeminiModelInfo {
    name: String,
    display_name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    input_token_limit: Option<u32>,
    #[serde(default)]
    output_token_limit: Option<u32>,
}

// Conversion implementations

impl GeminiRequest {
    fn from_completion_request(req: CompletionRequest) -> Self {
        let mut contents: Vec<GeminiContent> = Vec::new();
        let mut system_instruction: Option<GeminiContent> = None;

        for msg in req.messages {
            match msg.role {
                crate::Role::System => {
                    // Gemini uses system_instruction for system messages
                    system_instruction = Some(GeminiContent {
                        role: "user".to_string(), // System instruction doesn't need role but struct requires it
                        parts: vec![GeminiPart {
                            text: Some(msg.content),
                            inline_data: None,
                            function_call: None,
                            function_response: None,
                        }],
                    });
                }
                crate::Role::User => {
                    let mut parts = Vec::new();

                    // Add text content
                    if !msg.content.is_empty() {
                        parts.push(GeminiPart {
                            text: Some(msg.content),
                            inline_data: None,
                            function_call: None,
                            function_response: None,
                        });
                    }

                    // Add image content parts
                    for part in msg.content_parts {
                        match part {
                            crate::types::ContentPart::Text { text } => {
                                parts.push(GeminiPart {
                                    text: Some(text),
                                    inline_data: None,
                                    function_call: None,
                                    function_response: None,
                                });
                            }
                            crate::types::ContentPart::Image { source, .. } => {
                                if let crate::types::ImageSource::Base64 { media_type, data } =
                                    source
                                {
                                    parts.push(GeminiPart {
                                        text: None,
                                        inline_data: Some(GeminiInlineData {
                                            mime_type: media_type.as_mime_type().to_string(),
                                            data,
                                        }),
                                        function_call: None,
                                        function_response: None,
                                    });
                                }
                            }
                        }
                    }

                    contents.push(GeminiContent {
                        role: "user".to_string(),
                        parts,
                    });
                }
                crate::Role::Assistant => {
                    let mut parts = Vec::new();

                    if !msg.content.is_empty() {
                        parts.push(GeminiPart {
                            text: Some(msg.content),
                            inline_data: None,
                            function_call: None,
                            function_response: None,
                        });
                    }

                    // Add tool calls as function calls
                    for tc in msg.tool_calls {
                        parts.push(GeminiPart {
                            text: None,
                            inline_data: None,
                            function_call: Some(GeminiFunctionCall {
                                name: tc.name,
                                args: tc.arguments,
                            }),
                            function_response: None,
                        });
                    }

                    contents.push(GeminiContent {
                        role: "model".to_string(),
                        parts,
                    });
                }
                crate::Role::Tool => {
                    // Tool results are function responses
                    if let Some(name) = msg.name {
                        contents.push(GeminiContent {
                            role: "user".to_string(),
                            parts: vec![GeminiPart {
                                text: None,
                                inline_data: None,
                                function_call: None,
                                function_response: Some(GeminiFunctionResponse {
                                    name,
                                    response: serde_json::json!({ "result": msg.content }),
                                }),
                            }],
                        });
                    }
                }
            }
        }

        // Build tools
        let tools = req.tools.map(|tools| {
            vec![GeminiTool {
                function_declarations: tools
                    .into_iter()
                    .map(|t| GeminiFunctionDeclaration {
                        name: t.name,
                        description: t.description,
                        parameters: t.parameters,
                    })
                    .collect(),
            }]
        });

        // Build generation config
        let generation_config = if req.temperature.is_some()
            || req.max_tokens.is_some()
            || req.top_p.is_some()
            || req.stop.is_some()
        {
            Some(GeminiGenerationConfig {
                temperature: req.temperature,
                top_p: req.top_p,
                top_k: None,
                max_output_tokens: req.max_tokens,
                stop_sequences: req.stop,
            })
        } else {
            None
        };

        Self {
            contents,
            system_instruction,
            generation_config,
            tools,
            safety_settings: None,
        }
    }
}

impl GeminiResponse {
    fn into_completion_response(self, model: String) -> CompletionResponse {
        let candidate = self.candidates.into_iter().next();

        let (content, tool_calls, finish_reason) = if let Some(c) = candidate {
            let text_content: String = c
                .content
                .parts
                .iter()
                .filter_map(|p| p.text.clone())
                .collect::<Vec<_>>()
                .join("");

            let tool_calls: Vec<crate::ToolCall> = c
                .content
                .parts
                .iter()
                .filter_map(|p| {
                    p.function_call.as_ref().map(|fc| {
                        crate::ToolCall::new(
                            format!("call_{}", uuid_simple()),
                            fc.name.clone(),
                            fc.args.clone(),
                        )
                    })
                })
                .collect();

            let finish = c.finish_reason.map(|r| match r.as_str() {
                "STOP" => FinishReason::Stop,
                "MAX_TOKENS" => FinishReason::Length,
                "SAFETY" => FinishReason::ContentFilter,
                _ => FinishReason::Stop,
            });

            (text_content, tool_calls, finish)
        } else {
            (String::new(), Vec::new(), None)
        };

        let usage = self.usage_metadata.map_or(
            TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                ..Default::default()
            },
            |u| TokenUsage {
                prompt_tokens: u.prompt_token_count,
                completion_tokens: u.candidates_token_count,
                total_tokens: u.total_token_count,
                ..Default::default()
            },
        );

        CompletionResponse {
            content,
            tool_calls,
            finish_reason,
            usage,
            model,
            id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Message;

    #[test]
    fn test_gemini_request_conversion() {
        let request = CompletionRequest::new("gemini-2.0-flash")
            .with_message(Message::system("You are helpful"))
            .with_message(Message::user("Hello"))
            .with_temperature(0.7);

        let gemini_req = GeminiRequest::from_completion_request(request);

        assert!(gemini_req.system_instruction.is_some());
        assert_eq!(gemini_req.contents.len(), 1);
        assert!(gemini_req.generation_config.is_some());
        assert_eq!(gemini_req.generation_config.unwrap().temperature, Some(0.7));
    }

    #[test]
    fn test_gemini_multimodal_request() {
        let request = CompletionRequest::new("gemini-2.0-flash")
            .with_message(Message::user("Describe this image"))
            .with_temperature(0.5);

        let gemini_req = GeminiRequest::from_completion_request(request);

        assert_eq!(gemini_req.contents.len(), 1);
        assert_eq!(gemini_req.contents[0].role, "user");
    }
}
