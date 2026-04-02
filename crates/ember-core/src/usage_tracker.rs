//! Usage Cost Tracker — per-session token accounting and USD cost estimation
//!
//! Provides:
//! - Model pricing table for common Claude and OpenAI models
//! - Per-turn cost calculation from `TokenUsage`
//! - Session-level accumulation with a human-readable summary

use ember_llm::TokenUsage;

/// Pricing for a single model, expressed in USD per 1 million tokens.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelPricing {
    /// Cost per 1M input (prompt) tokens in USD
    pub input_cost_per_million: f64,
    /// Cost per 1M output (completion) tokens in USD
    pub output_cost_per_million: f64,
    /// Cost per 1M tokens written to the prompt cache in USD
    pub cache_creation_cost_per_million: f64,
    /// Cost per 1M tokens read from the prompt cache in USD
    pub cache_read_cost_per_million: f64,
}

/// Return the pricing for `model`.
///
/// Matching is done via substring/prefix so that versioned model IDs like
/// `claude-3-5-sonnet-20241022` are still handled correctly.
/// Unknown models fall back to a zero-cost sentinel so callers never panic.
pub fn pricing_for_model(model: &str) -> ModelPricing {
    let m = model.to_lowercase();

    // ---- Claude ----
    // Opus family (claude-3-opus, claude-4-opus, …)
    if m.contains("claude") && m.contains("opus") {
        return ModelPricing {
            input_cost_per_million: 15.0,
            output_cost_per_million: 75.0,
            cache_creation_cost_per_million: 18.75, // 1.25× input
            cache_read_cost_per_million: 1.50,      // 0.1× input
        };
    }

    // Sonnet family (claude-3-5-sonnet, claude-4-sonnet, …)
    if m.contains("claude") && m.contains("sonnet") {
        return ModelPricing {
            input_cost_per_million: 3.0,
            output_cost_per_million: 15.0,
            cache_creation_cost_per_million: 3.75,
            cache_read_cost_per_million: 0.30,
        };
    }

    // Haiku family (claude-3-haiku, …)
    if m.contains("claude") && m.contains("haiku") {
        return ModelPricing {
            input_cost_per_million: 1.0,
            output_cost_per_million: 5.0,
            cache_creation_cost_per_million: 1.25,
            cache_read_cost_per_million: 0.10,
        };
    }

    // ---- OpenAI ----
    if m.contains("gpt-4o-mini") {
        return ModelPricing {
            input_cost_per_million: 0.15,
            output_cost_per_million: 0.60,
            cache_creation_cost_per_million: 0.0,
            cache_read_cost_per_million: 0.075,
        };
    }

    if m.contains("gpt-4-turbo") || m.contains("gpt-4-turbo-preview") {
        return ModelPricing {
            input_cost_per_million: 10.0,
            output_cost_per_million: 30.0,
            cache_creation_cost_per_million: 0.0,
            cache_read_cost_per_million: 0.0,
        };
    }

    // gpt-4o (must come after gpt-4o-mini)
    if m.contains("gpt-4o") {
        return ModelPricing {
            input_cost_per_million: 2.50,
            output_cost_per_million: 10.0,
            cache_creation_cost_per_million: 0.0,
            cache_read_cost_per_million: 1.25,
        };
    }

    // ---- Default fallback ----
    ModelPricing {
        input_cost_per_million: 0.0,
        output_cost_per_million: 0.0,
        cache_creation_cost_per_million: 0.0,
        cache_read_cost_per_million: 0.0,
    }
}

// ---- Cost estimate -------------------------------------------------------

/// Estimated USD cost broken down by token category.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UsageCostEstimate {
    /// Cost attributed to input (prompt) tokens
    pub input_cost_usd: f64,
    /// Cost attributed to output (completion) tokens
    pub output_cost_usd: f64,
    /// Cost attributed to cache-creation tokens
    pub cache_creation_cost_usd: f64,
    /// Cost attributed to cache-read tokens
    pub cache_read_cost_usd: f64,
}

