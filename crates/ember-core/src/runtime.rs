//! Agentic conversation runtime — the loop that ties everything together.
//!
//! [`ConversationRuntime`] drives the standard ReAct-style agentic loop:
//!
//! ```text
//! user message
//!      ↓
//!   LLM call  ──text──────────────────────────────► TurnComplete
//!      │
//!      └──tool_use──► execute tool ──► tool result ──► LLM call …
//! ```
//!
//! The runtime is deliberately provider-agnostic: LLM and tool execution are
//! injected via [`LlmBackend`] and [`ToolBackend`] traits, so the same loop
//! works with any provider (OpenAI, Anthropic, local Ollama, mocks, …).
//!
//! # Example (synchronous mock)
//!
//! ```rust
//! use ember_core::runtime::{
//!     ConversationRuntime, LlmBackend, LlmResponse, ResponseBlock,
//!     RuntimeConfig, RuntimeError, ToolBackend, TokenUsageUpdate,
//! };
//! use ember_llm::Message;
//!
//! struct EchoLlm;
//!
//! impl LlmBackend for EchoLlm {
//!     fn complete(
//!         &self,
//!         _system: &str,
//!         _messages: &[Message],
//!     ) -> Result<LlmResponse, RuntimeError> {
//!         Ok(LlmResponse {
//!             content: vec![ResponseBlock::Text("Hello!".into())],
//!             usage: TokenUsageUpdate { input_tokens: 10, output_tokens: 5, total_cost_usd: 0.0 },
//!         })
//!     }
//! }
//!
//! let mut rt = ConversationRuntime::with_defaults();
//! let events = rt.execute_turn("Hi", "You are helpful.", &[], &EchoLlm, &());
//! ```

use ember_llm::Message;

// ---------------------------------------------------------------------------
// Public error type
// ---------------------------------------------------------------------------

/// Errors that can occur inside the runtime.
#[derive(Debug, Clone)]
pub enum RuntimeError {
    /// The LLM backend returned an error.
    LlmError(String),
    /// A tool reported a hard error (distinct from a tool that returns an
    /// error *result* — that goes through [`RuntimeEvent::ToolResult`] with
    /// `is_error: true`).
    ToolError(String),
    /// The maximum number of tool rounds per turn was exceeded.
    MaxToolRoundsExceeded(usize),
    /// Any other internal error.
    Internal(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LlmError(msg) => write!(f, "LLM error: {msg}"),
            Self::ToolError(msg) => write!(f, "Tool error: {msg}"),
            Self::MaxToolRoundsExceeded(n) => {
                write!(f, "Max tool rounds exceeded ({n} iterations)")
            }
            Self::Internal(msg) => write!(f, "Internal runtime error: {msg}"),
        }
    }
}

impl std::error::Error for RuntimeError {}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Events emitted during a single conversation turn.
///
/// Callers collect the full `Vec<RuntimeEvent>` returned by
/// [`ConversationRuntime::execute_turn`] or process them incrementally.
#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    /// LLM text output (streaming delta or full chunk).
    TextDelta(String),
    /// The LLM decided to invoke a tool.
    ToolUse {
        /// Unique ID assigned by the LLM to this tool call.
        id: String,
        /// Tool name.
        name: String,
        /// JSON-encoded arguments string.
        input: String,
    },
    /// Result of a tool invocation.
    ToolResult {
        /// Matches the [`RuntimeEvent::ToolUse`] `id`.
        id: String,
        /// Tool output (may be an error message when `is_error` is `true`).
        output: String,
        /// `true` when the tool backend reported an execution failure.
        is_error: bool,
    },
    /// Token usage snapshot after an LLM call.
    Usage(TokenUsageUpdate),
    /// The context was auto-compacted before this turn.
    Compacted(CompactionSummary),
    /// All LLM/tool rounds for this turn finished successfully.
    TurnComplete,
    /// A fatal error terminated the turn early.
    Error(String),
}

// ---------------------------------------------------------------------------
// Supporting data structures
// ---------------------------------------------------------------------------

