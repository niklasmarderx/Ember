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

    // =========================================================================
    // Export Methods
    // =========================================================================

    /// Export the conversation to JSON format.
    pub fn export_json(&self, provider: Option<&str>, model: Option<&str>) -> String {
        let export = ConversationExport::from_conversation(self, provider, model);
        serde_json::to_string_pretty(&export).unwrap_or_else(|_| "{}".to_string())
    }

    /// Export the conversation to Markdown format.
    pub fn export_markdown(&self, provider: Option<&str>, model: Option<&str>) -> String {
        let mut output = String::new();

        // Header
        output.push_str("# Chat Conversation\n\n");

        // Metadata
        let date = self.created_at.format("%B %d, %Y at %H:%M UTC");
        output.push_str(&format!("**Date:** {}  \n", date));

        if let Some(p) = provider {
            if let Some(m) = model {
                output.push_str(&format!("**Model:** {} ({})  \n", m, p));
            } else {
                output.push_str(&format!("**Provider:** {}  \n", p));
            }
        }

        if let Some(title) = &self.title {
            output.push_str(&format!("**Topic:** {}  \n", title));
        }

        let total = self.total_tokens();
        if total.total > 0 {
            output.push_str(&format!("**Tokens used:** {}  \n", total.total));
        }

        output.push_str("\n---\n\n");

        // Turns
        for (i, turn) in self.turns.iter().enumerate() {
            output.push_str(&format!("## Turn {}\n\n", i + 1));

            // User message
            output.push_str("**User:**\n");
            for line in turn.user_message.lines() {
                output.push_str(&format!("> {}\n", line));
            }
            output.push('\n');

            // Tool calls if any
            if !turn.tool_calls.is_empty() {
                output.push_str("**Tool Calls:**\n");
                for call in &turn.tool_calls {
                    output.push_str(&format!("- `{}`", call.name));
                    let args_str = call.arguments.to_string();
                    if args_str != "null" && args_str != "{}" {
                        output.push_str(&format!(": `{}`", args_str));
                    }
                    output.push('\n');
                }
                output.push('\n');
            }

            // Assistant response
            if !turn.assistant_response.is_empty() {
                output.push_str("**Assistant:**\n");
                for line in turn.assistant_response.lines() {
                    output.push_str(&format!("> {}\n", line));
                }
                output.push('\n');
            }

            // Error if any
            if let Some(error) = &turn.error {
                output.push_str(&format!("**Error:** {}\n\n", error));
            }
        }

        output
    }

    /// Export the conversation to HTML format.
    pub fn export_html(&self, provider: Option<&str>, model: Option<&str>) -> String {
        let mut output = String::new();

        // HTML header
        output.push_str(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Chat Conversation</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            line-height: 1.6;
            max-width: 800px;
            margin: 0 auto;
            padding: 20px;
            background: #f5f5f5;
        }
        .header {
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 20px;
            border-radius: 10px;
            margin-bottom: 20px;
        }
        .header h1 { margin-bottom: 10px; }
        .metadata { font-size: 0.9em; opacity: 0.9; }
        .turn {
            background: white;
            border-radius: 10px;
            margin-bottom: 15px;
            overflow: hidden;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }
        .message {
            padding: 15px 20px;
        }
        .user-message {
            background: #e3f2fd;
            border-left: 4px solid #2196f3;
        }
        .assistant-message {
            background: white;
            border-left: 4px solid #4caf50;
        }
        .role {
            font-weight: 600;
            margin-bottom: 5px;
            color: #333;
        }
        .content {
            white-space: pre-wrap;
            word-wrap: break-word;
        }
        .tool-calls {
            background: #fff3e0;
            padding: 10px 20px;
            font-size: 0.9em;
            border-left: 4px solid #ff9800;
        }
        .tool-call {
            font-family: 'Monaco', 'Consolas', monospace;
            background: rgba(0,0,0,0.05);
            padding: 2px 6px;
            border-radius: 3px;
        }
        .error {
            background: #ffebee;
            color: #c62828;
            padding: 10px 20px;
            border-left: 4px solid #f44336;
        }
        .footer {
            text-align: center;
            padding: 20px;
            color: #666;
            font-size: 0.85em;
        }
    </style>
</head>
<body>
"#,
        );

        // Header section
        output.push_str("    <div class=\"header\">\n");
        output.push_str("        <h1>Chat Conversation</h1>\n");
        output.push_str("        <div class=\"metadata\">\n");

        let date = self.created_at.format("%B %d, %Y at %H:%M UTC");
        output.push_str(&format!("            <div>Date: {}</div>\n", date));

        if let Some(p) = provider {
            if let Some(m) = model {
                output.push_str(&format!(
                    "            <div>Model: {} ({})</div>\n",
                    escape_html(m),
                    escape_html(p)
                ));
            }
        }

        if let Some(title) = &self.title {
            output.push_str(&format!(
                "            <div>Topic: {}</div>\n",
                escape_html(title)
            ));
        }

        let total = self.total_tokens();
        if total.total > 0 {
            output.push_str(&format!("            <div>Tokens: {}</div>\n", total.total));
        }

        output.push_str("        </div>\n");
        output.push_str("    </div>\n\n");

        // Turns
        for turn in &self.turns {
            output.push_str("    <div class=\"turn\">\n");

            // User message
            output.push_str("        <div class=\"message user-message\">\n");
            output.push_str("            <div class=\"role\">User</div>\n");
            output.push_str(&format!(
                "            <div class=\"content\">{}</div>\n",
                escape_html(&turn.user_message)
            ));
            output.push_str("        </div>\n");

            // Tool calls
            if !turn.tool_calls.is_empty() {
                output.push_str("        <div class=\"tool-calls\">\n");
                output.push_str("            <strong>Tool Calls:</strong>\n");
                for call in &turn.tool_calls {
                    output.push_str(&format!(
                        "            <div><span class=\"tool-call\">{}</span></div>\n",
                        escape_html(&call.name)
                    ));
                }
                output.push_str("        </div>\n");
            }

            // Assistant response
            if !turn.assistant_response.is_empty() {
                output.push_str("        <div class=\"message assistant-message\">\n");
                output.push_str("            <div class=\"role\">Assistant</div>\n");
                output.push_str(&format!(
                    "            <div class=\"content\">{}</div>\n",
                    escape_html(&turn.assistant_response)
                ));
                output.push_str("        </div>\n");
            }

            // Error
            if let Some(error) = &turn.error {
                output.push_str(&format!(
                    "        <div class=\"error\">Error: {}</div>\n",
                    escape_html(error)
                ));
            }

            output.push_str("    </div>\n\n");
        }

        // Footer
        output.push_str("    <div class=\"footer\">\n");
        output.push_str("        Exported with Ember AI Agent Framework\n");
        output.push_str("    </div>\n");
        output.push_str("</body>\n</html>\n");

        output
    }
}