impl UsageCostEstimate {
    /// Sum of all cost components.
    pub fn total_cost_usd(&self) -> f64 {
        self.input_cost_usd
            + self.output_cost_usd
            + self.cache_creation_cost_usd
            + self.cache_read_cost_usd
    }

    /// Add another estimate to this one (used for session accumulation).
    fn add(&self, other: &UsageCostEstimate) -> UsageCostEstimate {
        UsageCostEstimate {
            input_cost_usd: self.input_cost_usd + other.input_cost_usd,
            output_cost_usd: self.output_cost_usd + other.output_cost_usd,
            cache_creation_cost_usd: self.cache_creation_cost_usd + other.cache_creation_cost_usd,
            cache_read_cost_usd: self.cache_read_cost_usd + other.cache_read_cost_usd,
        }
    }
}

fn compute_cost(pricing: &ModelPricing, usage: &TokenUsage) -> UsageCostEstimate {
    let per_million = 1_000_000.0_f64;

    let input_cost_usd =
        (usage.prompt_tokens as f64 / per_million) * pricing.input_cost_per_million;
    let output_cost_usd =
        (usage.completion_tokens as f64 / per_million) * pricing.output_cost_per_million;
    let cache_creation_cost_usd = usage
        .cache_creation_tokens
        .map(|t| (t as f64 / per_million) * pricing.cache_creation_cost_per_million)
        .unwrap_or(0.0);
    let cache_read_cost_usd = usage
        .cache_read_tokens
        .map(|t| (t as f64 / per_million) * pricing.cache_read_cost_per_million)
        .unwrap_or(0.0);

    UsageCostEstimate {
        input_cost_usd,
        output_cost_usd,
        cache_creation_cost_usd,
        cache_read_cost_usd,
    }
}

// ---- Per-turn usage ------------------------------------------------------

/// Token counts and derived cost for a single API call.
#[derive(Debug, Clone)]
pub struct TurnUsage {
    /// Input (prompt) tokens consumed
    pub input_tokens: u32,
    /// Output (completion) tokens generated
    pub output_tokens: u32,
    /// Tokens written to the prompt cache
    pub cache_creation_tokens: u32,
    /// Tokens read from the prompt cache
    pub cache_read_tokens: u32,
    /// Estimated USD cost for this turn
    pub cost: UsageCostEstimate,
}

// ---- Session tracker -----------------------------------------------------

/// Accumulates per-turn token usage and cost across an entire chat session.
///
/// # Example
/// ```
/// use ember_core::usage_tracker::SessionUsageTracker;
/// use ember_llm::TokenUsage;
///
/// let mut tracker = SessionUsageTracker::new("claude-3-5-sonnet-20241022");
/// tracker.record_turn(TokenUsage::new(500, 200));
/// tracker.record_turn(TokenUsage::new(300, 150));
///
/// println!("{}", tracker.format_summary());
/// ```
pub struct SessionUsageTracker {
    turns: Vec<TurnUsage>,
    model: String,
}

impl SessionUsageTracker {
    /// Create a new tracker for the given model name.
    pub fn new(model: &str) -> Self {
        Self {
            turns: Vec::new(),
            model: model.to_string(),
        }
    }

    /// Record a completed turn from a `TokenUsage` returned by the provider.
    pub fn record_turn(&mut self, usage: TokenUsage) {
        let pricing = pricing_for_model(&self.model);
        let cost = compute_cost(&pricing, &usage);

        self.turns.push(TurnUsage {
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
            cache_creation_tokens: usage.cache_creation_tokens.unwrap_or(0),
            cache_read_tokens: usage.cache_read_tokens.unwrap_or(0),
            cost,
        });
    }

    /// Aggregated cost across all recorded turns.
    pub fn total_cost(&self) -> UsageCostEstimate {
        self.turns
            .iter()
            .fold(UsageCostEstimate::default(), |acc, t| acc.add(&t.cost))
    }

    /// `(total_input_tokens, total_output_tokens)` across all turns.
    pub fn total_tokens(&self) -> (u32, u32) {
        let input = self.turns.iter().map(|t| t.input_tokens).sum();
        let output = self.turns.iter().map(|t| t.output_tokens).sum();
        (input, output)
    }

