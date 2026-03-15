//! Mock LLM Provider for Testing
//!
//! This module provides a mock LLM provider that can be used in tests
//! without making real API calls.

use crate::{
    error::{Error, Result},
    CompletionRequest, CompletionResponse, LLMProvider, Message, ModelInfo, StreamChunk, ToolCall,
};
use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A mock LLM provider for testing purposes.
///
/// This provider allows you to configure expected responses and
/// verify that requests match expected patterns.
#[derive(Clone)]
pub struct MockProvider {
    name: String,
    default_model: String,
    responses: Arc<Mutex<Vec<MockResponse>>>,
    calls: Arc<Mutex<Vec<CompletionRequest>>>,
    delay: Option<Duration>,
}

/// A configured mock response.
#[derive(Clone, Debug)]
pub struct MockResponse {
    /// The content to return.
    pub content: String,
    /// Optional tool calls to include.
    pub tool_calls: Vec<ToolCall>,
    /// Whether to simulate an error.
    pub error: Option<String>,
    /// Whether this response has been used.
    pub used: bool,
}

impl MockProvider {
    /// Create a new mock provider.
    pub fn new() -> Self {
        Self {
            name: "mock".to_string(),
            default_model: "mock-model".to_string(),
            responses: Arc::new(Mutex::new(Vec::new())),
            calls: Arc::new(Mutex::new(Vec::new())),
            delay: None,
        }
    }

    /// Set the provider name.
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// Set the default model.
    pub fn with_default_model(mut self, model: &str) -> Self {
        self.default_model = model.to_string();
        self
    }

    /// Add a simulated delay to responses.
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    /// Queue a response to be returned on the next call.
    pub fn queue_response(&self, content: &str) -> &Self {
        let mut responses = self.responses.lock().unwrap();
        responses.push(MockResponse {
            content: content.to_string(),
            tool_calls: Vec::new(),
            error: None,
            used: false,
        });
        self
    }

    /// Queue a response with tool calls.
    pub fn queue_tool_response(&self, content: &str, tool_calls: Vec<ToolCall>) -> &Self {
        let mut responses = self.responses.lock().unwrap();
        responses.push(MockResponse {
            content: content.to_string(),
            tool_calls,
            error: None,
            used: false,
        });
        self
    }

    /// Queue an error response.
    pub fn queue_error(&self, error: &str) -> &Self {
        let mut responses = self.responses.lock().unwrap();
        responses.push(MockResponse {
            content: String::new(),
            tool_calls: Vec::new(),
            error: Some(error.to_string()),
            used: false,
        });
        self
    }

    /// Get all recorded calls.
    pub fn get_calls(&self) -> Vec<CompletionRequest> {
        self.calls.lock().unwrap().clone()
    }

    /// Get the number of calls made.
    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    /// Clear all recorded calls.
    pub fn clear_calls(&self) {
        self.calls.lock().unwrap().clear();
    }

    /// Clear all queued responses.
    pub fn clear_responses(&self) {
        self.responses.lock().unwrap().clear();
    }

    /// Reset the provider (clear calls and responses).
    pub fn reset(&self) {
        self.clear_calls();
        self.clear_responses();
    }

