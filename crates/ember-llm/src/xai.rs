//! xAI Grok Provider
//!
//! Implementation for xAI's Grok models including Grok-2 and Grok-2 mini.
//! Uses OpenAI-compatible API format at api.x.ai

#[cfg(test)]
use crate::Message;
use crate::{
    CompletionRequest, CompletionResponse, Error, FinishReason, LLMProvider, ModelInfo, Result,
    Role, StreamChunk, TokenUsage, ToolCall, ToolCallDelta, ToolDefinition,
};
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tracing::{debug, instrument};

/// xAI Grok API provider
pub struct XAIProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl XAIProvider {
    /// Create a new xAI provider with the given API key
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: "https://api.x.ai/v1".to_string(),
        }
    }

    /// Create from environment variable XAI_API_KEY
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("XAI_API_KEY").map_err(|_| Error::api_key_missing("xai"))?;
        Ok(Self::new(api_key))
    }

    /// Set a custom base URL (for proxies or compatible endpoints)
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    fn build_messages(&self, request: &CompletionRequest) -> Vec<XAIMessage> {
        let mut messages = Vec::new();

        // Convert messages (system messages are included in request.messages)
        for msg in &request.messages {
            let role = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
                Role::Tool => "tool",
            };

            let mut xai_msg = XAIMessage {
                role: role.to_string(),
                content: Some(msg.content.clone()),
                tool_calls: None,
                tool_call_id: None,
            };

            // Handle tool calls in assistant messages
            if !msg.tool_calls.is_empty() {
                xai_msg.tool_calls = Some(
                    msg.tool_calls
                        .iter()
                        .map(|tc| XAIToolCall {
                            id: tc.id.clone(),
                            r#type: "function".to_string(),
                            function: XAIFunction {
                                name: tc.name.clone(),
                                // Convert serde_json::Value to String for API
                                arguments: tc.arguments.to_string(),
                            },
                        })
                        .collect(),
                );
            }

            // Handle tool results
            if let Some(ref tool_id) = msg.tool_call_id {
                xai_msg.tool_call_id = Some(tool_id.clone());
            }

            messages.push(xai_msg);
        }

        messages
    }

    fn build_tools(&self, tools: Option<&Vec<ToolDefinition>>) -> Option<Vec<XAITool>> {
        let tools = tools?;
        if tools.is_empty() {
            return None;
        }

        Some(
            tools
                .iter()
                .map(|t| XAITool {
                    r#type: "function".to_string(),
                    function: XAIToolFunction {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.parameters.clone(),
                    },
                })
                .collect(),
        )
    }
}