/// Exported conversation format for JSON serialization.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConversationExport {
    /// Export metadata
    pub metadata: ExportMetadata,
    /// All messages in the conversation
    pub messages: Vec<ExportMessage>,
    /// Tool calls made during the conversation
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ExportToolCall>,
}

/// Metadata for exported conversations.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportMetadata {
    /// When the conversation was exported
    pub exported_at: DateTime<Utc>,
    /// When the conversation was created
    pub created_at: DateTime<Utc>,
    /// Conversation ID
    pub conversation_id: String,
    /// Provider used (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Model used (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Conversation title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Total tokens used
    pub total_tokens: u32,
    /// Number of turns
    pub turn_count: usize,
}

/// A message in the exported format.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportMessage {
    /// Role: user, assistant, or system
    pub role: String,
    /// Message content
    pub content: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// A tool call in the exported format.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportToolCall {
    /// Tool name
    pub name: String,
    /// Tool arguments (JSON string)
    pub arguments: String,
    /// Tool result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

impl ConversationExport {
    /// Create an export from a conversation.
    pub fn from_conversation(
        conv: &Conversation,
        provider: Option<&str>,
        model: Option<&str>,
    ) -> Self {
        let mut messages = Vec::new();
        let mut tool_calls = Vec::new();

        // Add system message
        messages.push(ExportMessage {
            role: "system".to_string(),
            content: conv.system_prompt.clone(),
            timestamp: conv.created_at,
        });

        // Add all turns
        for turn in &conv.turns {
            // User message
            messages.push(ExportMessage {
                role: "user".to_string(),
                content: turn.user_message.clone(),
                timestamp: turn.started_at,
            });

            // Tool calls
            for (call, result) in turn.tool_calls.iter().zip(turn.tool_results.iter()) {
                tool_calls.push(ExportToolCall {
                    name: call.name.clone(),
                    arguments: call.arguments.to_string(),
                    result: Some(result.output.clone()),
                    timestamp: turn.started_at,
                });
            }

            // Assistant response
            if !turn.assistant_response.is_empty() {
                messages.push(ExportMessage {
                    role: "assistant".to_string(),
                    content: turn.assistant_response.clone(),
                    timestamp: turn.completed_at.unwrap_or(turn.started_at),
                });
            }
        }

        let total = conv.total_tokens();

        Self {
            metadata: ExportMetadata {
                exported_at: Utc::now(),
                created_at: conv.created_at,
                conversation_id: conv.id.to_string(),
                provider: provider.map(String::from),
                model: model.map(String::from),
                title: conv.title.clone(),
                total_tokens: total.total,
                turn_count: conv.turns.len(),
            },
            messages,
            tool_calls,
        }
    }
}

/// Escape HTML special characters.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Supported export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// JSON format
    Json,
    /// Markdown format
    Markdown,
    /// HTML format
    Html,
}

impl ExportFormat {
    /// Get the file extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::Markdown => "md",
            ExportFormat::Html => "html",
        }
    }

    /// Parse from string.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "json" => Some(ExportFormat::Json),
            "markdown" | "md" => Some(ExportFormat::Markdown),
            "html" => Some(ExportFormat::Html),
            _ => None,
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