/// Snapshot of token usage after an LLM call.
#[derive(Debug, Clone)]
pub struct TokenUsageUpdate {
    /// Tokens in the prompt sent to the LLM.
    pub input_tokens: u32,
    /// Tokens in the completion returned by the LLM.
    pub output_tokens: u32,
    /// Estimated cost in USD (0.0 when unknown).
    pub total_cost_usd: f64,
}

/// Summary of a compaction pass that was triggered before a turn.
#[derive(Debug, Clone)]
pub struct CompactionSummary {
    /// Number of turns that were merged into the summary.
    pub turns_removed: usize,
    /// Approximate token savings from the compaction.
    pub tokens_saved: usize,
}

/// A block of content returned by the LLM.
#[derive(Debug, Clone)]
pub enum ResponseBlock {
    /// Plain-text output.
    Text(String),
    /// The LLM wants to call a tool.
    ToolUse {
        /// Unique call ID (e.g. `"call_abc123"`).
        id: String,
        /// Tool name.
        name: String,
        /// JSON-encoded arguments.
        input: String,
    },
}

/// A complete LLM response, including content blocks and usage.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// Content blocks returned by the LLM.
    pub content: Vec<ResponseBlock>,
    /// Token usage for this call.
    pub usage: TokenUsageUpdate,
}

// ---------------------------------------------------------------------------
// Backend traits
// ---------------------------------------------------------------------------

/// Abstraction over any LLM provider.
///
/// Implement this for OpenAI, Anthropic, local models, mocks, etc.
pub trait LlmBackend {
    /// Send a completion request and return the response synchronously.
    ///
    /// `system` is the system prompt; `messages` is the full conversation
    /// history including the current user turn at the end.
    fn complete(
        &self,
        system: &str,
        messages: &[Message],
    ) -> Result<LlmResponse, RuntimeError>;
}

/// Abstraction over tool execution.
///
/// Implement this to plug in the actual tool dispatch logic.
pub trait ToolBackend {
    /// Execute a tool by name with JSON-encoded `input`.
    ///
    /// Returns `Ok(output)` on success, `Err(error_message)` on failure.
    fn execute(&self, tool_name: &str, input: &str) -> Result<String, String>;

    /// Returns `true` when the named tool is registered and can be called.
    fn is_available(&self, tool_name: &str) -> bool;
}

/// A no-op [`ToolBackend`] that reports every tool as unavailable.
///
/// Useful as a default when the caller does not register any tools.
impl ToolBackend for () {
    fn execute(&self, tool_name: &str, _input: &str) -> Result<String, String> {
        Err(format!("Tool '{tool_name}' is not registered"))
    }

    fn is_available(&self, _tool_name: &str) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration knobs for [`ConversationRuntime`].
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Maximum number of tool-use iterations per turn before giving up.
    ///
    /// Prevents infinite loops when a tool keeps returning results that
    /// trigger another tool call. Default: `25`.
    pub max_tool_rounds: usize,

    /// Enable automatic context compaction when the token budget is close
    /// to being exhausted. Default: `true`.
    pub auto_compact: bool,

    /// Fraction of `max_context_tokens` at which compaction is triggered.
    /// Must be in `(0.0, 1.0]`. Default: `0.8`.
    pub compact_threshold: f64,

    /// Maximum wall-clock time (seconds) allowed per individual tool call.
    /// Default: `120`.
    pub tool_timeout_secs: u64,

