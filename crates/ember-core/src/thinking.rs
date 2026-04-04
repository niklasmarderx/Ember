//! Thinking blocks for structured reasoning.
//!
//! This module provides support for structured thinking blocks, similar to
//! how Cline uses `<thinking>` tags to make the AI's reasoning process
//! transparent and debuggable.
//!
//! # Overview
//!
//! Thinking blocks allow the agent to:
//! - Explicitly show its reasoning process
//! - Break down complex problems into steps
//! - Self-reflect on its approach
//! - Make decisions more transparent
//!
//! # Example
//!
//! ```rust
//! use ember_core::thinking::{ThinkingBlock, ThinkingType, ThinkingExtractor};
//!
//! let response = r#"
//! <thinking>
//! I need to analyze the user's request:
//! 1. They want to create a new file
//! 2. The file should be in src/
//! 3. It should be a Rust file
//!
//! I'll use the write_file tool for this.
//! </thinking>
//!
//! I'll create the file for you now.
//! "#;
//!
//! let (thinking_blocks, cleaned) = ThinkingExtractor::extract(response);
//! assert_eq!(thinking_blocks.len(), 1);
//! assert!(cleaned.contains("I'll create the file"));
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

/// Types of thinking blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ThinkingType {
    /// General reasoning and analysis
    #[default]
    Reasoning,
    /// Planning next steps
    Planning,
    /// Reflecting on previous actions
    Reflection,
    /// Evaluating options or decisions
    Evaluation,
    /// Summarizing information
    Summary,
    /// Debugging or troubleshooting
    Debug,
}

impl fmt::Display for ThinkingType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reasoning => write!(f, "reasoning"),
            Self::Planning => write!(f, "planning"),
            Self::Reflection => write!(f, "reflection"),
            Self::Evaluation => write!(f, "evaluation"),
            Self::Summary => write!(f, "summary"),
            Self::Debug => write!(f, "debug"),
        }
    }
}

/// A structured thinking block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingBlock {
    /// Type of thinking
    pub thinking_type: ThinkingType,

    /// The thinking content
    pub content: String,

    /// Optional title or subject
    pub title: Option<String>,

    /// Position in the original text (start index)
    pub position: usize,
}

impl ThinkingBlock {
    /// Create a new thinking block.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            thinking_type: ThinkingType::default(),
            content: content.into(),
            title: None,
            position: 0,
        }
    }

    /// Set the thinking type.
    pub fn with_type(mut self, thinking_type: ThinkingType) -> Self {
        self.thinking_type = thinking_type;
        self
    }

    /// Set the title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the position.
    pub fn with_position(mut self, position: usize) -> Self {
        self.position = position;
        self
    }

    /// Get a summary of the thinking (first line or limited characters).
    pub fn summary(&self, max_len: usize) -> String {
        let first_line = self.content.lines().next().unwrap_or("");
        if first_line.len() <= max_len {
            first_line.to_string()
        } else {
            format!("{}...", &first_line[..max_len.saturating_sub(3)])
        }
    }

    /// Check if this is a planning block.
    pub fn is_planning(&self) -> bool {
        self.thinking_type == ThinkingType::Planning
    }

    /// Check if this is a reflection block.
    pub fn is_reflection(&self) -> bool {
        self.thinking_type == ThinkingType::Reflection
    }
}

impl fmt::Display for ThinkingBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(title) = &self.title {
            writeln!(f, "[{}: {}]", self.thinking_type, title)?;
        } else {
            writeln!(f, "[{}]", self.thinking_type)?;
        }
        write!(f, "{}", self.content)
    }
}

/// Extracts and parses thinking blocks from text.
pub struct ThinkingExtractor;

