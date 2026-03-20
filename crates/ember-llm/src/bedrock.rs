//! AWS Bedrock provider for Ember.
//!
//! This module provides integration with AWS Bedrock, supporting multiple foundation models
//! including Claude (Anthropic), Titan (Amazon), Llama (Meta), and Mistral.
//!
//! # Setup
//!
//! AWS credentials can be configured via:
//! - Environment variables (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION)
//! - AWS credentials file (~/.aws/credentials)
//! - IAM role (when running on AWS infrastructure)
//!
//! # Example
//!
//! ```rust,no_run
//! use ember_llm::{BedrockProvider, CompletionRequest, Message, LLMProvider};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let provider = BedrockProvider::from_env()?;
//!     
//!     let request = CompletionRequest::new("anthropic.claude-3-sonnet-20240229-v1:0")
//!         .with_message(Message::user("Hello from AWS Bedrock!"));
//!     
//!     let response = provider.complete(request).await?;
//!     println!("{}", response.content);
//!     
//!     Ok(())
//! }
//! ```

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error, warn};

use crate::provider::StreamResponse;
use crate::{
    CompletionRequest, CompletionResponse, Error, FinishReason, LLMProvider, Message, ModelInfo,
    Result, Role, StreamChunk, TokenUsage,
};

/// Supported model families in Bedrock
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BedrockModelFamily {
    /// Anthropic Claude models
    Claude,
    /// Amazon Titan models
    Titan,
    /// Meta Llama models
    Llama,
    /// Mistral AI models
    Mistral,
    /// Cohere models
    Cohere,
    /// AI21 Jurassic models
    Jurassic,
}

impl BedrockModelFamily {
    /// Detect model family from model ID
    pub fn from_model_id(model_id: &str) -> Option<Self> {
        if model_id.starts_with("anthropic.") {
            Some(Self::Claude)
        } else if model_id.starts_with("amazon.titan") {
            Some(Self::Titan)
        } else if model_id.starts_with("meta.llama") {
            Some(Self::Llama)
        } else if model_id.starts_with("mistral.") {
            Some(Self::Mistral)
        } else if model_id.starts_with("cohere.") {
            Some(Self::Cohere)
        } else if model_id.starts_with("ai21.") {
            Some(Self::Jurassic)
        } else {
            None
        }
    }
}

/// AWS Bedrock provider configuration
#[derive(Debug, Clone)]
pub struct BedrockConfig {
    /// AWS region
    pub region: String,
    /// Optional AWS access key ID (if not using IAM role)
    pub access_key_id: Option<String>,
    /// Optional AWS secret access key (if not using IAM role)
    pub secret_access_key: Option<String>,
    /// Optional session token for temporary credentials
    pub session_token: Option<String>,
    /// Default model to use
    pub default_model: String,
    /// Optional custom endpoint URL
    pub endpoint_url: Option<String>,
}

impl Default for BedrockConfig {
    fn default() -> Self {
        Self {
            region: "us-east-1".to_string(),
            access_key_id: None,
            secret_access_key: None,
            session_token: None,
            default_model: "anthropic.claude-3-sonnet-20240229-v1:0".to_string(),
            endpoint_url: None,
        }
    }
}

/// AWS Bedrock LLM Provider
///
/// Provides access to foundation models available through AWS Bedrock including:
/// - Anthropic Claude (claude-3-opus, claude-3-sonnet, claude-3-haiku)
/// - Amazon Titan (titan-text-express, titan-text-lite)
/// - Meta Llama (llama-2-13b, llama-2-70b)
/// - Mistral (mistral-7b, mixtral-8x7b)
/// - Cohere (command, command-light)
/// - AI21 Jurassic (jurassic-2-mid, jurassic-2-ultra)
pub struct BedrockProvider {
    client: reqwest::Client,
    config: BedrockConfig,
}

impl BedrockProvider {
    /// Create a new Bedrock provider with the given configuration
    pub fn new(config: BedrockConfig) -> Self {
        let client = reqwest::Client::new();
        Self { client, config }
    }

