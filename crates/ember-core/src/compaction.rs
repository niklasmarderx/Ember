//! Session auto-compaction for long-running conversations.
//!
//! When a conversation grows large enough to threaten the model's context
//! window, this module summarises the oldest turns in-place, keeping the
//! most recent turns untouched and inserting a compact summary at the front.
//!
//! # Example
//!
//! ```rust
//! use ember_core::compaction::{CompactionConfig, compact_conversation};
//! use ember_core::Conversation;
//!
//! let mut conv = Conversation::new("You are a helpful assistant.");
//! // … populate turns …
//!
//! let config = CompactionConfig::default();
//! if config.should_compact(&conv) {
//!     let result = compact_conversation(&mut conv, &config);
//!     println!(
//!         "Compacted {} turns, saved ~{} tokens",
//!         result.turns_removed,
//!         result.original_tokens.saturating_sub(result.compacted_tokens)
//!     );
//! }
//! ```

use crate::conversation::{Conversation, TokenUsage, Turn};
use chrono::Utc;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Token estimation
// ---------------------------------------------------------------------------

/// Estimate the number of tokens in a string using the 4-chars-≈-1-token
/// heuristic.  This matches the approximation used in `context_manager.rs`.
fn estimate_str_tokens(s: &str) -> usize {
    // ceiling division: (len + 3) / 4
    s.len().saturating_add(3) / 4
}

/// Public version of the token estimator for arbitrary strings.
pub fn estimate_string_tokens(s: &str) -> usize {
    estimate_str_tokens(s)
}

/// Estimate the total token footprint of a [`Turn`].
///
/// Counts the user message, assistant response, and the text content of
/// every tool call (name + serialised arguments) and tool result.
fn estimate_turn_tokens(turn: &Turn) -> usize {
    let mut total =
        estimate_str_tokens(&turn.user_message) + estimate_str_tokens(&turn.assistant_response);

    for call in &turn.tool_calls {
        total += estimate_str_tokens(&call.name);
        total += estimate_str_tokens(&call.arguments.to_string());
    }
    for result in &turn.tool_results {
        total += estimate_str_tokens(&result.output);
    }

    total
}

/// Estimate the total token count of an entire [`Conversation`], including
/// the system prompt and all turns.
pub fn estimate_tokens(conversation: &Conversation) -> usize {
    let system_tokens = estimate_str_tokens(&conversation.system_prompt);
    let turn_tokens: usize = conversation.turns.iter().map(estimate_turn_tokens).sum();
    system_tokens + turn_tokens
}

/// Return `true` when the conversation exceeds `max_tokens * threshold`.
pub fn should_compact(conversation: &Conversation, max_tokens: usize, threshold: f64) -> bool {
    let estimated = estimate_tokens(conversation);
    let limit = (max_tokens as f64 * threshold) as usize;
    estimated >= limit
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the auto-compaction engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    /// Hard limit on context tokens (e.g. 100 000 for a 128k-token model
    /// with room left for the response).
    pub max_context_tokens: usize,

    /// Fraction of `max_context_tokens` that triggers compaction
    /// (e.g. `0.8` → compact when ≥ 80 % full).
    pub compact_threshold: f64,

    /// How many of the most-recent turns to leave untouched.
    pub keep_recent_turns: usize,

    /// Approximate token budget for the generated summary.  The actual
    /// summary is built from turn text, so this caps how many characters
    /// we include (converted via the 4-char heuristic).
    pub summary_max_tokens: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 100_000,
            compact_threshold: 0.8,
            keep_recent_turns: 4,
            summary_max_tokens: 2_000,
        }
    }
}

impl CompactionConfig {
    /// Return `true` when `conversation` should be compacted under this
    /// configuration.
    pub fn should_compact(&self, conversation: &Conversation) -> bool {
        should_compact(
            conversation,
            self.max_context_tokens,
            self.compact_threshold,
        )
    }
}

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