    /// Get the next response or a default.
    fn next_response(&self) -> MockResponse {
        let mut responses = self.responses.lock().unwrap();

        // Find first unused response
        for response in responses.iter_mut() {
            if !response.used {
                response.used = true;
                return response.clone();
            }
        }

        // Default response if none queued
        MockResponse {
            content: "Mock response".to_string(),
            tool_calls: Vec::new(),
            error: None,
            used: true,
        }
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LLMProvider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        // Record the call
        self.calls.lock().unwrap().push(request.clone());

        // Simulate delay if configured
        if let Some(delay) = self.delay {
            tokio::time::sleep(delay).await;
        }

        // Get the next response
        let response = self.next_response();

        // Return error if configured
        if let Some(error) = response.error {
            return Err(Error::InvalidRequest(error));
        }

        Ok(CompletionResponse {
            content: response.content,
            model: if request.model.is_empty() {
                self.default_model.clone()
            } else {
                request.model.clone()
            },
            tool_calls: response.tool_calls,
            usage: crate::TokenUsage::default(),
            finish_reason: Some(crate::FinishReason::Stop),
            id: None,
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, Result<StreamChunk>>> {
        // Record the call
        self.calls.lock().unwrap().push(request.clone());

        // Simulate delay if configured
        if let Some(delay) = self.delay {
            tokio::time::sleep(delay).await;
        }

        // Get the next response
        let response = self.next_response();

        // Return error if configured
        if let Some(error) = response.error {
            return Err(Error::InvalidRequest(error));
        }

        // Split content into chunks for streaming simulation
        let words: Vec<String> = response
            .content
            .split_whitespace()
            .map(|s| format!("{} ", s))
            .collect();

        // Convert tool calls to deltas for the first chunk
        let tool_call_deltas: Option<Vec<crate::ToolCallDelta>> = if response.tool_calls.is_empty()
        {
            None
        } else {
            Some(
                response
                    .tool_calls
                    .iter()
                    .enumerate()
                    .map(|(i, tc)| crate::ToolCallDelta {
                        index: i,
                        id: Some(tc.id.clone()),
                        name: Some(tc.name.clone()),
                        arguments: Some(tc.arguments.to_string()),
                    })
                    .collect(),
            )
        };

        let chunks: Vec<Result<StreamChunk>> = words
            .into_iter()
            .enumerate()
            .map(|(i, word)| {
                Ok(StreamChunk {
                    content: Some(word),
                    tool_calls: if i == 0 {
                        tool_call_deltas.clone()
                    } else {
                        None
                    },
                    done: false,
                    finish_reason: None,
                })
            })
            .chain(std::iter::once(Ok(StreamChunk {
                content: None,
                tool_calls: None,
                done: true,
                finish_reason: Some(crate::FinishReason::Stop),
            })))
            .collect();

        Ok(Box::pin(stream::iter(chunks)))
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(vec![ModelInfo {
            id: self.default_model.clone(),
            name: "Mock Model".to_string(),
            description: Some("A mock model for testing".to_string()),
            context_window: Some(4096),
            max_output_tokens: Some(1024),
            supports_tools: true,
            supports_vision: false,
            provider: self.name.clone(),
        }])
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_provider_basic() {
        let provider = MockProvider::new();
        provider.queue_response("Hello, world!");

        let request = CompletionRequest::new("test-model").with_message(Message::user("Hi"));

        let response = provider.complete(request).await.unwrap();
        assert_eq!(response.content, "Hello, world!");
        assert_eq!(provider.call_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_provider_multiple_responses() {
        let provider = MockProvider::new();
        provider.queue_response("First");
        provider.queue_response("Second");
        provider.queue_response("Third");

        let request = CompletionRequest::new("test-model");

        let r1 = provider.complete(request.clone()).await.unwrap();
        assert_eq!(r1.content, "First");

        let r2 = provider.complete(request.clone()).await.unwrap();
        assert_eq!(r2.content, "Second");

        let r3 = provider.complete(request).await.unwrap();
        assert_eq!(r3.content, "Third");

        assert_eq!(provider.call_count(), 3);
    }

    #[tokio::test]
    async fn test_mock_provider_error() {
        let provider = MockProvider::new();
        provider.queue_error("API rate limit exceeded");

        let request = CompletionRequest::new("test-model");
        let result = provider.complete(request).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_provider_tool_calls() {
        let provider = MockProvider::new();
        provider.queue_tool_response(
            "I'll help you with that.",
            vec![ToolCall {
                id: "call_123".to_string(),
                name: "shell".to_string(),
                arguments: serde_json::json!({"command": "ls -la"}),
            }],
        );

        let request = CompletionRequest::new("test-model");
        let response = provider.complete(request).await.unwrap();

        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].name, "shell");
    }

    #[tokio::test]
    async fn test_mock_provider_streaming() {
        let provider = MockProvider::new();
        provider.queue_response("Hello world test");

        let request = CompletionRequest::new("test-model");
        let mut stream = provider.complete_stream(request).await.unwrap();

        let mut content = String::new();
        while let Some(chunk) = stream.next().await {
            if let Some(c) = chunk.unwrap().content {
                content.push_str(&c);
            }
        }

        assert_eq!(content.trim(), "Hello world test");
    }

    #[tokio::test]
    async fn test_mock_provider_delay() {
        let provider = MockProvider::new().with_delay(Duration::from_millis(100));
        provider.queue_response("Delayed response");

        let start = std::time::Instant::now();
        let request = CompletionRequest::new("test-model");
        let _ = provider.complete(request).await.unwrap();
        let elapsed = start.elapsed();

        assert!(elapsed >= Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_mock_provider_call_recording() {
        let provider = MockProvider::new();
        provider.queue_response("Response 1");
        provider.queue_response("Response 2");

        let request1 =
            CompletionRequest::new("model-a").with_message(Message::user("First question"));
        let request2 =
            CompletionRequest::new("model-b").with_message(Message::user("Second question"));

        let _ = provider.complete(request1).await;
        let _ = provider.complete(request2).await;

        let calls = provider.get_calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].model, "model-a");
        assert_eq!(calls[1].model, "model-b");
    }
}