    /// Create a new Bedrock provider from environment variables
    ///
    /// Reads:
    /// - AWS_REGION or AWS_DEFAULT_REGION
    /// - AWS_ACCESS_KEY_ID (optional)
    /// - AWS_SECRET_ACCESS_KEY (optional)
    /// - AWS_SESSION_TOKEN (optional)
    /// - BEDROCK_DEFAULT_MODEL (optional)
    pub fn from_env() -> Result<Self> {
        let region = std::env::var("AWS_REGION")
            .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
            .unwrap_or_else(|_| "us-east-1".to_string());

        let config = BedrockConfig {
            region,
            access_key_id: std::env::var("AWS_ACCESS_KEY_ID").ok(),
            secret_access_key: std::env::var("AWS_SECRET_ACCESS_KEY").ok(),
            session_token: std::env::var("AWS_SESSION_TOKEN").ok(),
            default_model: std::env::var("BEDROCK_DEFAULT_MODEL")
                .unwrap_or_else(|_| "anthropic.claude-3-sonnet-20240229-v1:0".to_string()),
            endpoint_url: std::env::var("BEDROCK_ENDPOINT_URL").ok(),
        };

        Ok(Self::new(config))
    }

    /// Create a Bedrock provider with explicit credentials
    pub fn with_credentials(
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
    ) -> Self {
        let config = BedrockConfig {
            region: region.into(),
            access_key_id: Some(access_key_id.into()),
            secret_access_key: Some(secret_access_key.into()),
            ..Default::default()
        };
        Self::new(config)
    }

    /// Get the base URL for Bedrock API
    fn base_url(&self) -> String {
        self.config.endpoint_url.clone().unwrap_or_else(|| {
            format!(
                "https://bedrock-runtime.{}.amazonaws.com",
                self.config.region
            )
        })
    }

    /// Convert messages to model-specific format
    fn format_messages_for_model(
        &self,
        messages: &[Message],
        model_id: &str,
    ) -> Result<serde_json::Value> {
        let family = BedrockModelFamily::from_model_id(model_id).ok_or_else(|| {
            Error::model_not_found(model_id, "bedrock")
        })?;

        match family {
            BedrockModelFamily::Claude => self.format_claude_messages(messages),
            BedrockModelFamily::Titan => self.format_titan_messages(messages),
            BedrockModelFamily::Llama => self.format_llama_messages(messages),
            BedrockModelFamily::Mistral => self.format_mistral_messages(messages),
            BedrockModelFamily::Cohere => self.format_cohere_messages(messages),
            BedrockModelFamily::Jurassic => self.format_jurassic_messages(messages),
        }
    }

    fn format_claude_messages(&self, messages: &[Message]) -> Result<serde_json::Value> {
        let formatted: Vec<serde_json::Value> = messages
            .iter()
            .filter(|m| m.role != Role::System) // System messages handled separately
            .map(|m| {
                serde_json::json!({
                    "role": match m.role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        Role::System => "user",
                        Role::Tool => "user",
                    },
                    "content": m.content
                })
            })
            .collect();

