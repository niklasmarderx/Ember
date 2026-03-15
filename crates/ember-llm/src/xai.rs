//! xAI Grok Provider
//!
//! Implementation for xAI's Grok models including Grok-2 and Grok-2 mini.
//! Uses OpenAI-compatible API format at api.x.ai

use crate::{
    CompletionRequest, CompletionResponse, Error, LLMProvider, Message, ModelInfo, Result, Role,
    StreamChunk, ToolCall, ToolDefinition,
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
        let api_key = std::env::var("XAI_API_KEY")
            .map_err(|_| Error::Configuration("XAI_API_KEY environment variable not set".into()))?;
        Ok(Self::new(api_key))
    }

    /// Set a custom base URL (for proxies or compatible endpoints)
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    fn build_messages(&self, request: &CompletionRequest) -> Vec<XAIMessage> {
        let mut messages = Vec::new();

        // Add system message if present
        if let Some(ref system) = request.system_prompt {
            messages.push(XAIMessage {
                role: "system".to_string(),
                content: Some(system.clone()),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // Convert messages
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
                                arguments: tc.arguments.clone(),
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

    fn build_tools(&self, tools: &[ToolDefinition]) -> Option<Vec<XAITool>> {
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
struct XAIToolCall {
    id: String,
    r#type: String,
    function: XAIFunction,
}

#[derive(Debug, Serialize, Deserialize)]
struct XAIFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct XAITool {
    r#type: String,
    function: XAIToolFunction,
}

#[derive(Debug, Serialize)]
struct XAIToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct XAIResponse {
    id: String,
    choices: Vec<XAIChoice>,
    usage: Option<XAIUsage>,
}

#[derive(Debug, Deserialize)]
struct XAIChoice {
    message: XAIResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct XAIResponseMessage {
    role: String,
    content: Option<String>,
    tool_calls: Option<Vec<XAIToolCall>>,
}

#[derive(Debug, Deserialize)]
struct XAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct XAIStreamResponse {
    choices: Vec<XAIStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct XAIStreamChoice {
    delta: XAIStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct XAIStreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<XAIStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct XAIStreamToolCall {
    index: usize,
    id: Option<String>,
    function: Option<XAIStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct XAIStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct XAIErrorResponse {
    error: XAIErrorDetail,
}

#[derive(Debug, Deserialize)]
struct XAIErrorDetail {
    message: String,
    r#type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct XAIModelsResponse {
    data: Vec<XAIModel>,
}

#[derive(Debug, Deserialize)]
struct XAIModel {
    id: String,
    owned_by: String,
}

#[async_trait]
impl LLMProvider for XAIProvider {
    fn name(&self) -> &str {
        "xai"
    }

    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let messages = self.build_messages(&request);
        let tools = self.build_tools(&request.tools);

        let xai_request = XAIRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            top_p: request.top_p,
            tools,
            tool_choice: if !request.tools.is_empty() {
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
            .await
            .map_err(|e| Error::Network(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            if let Ok(error_resp) = serde_json::from_str::<XAIErrorResponse>(&error_text) {
                return Err(Error::Api {
                    status_code: status.as_u16(),
                    message: error_resp.error.message,
                });
            }
            return Err(Error::Api {
                status_code: status.as_u16(),
                message: error_text,
            });
        }

        let xai_response: XAIResponse = response
            .json()
            .await
            .map_err(|e| Error::Parsing(e.to_string()))?;

        let choice = xai_response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| Error::Parsing("No choices in response".into()))?;

        // Extract tool calls
        let tool_calls = choice
            .message
            .tool_calls
            .map(|calls| {
                calls
                    .into_iter()
                    .map(|tc| ToolCall {
                        id: tc.id,
                        name: tc.function.name,
                        arguments: tc.function.arguments,
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(CompletionResponse {
            content: choice.message.content.unwrap_or_default(),
            model: request.model,
            tool_calls,
            usage: xai_response.usage.map(|u| crate::Usage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            }),
            finish_reason: choice.finish_reason,
        })
    }

    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        let messages = self.build_messages(&request);
        let tools = self.build_tools(&request.tools);

        let xai_request = XAIRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            top_p: request.top_p,
            tools,
            tool_choice: if !request.tools.is_empty() {
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
            .await
            .map_err(|e| Error::Network(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            if let Ok(error_resp) = serde_json::from_str::<XAIErrorResponse>(&error_text) {
                return Err(Error::Api {
                    status_code: status.as_u16(),
                    message: error_resp.error.message,
                });
            }
            return Err(Error::Api {
                status_code: status.as_u16(),
                message: error_text,
            });
        }

        let stream = async_stream::stream! {
            use futures::StreamExt;
            use tokio::io::AsyncBufReadExt;

            let mut reader = tokio::io::BufReader::new(
                tokio_util::io::StreamReader::new(
                    response.bytes_stream().map(|r| r.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)))
                )
            );

            let mut line = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();

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
                                                content: content.clone(),
                                                tool_calls: vec![],
                                                finish_reason: choice.finish_reason.clone(),
                                            });
                                        }

                                        // Handle tool calls
                                        if let Some(tc_deltas) = &choice.delta.tool_calls {
                                            for tc_delta in tc_deltas {
                                                let idx = tc_delta.index;

                                                // Ensure we have enough slots
                                                while tool_calls.len() <= idx {
                                                    tool_calls.push(ToolCall {
                                                        id: String::new(),
                                                        name: String::new(),
                                                        arguments: String::new(),
                                                    });
                                                }

                                                // Update the tool call
                                                if let Some(id) = &tc_delta.id {
                                                    tool_calls[idx].id = id.clone();
                                                }
                                                if let Some(f) = &tc_delta.function {
                                                    if let Some(name) = &f.name {
                                                        tool_calls[idx].name = name.clone();
                                                    }
                                                    if let Some(args) = &f.arguments {
                                                        tool_calls[idx].arguments.push_str(args);
                                                    }
                                                }
                                            }
                                        }

                                        // Check for finish
                                        if choice.finish_reason.is_some() && !tool_calls.is_empty() {
                                            yield Ok(StreamChunk {
                                                content: String::new(),
                                                tool_calls: std::mem::take(&mut tool_calls),
                                                finish_reason: choice.finish_reason.clone(),
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
                        yield Err(Error::Network(e.to_string()));
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
            .await
            .map_err(|e| Error::Network(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::Api {
                status_code: status.as_u16(),
                message: error_text,
            });
        }

        let models_response: XAIModelsResponse = response
            .json()
            .await
            .map_err(|e| Error::Parsing(e.to_string()))?;

        Ok(models_response
            .data
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id.clone(),
                name: format_model_name(&m.id),
                provider: "xai".to_string(),
                description: Some(get_model_description(&m.id)),
            })
            .collect())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = XAIProvider::new("test-key");
        assert_eq!(provider.name(), "xai");
    }

    #[test]
    fn test_message_building() {
        let provider = XAIProvider::new("test-key");
        let request = CompletionRequest::new("grok-2")
            .with_system_prompt("You are Grok.")
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

        let xai_tools = provider.build_tools(&tools);
        assert!(xai_tools.is_some());
        let xai_tools = xai_tools.unwrap();
        assert_eq!(xai_tools.len(), 1);
        assert_eq!(xai_tools[0].function.name, "search");
    }
}
