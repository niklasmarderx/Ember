//! LLM Provider trait definition

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

use crate::{CompletionRequest, CompletionResponse, ModelInfo, Result, StreamChunk};

/// A boxed stream of stream chunks
pub type StreamResponse = Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>;

/// Trait for LLM providers
///
/// This trait defines the interface that all LLM providers must implement.
/// It allows Ember to work with different LLM backends through a unified API.
///
/// # Example
///
/// ```rust,no_run
/// use ember_llm::{LLMProvider, CompletionRequest, Message};
/// use std::sync::Arc;
///
/// async fn example(provider: Arc<dyn LLMProvider>) {
///     let request = CompletionRequest::new("gpt-4")
///         .with_message(Message::user("Hello!"));
///     
///     let response = provider.complete(request).await.unwrap();
///     println!("{}", response.content);
/// }
/// ```
#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &str;

    /// Send a completion request and get a response
    ///
    /// This is the main method for interacting with the LLM.
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;

    /// Send a completion request and get a streaming response
    ///
    /// Returns a stream of chunks that can be processed as they arrive.
    /// Use this for real-time output in interactive applications.
    async fn complete_stream(&self, request: CompletionRequest) -> Result<StreamResponse>;

    /// List available models from this provider
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;

    /// Check if the provider is available and configured correctly
    async fn health_check(&self) -> Result<()>;

    /// Check if this provider supports tool/function calling
    fn supports_tools(&self) -> bool {
        true
    }

    /// Check if this provider supports vision (image inputs)
    fn supports_vision(&self) -> bool {
        false
    }

    /// Get the default model for this provider
    fn default_model(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Error, Message, TokenUsage};

    struct MockProvider;

    #[async_trait]
    impl LLMProvider for MockProvider {
        fn name(&self) -> &str {
            "mock"
        }

        async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
            Ok(CompletionResponse {
                content: "Hello from mock!".to_string(),
                tool_calls: vec![],
                finish_reason: Some(crate::FinishReason::Stop),
                usage: TokenUsage::new(10, 5),
                model: "mock-model".to_string(),
                id: Some("mock-123".to_string()),
            })
        }

        async fn complete_stream(&self, _request: CompletionRequest) -> Result<StreamResponse> {
            Err(Error::InvalidRequest("Streaming not supported".to_string()))
        }

        async fn list_models(&self) -> Result<Vec<ModelInfo>> {
            Ok(vec![ModelInfo {
                id: "mock-model".to_string(),
                name: "Mock Model".to_string(),
                description: Some("A mock model for testing".to_string()),
                context_window: Some(4096),
                max_output_tokens: Some(1024),
                supports_tools: true,
                supports_vision: false,
                provider: "mock".to_string(),
            }])
        }

        async fn health_check(&self) -> Result<()> {
            Ok(())
        }

        fn default_model(&self) -> &str {
            "mock-model"
        }
    }

    #[tokio::test]
    async fn test_mock_provider() {
        let provider = MockProvider;

        let request = CompletionRequest::new("mock-model").with_message(Message::user("Hello"));

        let response = provider.complete(request).await.unwrap();
        assert_eq!(response.content, "Hello from mock!");
    }

    #[tokio::test]
    async fn test_list_models() {
        let provider = MockProvider;
        let models = provider.list_models().await.unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "mock-model");
    }
}