/// Outcome of a compaction pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    /// Estimated token count *before* compaction.
    pub original_tokens: usize,

    /// Estimated token count *after* compaction.
    pub compacted_tokens: usize,

    /// How many turns were replaced by the summary.
    pub turns_removed: usize,

    /// The summary text that was inserted as a new system turn.
    pub summary: String,
}

// ---------------------------------------------------------------------------
// Summary builder
// ---------------------------------------------------------------------------

/// Build a plain-text summary of `turns`, staying within the
/// `max_tokens` budget (4-char heuristic).
fn build_summary(turns: &[Turn], max_tokens: usize) -> String {
    // Budget in characters (4 chars ≈ 1 token)
    let char_budget = max_tokens.saturating_mul(4);

    let mut parts: Vec<String> = Vec::with_capacity(turns.len() * 2);
    let mut used_chars: usize = 0;

    let header = format!("[Conversation summary — {} earlier turn(s)]\n", turns.len());
    used_chars += header.len();
    parts.push(header);

    for (i, turn) in turns.iter().enumerate() {
        // User line
        let user_line = format!("Turn {}: User: {}", i + 1, turn.user_message);
        // Assistant line (skip if empty)
        let assistant_line = if turn.assistant_response.is_empty() {
            None
        } else {
            Some(format!(
                "Turn {}: Assistant: {}",
                i + 1,
                turn.assistant_response
            ))
        };

        let needed = user_line.len() + assistant_line.as_ref().map_or(0, |s| s.len() + 1) + 1; // newlines

        if used_chars + needed > char_budget {
            // Append a truncation marker and stop
            let marker = format!(
                "[… {} turn(s) omitted due to summary length limit]\n",
                turns.len() - i
            );
            parts.push(marker);
            break;
        }

        parts.push(format!("{}\n", user_line));
        used_chars += user_line.len() + 1;

        if let Some(line) = assistant_line {
            parts.push(format!("{}\n", line));
            used_chars += line.len() + 1;
        }
    }

    parts.concat()
}

// ---------------------------------------------------------------------------
// Core compaction function
// ---------------------------------------------------------------------------

/// Compact `conversation` in-place according to `config`.
///
/// # Behaviour
///
/// 1. If the conversation does not exceed the threshold, return a
///    zero-effect result immediately (non-destructive).
/// 2. Otherwise, identify the turns that will be summarised (everything
///    except the last `keep_recent_turns`).
/// 3. Build a summary of those turns.
/// 4. Replace them with a single synthetic system-role turn containing the
///    summary.
/// 5. Return a [`CompactionResult`] with before/after metrics.
pub fn compact_conversation(
    conversation: &mut Conversation,
    config: &CompactionConfig,
) -> CompactionResult {
    let original_tokens = estimate_tokens(conversation);

    // Nothing to do if we are below the threshold.
    if !config.should_compact(conversation) {
        return CompactionResult {
            original_tokens,
            compacted_tokens: original_tokens,
            turns_removed: 0,
            summary: String::new(),
        };
    }

    let total_turns = conversation.turns.len();

    // We need at least one turn to summarise; if keep_recent_turns already
    // covers all turns, there is nothing we can remove.
    if total_turns <= config.keep_recent_turns {
        return CompactionResult {
            original_tokens,
            compacted_tokens: original_tokens,
            turns_removed: 0,
            summary: String::new(),
        };
    }

    let turns_to_summarise = total_turns - config.keep_recent_turns;

    // Drain the oldest turns from the front of the vector.
    let old_turns: Vec<Turn> = conversation.turns.drain(0..turns_to_summarise).collect();

    // Build the summary text.
    let summary = build_summary(&old_turns, config.summary_max_tokens);

    // Insert a synthetic turn that carries the summary as a "system"
    // turn.  We model it as a turn with an empty user message and the
    // summary as the assistant response so that `to_messages()` picks it
    // up naturally, and we tag it via metadata so callers can identify it.
    //
    // A cleaner fit with the existing Conversation schema is to prepend a
    // Turn whose `user_message` is empty and whose `assistant_response`
    // holds the summary text.  That keeps `to_messages()` working without
    // any changes to the existing code.
    let mut summary_turn = Turn::new("");
    summary_turn.assistant_response = summary.clone();
    summary_turn.complete();
    // Mark the token usage so total_tokens() stays accurate.
    let summary_token_count = estimate_str_tokens(&summary) as u32;
    summary_turn.tokens_used = Some(TokenUsage::new(0, summary_token_count));

    conversation.turns.insert(0, summary_turn);
    conversation.updated_at = Utc::now();

    let compacted_tokens = estimate_tokens(conversation);

    CompactionResult {
        original_tokens,
        compacted_tokens,
        turns_removed: turns_to_summarise,
        summary,
    }
}

