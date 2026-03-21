//! # Context Window Manager
//!
//! Intelligent context window management for optimal LLM interactions.
//!
//! Features:
//! - Token counting and tracking
//! - Context pruning strategies
//! - Priority-based message retention
//! - Sliding window management
//! - Summary generation for long contexts

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tracing::{debug, warn};

/// Context window manager.
#[derive(Debug, Clone)]
pub struct ContextManager {
    /// Configuration
    config: ContextConfig,
    /// Current messages
    messages: VecDeque<ContextMessage>,
    /// System prompt (always retained)
    system_prompt: Option<String>,
    /// Token counter
    token_count: TokenCount,
    /// Pruning statistics
    stats: ContextStats,
}

/// Configuration for context management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Maximum tokens for context window
    pub max_tokens: usize,
    /// Reserved tokens for response
    pub reserved_response_tokens: usize,
    /// Reserved tokens for system prompt
    pub reserved_system_tokens: usize,
    /// Pruning strategy
    pub pruning_strategy: PruningStrategy,
    /// Enable auto-summarization
    pub enable_summarization: bool,
    /// Messages to preserve (most recent)
    pub preserve_recent_count: usize,
    /// Priority weights
    pub priority_weights: PriorityWeights,
    /// Token estimation method
    pub token_estimation: TokenEstimation,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens: 128_000,
            reserved_response_tokens: 4096,
            reserved_system_tokens: 2000,
            pruning_strategy: PruningStrategy::SlidingWindow,
            enable_summarization: true,
            preserve_recent_count: 4,
            priority_weights: PriorityWeights::default(),
            token_estimation: TokenEstimation::Approximate,
        }
    }
}

/// Pruning strategy for context management.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PruningStrategy {
    /// Simple sliding window - remove oldest messages
    SlidingWindow,
    /// Priority-based pruning
    PriorityBased,
    /// Summarize old messages
    Summarize,
    /// Hybrid approach
    Hybrid,
}

/// Token estimation method.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenEstimation {
    /// Approximate (4 chars = 1 token)
    Approximate,
    /// Tiktoken-based (more accurate)
    Tiktoken,
    /// Custom ratio
    Custom { chars_per_token: f64 },
}

/// Priority weights for message retention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityWeights {
    /// Weight for user messages
    pub user_message: f64,
    /// Weight for assistant messages
    pub assistant_message: f64,
    /// Weight for tool calls
    pub tool_call: f64,
    /// Weight for tool results
    pub tool_result: f64,
    /// Weight for system messages
    pub system_message: f64,
    /// Recency weight multiplier
    pub recency_multiplier: f64,
}

impl Default for PriorityWeights {
    fn default() -> Self {
        Self {
            user_message: 1.0,
            assistant_message: 0.8,
            tool_call: 0.6,
            tool_result: 0.5,
            system_message: 1.5,
            recency_multiplier: 0.1,
        }
    }
}

/// Message in the context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    /// Message role
    pub role: MessageRole,
    /// Message content
    pub content: String,
    /// Token count
    pub tokens: usize,
    /// Message index (for ordering)
    pub index: usize,
    /// Priority score
    pub priority: f64,
    /// Is this message pinned (never pruned)
    pub pinned: bool,
    /// Timestamp
    pub timestamp: u64,
    /// Associated metadata
    pub metadata: Option<MessageMetadata>,
}

/// Message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// System message
    System,
    /// User message
    User,
    /// Assistant message
    Assistant,
    /// Tool call
    Tool,
}

/// Additional message metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// Tool name (if tool message)
    pub tool_name: Option<String>,
    /// Is this a summary of previous messages
    pub is_summary: bool,
    /// Original message count (if summary)
    pub summarized_count: Option<usize>,
    /// Tags for filtering
    pub tags: Vec<String>,
}

impl Default for MessageMetadata {
    fn default() -> Self {
        Self {
            tool_name: None,
            is_summary: false,
            summarized_count: None,
            tags: vec![],
        }
    }
}

/// Token counting state.
#[derive(Debug, Clone, Default)]
pub struct TokenCount {
    /// Total tokens used
    pub total: usize,
    /// Tokens in system prompt
    pub system: usize,
    /// Tokens in user messages
    pub user: usize,
    /// Tokens in assistant messages
    pub assistant: usize,
    /// Tokens in tool messages
    pub tool: usize,
}