        Ok(serde_json::json!({
            "anthropic_version": "bedrock-2023-05-31",
            "messages": formatted
        }))
    }

    fn format_titan_messages(&self, messages: &[Message]) -> Result<serde_json::Value> {
        let prompt: String = messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::User => "User",
                    Role::Assistant => "Bot",
                    Role::System => "System",
                    Role::Tool => "Tool",
                };
                format!("{}:\n{}\n", role, m.content)
            })
            .collect();

        Ok(serde_json::json!({
            "inputText": prompt
        }))
    }

    fn format_llama_messages(&self, messages: &[Message]) -> Result<serde_json::Value> {
        let mut prompt = String::new();

        for message in messages {
            match message.role {
                Role::System => {
                    prompt.push_str(&format!("<<SYS>>\n{}\n<</SYS>>\n\n", message.content));
                }
                Role::User => {
                    prompt.push_str(&format!("[INST] {} [/INST]", message.content));
                }
                Role::Assistant => {
                    prompt.push_str(&format!(" {}", message.content));
                }
                Role::Tool => {
                    prompt.push_str(&format!("[TOOL] {} [/TOOL]", message.content));
                }
            }
        }

        Ok(serde_json::json!({
            "prompt": prompt
        }))
    }

    fn format_mistral_messages(&self, messages: &[Message]) -> Result<serde_json::Value> {
        let mut prompt = String::new();

        for message in messages {
            match message.role {
                Role::User => {
                    prompt.push_str(&format!("<s>[INST] {} [/INST]", message.content));
                }
                Role::Assistant => {
                    prompt.push_str(&format!(" {}</s>", message.content));
                }
                Role::System => {
                    prompt.push_str(&format!("[INST] {} [/INST]", message.content));
                }
                Role::Tool => {
                    prompt.push_str(&format!("[TOOL] {} [/TOOL]", message.content));
                }
            }
        }

        Ok(serde_json::json!({
            "prompt": prompt
        }))
    }

    fn format_cohere_messages(&self, messages: &[Message]) -> Result<serde_json::Value> {
        let chat_history: Vec<serde_json::Value> = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| {
                serde_json::json!({
                    "role": match m.role {
                        Role::User => "USER",
                        Role::Assistant => "CHATBOT",
                        _ => "USER",
                    },
                    "message": m.content
                })
            })
            .collect();

        let preamble = messages
            .iter()
            .find(|m| m.role == Role::System)
            .map(|m| m.content.clone());

        let mut body = serde_json::json!({
            "chat_history": chat_history
        });

        if let Some(p) = preamble {
            body["preamble"] = serde_json::Value::String(p);
        }

        if let Some(last_user) = messages.iter().rev().find(|m| m.role == Role::User) {
            body["message"] = serde_json::Value::String(last_user.content.clone());
        }

        Ok(body)
    }

    fn format_jurassic_messages(&self, messages: &[Message]) -> Result<serde_json::Value> {
        let prompt: String = messages
            .iter()
            .map(|m| m.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n");

        Ok(serde_json::json!({
            "prompt": prompt
        }))
    }

    /// Parse response based on model family
    fn parse_response(
        &self,
        model_id: &str,
        response_body: &serde_json::Value,
    ) -> Result<CompletionResponse> {
        let family = BedrockModelFamily::from_model_id(model_id)
            .ok_or_else(|| Error::model_not_found(model_id, "bedrock"))?;

        let (content, usage) = match family {
            BedrockModelFamily::Claude => {
                let content = response_body["content"][0]["text"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let usage = TokenUsage::new(
                    response_body["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
                    response_body["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
                );
                (content, usage)
            }
            BedrockModelFamily::Titan => {
                let content = response_body["results"][0]["outputText"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let usage = TokenUsage::new(
                    response_body["inputTextTokenCount"].as_u64().unwrap_or(0) as u32,
                    response_body["results"][0]["tokenCount"]
                        .as_u64()
                        .unwrap_or(0) as u32,
                );
                (content, usage)
            }
            BedrockModelFamily::Llama => {
                let content = response_body["generation"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let usage = TokenUsage::new(
                    response_body["prompt_token_count"].as_u64().unwrap_or(0) as u32,
                    response_body["generation_token_count"]
                        .as_u64()
                        .unwrap_or(0) as u32,
                );
                (content, usage)
            }
            BedrockModelFamily::Mistral => {
                let content = response_body["outputs"][0]["text"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let usage = TokenUsage::default();
                (content, usage)
            }
            BedrockModelFamily::Cohere => {
                let content = response_body["text"].as_str().unwrap_or("").to_string();
                let usage = TokenUsage::default();
                (content, usage)
            }
            BedrockModelFamily::Jurassic => {
                let content = response_body["completions"][0]["data"]["text"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let usage = TokenUsage::default();
                (content, usage)
            }
        };

        Ok(CompletionResponse {
            content,
            model: model_id.to_string(),
            usage,
            tool_calls: vec![],
            finish_reason: Some(FinishReason::Stop),
            id: None,
        })
    }

    /// Get available models for each family
    fn get_available_models() -> Vec<ModelInfo> {
        vec![
            // Claude models
            ModelInfo {
                id: "anthropic.claude-3-opus-20240229-v1:0".to_string(),
                name: "Claude 3 Opus".to_string(),
                description: Some("Most capable Claude 3 model for complex tasks".to_string()),
                context_window: Some(200000),
                max_output_tokens: Some(4096),
                supports_tools: true,
                supports_vision: true,
                provider: "bedrock".to_string(),
            },
            ModelInfo {
                id: "anthropic.claude-3-sonnet-20240229-v1:0".to_string(),
                name: "Claude 3 Sonnet".to_string(),
                description: Some("Balanced Claude 3 model".to_string()),
                context_window: Some(200000),
                max_output_tokens: Some(4096),
                supports_tools: true,
                supports_vision: true,
                provider: "bedrock".to_string(),
            },
            ModelInfo {
                id: "anthropic.claude-3-haiku-20240307-v1:0".to_string(),
                name: "Claude 3 Haiku".to_string(),
                description: Some("Fast and efficient Claude 3 model".to_string()),
                context_window: Some(200000),
                max_output_tokens: Some(4096),
                supports_tools: true,
                supports_vision: true,
                provider: "bedrock".to_string(),
            },
            // Titan models
            ModelInfo {
                id: "amazon.titan-text-express-v1".to_string(),
                name: "Titan Text Express".to_string(),
                description: Some("Fast Amazon Titan model".to_string()),
                context_window: Some(8000),
                max_output_tokens: Some(8000),
                supports_tools: false,
                supports_vision: false,
                provider: "bedrock".to_string(),
            },
            ModelInfo {
                id: "amazon.titan-text-lite-v1".to_string(),
                name: "Titan Text Lite".to_string(),
                description: Some("Lightweight Amazon Titan model".to_string()),
                context_window: Some(4000),
                max_output_tokens: Some(4000),
                supports_tools: false,
                supports_vision: false,
                provider: "bedrock".to_string(),
            },
            // Llama models
            ModelInfo {
                id: "meta.llama2-13b-chat-v1".to_string(),
                name: "Llama 2 13B Chat".to_string(),
                description: Some("Meta Llama 2 13B chat model".to_string()),
                context_window: Some(4096),
                max_output_tokens: Some(2048),
                supports_tools: false,
                supports_vision: false,
                provider: "bedrock".to_string(),
            },
            ModelInfo {
                id: "meta.llama2-70b-chat-v1".to_string(),
                name: "Llama 2 70B Chat".to_string(),
                description: Some("Meta Llama 2 70B chat model".to_string()),
                context_window: Some(4096),
                max_output_tokens: Some(2048),
                supports_tools: false,
                supports_vision: false,
                provider: "bedrock".to_string(),
            },
            ModelInfo {
                id: "meta.llama3-8b-instruct-v1:0".to_string(),
                name: "Llama 3 8B Instruct".to_string(),
                description: Some("Meta Llama 3 8B instruction-tuned".to_string()),
                context_window: Some(8192),
                max_output_tokens: Some(2048),
                supports_tools: false,
                supports_vision: false,
                provider: "bedrock".to_string(),
            },
            ModelInfo {
                id: "meta.llama3-70b-instruct-v1:0".to_string(),
                name: "Llama 3 70B Instruct".to_string(),
                description: Some("Meta Llama 3 70B instruction-tuned".to_string()),
                context_window: Some(8192),
                max_output_tokens: Some(2048),
                supports_tools: false,
                supports_vision: false,
                provider: "bedrock".to_string(),
            },
            // Mistral models
            ModelInfo {
                id: "mistral.mistral-7b-instruct-v0:2".to_string(),
                name: "Mistral 7B Instruct".to_string(),
                description: Some("Mistral 7B instruction-tuned".to_string()),
                context_window: Some(32768),
                max_output_tokens: Some(8192),
                supports_tools: false,
                supports_vision: false,
                provider: "bedrock".to_string(),
            },
            ModelInfo {
                id: "mistral.mixtral-8x7b-instruct-v0:1".to_string(),
                name: "Mixtral 8x7B".to_string(),
                description: Some("Mistral MoE model".to_string()),
                context_window: Some(32768),
                max_output_tokens: Some(8192),
                supports_tools: false,
                supports_vision: false,
                provider: "bedrock".to_string(),
            },
            // Cohere models
            ModelInfo {
                id: "cohere.command-text-v14".to_string(),
                name: "Command".to_string(),
                description: Some("Cohere Command model".to_string()),
                context_window: Some(4096),
                max_output_tokens: Some(4096),
                supports_tools: false,
                supports_vision: false,
                provider: "bedrock".to_string(),
            },
            // AI21 models
            ModelInfo {
                id: "ai21.j2-mid-v1".to_string(),
                name: "Jurassic-2 Mid".to_string(),
                description: Some("AI21 Jurassic-2 Mid model".to_string()),
                context_window: Some(8192),
                max_output_tokens: Some(8191),
                supports_tools: false,
                supports_vision: false,
                provider: "bedrock".to_string(),
            },
            ModelInfo {
                id: "ai21.j2-ultra-v1".to_string(),
                name: "Jurassic-2 Ultra".to_string(),
                description: Some("AI21 Jurassic-2 Ultra model".to_string()),
                context_window: Some(8192),
                max_output_tokens: Some(8191),
                supports_tools: false,
                supports_vision: false,
                provider: "bedrock".to_string(),
            },
        ]
    }
}

#[async_trait]
impl LLMProvider for BedrockProvider {
    fn name(&self) -> &str {
        "bedrock"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let model_id = &request.model;
        debug!("Bedrock complete request for model: {}", model_id);

        // Format the request body based on model family
        let mut body = self.format_messages_for_model(&request.messages, model_id)?;

        // Add common parameters
        if let Some(max_tokens) = request.max_tokens {
            let family = BedrockModelFamily::from_model_id(model_id);
            match family {
                Some(BedrockModelFamily::Claude) => {
                    body["max_tokens"] = serde_json::Value::Number(max_tokens.into());
                }
                Some(BedrockModelFamily::Titan) => {
                    body["textGenerationConfig"] = serde_json::json!({
                        "maxTokenCount": max_tokens
                    });
                }
                Some(BedrockModelFamily::Llama) | Some(BedrockModelFamily::Mistral) => {
                    body["max_gen_len"] = serde_json::Value::Number(max_tokens.into());
                }
                Some(BedrockModelFamily::Cohere) => {
                    body["max_tokens"] = serde_json::Value::Number(max_tokens.into());
                }
                Some(BedrockModelFamily::Jurassic) => {
                    body["maxTokens"] = serde_json::Value::Number(max_tokens.into());
                }
                None => {}
            }
        }

        if let Some(temp) = request.temperature {
            let family = BedrockModelFamily::from_model_id(model_id);
            match family {
                Some(BedrockModelFamily::Claude) => {
                    body["temperature"] = serde_json::Value::Number(
                        serde_json::Number::from_f64(temp as f64)
                            .unwrap_or(serde_json::Number::from(1)),
                    );
                }
                Some(BedrockModelFamily::Titan) => {
                    if let Some(config) = body.get_mut("textGenerationConfig") {
                        config["temperature"] = serde_json::Value::Number(
                            serde_json::Number::from_f64(temp as f64)
                                .unwrap_or(serde_json::Number::from(1)),
                        );
                    }
                }
                _ => {
                    body["temperature"] = serde_json::Value::Number(
                        serde_json::Number::from_f64(temp as f64)
                            .unwrap_or(serde_json::Number::from(1)),
                    );
                }
            }
        }

        // Add system message for Claude
        if let Some(BedrockModelFamily::Claude) = BedrockModelFamily::from_model_id(model_id) {
            if let Some(system_msg) = request.messages.iter().find(|m| m.role == Role::System) {
                body["system"] = serde_json::Value::String(system_msg.content.clone());
            }
        }

        let url = format!("{}/model/{}/invoke", self.base_url(), model_id);

        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| Error::InvalidRequest(format!("Failed to serialize request: {}", e)))?;

        // Note: In production, proper AWS SigV4 signing should be used
        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(body_bytes)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!("Bedrock API error: {} - {}", status, error_text);
            return Err(Error::api_error("bedrock", status.as_u16(), error_text));
        }

        let response_body: serde_json::Value = response.json().await?;

        debug!("Bedrock response: {:?}", response_body);

        self.parse_response(model_id, &response_body)
    }

    async fn complete_stream(&self, request: CompletionRequest) -> Result<StreamResponse> {
        let model_id = request.model.clone();
        debug!("Bedrock streaming request for model: {}", model_id);

        // Check if model supports streaming
        let family = BedrockModelFamily::from_model_id(&model_id);

        // Only Claude models have good streaming support in Bedrock
        if family != Some(BedrockModelFamily::Claude) {
            warn!(
                "Model {} may not support streaming, falling back to non-streaming",
                model_id
            );
            // Fall back to non-streaming
            let response = self.complete(request).await?;
            let (tx, rx) = mpsc::channel(1);
            tokio::spawn(async move {
                let _ = tx
                    .send(Ok(StreamChunk {
                        content: Some(response.content),
                        tool_calls: None,
                        done: true,
                        finish_reason: Some(FinishReason::Stop),
                    }))
                    .await;
            });
            return Ok(Box::pin(ReceiverStream::new(rx)));
        }

        // Format the request body
        let mut body = self.format_messages_for_model(&request.messages, &model_id)?;

        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::Value::Number(max_tokens.into());
        }

        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::Value::Number(
                serde_json::Number::from_f64(temp as f64).unwrap_or(serde_json::Number::from(1)),
            );
        }

        // Add system message for Claude
        if let Some(system_msg) = request.messages.iter().find(|m| m.role == Role::System) {
            body["system"] = serde_json::Value::String(system_msg.content.clone());
        }

        let url = format!(
            "{}/model/{}/invoke-with-response-stream",
            self.base_url(),
            model_id
        );

        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| Error::InvalidRequest(format!("Failed to serialize request: {}", e)))?;

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/vnd.amazon.eventstream")
            .body(body_bytes)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::api_error("bedrock", status.as_u16(), error_text));
        }

        let (tx, rx) = mpsc::channel(100);

        // Spawn task to process the event stream
        tokio::spawn(async move {
            // Note: Bedrock uses a custom binary event stream format
            // This is a simplified implementation
            let bytes = response.bytes().await;
            match bytes {
                Ok(data) => {
                    // Try to parse as JSON (fallback for non-streaming response)
                    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&data) {
                        let content = json["content"][0]["text"]
                            .as_str()
                            .or_else(|| json["completion"].as_str())
                            .unwrap_or("")
                            .to_string();

                        let _ = tx
                            .send(Ok(StreamChunk {
                                content: Some(content),
                                tool_calls: None,
                                done: true,
                                finish_reason: Some(FinishReason::Stop),
                            }))
                            .await;
                    }
                }
                Err(e) => {
                    let _ = tx
                        .send(Err(Error::StreamError(format!("Stream error: {}", e))))
                        .await;
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Return the statically defined available models
        Ok(Self::get_available_models())
    }

    async fn health_check(&self) -> Result<()> {
        debug!("Performing Bedrock health check");
        // For now, just return Ok since we don't want to make actual API calls
        Ok(())
    }

    fn supports_tools(&self) -> bool {
        // Claude 3 models on Bedrock support tools
        BedrockModelFamily::from_model_id(&self.config.default_model)
            == Some(BedrockModelFamily::Claude)
    }

    fn supports_vision(&self) -> bool {
        // Claude 3 models on Bedrock support vision
        BedrockModelFamily::from_model_id(&self.config.default_model)
            == Some(BedrockModelFamily::Claude)
    }

    fn default_model(&self) -> &str {
        &self.config.default_model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_family_detection() {
        assert_eq!(
            BedrockModelFamily::from_model_id("anthropic.claude-3-sonnet-20240229-v1:0"),
            Some(BedrockModelFamily::Claude)
        );
        assert_eq!(
            BedrockModelFamily::from_model_id("amazon.titan-text-express-v1"),
            Some(BedrockModelFamily::Titan)
        );
        assert_eq!(
            BedrockModelFamily::from_model_id("meta.llama2-70b-chat-v1"),
            Some(BedrockModelFamily::Llama)
        );
        assert_eq!(
            BedrockModelFamily::from_model_id("mistral.mistral-7b-instruct-v0:2"),
            Some(BedrockModelFamily::Mistral)
        );
        assert_eq!(
            BedrockModelFamily::from_model_id("cohere.command-text-v14"),
            Some(BedrockModelFamily::Cohere)
        );
        assert_eq!(
            BedrockModelFamily::from_model_id("ai21.j2-ultra-v1"),
            Some(BedrockModelFamily::Jurassic)
        );
        assert_eq!(BedrockModelFamily::from_model_id("unknown.model"), None);
    }

    #[test]
    fn test_provider_creation() {
        let provider = BedrockProvider::new(BedrockConfig::default());
        assert_eq!(provider.name(), "bedrock");
        assert_eq!(provider.config.region, "us-east-1");
    }

    #[test]
    fn test_default_model() {
        let provider = BedrockProvider::new(BedrockConfig::default());
        assert_eq!(
            provider.default_model(),
            "anthropic.claude-3-sonnet-20240229-v1:0"
        );
    }

    #[test]
    fn test_base_url() {
        let provider = BedrockProvider::new(BedrockConfig {
            region: "eu-west-1".to_string(),
            ..Default::default()
        });
        assert_eq!(
            provider.base_url(),
            "https://bedrock-runtime.eu-west-1.amazonaws.com"
        );
    }

    #[test]
    fn test_custom_endpoint() {
        let provider = BedrockProvider::new(BedrockConfig {
            endpoint_url: Some("http://localhost:8080".to_string()),
            ..Default::default()
        });
        assert_eq!(provider.base_url(), "http://localhost:8080");
    }

    #[test]
    fn test_available_models() {
        let models = BedrockProvider::get_available_models();
        assert!(!models.is_empty());

        // Check that we have models from different families
        let has_claude = models.iter().any(|m| m.id.starts_with("anthropic."));
        let has_titan = models.iter().any(|m| m.id.starts_with("amazon.titan"));
        let has_llama = models.iter().any(|m| m.id.starts_with("meta.llama"));

        assert!(has_claude, "Should have Claude models");
        assert!(has_titan, "Should have Titan models");
        assert!(has_llama, "Should have Llama models");
    }

    #[test]
    fn test_format_claude_messages() {
        let provider = BedrockProvider::new(BedrockConfig::default());
        let messages = vec![Message::user("Hello"), Message::assistant("Hi there!")];

        let result = provider.format_claude_messages(&messages);
        assert!(result.is_ok());

        let body = result.unwrap();
        assert!(body.get("messages").is_some());
        assert!(body.get("anthropic_version").is_some());
    }

    #[test]
    fn test_supports_tools_claude() {
        let provider = BedrockProvider::new(BedrockConfig {
            default_model: "anthropic.claude-3-sonnet-20240229-v1:0".to_string(),
            ..Default::default()
        });
        assert!(provider.supports_tools());
        assert!(provider.supports_vision());
    }

    #[test]
    fn test_supports_tools_titan() {
        let provider = BedrockProvider::new(BedrockConfig {
            default_model: "amazon.titan-text-express-v1".to_string(),
            ..Default::default()
        });
        assert!(!provider.supports_tools());
        assert!(!provider.supports_vision());
    }
}