impl ThinkingExtractor {
    /// Extract all thinking blocks from text.
    ///
    /// Returns a tuple of (thinking_blocks, cleaned_text) where cleaned_text
    /// has all thinking blocks removed.
    pub fn extract(text: &str) -> (Vec<ThinkingBlock>, String) {
        let mut blocks = Vec::new();
        let mut cleaned = String::with_capacity(text.len());
        let mut remaining = text;
        let mut position = 0;

        while let Some(start_idx) = remaining.find("<thinking") {
            // Add text before the thinking block
            cleaned.push_str(&remaining[..start_idx]);
            position += start_idx;

            // Find the end of the opening tag
            let after_start = &remaining[start_idx..];
            let tag_end = after_start.find('>').map(|i| i + 1);

            if let Some(tag_end) = tag_end {
                // Parse attributes from opening tag
                let opening_tag = &after_start[..tag_end];
                let thinking_type = Self::parse_type_attribute(opening_tag);
                let title = Self::parse_title_attribute(opening_tag);

                // Find closing tag
                let content_start = &after_start[tag_end..];
                if let Some(end_idx) = content_start.find("</thinking>") {
                    let content = content_start[..end_idx].trim().to_string();

                    // Infer type before creating the block if needed
                    let inferred_type = if thinking_type.is_none() {
                        Some(Self::infer_type(&content))
                    } else {
                        None
                    };

                    let mut block = ThinkingBlock::new(content).with_position(position);

                    if let Some(t) = thinking_type {
                        block = block.with_type(t);
                    } else if let Some(t) = inferred_type {
                        block = block.with_type(t);
                    }

                    if let Some(title) = title {
                        block = block.with_title(title);
                    }

                    blocks.push(block);

                    // Move past the closing tag
                    let skip = start_idx + tag_end + end_idx + "</thinking>".len();
                    remaining = &remaining[skip..];
                    position = 0; // Reset position tracking
                } else {
                    // No closing tag found, treat as regular text
                    cleaned.push_str(&remaining[..start_idx + tag_end]);
                    remaining = &remaining[start_idx + tag_end..];
                    position += tag_end;
                }
            } else {
                // Malformed tag, include as regular text
                cleaned.push_str(&remaining[..=start_idx]);
                remaining = &remaining[start_idx + 1..];
                position += 1;
            }
        }

        // Add remaining text
        cleaned.push_str(remaining);

        // Clean up extra whitespace
        let cleaned = Self::normalize_whitespace(&cleaned);

        (blocks, cleaned)
    }

    /// Parse type attribute from opening tag.
    fn parse_type_attribute(tag: &str) -> Option<ThinkingType> {
        // Look for type="..." or type='...'
        let type_patterns = ["type=\"", "type='"];

        for pattern in &type_patterns {
            if let Some(start) = tag.find(pattern) {
                let value_start = start + pattern.len();
                let quote = pattern.chars().last()?;
                if let Some(end) = tag[value_start..].find(quote) {
                    let value = &tag[value_start..value_start + end];
                    return match value.to_lowercase().as_str() {
                        "reasoning" => Some(ThinkingType::Reasoning),
                        "planning" => Some(ThinkingType::Planning),
                        "reflection" => Some(ThinkingType::Reflection),
                        "evaluation" => Some(ThinkingType::Evaluation),
                        "summary" => Some(ThinkingType::Summary),
                        "debug" => Some(ThinkingType::Debug),
                        _ => None,
                    };
                }
            }
        }
        None
    }

    /// Parse title attribute from opening tag.
    fn parse_title_attribute(tag: &str) -> Option<String> {
        let title_patterns = ["title=\"", "title='"];

        for pattern in &title_patterns {
            if let Some(start) = tag.find(pattern) {
                let value_start = start + pattern.len();
                let quote = pattern.chars().last()?;
                if let Some(end) = tag[value_start..].find(quote) {
                    return Some(tag[value_start..value_start + end].to_string());
                }
            }
        }
        None
    }