    /// Number of turns recorded so far.
    pub fn turn_count(&self) -> usize {
        self.turns.len()
    }

    /// Returns a slice of all recorded turns.
    pub fn turns(&self) -> &[TurnUsage] {
        &self.turns
    }

    /// Human-readable one-liner: `"3 turns | 12.4K tokens | $0.04"`.
    ///
    /// Token count is the combined input + output total; values ≥ 1 000 are
    /// formatted with one decimal place and a `K` suffix.
    pub fn format_summary(&self) -> String {
        let (input, output) = self.total_tokens();
        let total_tokens = input + output;
        let cost = self.total_cost().total_cost_usd();

        let token_str = if total_tokens >= 1_000 {
            format!("{:.1}K", total_tokens as f64 / 1_000.0)
        } else {
            total_tokens.to_string()
        };

        format!(
            "{} turns | {} tokens | ${:.2}",
            self.turns.len(),
            token_str,
            cost,
        )
    }
}

// ---- Tests ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Pricing lookup --------------------------------------------------

    #[test]
    fn pricing_claude_haiku() {
        let p = pricing_for_model("claude-3-haiku-20240307");
        assert_eq!(p.input_cost_per_million, 1.0);
        assert_eq!(p.output_cost_per_million, 5.0);
    }

    #[test]
    fn pricing_claude_sonnet_v35() {
        let p = pricing_for_model("claude-3-5-sonnet-20241022");
        assert_eq!(p.input_cost_per_million, 3.0);
        assert_eq!(p.output_cost_per_million, 15.0);
    }

    #[test]
    fn pricing_claude_opus() {
        let p = pricing_for_model("claude-3-opus-20240229");
        assert_eq!(p.input_cost_per_million, 15.0);
        assert_eq!(p.output_cost_per_million, 75.0);
    }

    #[test]
    fn pricing_gpt4o() {
        let p = pricing_for_model("gpt-4o");
        assert_eq!(p.input_cost_per_million, 2.50);
        assert_eq!(p.output_cost_per_million, 10.0);
    }

    #[test]
    fn pricing_gpt4o_mini() {
        let p = pricing_for_model("gpt-4o-mini");
        assert_eq!(p.input_cost_per_million, 0.15);
        assert_eq!(p.output_cost_per_million, 0.60);
    }

    #[test]
    fn pricing_gpt4_turbo() {
        let p = pricing_for_model("gpt-4-turbo");
        assert_eq!(p.input_cost_per_million, 10.0);
        assert_eq!(p.output_cost_per_million, 30.0);
    }

    #[test]
    fn pricing_fallback_unknown_model() {
        let p = pricing_for_model("totally-unknown-model-xyz");
        assert_eq!(p.input_cost_per_million, 0.0);
        assert_eq!(p.output_cost_per_million, 0.0);
        assert_eq!(p.cache_creation_cost_per_million, 0.0);
        assert_eq!(p.cache_read_cost_per_million, 0.0);
    }

    // ---- Cost calculation ------------------------------------------------

    #[test]
    fn cost_calculation_correct() {
        // 1M input @ $3/M  = $3.00
        // 500K output @ $15/M = $7.50
        let usage = TokenUsage {
            prompt_tokens: 1_000_000,
            completion_tokens: 500_000,
            total_tokens: 1_500_000,
            cache_creation_tokens: None,
            cache_read_tokens: None,
        };
        let pricing = pricing_for_model("claude-3-5-sonnet-20241022");
        let cost = compute_cost(&pricing, &usage);
        assert!((cost.input_cost_usd - 3.0).abs() < 1e-9);
        assert!((cost.output_cost_usd - 7.5).abs() < 1e-9);
        assert!((cost.total_cost_usd() - 10.5).abs() < 1e-9);
    }

    #[test]
    fn cache_costs_correct() {
        // 100K cache-creation tokens, 200K cache-read tokens on Sonnet
        // creation: 100_000 / 1_000_000 * 3.75 = 0.375
        // read:     200_000 / 1_000_000 * 0.30 = 0.060
        let usage = TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cache_creation_tokens: Some(100_000),
            cache_read_tokens: Some(200_000),
        };
        let pricing = pricing_for_model("claude-3-5-sonnet-20241022");
        let cost = compute_cost(&pricing, &usage);
        assert!((cost.cache_creation_cost_usd - 0.375).abs() < 1e-9);
        assert!((cost.cache_read_cost_usd - 0.060).abs() < 1e-9);
    }

    // ---- Session tracking ------------------------------------------------

    #[test]
    fn session_tracking_multiple_turns() {
        let mut tracker = SessionUsageTracker::new("gpt-4o-mini");
        tracker.record_turn(TokenUsage::new(1000, 500));
        tracker.record_turn(TokenUsage::new(2000, 800));
        tracker.record_turn(TokenUsage::new(500, 200));

        assert_eq!(tracker.turn_count(), 3);
        let (inp, out) = tracker.total_tokens();
        assert_eq!(inp, 3_500);
        assert_eq!(out, 1_500);
    }

    #[test]
    fn total_tokens_sum_correct() {
        let mut tracker = SessionUsageTracker::new("gpt-4o");
        tracker.record_turn(TokenUsage::new(100, 50));
        tracker.record_turn(TokenUsage::new(200, 100));

        let (inp, out) = tracker.total_tokens();
        assert_eq!(inp, 300);
        assert_eq!(out, 150);
        assert_eq!(inp + out, 450);
    }

    #[test]
    fn empty_session() {
        let tracker = SessionUsageTracker::new("claude-3-5-sonnet-20241022");
        assert_eq!(tracker.turn_count(), 0);
        let (inp, out) = tracker.total_tokens();
        assert_eq!(inp, 0);
        assert_eq!(out, 0);
        assert_eq!(tracker.total_cost().total_cost_usd(), 0.0);
        assert_eq!(tracker.format_summary(), "0 turns | 0 tokens | $0.00");
    }

    #[test]
    fn format_summary_output() {
        let mut tracker = SessionUsageTracker::new("claude-3-5-sonnet-20241022");
        // 3 turns, total: 500+300+200 = 1000 input, 200+150+100 = 450 output = 1450 tokens
        // 1450 / 1000 = 1.45 → {:.1} rounds to "1.4" (round-half-to-even)
        // cost: (1000/1M)*3 + (450/1M)*15 = 0.003 + 0.00675 = 0.00975 → "$0.01"
        tracker.record_turn(TokenUsage::new(500, 200));
        tracker.record_turn(TokenUsage::new(300, 150));
        tracker.record_turn(TokenUsage::new(200, 100));

        let summary = tracker.format_summary();
        assert!(summary.starts_with("3 turns"), "summary: {}", summary);
        assert!(summary.contains("1.4K tokens"), "summary: {}", summary);
        assert!(summary.contains('$'), "summary: {}", summary);
    }

    #[test]
    fn total_cost_accumulates_across_turns() {
        let mut tracker = SessionUsageTracker::new("gpt-4o");
        // gpt-4o: input $2.50/M, output $10/M
        // turn 1: 1M input → $2.50, 0 output → $0
        // turn 2: 0 input → $0, 1M output → $10
        let big_input = TokenUsage {
            prompt_tokens: 1_000_000,
            completion_tokens: 0,
            total_tokens: 1_000_000,
            cache_creation_tokens: None,
            cache_read_tokens: None,
        };
        let big_output = TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 1_000_000,
            total_tokens: 1_000_000,
            cache_creation_tokens: None,
            cache_read_tokens: None,
        };
        tracker.record_turn(big_input);
        tracker.record_turn(big_output);

        let total = tracker.total_cost();
        assert!((total.input_cost_usd - 2.50).abs() < 1e-9);
        assert!((total.output_cost_usd - 10.0).abs() < 1e-9);
        assert!((total.total_cost_usd() - 12.50).abs() < 1e-9);
    }
}
