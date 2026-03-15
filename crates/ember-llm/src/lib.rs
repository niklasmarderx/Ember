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

pub mod router;

#[cfg(any(test, feature = "mock"))]
pub mod mock;

// Re-exports
pub use error::{Error, Result};
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

pub use retry::{complete_with_retry, RetryConfig};
pub use router::LLMRouter;

#[cfg(any(test, feature = "mock"))]
pub use mock::MockProvider;