    /// Infer the thinking type from content.
    fn infer_type(content: &str) -> ThinkingType {
        let lower = content.to_lowercase();

        // Check for planning indicators
        if lower.contains("i will")
            || lower.contains("i'll")
            || lower.contains("plan to")
            || lower.contains("steps:")
            || lower.contains("step 1")
            || lower.contains("first,")
            || lower.contains("then,")
        {
            return ThinkingType::Planning;
        }

        // Check for reflection indicators
        if lower.contains("looking back")
            || lower.contains("i notice")
            || lower.contains("reflecting")
            || lower.contains("i should have")
            || lower.contains("in retrospect")
        {
            return ThinkingType::Reflection;
        }

        // Check for evaluation indicators
        if lower.contains("comparing")
            || lower.contains("option a")
            || lower.contains("pros and cons")
            || lower.contains("trade-off")
            || lower.contains("better to")
        {
            return ThinkingType::Evaluation;
        }

        // Check for summary indicators
        if lower.contains("in summary")
            || lower.contains("to summarize")
            || lower.contains("overall")
            || lower.contains("in conclusion")
        {
            return ThinkingType::Summary;
        }

        // Check for debug indicators
        if lower.contains("error")
            || lower.contains("bug")
            || lower.contains("issue")
            || lower.contains("debug")
            || lower.contains("investigating")
        {
            return ThinkingType::Debug;
        }

        // Default to reasoning
        ThinkingType::Reasoning
    }

    /// Normalize whitespace in cleaned text.
    fn normalize_whitespace(text: &str) -> String {
        let mut result = String::with_capacity(text.len());
        let mut prev_newline = false;

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !prev_newline {
                    result.push('\n');
                    prev_newline = true;
                }
            } else {
                if prev_newline && !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(trimmed);
                result.push('\n');
                prev_newline = false;
            }
        }

        result.trim().to_string()
    }

    /// Check if text contains thinking blocks.
    pub fn has_thinking(text: &str) -> bool {
        text.contains("<thinking") && text.contains("</thinking>")
    }
}

/// Builder for creating thinking prompts.
pub struct ThinkingPromptBuilder {
    base_prompt: String,
    require_thinking: bool,
    thinking_types: Vec<ThinkingType>,
}

impl ThinkingPromptBuilder {
    /// Create a new thinking prompt builder.
    pub fn new() -> Self {
        Self {
            base_prompt: String::new(),
            require_thinking: true,
            thinking_types: vec![ThinkingType::Reasoning, ThinkingType::Planning],
        }
    }

    /// Set the base prompt.
    pub fn base_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.base_prompt = prompt.into();
        self
    }

    /// Set whether thinking is required.
    pub fn require_thinking(mut self, required: bool) -> Self {
        self.require_thinking = required;
        self
    }

    /// Set allowed thinking types.
    pub fn thinking_types(mut self, types: Vec<ThinkingType>) -> Self {
        self.thinking_types = types;
        self
    }

    /// Build the enhanced prompt with thinking instructions.
    pub fn build(&self) -> String {
        let thinking_instruction = if self.require_thinking {
            format!(
                r#"
Before responding, use <thinking> tags to show your reasoning process.
You can use different types of thinking:
{}

Example:
<thinking type="reasoning">
Analyzing the user's request...
- Key point 1
- Key point 2
</thinking>

<thinking type="planning">
Steps to accomplish this:
1. First step
2. Second step
</thinking>

Your response goes here, outside the thinking tags.
"#,
                self.thinking_types
                    .iter()
                    .map(|t| format!("- {}: for {}", t, Self::describe_type(*t)))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        } else {
            String::new()
        };

        format!("{}\n\n{}", self.base_prompt, thinking_instruction)
            .trim()
            .to_string()
    }

    /// Get description for a thinking type.
    fn describe_type(t: ThinkingType) -> &'static str {
        match t {
            ThinkingType::Reasoning => "general analysis and understanding",
            ThinkingType::Planning => "outlining steps and approach",
            ThinkingType::Reflection => "reviewing and learning from actions",
            ThinkingType::Evaluation => "comparing options and trade-offs",
            ThinkingType::Summary => "condensing information",
            ThinkingType::Debug => "troubleshooting and error analysis",
        }
    }
}