impl TokenCount {
    /// Get available tokens for messages.
    pub fn available(&self, config: &ContextConfig) -> usize {
        let max_message_tokens = config
            .max_tokens
            .saturating_sub(config.reserved_response_tokens)
            .saturating_sub(config.reserved_system_tokens);
        max_message_tokens.saturating_sub(self.total)
    }
}

/// Context management statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextStats {
    /// Total messages added
    pub messages_added: usize,
    /// Messages pruned
    pub messages_pruned: usize,
    /// Summaries generated
    pub summaries_generated: usize,
    /// Total tokens processed
    pub tokens_processed: usize,
    /// Tokens saved by pruning
    pub tokens_saved: usize,
}

impl ContextManager {
    /// Create a new context manager with default configuration.
    pub fn new() -> Self {
        Self::with_config(ContextConfig::default())
    }

    /// Create with custom configuration.
    pub fn with_config(config: ContextConfig) -> Self {
        Self {
            config,
            messages: VecDeque::new(),
            system_prompt: None,
            token_count: TokenCount::default(),
            stats: ContextStats::default(),
        }
    }

    /// Set system prompt.
    pub fn set_system_prompt(&mut self, prompt: &str) {
        let tokens = self.estimate_tokens(prompt);
        self.token_count.system = tokens;
        self.system_prompt = Some(prompt.to_string());
        debug!("System prompt set: {} tokens", tokens);
    }

    /// Add a message to the context.
    pub fn add_message(&mut self, role: MessageRole, content: &str) -> &ContextMessage {
        let tokens = self.estimate_tokens(content);
        let index = self.stats.messages_added;

        let message = ContextMessage {
            role,
            content: content.to_string(),
            tokens,
            index,
            priority: self.calculate_priority(role, index),
            pinned: false,
            timestamp: current_timestamp(),
            metadata: None,
        };

        self.add_message_internal(message);
        self.stats.messages_added += 1;
        self.stats.tokens_processed += tokens;

        // Check if pruning is needed
        self.prune_if_needed();

        self.messages.back().unwrap()
    }

    /// Add message with metadata.
    pub fn add_message_with_metadata(
        &mut self,
        role: MessageRole,
        content: &str,
        metadata: MessageMetadata,
    ) -> &ContextMessage {
        let tokens = self.estimate_tokens(content);
        let index = self.stats.messages_added;

        let message = ContextMessage {
            role,
            content: content.to_string(),
            tokens,
            index,
            priority: self.calculate_priority(role, index),
            pinned: false,
            timestamp: current_timestamp(),
            metadata: Some(metadata),
        };

        self.add_message_internal(message);
        self.stats.messages_added += 1;
        self.stats.tokens_processed += tokens;

        self.prune_if_needed();

        self.messages.back().unwrap()
    }

    fn add_message_internal(&mut self, message: ContextMessage) {
        match message.role {
            MessageRole::System => self.token_count.system += message.tokens,
            MessageRole::User => self.token_count.user += message.tokens,
            MessageRole::Assistant => self.token_count.assistant += message.tokens,
            MessageRole::Tool => self.token_count.tool += message.tokens,
        }
        self.token_count.total += message.tokens;
        self.messages.push_back(message);
    }

    /// Pin a message to prevent pruning.
    pub fn pin_message(&mut self, index: usize) {
        if let Some(msg) = self.messages.iter_mut().find(|m| m.index == index) {
            msg.pinned = true;
        }
    }

    /// Get all messages for API call.
    pub fn get_messages(&self) -> Vec<&ContextMessage> {
        self.messages.iter().collect()
    }

    /// Get formatted messages for LLM.
    pub fn get_formatted_messages(&self) -> Vec<FormattedMessage> {
        let mut result = Vec::new();

        // Add system prompt first
        if let Some(ref prompt) = self.system_prompt {
            result.push(FormattedMessage {
                role: "system".to_string(),
                content: prompt.clone(),
            });
        }

        // Add context messages
        for msg in &self.messages {
            let role = match msg.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "tool",
            };
            result.push(FormattedMessage {
                role: role.to_string(),
                content: msg.content.clone(),
            });
        }

