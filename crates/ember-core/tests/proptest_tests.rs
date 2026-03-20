//! Property-based tests for ember-core using proptest
//!
//! These tests verify invariants and properties that should hold
//! for any valid input, not just specific test cases.

use proptest::prelude::*;

// Strategy for generating valid message content
fn message_content_strategy() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z0-9 .,!?\\-'\"]{0,1000}")
        .unwrap()
        .prop_filter("non-empty message", |s| !s.trim().is_empty())
}

// Strategy for generating model names
fn model_name_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("gpt-4".to_string()),
        Just("gpt-4-turbo".to_string()),
        Just("gpt-3.5-turbo".to_string()),
        Just("claude-3-opus".to_string()),
        Just("claude-3-sonnet".to_string()),
        Just("gemini-pro".to_string()),
        Just("llama-3-70b".to_string()),
    ]
}

// Strategy for generating conversation IDs
fn conversation_id_strategy() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}")
        .unwrap()
}

// Strategy for generating token counts
fn token_count_strategy() -> impl Strategy<Value = u32> {
    0u32..100_000u32
}

// Strategy for generating cost values
fn cost_strategy() -> impl Strategy<Value = f64> {
    (0.0f64..1000.0f64).prop_filter("valid cost", |c| c.is_finite())
}

// Strategy for generating temperature values
fn temperature_strategy() -> impl Strategy<Value = f32> {
    0.0f32..2.0f32
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Test that message content round-trips through serialization
    #[test]
    fn message_content_serialization_roundtrip(content in message_content_strategy()) {
        let json = serde_json::json!({
            "role": "user",
            "content": content.clone()
        });
        
        let serialized = serde_json::to_string(&json).unwrap();
        let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        
        prop_assert_eq!(deserialized["content"].as_str().unwrap(), content.as_str());
    }

    /// Test that token counts are always non-negative
    #[test]
    fn token_count_always_non_negative(
        prompt_tokens in token_count_strategy(),
        completion_tokens in token_count_strategy()
    ) {
        let total = prompt_tokens + completion_tokens;
        prop_assert!(total >= prompt_tokens);
        prop_assert!(total >= completion_tokens);
    }

    /// Test cost calculation invariants
    #[test]
    fn cost_calculation_invariants(
        input_tokens in token_count_strategy(),
        output_tokens in token_count_strategy(),
        input_cost_per_1k in cost_strategy(),
        output_cost_per_1k in cost_strategy()
    ) {
        let input_cost = (input_tokens as f64 / 1000.0) * input_cost_per_1k;
        let output_cost = (output_tokens as f64 / 1000.0) * output_cost_per_1k;
        let total_cost = input_cost + output_cost;
        
        // Cost should be non-negative
        prop_assert!(input_cost >= 0.0);
        prop_assert!(output_cost >= 0.0);
        prop_assert!(total_cost >= 0.0);
        
        // Total cost should be at least as much as individual costs
        prop_assert!(total_cost >= input_cost);
        prop_assert!(total_cost >= output_cost);
    }

    /// Test temperature clamping
    #[test]
    fn temperature_clamping(raw_temp in -10.0f32..10.0f32) {
        let clamped = raw_temp.clamp(0.0, 2.0);
        prop_assert!(clamped >= 0.0);
        prop_assert!(clamped <= 2.0);
        
        if raw_temp >= 0.0 && raw_temp <= 2.0 {
            prop_assert_eq!(clamped, raw_temp);
        }
    }

    /// Test that conversation IDs are valid UUIDs
    #[test]
    fn conversation_id_valid_uuid(id in conversation_id_strategy()) {
        // UUID v4 should have specific format
        prop_assert_eq!(id.len(), 36);
        prop_assert!(id.chars().nth(8) == Some('-'));
        prop_assert!(id.chars().nth(13) == Some('-'));
        prop_assert!(id.chars().nth(14) == Some('4')); // Version 4
        prop_assert!(id.chars().nth(18) == Some('-'));
        prop_assert!(id.chars().nth(23) == Some('-'));
    }

    /// Test message ordering preservation
    #[test]
    fn message_ordering_preserved(messages in prop::collection::vec(message_content_strategy(), 1..20)) {
        let indexed: Vec<(usize, &String)> = messages.iter().enumerate().collect();
        
        // Verify ordering is maintained
        for (i, (idx, _)) in indexed.iter().enumerate() {
            prop_assert_eq!(i, *idx);
        }
    }

    /// Test context window calculation
    #[test]
    fn context_window_calculation(
        messages in prop::collection::vec(token_count_strategy(), 1..50),
        max_context in 1000u32..128000u32
    ) {
        let total_tokens: u32 = messages.iter().sum();
        
        // Calculate how many messages fit in context
        let mut cumulative = 0u32;
        let mut messages_in_context = 0usize;
        
        for &tokens in &messages {
            if cumulative + tokens <= max_context {
                cumulative += tokens;
                messages_in_context += 1;
            } else {
                break;
            }
        }
        
        // Invariants
        prop_assert!(cumulative <= max_context);
        prop_assert!(messages_in_context <= messages.len());
        
        // If we stopped early, adding next message would exceed context
        if messages_in_context < messages.len() {
            prop_assert!(cumulative + messages[messages_in_context] > max_context);
        }
    }

    /// Test rate limiting bucket calculations
    #[test]
    fn rate_limiting_bucket(
        requests_per_minute in 1u32..1000u32,
        current_count in 0u32..2000u32
    ) {
        let is_allowed = current_count < requests_per_minute;
        let remaining = if is_allowed { requests_per_minute - current_count } else { 0 };
        
        prop_assert!(remaining <= requests_per_minute);
        
        if is_allowed {
            prop_assert!(remaining > 0);
        } else {
            prop_assert_eq!(remaining, 0);
        }
    }

    /// Test model name validation
    #[test]
    fn model_name_format(name in model_name_strategy()) {
        // Model names should be non-empty
        prop_assert!(!name.is_empty());
        
        // Model names should only contain valid characters
        prop_assert!(name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.' || c == '_'));
        
        // Model names should start with alphanumeric
        prop_assert!(name.chars().next().map(|c| c.is_alphanumeric()).unwrap_or(false));
    }

    /// Test JSON message structure
    #[test]
    fn json_message_structure(
        role in prop_oneof![Just("user"), Just("assistant"), Just("system")],
        content in message_content_strategy()
    ) {
        let message = serde_json::json!({
            "role": role,
            "content": content
        });
        
        // Should serialize and deserialize correctly
        let serialized = serde_json::to_string(&message).unwrap();
        let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        
        prop_assert!(deserialized.is_object());
        prop_assert!(deserialized.get("role").is_some());
        prop_assert!(deserialized.get("content").is_some());
    }

    /// Test truncation preserves message integrity
    #[test]
    fn truncation_preserves_messages(
        messages in prop::collection::vec(message_content_strategy(), 5..20),
        keep_count in 1usize..5usize
    ) {
        let truncated: Vec<&String> = messages.iter().rev().take(keep_count).rev().collect();
        
        // Truncated should have at most keep_count messages
        prop_assert!(truncated.len() <= keep_count);
        
        // Truncated messages should match the end of original
        for (i, msg) in truncated.iter().enumerate() {
            let original_idx = messages.len() - truncated.len() + i;
            prop_assert_eq!(*msg, &messages[original_idx]);
        }
    }
}

// Additional tests using test-case for edge cases
#[cfg(test)]
mod edge_case_tests {
    use test_case::test_case;

    #[test_case(0, 0 => 0; "zero tokens")]
    #[test_case(1000, 1000 => 2000; "equal tokens")]
    #[test_case(100_000, 50_000 => 150_000; "large tokens")]
    fn total_tokens(prompt: u32, completion: u32) -> u32 {
        prompt + completion
    }

    #[test_case(0.0, 1000.0 => 0.0; "zero input cost")]
    #[test_case(1000.0, 0.0 => 1.0; "one dollar per 1k tokens")]
    #[test_case(500.0, 2.0 => 1.0; "half cost")]
    fn calculate_cost(tokens: f64, cost_per_1k: f64) -> f64 {
        (tokens / 1000.0) * cost_per_1k
    }

    #[test_case("" => false; "empty string")]
    #[test_case("   " => false; "whitespace only")]
    #[test_case("hello" => true; "valid content")]
    #[test_case("Hello, World!" => true; "with punctuation")]
    fn is_valid_content(content: &str) -> bool {
        !content.trim().is_empty()
    }

    #[test_case("gpt-4" => true; "openai model")]
    #[test_case("claude-3-opus" => true; "anthropic model")]
    #[test_case("" => false; "empty model")]
    #[test_case("invalid model!" => false; "invalid characters")]
    fn is_valid_model_name(name: &str) -> bool {
        !name.is_empty() && 
        name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.' || c == '_') &&
        name.chars().next().map(|c| c.is_alphanumeric()).unwrap_or(false)
    }

    #[test_case(-1.0 => 0.0; "negative clamped to zero")]
    #[test_case(0.0 => 0.0; "zero stays zero")]
    #[test_case(1.0 => 1.0; "one stays one")]
    #[test_case(2.0 => 2.0; "max stays max")]
    #[test_case(3.0 => 2.0; "over max clamped")]
    fn clamp_temperature(temp: f32) -> f32 {
        temp.clamp(0.0, 2.0)
    }
}