    /// Maximum number of tokens the LLM may produce in a single completion.
    /// Default: `4096`.
    pub max_output_tokens: u32,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_tool_rounds: 25,
            auto_compact: true,
            compact_threshold: 0.8,
            tool_timeout_secs: 120,
            max_output_tokens: 4096,
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime stats
// ---------------------------------------------------------------------------

/// Aggregate statistics accumulated by a [`ConversationRuntime`].
#[derive(Debug, Clone)]
pub struct RuntimeStats {
    /// Total completed turns.
    pub turn_count: usize,
    /// Total input tokens across all turns.
    pub total_input_tokens: u32,
    /// Total output tokens across all turns.
    pub total_output_tokens: u32,
    /// Average tokens (input + output) per turn.
    /// `0.0` when no turns have been executed yet.
    pub avg_tokens_per_turn: f64,
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

/// The agentic conversation runtime.
///
/// Holds configuration and cross-turn token accounting. The runtime itself
/// is stateless with respect to conversation history — callers own and pass
/// the message history on every call to [`execute_turn`](Self::execute_turn).
///
/// # Thread safety
///
/// `ConversationRuntime` is not `Sync`. If you need concurrent access, wrap
/// it in a `Mutex`.
pub struct ConversationRuntime {
    config: RuntimeConfig,
    turn_count: usize,
    total_input_tokens: u32,
    total_output_tokens: u32,
}

impl ConversationRuntime {
    /// Create a runtime with the supplied configuration.
    pub fn new(config: RuntimeConfig) -> Self {
        Self {
            config,
            turn_count: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    /// Create a runtime with [`RuntimeConfig::default`].
    pub fn with_defaults() -> Self {
        Self::new(RuntimeConfig::default())
    }

    // -----------------------------------------------------------------------
    // Core loop
    // -----------------------------------------------------------------------

    /// Execute one full agentic turn.
    ///
    /// # Arguments
    ///
    /// * `user_message` — the new user message for this turn.
    /// * `system_prompt` — system prompt sent on every LLM call.
    /// * `history` — prior conversation messages (does **not** include
    ///   `user_message`; the runtime appends it internally).
    /// * `llm` — the LLM backend to call.
    /// * `tools` — the tool backend to invoke when the LLM emits tool-use.
    ///
    /// # Returns
    ///
    /// A `Vec<RuntimeEvent>` describing everything that happened during this
    /// turn, always ending with either [`RuntimeEvent::TurnComplete`] or
    /// [`RuntimeEvent::Error`].
    ///
    /// # Loop termination
    ///
    /// The loop continues until one of:
    /// 1. The LLM responds with text only (no tool-use blocks).
    /// 2. [`RuntimeConfig::max_tool_rounds`] is reached.
    /// 3. The LLM backend returns an error.
    pub fn execute_turn(
        &mut self,
        user_message: &str,
        system_prompt: &str,
        history: &[Message],
        llm: &dyn LlmBackend,
        tools: &dyn ToolBackend,
    ) -> Vec<RuntimeEvent> {
        let mut events: Vec<RuntimeEvent> = Vec::new();

        // Build the working message list: history + new user message.
        let mut messages: Vec<Message> = history.to_vec();
        messages.push(Message::user(user_message));

        let mut tool_rounds: usize = 0;

        loop {
            // --- LLM call ---------------------------------------------------
            let response = match llm.complete(system_prompt, &messages) {
                Ok(r) => r,
                Err(e) => {
                    let msg = e.to_string();
                    events.push(RuntimeEvent::Error(msg));
                    return events;
                }
            };

            // Accumulate token usage.
            self.total_input_tokens += response.usage.input_tokens;
            self.total_output_tokens += response.usage.output_tokens;
            events.push(RuntimeEvent::Usage(response.usage.clone()));

            // --- Classify response blocks -----------------------------------
            let mut text_parts: Vec<String> = Vec::new();
            let mut tool_calls: Vec<(String, String, String)> = Vec::new(); // (id, name, input)

            for block in &response.content {
                match block {
                    ResponseBlock::Text(t) => text_parts.push(t.clone()),
                    ResponseBlock::ToolUse { id, name, input } => {
                        tool_calls.push((id.clone(), name.clone(), input.clone()));
                    }
                }
            }

            // Emit text deltas.
            for text in &text_parts {
                if !text.is_empty() {
                    events.push(RuntimeEvent::TextDelta(text.clone()));
                }
            }

            // --- No tool calls → turn is done --------------------------------
            if tool_calls.is_empty() {
                break;
            }

            // --- Tool-round limit check -------------------------------------
            if tool_rounds >= self.config.max_tool_rounds {
                events.push(RuntimeEvent::Error(
                    RuntimeError::MaxToolRoundsExceeded(tool_rounds).to_string(),
                ));
                return events;
            }

            // --- Execute tool calls -----------------------------------------
            // Append the assistant message (with tool-use) to the working history
            // so the next LLM call sees a coherent context.
            let assistant_text = text_parts.join("");
            let mut assistant_msg = Message::assistant(&assistant_text);
            // Attach tool calls as ember_llm ToolCall objects.
            let llm_tool_calls: Vec<ember_llm::ToolCall> = tool_calls
                .iter()
                .map(|(id, name, input)| ember_llm::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: serde_json::from_str(input)
                        .unwrap_or(serde_json::Value::String(input.clone())),
                })
                .collect();
            assistant_msg.tool_calls = llm_tool_calls;
            messages.push(assistant_msg);

            for (id, name, input) in &tool_calls {
                events.push(RuntimeEvent::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                });

                // Execute the tool.
                let (output, is_error) = if tools.is_available(name) {
                    match tools.execute(name, input) {
                        Ok(out) => (out, false),
                        Err(err) => (err, true),
                    }
                } else {
                    (format!("Tool '{name}' is not available"), true)
                };

                events.push(RuntimeEvent::ToolResult {
                    id: id.clone(),
                    output: output.clone(),
                    is_error,
                });

                // Append the tool result as a tool-result message.
                let mut tool_result_msg = Message::user(&output);
                tool_result_msg.tool_call_id = Some(id.clone());
                tool_result_msg.name = Some(name.clone());
                messages.push(tool_result_msg);
            }

            tool_rounds += 1;
        }

        self.turn_count += 1;
        events.push(RuntimeEvent::TurnComplete);
        events
    }

