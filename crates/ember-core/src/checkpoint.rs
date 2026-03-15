//! Checkpoint system for undo/restore functionality.
//!
//! This module provides checkpointing capabilities that allow saving and
//! restoring agent state, enabling undo operations and recovery from errors.

use crate::{context::Context, conversation::Conversation, Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use uuid::Uuid;

/// Unique identifier for a checkpoint
pub type CheckpointId = Uuid;

/// A snapshot of the agent's state at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique identifier
    pub id: CheckpointId,
    /// Human-readable name (optional)
    pub name: Option<String>,
    /// Description of what was done
    pub description: Option<String>,
    /// Timestamp when checkpoint was created
    pub timestamp: DateTime<Utc>,
    /// Snapshot of the conversation
    pub conversation: Option<Conversation>,
    /// Snapshot of the context
    pub context_messages: Vec<ember_llm::Message>,
    /// Token count at checkpoint
    pub token_count: u32,
    /// Whether this is an auto-checkpoint
    pub auto_created: bool,
    /// Tags for categorization
    pub tags: Vec<String>,
}

impl Checkpoint {
    /// Create a new checkpoint
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: None,
            description: None,
            timestamp: Utc::now(),
            conversation: None,
            context_messages: Vec::new(),
            token_count: 0,
            auto_created: false,
            tags: Vec::new(),
        }
    }

    /// Create a checkpoint with a name
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            ..Self::new()
        }
    }

    /// Set the description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Mark as auto-created
    pub fn auto(mut self) -> Self {
        self.auto_created = true;
        self
    }

    /// Add a tag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set context messages
    pub fn with_context(mut self, messages: Vec<ember_llm::Message>) -> Self {
        self.context_messages = messages;
        self
    }

    /// Set conversation
    pub fn with_conversation(mut self, conversation: Conversation) -> Self {
        self.conversation = Some(conversation);
        self
    }

    /// Set token count
    pub fn with_token_count(mut self, count: u32) -> Self {
        self.token_count = count;
        self
    }

    /// Get a display name
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("Checkpoint {}", self.timestamp.format("%Y-%m-%d %H:%M:%S")))
    }

    /// Get age in human-readable format
    pub fn age(&self) -> String {
        let now = Utc::now();
        let duration = now.signed_duration_since(self.timestamp);

        if duration.num_seconds() < 60 {
            format!("{} seconds ago", duration.num_seconds())
        } else if duration.num_minutes() < 60 {
            format!("{} minutes ago", duration.num_minutes())
        } else if duration.num_hours() < 24 {
            format!("{} hours ago", duration.num_hours())
        } else {
            format!("{} days ago", duration.num_days())
        }
    }
}

impl Default for Checkpoint {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for the checkpoint manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    /// Maximum number of checkpoints to keep
    pub max_checkpoints: usize,
    /// Auto-checkpoint after every N turns
    pub auto_checkpoint_interval: Option<usize>,
    /// Auto-checkpoint before tool execution
    pub checkpoint_before_tools: bool,
    /// Preserve checkpoints with specific tags
    pub preserve_tags: Vec<String>,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            max_checkpoints: 50,
            auto_checkpoint_interval: Some(5),
            checkpoint_before_tools: true,
            preserve_tags: vec!["important".to_string(), "manual".to_string()],
        }
    }
}

/// Manages checkpoints for an agent.
pub struct CheckpointManager {
    /// Configuration
    config: CheckpointConfig,
    /// Stored checkpoints (oldest first)
    checkpoints: VecDeque<Checkpoint>,
    /// Turn counter for auto-checkpointing
    turn_counter: usize,
}

impl CheckpointManager {
    /// Create a new checkpoint manager with default config.
    pub fn new() -> Self {
        Self::with_config(CheckpointConfig::default())
    }

    /// Create a new checkpoint manager with custom config.
    pub fn with_config(config: CheckpointConfig) -> Self {
        Self {
            config,
            checkpoints: VecDeque::new(),
            turn_counter: 0,
        }
    }

    /// Get the configuration.
    pub fn config(&self) -> &CheckpointConfig {
        &self.config
    }