// ---------------------------------------------------------------------------
// ContextBudget — lightweight token pressure tracker for raw Message lists
// ---------------------------------------------------------------------------

/// Tracks token pressure for a context window and determines when compaction
/// is needed. Works with raw `Message` lists (unlike `CompactionConfig` which
/// works with `Conversation` structs).
#[derive(Debug, Clone)]
pub struct ContextBudget {
    /// Maximum tokens the model supports.
    pub max_tokens: usize,
    /// Estimated tokens used by the system prompt.
    pub system_tokens: usize,
    /// Estimated tokens used by conversation history.
    pub conversation_tokens: usize,
    /// Fraction of context that triggers compaction (default 0.75).
    pub threshold: f64,
}

impl ContextBudget {
    /// Create a budget for a model with the given context window size.
    pub fn for_model(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            system_tokens: 0,
            conversation_tokens: 0,
            threshold: 0.75,
        }
    }

    /// Record the estimated system prompt token count.
    pub fn set_system_tokens(&mut self, tokens: usize) {
        self.system_tokens = tokens;
    }

    /// Record the estimated conversation token count.
    pub fn set_conversation_tokens(&mut self, tokens: usize) {
        self.conversation_tokens = tokens;
    }

    /// Returns `true` when total tokens exceed threshold × max_tokens.
    pub fn needs_compaction(&self) -> bool {
        let total = self.system_tokens + self.conversation_tokens;
        let limit = (self.max_tokens as f64 * self.threshold) as usize;
        total >= limit
    }
}

/// Result of compacting a raw message history.
#[derive(Debug, Clone)]
pub struct CompactMessageInfo {
    /// Number of messages removed.
    pub messages_removed: usize,
    /// Estimated tokens before compaction.
    pub tokens_before: usize,
    /// Estimated tokens after compaction.
    pub tokens_after: usize,
}

/// Compact a raw `Message` history by summarising older messages into a single
/// system message. Keeps the first message (system prompt) and the last
/// `keep_recent` messages intact.
pub fn compact_message_history(
    history: &mut Vec<ember_llm::Message>,
    _budget: &ContextBudget,
) -> Option<CompactMessageInfo> {
    let keep_recent = 6; // keep last 6 messages

    if history.len() <= keep_recent + 1 {
        return None; // not enough messages to compact
    }

    let tokens_before: usize = history.iter().map(|m| estimate_str_tokens(&m.content)).sum();

    // Keep first message (system prompt) and last `keep_recent`
    let remove_start = 1;
    let remove_end = history.len() - keep_recent;
    let removed_count = remove_end - remove_start;

    if removed_count == 0 {
        return None;
    }

    // Build summary of removed messages
    let mut summary_parts: Vec<String> = Vec::new();
    summary_parts.push(format!(
        "[Context summary — {} earlier messages compacted]",
        removed_count
    ));
    for msg in &history[remove_start..remove_end] {
        let prefix = match msg.role {
            ember_llm::Role::User => "User",
            ember_llm::Role::Assistant => "Assistant",
            _ => continue,
        };
        let truncated = if msg.content.len() > 200 {
            format!("{}…", &msg.content[..200])
        } else {
            msg.content.clone()
        };
        summary_parts.push(format!("{}: {}", prefix, truncated));
    }

    let summary = summary_parts.join("\n");

    // Remove old messages and insert summary
    history.drain(remove_start..remove_end);
    history.insert(remove_start, ember_llm::Message::system(&summary));

    let tokens_after: usize = history.iter().map(|m| estimate_str_tokens(&m.content)).sum();

    Some(CompactMessageInfo {
        messages_removed: removed_count,
        tokens_before,
        tokens_after,
    })
}