    // -----------------------------------------------------------------------
    // Compaction helpers
    // -----------------------------------------------------------------------

    /// Returns `true` when the accumulated token count suggests the context
    /// is approaching `max_context_tokens`.
    ///
    /// Uses the same threshold fraction as [`RuntimeConfig::compact_threshold`].
    /// Note: this is a *cross-turn* heuristic based on total tokens seen by
    /// this runtime instance, not an exact per-conversation estimate. For
    /// precise per-conversation compaction, use
    /// [`crate::compaction::should_compact`] directly.
    pub fn should_compact(&self, max_context_tokens: usize) -> bool {
        let total = (self.total_input_tokens + self.total_output_tokens) as f64;
        let limit = max_context_tokens as f64 * self.config.compact_threshold;
        total >= limit
    }

    // -----------------------------------------------------------------------
    // Stats / accessors
    // -----------------------------------------------------------------------

    /// Return a snapshot of runtime statistics.
    pub fn stats(&self) -> RuntimeStats {
        let total = self.total_input_tokens + self.total_output_tokens;
        let avg = if self.turn_count == 0 {
            0.0
        } else {
            f64::from(total) / self.turn_count as f64
        };
        RuntimeStats {
            turn_count: self.turn_count,
            total_input_tokens: self.total_input_tokens,
            total_output_tokens: self.total_output_tokens,
            avg_tokens_per_turn: avg,
        }
    }

    /// Number of completed turns.
    pub fn turn_count(&self) -> usize {
        self.turn_count
    }