        result
    }

    /// Get current token usage.
    pub fn get_token_usage(&self) -> &TokenCount {
        &self.token_count
    }

    /// Get available tokens.
    pub fn available_tokens(&self) -> usize {
        self.token_count.available(&self.config)
    }

    /// Get statistics.
    pub fn get_stats(&self) -> &ContextStats {
        &self.stats
    }

    /// Check if context needs pruning.
    pub fn needs_pruning(&self) -> bool {
        let max_message_tokens = self
            .config
            .max_tokens
            .saturating_sub(self.config.reserved_response_tokens)
            .saturating_sub(self.config.reserved_system_tokens);
        self.token_count.total > max_message_tokens
    }

    /// Prune context if needed.
    fn prune_if_needed(&mut self) {
        while self.needs_pruning() {
            match self.config.pruning_strategy {
                PruningStrategy::SlidingWindow => self.prune_sliding_window(),
                PruningStrategy::PriorityBased => self.prune_priority_based(),
                PruningStrategy::Summarize => self.prune_with_summary(),
                PruningStrategy::Hybrid => self.prune_hybrid(),
            }
        }
    }

    /// Sliding window pruning - remove oldest messages.
    fn prune_sliding_window(&mut self) {
        // Skip recent messages that should be preserved
        let skip_count = self.config.preserve_recent_count;

        if self.messages.len() <= skip_count {
            warn!(
                "Cannot prune: only {} messages, need to preserve {}",
                self.messages.len(),
                skip_count
            );
            return;
        }

        // Find first non-pinned message
        let mut removed = false;
        for i in 0..self.messages.len().saturating_sub(skip_count) {
            if let Some(msg) = self.messages.get(i) {
                if !msg.pinned {
                    let msg = self.messages.remove(i).unwrap();
                    self.update_token_count_on_remove(&msg);
                    self.stats.messages_pruned += 1;
                    self.stats.tokens_saved += msg.tokens;
                    debug!("Pruned message {} ({} tokens)", msg.index, msg.tokens);
                    removed = true;
                    break;
                }
            }
        }

        if !removed {
            warn!("No prunable messages found");
        }
    }

    /// Priority-based pruning - remove lowest priority messages.
    fn prune_priority_based(&mut self) {
        let skip_count = self.config.preserve_recent_count;

        if self.messages.len() <= skip_count {
            return;
        }

        // Update priorities
        self.update_priorities();

        // Find lowest priority non-pinned message
        let mut lowest_idx = None;
        let mut lowest_priority = f64::MAX;

        for (i, msg) in self.messages.iter().enumerate() {
            if i >= self.messages.len().saturating_sub(skip_count) {
                break; // Preserve recent messages
            }
            if !msg.pinned && msg.priority < lowest_priority {
                lowest_priority = msg.priority;
                lowest_idx = Some(i);
            }
        }

        if let Some(idx) = lowest_idx {
            let msg = self.messages.remove(idx).unwrap();
            self.update_token_count_on_remove(&msg);
            self.stats.messages_pruned += 1;
            self.stats.tokens_saved += msg.tokens;
            debug!(
                "Pruned message {} (priority: {:.2}, {} tokens)",
                msg.index, msg.priority, msg.tokens
            );
        }
    }

    /// Prune with summarization.
    fn prune_with_summary(&mut self) {
        if !self.config.enable_summarization {
            self.prune_sliding_window();
            return;
        }

        let skip_count = self.config.preserve_recent_count;
        let messages_to_summarize = self.messages.len().saturating_sub(skip_count);

        if messages_to_summarize < 4 {
            self.prune_sliding_window();
            return;
        }

        // Collect messages to summarize
        let to_summarize: Vec<_> = self.messages.drain(0..messages_to_summarize / 2).collect();

        let total_tokens: usize = to_summarize.iter().map(|m| m.tokens).sum();
        let message_count = to_summarize.len();

        // Create summary (in real implementation, this would call the LLM)
        let summary = self.create_summary_placeholder(&to_summarize);
        let summary_tokens = self.estimate_tokens(&summary);

        // Update stats
        for msg in &to_summarize {
            self.update_token_count_on_remove(msg);
        }

        // Add summary as new message
        let summary_msg = ContextMessage {
            role: MessageRole::System,
            content: summary,
            tokens: summary_tokens,
            index: self.stats.messages_added,
            priority: 0.9, // High priority for summaries
            pinned: true,
            timestamp: current_timestamp(),
            metadata: Some(MessageMetadata {
                is_summary: true,
                summarized_count: Some(message_count),
                ..Default::default()
            }),
        };

        self.messages.push_front(summary_msg);
        self.token_count.total += summary_tokens;
        self.token_count.system += summary_tokens;

        self.stats.summaries_generated += 1;
        self.stats.messages_pruned += message_count;
        self.stats.tokens_saved += total_tokens.saturating_sub(summary_tokens);

        debug!(
            "Summarized {} messages ({} -> {} tokens)",
            message_count, total_tokens, summary_tokens
        );
    }

    /// Hybrid pruning approach.
    fn prune_hybrid(&mut self) {
        // First try summarization if enabled
        if self.config.enable_summarization && self.messages.len() > 10 {
            self.prune_with_summary();
        } else {
            // Fall back to priority-based
            self.prune_priority_based();
        }
    }

    /// Create a placeholder summary.
    fn create_summary_placeholder(&self, messages: &[ContextMessage]) -> String {
        let user_count = messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .count();
        let assistant_count = messages
            .iter()
            .filter(|m| m.role == MessageRole::Assistant)
            .count();
        let tool_count = messages
            .iter()
            .filter(|m| m.role == MessageRole::Tool)
            .count();

        format!(
            "[Summary of previous conversation: {} user messages, {} assistant responses, {} tool interactions]",
            user_count, assistant_count, tool_count
        )
    }

    /// Update token count when removing a message.
    fn update_token_count_on_remove(&mut self, msg: &ContextMessage) {
        self.token_count.total = self.token_count.total.saturating_sub(msg.tokens);
        match msg.role {
            MessageRole::System => {
                self.token_count.system = self.token_count.system.saturating_sub(msg.tokens);
            }
            MessageRole::User => {
                self.token_count.user = self.token_count.user.saturating_sub(msg.tokens);
            }
            MessageRole::Assistant => {
                self.token_count.assistant = self.token_count.assistant.saturating_sub(msg.tokens);
            }
            MessageRole::Tool => {
                self.token_count.tool = self.token_count.tool.saturating_sub(msg.tokens);
            }
        }
    }

    /// Update priorities for all messages.
    fn update_priorities(&mut self) {
        let current_index = self.stats.messages_added;
        let recency_multiplier = self.config.priority_weights.recency_multiplier;
        let system_weight = self.config.priority_weights.system_message;
        let user_weight = self.config.priority_weights.user_message;
        let assistant_weight = self.config.priority_weights.assistant_message;
        let tool_weight = self.config.priority_weights.tool_call;

        for msg in &mut self.messages {
            let recency = (current_index - msg.index) as f64;
            let recency_factor = 1.0 / (1.0 + recency * recency_multiplier);
            let base_priority = match msg.role {
                MessageRole::System => system_weight,
                MessageRole::User => user_weight,
                MessageRole::Assistant => assistant_weight,
                MessageRole::Tool => tool_weight,
            };
            msg.priority = base_priority * recency_factor;
        }
    }

    /// Calculate priority for a message.
    fn calculate_priority(&self, role: MessageRole, index: usize) -> f64 {
        let base_priority = match role {
            MessageRole::System => self.config.priority_weights.system_message,
            MessageRole::User => self.config.priority_weights.user_message,
            MessageRole::Assistant => self.config.priority_weights.assistant_message,
            MessageRole::Tool => self.config.priority_weights.tool_call,
        };

        let recency = (self.stats.messages_added.saturating_sub(index)) as f64;
        let recency_factor =
            1.0 / (1.0 + recency * self.config.priority_weights.recency_multiplier);

        base_priority * recency_factor
    }

    /// Estimate tokens for content.
    fn estimate_tokens(&self, content: &str) -> usize {
        match self.config.token_estimation {
            TokenEstimation::Approximate => {
                // Rough estimate: 4 characters per token
                (content.len() as f64 / 4.0).ceil() as usize
            }
            TokenEstimation::Tiktoken => {
                // Would use tiktoken library
                (content.len() as f64 / 4.0).ceil() as usize
            }
            TokenEstimation::Custom { chars_per_token } => {
                (content.len() as f64 / chars_per_token).ceil() as usize
            }
        }
    }

    /// Clear all messages (keep system prompt).
    pub fn clear(&mut self) {
        self.messages.clear();
        self.token_count = TokenCount {
            system: self.token_count.system,
            ..Default::default()
        };
    }

    /// Get message count.
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Formatted message for API calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedMessage {
    /// Role
    pub role: String,
    /// Content
    pub content: String,
}