// ---------------------------------------------------------------------------
// StrategyTracker — detects repeated failures and suggests alternatives
// ---------------------------------------------------------------------------

/// Tracks tool execution success/failure to detect when the agent is stuck.
#[derive(Debug, Clone)]
pub struct StrategyTracker {
    recent_results: Vec<(String, bool)>,
    consecutive_failures: usize,
}

/// Reflection output from the strategy tracker.
#[derive(Debug, Clone)]
pub struct StrategyReflection {
    /// Human-readable reasoning about why a strategy change is suggested.
    pub reasoning: String,
    /// Optional alternative approach to try.
    pub alternative_strategy: Option<String>,
}

impl StrategyTracker {
    /// Create a new strategy tracker.
    pub fn new() -> Self {
        Self {
            recent_results: Vec::new(),
            consecutive_failures: 0,
        }
    }

    /// Record a tool execution result.
    pub fn record(&mut self, tool_name: &str, success: bool, _error: Option<&str>) {
        self.recent_results
            .push((tool_name.to_string(), success));
        if success {
            self.consecutive_failures = 0;
        } else {
            self.consecutive_failures += 1;
        }
        // Keep only last 20 results
        if self.recent_results.len() > 20 {
            self.recent_results.remove(0);
        }
    }

    /// Returns `true` if the agent appears stuck (3+ consecutive failures).
    pub fn should_switch_strategy(&self) -> bool {
        self.consecutive_failures >= 3
    }

    /// Generate a reflection about what's going wrong.
    pub fn reflect(&self) -> StrategyReflection {
        let recent_failures: Vec<&str> = self
            .recent_results
            .iter()
            .rev()
            .take(self.consecutive_failures)
            .map(|(name, _)| name.as_str())
            .collect();

        let reasoning = format!(
            "The last {} tool calls failed ({}). Consider a different approach.",
            self.consecutive_failures,
            recent_failures.join(", ")
        );

        let alternative = if self.consecutive_failures >= 5 {
            Some("Try breaking the problem into smaller steps, or ask the user for clarification.".to_string())
        } else {
            Some("Try a different tool or different arguments.".to_string())
        };

        StrategyReflection {
            reasoning,
            alternative_strategy: alternative,
        }
    }
}

