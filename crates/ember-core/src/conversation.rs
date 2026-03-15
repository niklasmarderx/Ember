//! Conversation management for agent interactions.

use chrono::{DateTime, Utc};
use ember_llm::{Message, ToolCall, ToolResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConversationId(pub Uuid);

impl ConversationId {
    /// Create a new random conversation ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for ConversationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ConversationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A single turn in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    /// Turn identifier
    pub id: Uuid,

    /// User's input message
    pub user_message: String,

    /// Assistant's response
    pub assistant_response: String,

    /// Tool calls made during this turn
    pub tool_calls: Vec<ToolCall>,

    /// Results from tool calls
    pub tool_results: Vec<ToolResult>,

    /// When this turn started
    pub started_at: DateTime<Utc>,

    /// When this turn completed
    pub completed_at: Option<DateTime<Utc>>,

    /// Number of tokens used (input + output)
    pub tokens_used: Option<TokenUsage>,

    /// Any error that occurred
    pub error: Option<String>,
}

impl Turn {
    /// Create a new turn.
    pub fn new(user_message: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_message: user_message.into(),
            assistant_response: String::new(),
            tool_calls: Vec::new(),
            tool_results: Vec::new(),
            started_at: Utc::now(),
            completed_at: None,
            tokens_used: None,
            error: None,
        }
    }

    /// Set the assistant response.
    pub fn with_response(mut self, response: impl Into<String>) -> Self {
        self.assistant_response = response.into();
        self
    }

    /// Add a tool call.
    pub fn add_tool_call(&mut self, tool_call: ToolCall) {
        self.tool_calls.push(tool_call);
    }

    /// Add a tool result.
    pub fn add_tool_result(&mut self, result: ToolResult) {
        self.tool_results.push(result);
    }

    /// Mark the turn as complete.
    pub fn complete(&mut self) {
        self.completed_at = Some(Utc::now());
    }

    /// Mark the turn as failed.
    pub fn fail(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
        self.completed_at = Some(Utc::now());
    }

    /// Check if the turn is complete.
    pub fn is_complete(&self) -> bool {
        self.completed_at.is_some()
    }

    /// Check if the turn has an error.
    pub fn has_error(&self) -> bool {
        self.error.is_some()
    }

    /// Get the duration of this turn.
    pub fn duration(&self) -> Option<chrono::Duration> {
        self.completed_at.map(|end| end - self.started_at)
    }
}

/// Token usage statistics.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens (prompt)
    pub input: u32,
    /// Output tokens (completion)
    pub output: u32,
    /// Total tokens
    pub total: u32,
}

impl TokenUsage {
    /// Create new token usage stats.
    pub fn new(input: u32, output: u32) -> Self {
        Self {
            input,
            output,
            total: input + output,
        }
    }
}

/// A complete conversation with an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    /// Unique identifier
    pub id: ConversationId,

    /// Conversation title (auto-generated or user-set)
    pub title: Option<String>,

    /// System prompt used
    pub system_prompt: String,

    /// All turns in this conversation
    pub turns: Vec<Turn>,

    /// When the conversation was created
    pub created_at: DateTime<Utc>,

    /// When the conversation was last updated
    pub updated_at: DateTime<Utc>,

    /// Conversation metadata
    pub metadata: std::collections::HashMap<String, String>,
}

impl Conversation {
    /// Create a new conversation.
    pub fn new(system_prompt: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: ConversationId::new(),
            title: None,
            system_prompt: system_prompt.into(),
            turns: Vec::new(),
            created_at: now,
            updated_at: now,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Create a conversation with a specific ID.
    pub fn with_id(id: ConversationId, system_prompt: impl Into<String>) -> Self {
        let mut conv = Self::new(system_prompt);
        conv.id = id;
        conv
    }

    /// Set the conversation title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Start a new turn.
    pub fn start_turn(&mut self, user_message: impl Into<String>) -> &mut Turn {
        let turn = Turn::new(user_message);
        self.turns.push(turn);
        self.updated_at = Utc::now();
        self.turns.last_mut().unwrap()
    }

    /// Get the current turn (most recent).
    pub fn current_turn(&self) -> Option<&Turn> {
        self.turns.last()
    }

    /// Get the current turn mutably.
    pub fn current_turn_mut(&mut self) -> Option<&mut Turn> {
        self.turns.last_mut()
    }

    /// Get the number of turns.
    pub fn len(&self) -> usize {
        self.turns.len()
    }

    /// Check if the conversation is empty.
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }

    /// Convert to a list of messages for the LLM.
    pub fn to_messages(&self) -> Vec<Message> {
        let mut messages = Vec::with_capacity(self.turns.len() * 2 + 1);

        // Add system message
        messages.push(Message::system(&self.system_prompt));

        // Add all turns
        for turn in &self.turns {
            messages.push(Message::user(&turn.user_message));

            // Add tool calls and results if any
            for (call, result) in turn.tool_calls.iter().zip(turn.tool_results.iter()) {
                // Assistant message with tool calls
                messages.push(Message::assistant("").with_tool_calls(vec![call.clone()]));
                // Tool result message
                messages.push(Message::tool_result(&call.id, &result.output).with_name(&call.name));
            }

            // Add assistant response if not empty
            if !turn.assistant_response.is_empty() {
                messages.push(Message::assistant(&turn.assistant_response));
            }
        }

        messages
    }

    /// Get total tokens used in this conversation.
    pub fn total_tokens(&self) -> TokenUsage {
        self.turns
            .iter()
            .filter_map(|t| t.tokens_used)
            .fold(TokenUsage::new(0, 0), |acc, usage| {
                TokenUsage::new(acc.input + usage.input, acc.output + usage.output)
            })
    }

    /// Generate a title from the first user message.
    pub fn auto_title(&mut self) {
        if self.title.is_none() && !self.turns.is_empty() {
            let first_msg = &self.turns[0].user_message;
            // Take first 50 chars or until first newline
            let title: String = first_msg
                .chars()
                .take(50)
                .take_while(|c| *c != '\n')
                .collect();
            self.title = Some(if title.len() < first_msg.len() {
                format!("{}...", title.trim())
            } else {
                title.trim().to_string()
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ember_llm::Role;

    #[test]
    fn test_conversation_creation() {
        let conv = Conversation::new("You are helpful.");
        assert!(conv.is_empty());
        assert!(conv.title.is_none());
    }

    #[test]
    fn test_turn_management() {
        let mut conv = Conversation::new("System");

        let turn = conv.start_turn("Hello!");
        turn.assistant_response = "Hi there!".to_string();
        turn.complete();

        assert_eq!(conv.len(), 1);
        assert!(conv.current_turn().unwrap().is_complete());
    }

    #[test]
    fn test_to_messages() {
        let mut conv = Conversation::new("System prompt");

        let turn = conv.start_turn("Hello");
        turn.assistant_response = "Hi!".to_string();
        turn.complete();

        let messages = conv.to_messages();
        assert_eq!(messages.len(), 3); // System + User + Assistant
        assert!(matches!(messages[0].role, Role::System));
        assert!(matches!(messages[1].role, Role::User));
        assert!(matches!(messages[2].role, Role::Assistant));
    }

    #[test]
    fn test_auto_title() {
        let mut conv = Conversation::new("System");
        conv.start_turn("What is the capital of France?");

        conv.auto_title();
        assert_eq!(
            conv.title,
            Some("What is the capital of France?".to_string())
        );
    }

    #[test]
    fn test_auto_title_truncation() {
        let mut conv = Conversation::new("System");
        conv.start_turn("This is a very long message that should be truncated because it exceeds fifty characters");

        conv.auto_title();
        assert!(conv.title.as_ref().unwrap().ends_with("..."));
        assert!(conv.title.as_ref().unwrap().len() <= 53); // 50 + "..."
    }

    #[test]
    fn test_token_tracking() {
        let mut conv = Conversation::new("System");

        let turn1 = conv.start_turn("Hello");
        turn1.tokens_used = Some(TokenUsage::new(10, 20));
        turn1.complete();

        let turn2 = conv.start_turn("World");
        turn2.tokens_used = Some(TokenUsage::new(15, 25));
        turn2.complete();

        let total = conv.total_tokens();
        assert_eq!(total.input, 25);
        assert_eq!(total.output, 45);
        assert_eq!(total.total, 70);
    }
}
