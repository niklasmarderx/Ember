//! # ember-llm
//!
//! LLM provider abstraction layer for Ember.
//!
//! This crate provides a unified interface for interacting with various LLM providers
//! including OpenAI, Anthropic, Ollama, and local models.
//!
//! ## Features
//!
//! - `openai` - OpenAI API support (enabled by default)
//! - `anthropic` - Anthropic Claude API support
//! - `ollama` - Ollama local model support (enabled by default)
//! - `groq` - Groq API support
//! - `local` - Local model support via llama.cpp
//!
//! ## Example
//!
//! ```rust,no_run
//! use ember_llm::{LLMProvider, OpenAIProvider, CompletionRequest, Message, Role};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let provider = OpenAIProvider::from_env()?;
//!     
//!     let request = CompletionRequest::new("gpt-4o")
//!         .with_message(Message::user("Hello, world!"));
//!     
//!     let response = provider.complete(request).await?;
//!     println!("{}", response.content);
//!     
//!     Ok(())
//! }
//! ```

#![deny(missing_docs)]
#![deny(unsafe_code)]

mod error;
mod provider;
pub mod retry;
mod types;

#[cfg(feature = "openai")]
pub mod openai;

#[cfg(feature = "anthropic")]
pub mod anthropic;

#[cfg(feature = "ollama")]
pub mod ollama;

#[cfg(feature = "groq")]
pub mod groq;

#[cfg(feature = "gemini")]
pub mod gemini;

#[cfg(feature = "mistral")]
pub mod mistral;

#[cfg(feature = "deepseek")]
pub mod deepseek;

#[cfg(feature = "openrouter")]
pub mod openrouter;

#[cfg(feature = "xai")]
pub mod xai;

#[cfg(feature = "bedrock")]
pub mod bedrock;

pub mod analyzer;
pub mod connection_pool;
pub mod function_calling;
pub mod model_registry;
pub mod router;
pub mod scorer;
pub mod streaming;
pub mod vision;

#[cfg(any(test, feature = "mock"))]
pub mod mock;

// Re-exports
pub use error::{Error, ErrorCode, Result};
pub use provider::LLMProvider;
pub use types::*;

#[cfg(feature = "openai")]
pub use openai::OpenAIProvider;

#[cfg(feature = "anthropic")]
pub use anthropic::AnthropicProvider;

#[cfg(feature = "ollama")]
pub use ollama::OllamaProvider;

#[cfg(feature = "groq")]
pub use groq::GroqProvider;

#[cfg(feature = "gemini")]
pub use gemini::GeminiProvider;

#[cfg(feature = "mistral")]
pub use mistral::MistralProvider;

#[cfg(feature = "deepseek")]
pub use deepseek::DeepSeekProvider;

#[cfg(feature = "openrouter")]
pub use openrouter::OpenRouterProvider;

#[cfg(feature = "xai")]
pub use xai::XAIProvider;

#[cfg(feature = "bedrock")]
pub use bedrock::{BedrockConfig, BedrockModelFamily, BedrockProvider};

pub use analyzer::{TaskAnalysis, TaskAnalyzer, TaskComplexity, TaskType};
pub use model_registry::{
    CostEstimate, ModelCapabilities, ModelMetadata, ModelRegistry, MODEL_REGISTRY,
};
pub use retry::{complete_with_retry, RetryConfig};
pub use router::LLMRouter;
pub use scorer::{
    ModelCapabilities as ScorerModelCapabilities, ModelScore, ModelScorer, UserPreferences,
};

// Vision and Function Calling
pub use function_calling::{
    FunctionBuilder, FunctionCall, FunctionCallingCapable, FunctionCallingModels,
    FunctionDefinition, FunctionResult, JsonSchema, PropertySchema, PropertyType, ToolChoice,
};
pub use vision::{
    ContentPart, ImageDetail, ImageInput, ImageSource, MediaType, MultimodalContent,
    VisionCapable, VisionModels,
};

#[cfg(any(test, feature = "mock"))]
pub use mock::MockProvider;