impl Default for StrategyTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::Conversation;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Build a conversation with `n` turns, each carrying ~`chars_per_turn`
    /// characters of content so token counts are predictable.
    fn make_conversation(n: usize, chars_per_turn: usize) -> Conversation {
        let mut conv = Conversation::new("You are a test assistant.");
        for i in 0..n {
            let msg = "x".repeat(chars_per_turn);
            let turn = conv.start_turn(format!("User turn {} — {}", i, msg));
            turn.assistant_response = format!("Assistant turn {} — {}", i, msg);
            turn.complete();
        }
        conv
    }

    // -----------------------------------------------------------------------
    // estimate_tokens
    // -----------------------------------------------------------------------

    #[test]
    fn test_estimate_tokens_empty() {
        let conv = Conversation::new("");
        assert_eq!(estimate_tokens(&conv), 0);
    }

    #[test]
    fn test_estimate_tokens_counts_system_prompt() {
        let conv = Conversation::new("abcd"); // exactly 4 chars → 1 token
        assert_eq!(estimate_tokens(&conv), 1);
    }

    #[test]
    fn test_estimate_tokens_counts_turns() {
        let mut conv = Conversation::new("");
        let turn = conv.start_turn("aaaa"); // 4 chars → 1 token
        turn.assistant_response = "bbbbbbbb".to_string(); // 8 chars → 2 tokens
        turn.complete();
        // total = 0 (system) + 1 (user) + 2 (assistant) = 3
        assert_eq!(estimate_tokens(&conv), 3);
    }

    // -----------------------------------------------------------------------
    // should_compact
    // -----------------------------------------------------------------------

    #[test]
    fn test_should_compact_below_threshold() {
        // 5 turns × 8 chars each ≈ 20 tokens; threshold = 0.8 × 10_000 = 8_000
        let conv = make_conversation(5, 8);
        assert!(!should_compact(&conv, 10_000, 0.8));
    }

    #[test]
    fn test_should_compact_above_threshold() {
        // 20 turns × 400 chars each = 8_000 chars → 2_000 tokens
        // threshold = 0.8 × 1_000 = 800 → should trigger
        let conv = make_conversation(20, 400);
        assert!(should_compact(&conv, 1_000, 0.8));
    }

    #[test]
    fn test_should_compact_at_exact_threshold() {
        // 4 chars = 1 token; 4_000 chars = 1_000 tokens; threshold = 1.0 × 1_000 = 1_000
        // estimated >= limit  →  1_000 >= 1_000  →  true
        let conv = Conversation::new("a".repeat(4_000));
        assert!(should_compact(&conv, 1_000, 1.0));
    }

    // -----------------------------------------------------------------------
    // compact_conversation — no-op path
    // -----------------------------------------------------------------------

    #[test]
    fn test_compact_noop_when_below_threshold() {
        let mut conv = make_conversation(2, 8);
        let config = CompactionConfig {
            max_context_tokens: 1_000_000,
            compact_threshold: 0.8,
            keep_recent_turns: 4,
            summary_max_tokens: 500,
        };
        let result = compact_conversation(&mut conv, &config);

        assert_eq!(result.turns_removed, 0);
        assert!(result.summary.is_empty());
        assert_eq!(result.original_tokens, result.compacted_tokens);
        assert_eq!(conv.turns.len(), 2); // unchanged
    }

    #[test]
    fn test_compact_noop_when_all_turns_recent() {
        // 3 turns but keep_recent_turns = 3 → nothing to summarise
        let mut conv = make_conversation(3, 2_000);
        let config = CompactionConfig {
            max_context_tokens: 100,
            compact_threshold: 0.01, // always fires
            keep_recent_turns: 3,
            summary_max_tokens: 500,
        };
        let result = compact_conversation(&mut conv, &config);

        assert_eq!(result.turns_removed, 0);
        assert_eq!(conv.turns.len(), 3);
    }

    // -----------------------------------------------------------------------
    // compact_conversation — active compaction
    // -----------------------------------------------------------------------

    #[test]
    fn test_compact_removes_old_turns() {
        // 10 turns × 400 chars ≈ 1_000 tokens; threshold = 0.01 × 1_000 = 10 → always fires
        let mut conv = make_conversation(10, 400);
        let config = CompactionConfig {
            max_context_tokens: 1_000,
            compact_threshold: 0.01,
            keep_recent_turns: 4,
            summary_max_tokens: 500,
        };

        let result = compact_conversation(&mut conv, &config);

        // 10 - 4 = 6 turns summarised + 1 summary turn inserted
        assert_eq!(result.turns_removed, 6);
        assert_eq!(conv.turns.len(), 5); // 4 recent + 1 summary
    }

    #[test]
    fn test_compact_keeps_recent_turns_intact() {
        let mut conv = make_conversation(8, 400);
        let config = CompactionConfig {
            max_context_tokens: 1_000,
            compact_threshold: 0.01,
            keep_recent_turns: 3,
            summary_max_tokens: 500,
        };

        let original_recent: Vec<String> = conv.turns[5..]
            .iter()
            .map(|t| t.user_message.clone())
            .collect();

        compact_conversation(&mut conv, &config);

        // turns[0] is the summary; turns[1..=3] are the preserved recent ones
        let preserved: Vec<String> = conv.turns[1..]
            .iter()
            .map(|t| t.user_message.clone())
            .collect();

        assert_eq!(preserved, original_recent);
    }

    #[test]
    fn test_compact_summary_turn_is_first() {
        let mut conv = make_conversation(6, 400);
        let config = CompactionConfig {
            max_context_tokens: 1_000,
            compact_threshold: 0.01,
            keep_recent_turns: 2,
            summary_max_tokens: 500,
        };

        let result = compact_conversation(&mut conv, &config);

        // The first turn is the synthetic summary turn
        let first = conv.turns.first().unwrap();
        assert!(!first.assistant_response.is_empty());
        assert_eq!(first.assistant_response, result.summary);
        // Its user_message is empty (it's a sentinel)
        assert!(first.user_message.is_empty());
    }

    #[test]
    fn test_compact_summary_is_non_empty() {
        let mut conv = make_conversation(6, 400);
        let config = CompactionConfig {
            max_context_tokens: 1_000,
            compact_threshold: 0.01,
            keep_recent_turns: 2,
            summary_max_tokens: 500,
        };

        let result = compact_conversation(&mut conv, &config);

        assert!(!result.summary.is_empty());
        assert!(result.summary.contains("summary"));
    }

    #[test]
    fn test_compact_reduces_token_count() {
        let mut conv = make_conversation(20, 400);
        let config = CompactionConfig {
            max_context_tokens: 1_000,
            compact_threshold: 0.01,
            keep_recent_turns: 4,
            summary_max_tokens: 200,
        };

        let result = compact_conversation(&mut conv, &config);

        assert!(result.compacted_tokens < result.original_tokens);
    }

    #[test]
    fn test_compact_result_metrics_consistent() {
        let mut conv = make_conversation(10, 400);
        let config = CompactionConfig {
            max_context_tokens: 1_000,
            compact_threshold: 0.01,
            keep_recent_turns: 4,
            summary_max_tokens: 300,
        };

        let result = compact_conversation(&mut conv, &config);

        // compacted_tokens should match a fresh estimate of the modified conv
        assert_eq!(result.compacted_tokens, estimate_tokens(&conv));
    }

    // -----------------------------------------------------------------------
    // Default config
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_config_values() {
        let cfg = CompactionConfig::default();
        assert_eq!(cfg.max_context_tokens, 100_000);
        assert!((cfg.compact_threshold - 0.8).abs() < f64::EPSILON);
        assert_eq!(cfg.keep_recent_turns, 4);
        assert_eq!(cfg.summary_max_tokens, 2_000);
    }

    #[test]
    fn test_default_config_no_compact_on_short_conv() {
        let conv = make_conversation(3, 100);
        let cfg = CompactionConfig::default();
        assert!(!cfg.should_compact(&conv));
    }

    // -----------------------------------------------------------------------
    // build_summary internals
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_summary_respects_token_budget() {
        let mut conv = make_conversation(50, 200);
        let turns: Vec<Turn> = conv.turns.drain(..).collect();

        let max_tokens = 50; // tight budget
        let summary = build_summary(&turns, max_tokens);
        let estimated = estimate_str_tokens(&summary);

        // Allow a small overshoot from the header/marker lines that we add
        // after the budget check — they are always short.
        assert!(estimated <= max_tokens + 20);
    }

    #[test]
    fn test_build_summary_contains_turn_count() {
        let mut conv = make_conversation(3, 20);
        let turns: Vec<Turn> = conv.turns.drain(..).collect();
        let summary = build_summary(&turns, 1_000);
        assert!(summary.contains("3"));
    }
}