// API request/response types
#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct XAIRequest {
    model: String,
    messages: Vec<XAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<XAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct XAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<XAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct XAIToolCall {
    id: String,
    r#type: String,
    function: XAIFunction,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct XAIFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct XAITool {
    r#type: String,
    function: XAIToolFunction,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct XAIToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct XAIResponse {
    id: String,
    choices: Vec<XAIChoice>,
    usage: Option<XAIUsage>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct XAIChoice {
    message: XAIResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct XAIResponseMessage {
    role: String,
    content: Option<String>,
    tool_calls: Option<Vec<XAIToolCall>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct XAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct XAIStreamResponse {
    choices: Vec<XAIStreamChoice>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct XAIStreamChoice {
    delta: XAIStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct XAIStreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<XAIStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct XAIStreamToolCall {
    index: usize,
    id: Option<String>,
    function: Option<XAIStreamFunction>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct XAIStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct XAIErrorResponse {
    error: XAIErrorDetail,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct XAIErrorDetail {
    message: String,
    r#type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct XAIModelsResponse {
    data: Vec<XAIModel>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct XAIModel {
    id: String,
    owned_by: String,
}

#[async_trait]
impl LLMProvider for XAIProvider {
    fn name(&self) -> &str {
        "xai"
    }

    fn default_model(&self) -> &str {
        "grok-2"
    }

    async fn health_check(&self) -> Result<()> {
        // Try to list models to verify API key and connectivity
        let response = self
            .client
            .get(format!("{}/models", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| Error::provider_unavailable("xai", e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(Error::provider_unavailable(
                "xai",
                format!("API returned status {}", response.status()),
            ))
        }
    }

    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let messages = self.build_messages(&request);
        let tools = self.build_tools(request.tools.as_ref());

        let has_tools = request
            .tools
            .as_ref()
            .map(|t| !t.is_empty())
            .unwrap_or(false);
        let xai_request = XAIRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            top_p: request.top_p,
            tools,
            tool_choice: if has_tools {
                Some("auto".to_string())
            } else {
                None
            },
            stream: false,
        };

        debug!("Sending request to xAI API");

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&xai_request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            if let Ok(error_resp) = serde_json::from_str::<XAIErrorResponse>(&error_text) {
                return Err(Error::api_error(
                    "xai",
                    status.as_u16(),
                    error_resp.error.message,
                ));
            }
            return Err(Error::api_error("xai", status.as_u16(), error_text));
        }

        let xai_response: XAIResponse = response.json().await?;

        let choice = xai_response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| Error::InvalidRequest("No choices in response".into()))?;

        // Extract tool calls - convert arguments from String to serde_json::Value
        let tool_calls = choice
            .message
            .tool_calls
            .map(|calls| {
                calls
                    .into_iter()
                    .map(|tc| ToolCall {
                        id: tc.id,
                        name: tc.function.name,
                        arguments: serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(serde_json::Value::String(tc.function.arguments)),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(CompletionResponse {
            content: choice.message.content.unwrap_or_default(),
            model: request.model,
            tool_calls,
            usage: xai_response
                .usage
                .map(|u| TokenUsage {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                    ..Default::default()
                })
                .unwrap_or_default(),
            finish_reason: parse_finish_reason(choice.finish_reason),
            id: Some(xai_response.id),
        })
    }

    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        let messages = self.build_messages(&request);
        let tools = self.build_tools(request.tools.as_ref());

        let has_tools = request
            .tools
            .as_ref()
            .map(|t| !t.is_empty())
            .unwrap_or(false);
        let xai_request = XAIRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            top_p: request.top_p,
            tools,
            tool_choice: if has_tools {
                Some("auto".to_string())
            } else {
                None
            },
            stream: true,
        };

        debug!("Starting streaming request to xAI API");

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&xai_request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            if let Ok(error_resp) = serde_json::from_str::<XAIErrorResponse>(&error_text) {
                return Err(Error::api_error(
                    "xai",
                    status.as_u16(),
                    error_resp.error.message,
                ));
            }
            return Err(Error::api_error("xai", status.as_u16(), error_text));
        }

        let stream = async_stream::stream! {
            use futures::StreamExt;
            use tokio::io::AsyncBufReadExt;

            let mut reader = tokio::io::BufReader::new(
                tokio_util::io::StreamReader::new(
                    response.bytes_stream().map(|r| r.map_err(std::io::Error::other))
                )
            );

            let mut line = String::new();
            let mut tool_call_deltas: Vec<ToolCallDelta> = Vec::new();

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let line = line.trim();
                        if line.is_empty() || line == "data: [DONE]" {
                            continue;
                        }

                        if let Some(data) = line.strip_prefix("data: ") {
                            match serde_json::from_str::<XAIStreamResponse>(data) {
                                Ok(chunk) => {
                                    if let Some(choice) = chunk.choices.first() {
                                        // Handle content
                                        if let Some(content) = &choice.delta.content {
                                            yield Ok(StreamChunk {
                                                content: Some(content.clone()),
                                                tool_calls: None,
                                                done: false,
                                                finish_reason: None,
                                            });
                                        }

                                        // Handle tool calls
                                        if let Some(tc_deltas) = &choice.delta.tool_calls {
                                            for tc_delta in tc_deltas {
                                                let delta = ToolCallDelta {
                                                    index: tc_delta.index,
                                                    id: tc_delta.id.clone(),
                                                    name: tc_delta.function.as_ref().and_then(|f| f.name.clone()),
                                                    arguments: tc_delta.function.as_ref().and_then(|f| f.arguments.clone()),
                                                };
                                                tool_call_deltas.push(delta);
                                            }
                                        }

                                        // Check for finish
                                        if choice.finish_reason.is_some() {
                                            let finish_reason = parse_finish_reason(choice.finish_reason.clone());
                                            let tool_calls = if tool_call_deltas.is_empty() {
                                                None
                                            } else {
                                                Some(std::mem::take(&mut tool_call_deltas))
                                            };
                                            yield Ok(StreamChunk {
                                                content: None,
                                                tool_calls,
                                                done: true,
                                                finish_reason,
                                            });
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::debug!("Failed to parse stream chunk: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(Error::StreamError(e.to_string()));
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(format!("{}/models", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::api_error("xai", status.as_u16(), error_text));
        }

        let models_response: XAIModelsResponse = response.json().await?;

        Ok(models_response
            .data
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id.clone(),
                name: format_model_name(&m.id),
                provider: "xai".to_string(),
                description: Some(get_model_description(&m.id)),
                context_window: get_context_window(&m.id),
                max_output_tokens: None,
                supports_tools: true,
                supports_vision: m.id.contains("vision"),
            })
            .collect())
    }
}

fn get_context_window(model: &str) -> Option<u32> {
    match model {
        "grok-2" | "grok-2-mini" => Some(131072),
        "grok-2-vision-1212" => Some(32768),
        _ => None,
    }
}

fn format_model_name(id: &str) -> String {
    match id {
        "grok-2" => "Grok 2".to_string(),
        "grok-2-mini" => "Grok 2 Mini".to_string(),
        "grok-2-vision-1212" => "Grok 2 Vision".to_string(),
        "grok-2-image-1212" => "Grok 2 Image".to_string(),
        _ => id.to_string(),
    }
}

fn get_model_description(id: &str) -> String {
    match id {
        "grok-2" => "Most capable Grok model, excellent for complex reasoning".to_string(),
        "grok-2-mini" => "Faster, more efficient Grok model for everyday tasks".to_string(),
        "grok-2-vision-1212" => {
            "Grok 2 with vision capabilities for image understanding".to_string()
        }
        "grok-2-image-1212" => "Grok 2 for image generation".to_string(),
        _ => "xAI Grok model".to_string(),
    }
}

/// Parse finish reason string from API response to FinishReason enum
fn parse_finish_reason(reason: Option<String>) -> Option<FinishReason> {
    reason.and_then(|r| match r.as_str() {
        "stop" => Some(FinishReason::Stop),
        "length" => Some(FinishReason::Length),
        "tool_calls" => Some(FinishReason::ToolCalls),
        "content_filter" => Some(FinishReason::ContentFilter),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = XAIProvider::new("test-key");
        assert_eq!(provider.name(), "xai");
        assert_eq!(provider.default_model(), "grok-2");
    }

    #[test]
    fn test_message_building() {
        let provider = XAIProvider::new("test-key");
        let request = CompletionRequest::new("grok-2")
            .with_message(Message::system("You are Grok."))
            .with_message(Message::user("Hello!"));

        let messages = provider.build_messages(&request);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
    }

    #[test]
    fn test_tool_building() {
        let provider = XAIProvider::new("test-key");
        let tools = vec![ToolDefinition {
            name: "search".to_string(),
            description: "Search the web".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                },
                "required": ["query"]
            }),
        }];

        let xai_tools = provider.build_tools(Some(&tools));
        assert!(xai_tools.is_some());
        let xai_tools = xai_tools.unwrap();
        assert_eq!(xai_tools.len(), 1);
        assert_eq!(xai_tools[0].function.name, "search");
    }

    #[test]
    fn test_tool_building_none() {
        let provider = XAIProvider::new("test-key");
        let xai_tools = provider.build_tools(None);
        assert!(xai_tools.is_none());
    }
}