    /// `(input_tokens, output_tokens)` accumulated across all turns.
    pub fn total_tokens(&self) -> (u32, u32) {
        (self.total_input_tokens, self.total_output_tokens)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ember_llm::Message;

    // -----------------------------------------------------------------------
    // Mock backends
    // -----------------------------------------------------------------------

    /// A mock LLM that returns a fixed text response on every call.
    struct TextOnlyLlm {
        text: &'static str,
        input_tokens: u32,
        output_tokens: u32,
    }

    impl TextOnlyLlm {
        fn new(text: &'static str) -> Self {
            Self {
                text,
                input_tokens: 10,
                output_tokens: 5,
            }
        }
    }

    impl LlmBackend for TextOnlyLlm {
        fn complete(
            &self,
            _system: &str,
            _messages: &[Message],
        ) -> Result<LlmResponse, RuntimeError> {
            Ok(LlmResponse {
                content: vec![ResponseBlock::Text(self.text.to_string())],
                usage: TokenUsageUpdate {
                    input_tokens: self.input_tokens,
                    output_tokens: self.output_tokens,
                    total_cost_usd: 0.0,
                },
            })
        }
    }

    /// A mock LLM that emits one tool-use on the first call, then plain text.
    struct OneToolLlm {
        tool_name: &'static str,
        tool_input: &'static str,
        call_count: std::cell::Cell<usize>,
    }

    impl OneToolLlm {
        fn new(tool_name: &'static str, tool_input: &'static str) -> Self {
            Self {
                tool_name,
                tool_input,
                call_count: std::cell::Cell::new(0),
            }
        }
    }

    impl LlmBackend for OneToolLlm {
        fn complete(
            &self,
            _system: &str,
            _messages: &[Message],
        ) -> Result<LlmResponse, RuntimeError> {
            let count = self.call_count.get();
            self.call_count.set(count + 1);

            if count == 0 {
                // First call: request tool use.
                Ok(LlmResponse {
                    content: vec![ResponseBlock::ToolUse {
                        id: "call_1".to_string(),
                        name: self.tool_name.to_string(),
                        input: self.tool_input.to_string(),
                    }],
                    usage: TokenUsageUpdate {
                        input_tokens: 20,
                        output_tokens: 10,
                        total_cost_usd: 0.0,
                    },
                })
            } else {
                // Subsequent calls: plain text response.
                Ok(LlmResponse {
                    content: vec![ResponseBlock::Text("Done!".to_string())],
                    usage: TokenUsageUpdate {
                        input_tokens: 30,
                        output_tokens: 15,
                        total_cost_usd: 0.0,
                    },
                })
            }
        }
    }

    /// A mock LLM that always requests the same tool — used to test round limits.
    struct InfiniteToolLlm {
        tool_name: &'static str,
    }

    impl LlmBackend for InfiniteToolLlm {
        fn complete(
            &self,
            _system: &str,
            _messages: &[Message],
        ) -> Result<LlmResponse, RuntimeError> {
            Ok(LlmResponse {
                content: vec![ResponseBlock::ToolUse {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: self.tool_name.to_string(),
                    input: "{}".to_string(),
                }],
                usage: TokenUsageUpdate {
                    input_tokens: 5,
                    output_tokens: 5,
                    total_cost_usd: 0.0,
                },
            })
        }
    }

    /// A mock LLM that always returns an error.
    struct FailingLlm;

    impl LlmBackend for FailingLlm {
        fn complete(
            &self,
            _system: &str,
            _messages: &[Message],
        ) -> Result<LlmResponse, RuntimeError> {
            Err(RuntimeError::LlmError("connection refused".to_string()))
        }
    }

    /// A mock tool backend backed by a closure.
    struct MockTools {
        name: &'static str,
        result: &'static str,
    }

    impl ToolBackend for MockTools {
        fn execute(&self, tool_name: &str, _input: &str) -> Result<String, String> {
            if tool_name == self.name {
                Ok(self.result.to_string())
            } else {
                Err(format!("unknown tool: {tool_name}"))
            }
        }

        fn is_available(&self, tool_name: &str) -> bool {
            tool_name == self.name
        }
    }

    // -----------------------------------------------------------------------
    // Test: RuntimeConfig defaults
    // -----------------------------------------------------------------------

    #[test]
    fn test_runtime_config_defaults_are_sensible() {
        let cfg = RuntimeConfig::default();
        assert_eq!(cfg.max_tool_rounds, 25);
        assert!(cfg.auto_compact);
        assert!((cfg.compact_threshold - 0.8).abs() < f64::EPSILON);
        assert_eq!(cfg.tool_timeout_secs, 120);
        assert_eq!(cfg.max_output_tokens, 4096);
    }

    // -----------------------------------------------------------------------
    // Test: ConversationRuntime creation
    // -----------------------------------------------------------------------

    #[test]
    fn test_runtime_creation_with_defaults() {
        let rt = ConversationRuntime::with_defaults();
        assert_eq!(rt.turn_count(), 0);
        let (inp, out) = rt.total_tokens();
        assert_eq!(inp, 0);
        assert_eq!(out, 0);
    }

    #[test]
    fn test_runtime_creation_with_custom_config() {
        let cfg = RuntimeConfig {
            max_tool_rounds: 5,
            ..Default::default()
        };
        let rt = ConversationRuntime::new(cfg);
        assert_eq!(rt.config.max_tool_rounds, 5);
    }

    // -----------------------------------------------------------------------
    // Test: text-only turn
    // -----------------------------------------------------------------------

    #[test]
    fn test_execute_turn_text_only_emits_text_delta_and_turn_complete() {
        let mut rt = ConversationRuntime::with_defaults();
        let llm = TextOnlyLlm::new("Hello, world!");
        let events = rt.execute_turn("Hi", "System", &[], &llm, &());

        // Must contain a TextDelta and end with TurnComplete.
        let has_text = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::TextDelta(t) if t == "Hello, world!"));
        let has_complete = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::TurnComplete));