    /// Create a checkpoint from current state.
    pub fn create_checkpoint(
        &mut self,
        context: &Context,
        conversation: Option<&Conversation>,
    ) -> CheckpointId {
        let checkpoint = Checkpoint::new()
            .with_context(context.messages())
            .with_token_count(context.token_count() as u32);

        let checkpoint = if let Some(conv) = conversation {
            checkpoint.with_conversation(conv.clone())
        } else {
            checkpoint
        };

        let id = checkpoint.id;
        self.add_checkpoint(checkpoint);
        id
    }

    /// Create a named checkpoint.
    pub fn create_named_checkpoint(
        &mut self,
        name: impl Into<String>,
        context: &Context,
        conversation: Option<&Conversation>,
    ) -> CheckpointId {
        let mut checkpoint = Checkpoint::with_name(name)
            .with_context(context.messages())
            .with_token_count(context.token_count() as u32)
            .tag("manual");

        if let Some(conv) = conversation {
            checkpoint = checkpoint.with_conversation(conv.clone());
        }

        let id = checkpoint.id;
        self.add_checkpoint(checkpoint);
        id
    }

    /// Add a checkpoint to storage.
    fn add_checkpoint(&mut self, checkpoint: Checkpoint) {
        self.checkpoints.push_back(checkpoint);
        self.enforce_limit();
    }

    /// Enforce the maximum checkpoint limit.
    fn enforce_limit(&mut self) {
        while self.checkpoints.len() > self.config.max_checkpoints {
            // Try to remove oldest non-preserved checkpoint
            let remove_idx = self.checkpoints.iter().position(|cp| {
                !cp.tags
                    .iter()
                    .any(|t| self.config.preserve_tags.contains(t))
            });

            if let Some(idx) = remove_idx {
                self.checkpoints.remove(idx);
            } else {
                // All checkpoints are preserved, remove oldest anyway
                self.checkpoints.pop_front();
            }
        }
    }

    /// Get a checkpoint by ID.
    pub fn get(&self, id: CheckpointId) -> Option<&Checkpoint> {
        self.checkpoints.iter().find(|cp| cp.id == id)
    }

    /// Get the most recent checkpoint.
    pub fn latest(&self) -> Option<&Checkpoint> {
        self.checkpoints.back()
    }

    /// Get checkpoint at index from most recent (0 = most recent).
    pub fn get_from_latest(&self, index: usize) -> Option<&Checkpoint> {
        let len = self.checkpoints.len();
        if index >= len {
            return None;
        }
        self.checkpoints.get(len - 1 - index)
    }

    /// List all checkpoints (most recent first).
    pub fn list(&self) -> Vec<&Checkpoint> {
        self.checkpoints.iter().rev().collect()
    }

    /// Get number of checkpoints.
    pub fn len(&self) -> usize {
        self.checkpoints.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.checkpoints.is_empty()
    }

    /// Delete a checkpoint by ID.
    pub fn delete(&mut self, id: CheckpointId) -> bool {
        if let Some(idx) = self.checkpoints.iter().position(|cp| cp.id == id) {
            self.checkpoints.remove(idx);
            true
        } else {
            false
        }
    }

    /// Delete all checkpoints.
    pub fn clear(&mut self) {
        self.checkpoints.clear();
    }

    /// Restore state from a checkpoint.
    ///
    /// Returns the context messages and conversation to restore.
    pub fn restore(
        &self,
        id: CheckpointId,
    ) -> Result<(Vec<ember_llm::Message>, Option<Conversation>)> {
        let checkpoint = self
            .get(id)
            .ok_or_else(|| Error::config(format!("Checkpoint not found: {}", id)))?;

        Ok((
            checkpoint.context_messages.clone(),
            checkpoint.conversation.clone(),
        ))
    }

    /// Increment turn counter and check if auto-checkpoint is needed.
    pub fn tick_turn(&mut self) -> bool {
        self.turn_counter += 1;

        if let Some(interval) = self.config.auto_checkpoint_interval {
            if self.turn_counter >= interval {
                self.turn_counter = 0;
                return true;
            }
        }

        false
    }