impl Default for ThinkingPromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about thinking in a response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThinkingStats {
    /// Number of thinking blocks
    pub block_count: usize,

    /// Total characters in thinking
    pub total_chars: usize,

    /// Types of thinking used
    pub types_used: Vec<ThinkingType>,

    /// Whether planning was included
    pub has_planning: bool,

    /// Whether reflection was included
    pub has_reflection: bool,
}

impl ThinkingStats {
    /// Calculate stats from thinking blocks.
    pub fn from_blocks(blocks: &[ThinkingBlock]) -> Self {
        let types_used: Vec<ThinkingType> = blocks
            .iter()
            .map(|b| b.thinking_type)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        Self {
            block_count: blocks.len(),
            total_chars: blocks.iter().map(|b| b.content.len()).sum(),
            has_planning: types_used.contains(&ThinkingType::Planning),
            has_reflection: types_used.contains(&ThinkingType::Reflection),
            types_used,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_thinking() {
        let text = r#"
<thinking>
This is my reasoning.
</thinking>

Here is my response.
"#;
        let (blocks, cleaned) = ThinkingExtractor::extract(text);

        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].content.contains("This is my reasoning"));
        assert!(cleaned.contains("Here is my response"));
        assert!(!cleaned.contains("thinking"));
    }

    #[test]
    fn test_extract_typed_thinking() {
        let text = r#"
<thinking type="planning">
Step 1: Do this
Step 2: Do that
</thinking>

I will help you.
"#;
        let (blocks, _) = ThinkingExtractor::extract(text);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].thinking_type, ThinkingType::Planning);
    }

    #[test]
    fn test_extract_multiple_blocks() {
        let text = r#"
<thinking type="reasoning">
First analysis.
</thinking>

Some text.

<thinking type="planning">
The plan.
</thinking>

Final response.
"#;
        let (blocks, cleaned) = ThinkingExtractor::extract(text);

        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].thinking_type, ThinkingType::Reasoning);
        assert_eq!(blocks[1].thinking_type, ThinkingType::Planning);
        assert!(cleaned.contains("Some text"));
        assert!(cleaned.contains("Final response"));
    }

    #[test]
    fn test_infer_planning_type() {
        let content = "I will first check the file, then modify it.";
        let inferred = ThinkingExtractor::infer_type(content);
        assert_eq!(inferred, ThinkingType::Planning);
    }

    #[test]
    fn test_infer_debug_type() {
        let content = "There seems to be an error in the configuration.";
        let inferred = ThinkingExtractor::infer_type(content);
        assert_eq!(inferred, ThinkingType::Debug);
    }

    #[test]
    fn test_thinking_block_summary() {
        let block =
            ThinkingBlock::new("This is a very long thinking block content that goes on and on.");
        let summary = block.summary(20);
        assert!(summary.len() <= 20);
        assert!(summary.ends_with("..."));
    }

    #[test]
    fn test_thinking_stats() {
        let blocks = vec![
            ThinkingBlock::new("Reasoning").with_type(ThinkingType::Reasoning),
            ThinkingBlock::new("Planning").with_type(ThinkingType::Planning),
        ];

        let stats = ThinkingStats::from_blocks(&blocks);

        assert_eq!(stats.block_count, 2);
        assert!(stats.has_planning);
        assert!(!stats.has_reflection);
    }

    #[test]
    fn test_prompt_builder() {
        let prompt = ThinkingPromptBuilder::new()
            .base_prompt("You are a helpful assistant.")
            .require_thinking(true)
            .build();

        assert!(prompt.contains("helpful assistant"));
        assert!(prompt.contains("<thinking>"));
    }

    #[test]
    fn test_has_thinking() {
        assert!(ThinkingExtractor::has_thinking("<thinking>test</thinking>"));
        assert!(!ThinkingExtractor::has_thinking("no thinking here"));
        assert!(!ThinkingExtractor::has_thinking("<thinking>unclosed"));
    }
}