/// Get current timestamp.
fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Builder for context manager.
#[derive(Debug, Default)]
pub struct ContextManagerBuilder {
    config: ContextConfig,
    system_prompt: Option<String>,
}

impl ContextManagerBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum tokens.
    pub fn max_tokens(mut self, tokens: usize) -> Self {
        self.config.max_tokens = tokens;
        self
    }

    /// Set reserved response tokens.
    pub fn reserved_response_tokens(mut self, tokens: usize) -> Self {
        self.config.reserved_response_tokens = tokens;
        self
    }

    /// Set pruning strategy.
    pub fn pruning_strategy(mut self, strategy: PruningStrategy) -> Self {
        self.config.pruning_strategy = strategy;
        self
    }

    /// Enable or disable summarization.
    pub fn enable_summarization(mut self, enable: bool) -> Self {
        self.config.enable_summarization = enable;
        self
    }

    /// Set system prompt.
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Build the context manager.
    pub fn build(self) -> ContextManager {
        let mut manager = ContextManager::with_config(self.config);
        if let Some(prompt) = self.system_prompt {
            manager.set_system_prompt(&prompt);
        }
        manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_messages() {
        let mut manager = ContextManager::new();
        manager.set_system_prompt("You are a helpful assistant.");

        manager.add_message(MessageRole::User, "Hello");
        manager.add_message(MessageRole::Assistant, "Hi there!");

        assert_eq!(manager.message_count(), 2);
        assert!(manager.get_token_usage().total > 0);
    }

    #[test]
    fn test_token_estimation() {
        let manager = ContextManager::new();

        // ~4 chars per token
        let tokens = manager.estimate_tokens("Hello, how are you?"); // 19 chars
        assert!(tokens >= 4 && tokens <= 6);
    }

    #[test]
    fn test_pruning() {
        let config = ContextConfig {
            max_tokens: 100,
            reserved_response_tokens: 20,
            reserved_system_tokens: 20,
            preserve_recent_count: 2,
            ..Default::default()
        };

        let mut manager = ContextManager::with_config(config);

        // Add messages until pruning is triggered
        for i in 0..20 {
            manager.add_message(MessageRole::User, &format!("Message {}", i));
        }

        // Should have pruned some messages
        assert!(manager.stats.messages_pruned > 0);
        assert!(manager.message_count() < 20);
    }

    #[test]
    fn test_pinned_messages() {
        let config = ContextConfig {
            max_tokens: 50,
            reserved_response_tokens: 10,
            reserved_system_tokens: 10,
            preserve_recent_count: 1,
            ..Default::default()
        };

        let mut manager = ContextManager::with_config(config);

        manager.add_message(MessageRole::User, "Important message");
        manager.pin_message(0); // Pin first message

        // Add more messages to trigger pruning
        for i in 0..10 {
            manager.add_message(MessageRole::User, &format!("Message {}", i));
        }

        // Pinned message should still exist
        let messages = manager.get_messages();
        assert!(messages.iter().any(|m| m.pinned && m.index == 0));
    }

    #[test]
    fn test_builder() {
        let manager = ContextManagerBuilder::new()
            .max_tokens(50000)
            .pruning_strategy(PruningStrategy::PriorityBased)
            .system_prompt("Test prompt")
            .build();

        assert_eq!(manager.config.max_tokens, 50000);
        assert_eq!(
            manager.config.pruning_strategy,
            PruningStrategy::PriorityBased
        );
        assert!(manager.system_prompt.is_some());
    }

    #[test]
    fn test_formatted_messages() {
        let mut manager = ContextManager::new();
        manager.set_system_prompt("System");
        manager.add_message(MessageRole::User, "User message");
        manager.add_message(MessageRole::Assistant, "Assistant message");

        let formatted = manager.get_formatted_messages();

        assert_eq!(formatted.len(), 3);
        assert_eq!(formatted[0].role, "system");
        assert_eq!(formatted[1].role, "user");
        assert_eq!(formatted[2].role, "assistant");
    }
}
