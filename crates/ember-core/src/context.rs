//! Context management for agent conversations.

use ember_llm::{Message, Role};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Represents the current context of an agent conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    /// System message (always first)
    system_message: Message,

    /// Message history
    messages: VecDeque<Message>,

    /// Current token count estimate
    token_count: usize,

    /// Maximum tokens allowed
    max_tokens: usize,
}

impl Context {
    /// Create a new context with a system prompt.
    pub fn new(system_prompt: impl Into<String>, max_tokens: usize) -> Self {
        let system_message = Message::system(system_prompt);
        let system_tokens = estimate_tokens(&system_message.content);

        Self {
            system_message,
            messages: VecDeque::new(),
            token_count: system_tokens,
            max_tokens,
        }
    }

    /// Add a user message to the context.
    pub fn add_user_message(&mut self, content: impl Into<String>) {
        self.add_message(Message::user(content));
    }

    /// Add an assistant message to the context.
    pub fn add_assistant_message(&mut self, content: impl Into<String>) {
        self.add_message(Message::assistant(content));
    }

    /// Add a message to the context.
    pub fn add_message(&mut self, message: Message) {
        let tokens = estimate_tokens(&message.content);
        self.messages.push_back(message);
        self.token_count += tokens;

        // Trim old messages if we exceed the limit
        self.trim_to_fit();
    }

    /// Get all messages including the system message.
    pub fn messages(&self) -> Vec<Message> {
        let mut all_messages = Vec::with_capacity(self.messages.len() + 1);
        all_messages.push(self.system_message.clone());
        all_messages.extend(self.messages.iter().cloned());
        all_messages
    }

    /// Get the number of messages (excluding system).
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Check if context is empty (no messages besides system).
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Get the current token count estimate.
    pub fn token_count(&self) -> usize {
        self.token_count
    }

    /// Get remaining token capacity.
    pub fn remaining_tokens(&self) -> usize {
        self.max_tokens.saturating_sub(self.token_count)
    }

    /// Check if there's room for more tokens.
    pub fn has_capacity(&self, tokens: usize) -> bool {
        self.token_count + tokens <= self.max_tokens
    }

    /// Clear all messages except the system message.
    pub fn clear(&mut self) {
        self.messages.clear();
        self.token_count = estimate_tokens(&self.system_message.content);
    }

    /// Get the last message.
    pub fn last_message(&self) -> Option<&Message> {
        self.messages.back()
    }

    /// Get the last assistant message.
    pub fn last_assistant_message(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, Role::Assistant))
    }

    /// Update the system prompt.
    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        let old_tokens = estimate_tokens(&self.system_message.content);
        self.system_message = Message::system(prompt);
        let new_tokens = estimate_tokens(&self.system_message.content);
        self.token_count = self.token_count - old_tokens + new_tokens;
    }

    /// Trim messages to fit within token limit.
    fn trim_to_fit(&mut self) {
        while self.token_count > self.max_tokens && !self.messages.is_empty() {
            if let Some(removed) = self.messages.pop_front() {
                self.token_count -= estimate_tokens(&removed.content);
            }
        }
    }
}

/// Context manager for handling multiple conversations.
#[derive(Debug, Default)]
pub struct ContextManager {
    /// Default system prompt
    default_system_prompt: String,

    /// Default max tokens
    default_max_tokens: usize,
}

impl ContextManager {
    /// Create a new context manager.
    pub fn new(default_system_prompt: impl Into<String>, default_max_tokens: usize) -> Self {
        Self {
            default_system_prompt: default_system_prompt.into(),
            default_max_tokens,
        }
    }

    /// Create a new context with default settings.
    pub fn create_context(&self) -> Context {
        Context::new(&self.default_system_prompt, self.default_max_tokens)
    }

    /// Create a context with a custom system prompt.
    pub fn create_context_with_prompt(&self, system_prompt: impl Into<String>) -> Context {
        Context::new(system_prompt, self.default_max_tokens)
    }

    /// Create a context with custom settings.
    pub fn create_context_custom(
        &self,
        system_prompt: impl Into<String>,
        max_tokens: usize,
    ) -> Context {
        Context::new(system_prompt, max_tokens)
    }
}

/// Estimate token count for a string.
/// This is a simple approximation (4 chars ≈ 1 token).
/// For production, use a proper tokenizer like tiktoken.
fn estimate_tokens(text: &str) -> usize {
    // Simple heuristic: ~4 characters per token for English text
    // This is a rough approximation; actual tokenization varies by model
    text.len().div_ceil(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_creation() {
        let ctx = Context::new("You are helpful.", 1000);
        assert!(ctx.is_empty());
        assert!(ctx.token_count() > 0); // System message has tokens
    }

    #[test]
    fn test_add_messages() {
        let mut ctx = Context::new("System", 1000);
        ctx.add_user_message("Hello");
        ctx.add_assistant_message("Hi there!");

        assert_eq!(ctx.len(), 2);

        let messages = ctx.messages();
        assert_eq!(messages.len(), 3); // Including system
        assert!(matches!(messages[0].role, Role::System));
        assert!(matches!(messages[1].role, Role::User));
        assert!(matches!(messages[2].role, Role::Assistant));
    }

    #[test]
    fn test_token_trimming() {
        let mut ctx = Context::new("System", 100);

        // Add messages until we exceed limit
        for i in 0..50 {
            ctx.add_user_message(format!("Message {i} with some extra text"));
        }

        // Should have trimmed old messages
        assert!(ctx.token_count() <= 100);
    }

    #[test]
    fn test_context_clear() {
        let mut ctx = Context::new("System", 1000);
        ctx.add_user_message("Hello");
        ctx.add_assistant_message("Hi!");

        let initial_tokens = estimate_tokens("System");
        ctx.clear();

        assert!(ctx.is_empty());
        assert_eq!(ctx.token_count(), initial_tokens);
    }

    #[test]
    fn test_context_manager() {
        let manager = ContextManager::new("Default prompt", 8192);

        let ctx1 = manager.create_context();
        let ctx2 = manager.create_context_with_prompt("Custom prompt");

        assert!(ctx1.messages()[0].content.contains("Default"));
        assert!(ctx2.messages()[0].content.contains("Custom"));
    }
}