        assert!(has_text, "Expected TextDelta event");
        assert!(has_complete, "Expected TurnComplete event");
        assert_eq!(rt.turn_count(), 1);
    }

    // -----------------------------------------------------------------------
    // Test: tool-use turn
    // -----------------------------------------------------------------------

    #[test]
    fn test_execute_turn_with_tool_use_emits_all_expected_events() {
        let mut rt = ConversationRuntime::with_defaults();
        let llm = OneToolLlm::new("search", r#"{"q":"rust"}"#);
        let tools = MockTools {
            name: "search",
            result: "Rust is a systems language",
        };
        let events = rt.execute_turn("Search for Rust", "System", &[], &llm, &tools);

        let has_tool_use = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ToolUse { name, .. } if name == "search"));
        let has_tool_result = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ToolResult { is_error, .. } if !is_error));
        let has_text = events.iter().any(|e| matches!(e, RuntimeEvent::TextDelta(_)));
        let has_complete = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::TurnComplete));

        assert!(has_tool_use, "Expected ToolUse event");
        assert!(has_tool_result, "Expected ToolResult event");
        assert!(has_text, "Expected TextDelta event");
        assert!(has_complete, "Expected TurnComplete event");
        assert_eq!(rt.turn_count(), 1);
    }

    // -----------------------------------------------------------------------
    // Test: max_tool_rounds enforcement
    // -----------------------------------------------------------------------

    #[test]
    fn test_max_tool_rounds_limits_iterations() {
        let mut rt = ConversationRuntime::new(RuntimeConfig {
            max_tool_rounds: 3,
            ..Default::default()
        });
        let llm = InfiniteToolLlm { tool_name: "loop_tool" };
        let tools = MockTools {
            name: "loop_tool",
            result: "still going",
        };
        let events = rt.execute_turn("Go", "System", &[], &llm, &tools);

        // Should end with an Error, not TurnComplete.
        let last = events.last().unwrap();
        assert!(
            matches!(last, RuntimeEvent::Error(_)),
            "Expected Error as last event, got {:?}",
            last
        );
        // TurnComplete should NOT be present.
        assert!(
            !events.iter().any(|e| matches!(e, RuntimeEvent::TurnComplete)),
            "TurnComplete should not appear when limit is exceeded"
        );
        // turn_count should NOT increment when the loop aborts.
        assert_eq!(rt.turn_count(), 0);
    }

    // -----------------------------------------------------------------------
    // Test: should_compact threshold
    // -----------------------------------------------------------------------

    #[test]
    fn test_should_compact_below_threshold() {
        let rt = ConversationRuntime::with_defaults(); // 0 tokens used
        // 0 >= 0.8 * 100_000 = 80_000 → false
        assert!(!rt.should_compact(100_000));
    }

    #[test]
    fn test_should_compact_above_threshold() {
        let mut rt = ConversationRuntime::with_defaults();
        // Simulate having used 90_000 tokens (> 0.8 × 100_000).
        rt.total_input_tokens = 60_000;
        rt.total_output_tokens = 30_000;
        assert!(rt.should_compact(100_000));
    }

    #[test]
    fn test_should_compact_exactly_at_threshold() {
        let mut rt = ConversationRuntime::with_defaults();
        // Exactly 80 000 tokens with threshold 0.8 × 100 000 = 80 000 → true.
        rt.total_input_tokens = 40_000;
        rt.total_output_tokens = 40_000;
        assert!(rt.should_compact(100_000));
    }

    // -----------------------------------------------------------------------
    // Test: stats calculation
    // -----------------------------------------------------------------------

    #[test]
    fn test_stats_after_single_turn() {
        let mut rt = ConversationRuntime::with_defaults();
        let llm = TextOnlyLlm {
            text: "ok",
            input_tokens: 10,
            output_tokens: 5,
        };
        rt.execute_turn("Hello", "System", &[], &llm, &());

        let stats = rt.stats();
        assert_eq!(stats.turn_count, 1);
        assert_eq!(stats.total_input_tokens, 10);
        assert_eq!(stats.total_output_tokens, 5);
        // avg = (10 + 5) / 1 = 15.0
        assert!((stats.avg_tokens_per_turn - 15.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // Test: total_tokens accumulates across turns
    // -----------------------------------------------------------------------

    #[test]
    fn test_total_tokens_accumulates_across_turns() {
        let mut rt = ConversationRuntime::with_defaults();
        let llm = TextOnlyLlm {
            text: "reply",
            input_tokens: 10,
            output_tokens: 5,
        };
        rt.execute_turn("Turn 1", "System", &[], &llm, &());
        rt.execute_turn("Turn 2", "System", &[], &llm, &());

        let (inp, out) = rt.total_tokens();
        assert_eq!(inp, 20); // 2 × 10
        assert_eq!(out, 10); // 2 × 5
        assert_eq!(rt.turn_count(), 2);
    }

    // -----------------------------------------------------------------------
    // Test: Error event on LLM failure
    // -----------------------------------------------------------------------

    #[test]
    fn test_error_event_on_llm_failure() {
        let mut rt = ConversationRuntime::with_defaults();
        let events = rt.execute_turn("Hi", "System", &[], &FailingLlm, &());

        assert!(
            matches!(events.last(), Some(RuntimeEvent::Error(_))),
            "Expected Error event as last event"
        );
        // Tokens should not accumulate on failure.
        let (inp, out) = rt.total_tokens();
        assert_eq!(inp, 0);
        assert_eq!(out, 0);
    }

    // -----------------------------------------------------------------------
    // Test: RuntimeStats avg_tokens_per_turn with no turns
    // -----------------------------------------------------------------------

    #[test]
    fn test_runtime_stats_avg_tokens_zero_turns() {
        let rt = ConversationRuntime::with_defaults();
        let stats = rt.stats();
        assert_eq!(stats.turn_count, 0);
        assert!((stats.avg_tokens_per_turn - 0.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // Test: RuntimeStats avg_tokens_per_turn across multiple turns
    // -----------------------------------------------------------------------

    #[test]
    fn test_runtime_stats_avg_tokens_multiple_turns() {
        let mut rt = ConversationRuntime::with_defaults();

        // Turn 1: 10 in + 5 out = 15
        let llm1 = TextOnlyLlm {
            text: "a",
            input_tokens: 10,
            output_tokens: 5,
        };
        rt.execute_turn("t1", "S", &[], &llm1, &());

        // Turn 2: 30 in + 10 out = 40
        let llm2 = TextOnlyLlm {
            text: "b",
            input_tokens: 30,
            output_tokens: 10,
        };
        rt.execute_turn("t2", "S", &[], &llm2, &());

        let stats = rt.stats();
        // total = 15 + 40 = 55, turns = 2, avg = 27.5
        assert!((stats.avg_tokens_per_turn - 27.5).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // Test: Usage events are emitted
    // -----------------------------------------------------------------------

    #[test]
    fn test_usage_events_are_emitted() {
        let mut rt = ConversationRuntime::with_defaults();
        let llm = TextOnlyLlm::new("hi");
        let events = rt.execute_turn("Hey", "System", &[], &llm, &());

        assert!(
            events.iter().any(|e| matches!(e, RuntimeEvent::Usage(_))),
            "Expected at least one Usage event"
        );
    }
}