    /// Check if we should checkpoint before tool execution.
    pub fn should_checkpoint_before_tool(&self) -> bool {
        self.config.checkpoint_before_tools
    }

    /// Find checkpoints by tag.
    pub fn find_by_tag(&self, tag: &str) -> Vec<&Checkpoint> {
        self.checkpoints
            .iter()
            .filter(|cp| cp.tags.contains(&tag.to_string()))
            .collect()
    }

    /// Get checkpoint summary for display.
    pub fn summary(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!("Checkpoints: {}\n", self.checkpoints.len()));
        output.push_str(&format!("Max: {}\n\n", self.config.max_checkpoints));

        for (i, cp) in self.checkpoints.iter().rev().enumerate().take(5) {
            let name = cp.display_name();
            let age = cp.age();
            let tokens = cp.token_count;
            let auto = if cp.auto_created { " (auto)" } else { "" };

            output.push_str(&format!(
                "{}. {} - {} ({} tokens){}\n",
                i + 1,
                name,
                age,
                tokens,
                auto
            ));
        }

        if self.checkpoints.len() > 5 {
            output.push_str(&format!("... and {} more\n", self.checkpoints.len() - 5));
        }

        output
    }
}

impl Default for CheckpointManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;

    fn create_test_context() -> Context {
        let mut context = Context::new("Test system prompt", 4096);
        context.add_user_message("Hello");
        context.add_assistant_message("Hi there!");
        context
    }

    #[test]
    fn test_checkpoint_creation() {
        let checkpoint = Checkpoint::new();
        assert!(checkpoint.name.is_none());
        assert!(!checkpoint.auto_created);
    }

    #[test]
    fn test_named_checkpoint() {
        let checkpoint = Checkpoint::with_name("Before refactor")
            .description("Saving state before major changes")
            .tag("important");

        assert_eq!(checkpoint.name, Some("Before refactor".to_string()));
        assert!(checkpoint.tags.contains(&"important".to_string()));
    }

    #[test]
    fn test_checkpoint_manager() {
        let mut manager = CheckpointManager::new();
        let context = create_test_context();

        let id = manager.create_checkpoint(&context, None);

        assert_eq!(manager.len(), 1);
        assert!(manager.get(id).is_some());
    }

    #[test]
    fn test_checkpoint_limit() {
        let config = CheckpointConfig {
            max_checkpoints: 3,
            ..Default::default()
        };
        let mut manager = CheckpointManager::with_config(config);
        let context = create_test_context();

        // Create 5 checkpoints
        for _ in 0..5 {
            manager.create_checkpoint(&context, None);
        }

        assert_eq!(manager.len(), 3);
    }

    #[test]
    fn test_checkpoint_restore() {
        let mut manager = CheckpointManager::new();
        let context = create_test_context();

        let id = manager.create_checkpoint(&context, None);

        let (messages, _) = manager.restore(id).unwrap();
        assert!(!messages.is_empty());
    }

    #[test]
    fn test_auto_checkpoint_trigger() {
        let config = CheckpointConfig {
            auto_checkpoint_interval: Some(3),
            ..Default::default()
        };
        let mut manager = CheckpointManager::with_config(config);

        assert!(!manager.tick_turn());
        assert!(!manager.tick_turn());
        assert!(manager.tick_turn()); // Third tick triggers
        assert!(!manager.tick_turn()); // Counter reset
    }

    #[test]
    fn test_checkpoint_age() {
        let checkpoint = Checkpoint::new();
        let age = checkpoint.age();
        assert!(age.contains("seconds"));
    }

    #[test]
    fn test_find_by_tag() {
        let mut manager = CheckpointManager::new();
        let context = create_test_context();

        manager.create_named_checkpoint("Test 1", &context, None);

        let found = manager.find_by_tag("manual");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn test_delete_checkpoint() {
        let mut manager = CheckpointManager::new();
        let context = create_test_context();

        let id = manager.create_checkpoint(&context, None);
        assert_eq!(manager.len(), 1);

        assert!(manager.delete(id));
        assert_eq!(manager.len(), 0);

        // Deleting non-existent should return false
        assert!(!manager.delete(id));
    }
}
