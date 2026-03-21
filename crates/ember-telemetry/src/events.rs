//! Telemetry event definitions
//!
//! All events are designed to be privacy-preserving and contain no PII.

use crate::EventCategory;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A telemetry event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    /// Unique event ID
    pub id: String,

    /// Event timestamp (UTC)
    pub timestamp: DateTime<Utc>,

    /// Event category
    pub category: EventCategory,

    /// Event type name
    pub event_type: String,

    /// Event data (varies by type)
    pub data: EventData,

    /// Ember version
    pub version: String,

    /// Platform (os type only, no details)
    pub platform: String,
}

impl TelemetryEvent {
    /// Create a new event with the given type and data
    fn new(category: EventCategory, event_type: &str, data: EventData) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            category,
            event_type: event_type.to_string(),
            data,
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
        }
    }

    /// Create a command usage event
    pub fn command_used(command: &str) -> Self {
        Self::new(
            EventCategory::Usage,
            "command_used",
            EventData::CommandUsed {
                command: command.to_string(),
            },
        )
    }

    /// Create a provider usage event
    pub fn provider_used(provider: &str, model: &str) -> Self {
        Self::new(
            EventCategory::Provider,
            "provider_used",
            EventData::ProviderUsed {
                provider: provider.to_string(),
                model: anonymize_model_name(model),
            },
        )
    }

    /// Create an error occurrence event
    pub fn error_occurred(error_type: &str, recoverable: bool) -> Self {
        Self::new(
            EventCategory::Error,
            "error_occurred",
            EventData::ErrorOccurred {
                error_type: error_type.to_string(),
                recoverable,
            },
        )
    }

    /// Create a latency metric event
    pub fn latency_recorded(operation: &str, latency_ms: u64) -> Self {
        Self::new(
            EventCategory::Performance,
            "latency_recorded",
            EventData::LatencyRecorded {
                operation: operation.to_string(),
                latency_bucket: bucket_latency(latency_ms),
            },
        )
    }

    /// Create a feature usage event
    pub fn feature_used(feature: &str) -> Self {
        Self::new(
            EventCategory::Usage,
            "feature_used",
            EventData::FeatureUsed {
                feature: feature.to_string(),
            },
        )
    }

    /// Create a session started event
    pub fn session_started() -> Self {
        Self::new(
            EventCategory::Session,
            "session_started",
            EventData::SessionStarted,
        )
    }

    /// Create a session ended event
    pub fn session_ended(duration_secs: u64) -> Self {
        Self::new(
            EventCategory::Session,
            "session_ended",
            EventData::SessionEnded {
                duration_bucket: bucket_duration(duration_secs),
            },
        )
    }
}

/// Event data variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventData {
    /// Command was used
    CommandUsed { command: String },

    /// Provider was used
    ProviderUsed { provider: String, model: String },

    /// Error occurred (type only, no message)
    ErrorOccurred {
        error_type: String,
        recoverable: bool,
    },

    /// Latency was recorded
    LatencyRecorded {
        operation: String,
        /// Bucketed latency (e.g., "0-100ms", "100-500ms")
        latency_bucket: String,
    },

    /// Feature was used
    FeatureUsed { feature: String },

    /// Session started
    SessionStarted,

    /// Session ended
    SessionEnded {
        /// Bucketed duration (e.g., "0-1min", "1-5min")
        duration_bucket: String,
    },
}

/// Bucket latency values to protect exact timing information
fn bucket_latency(latency_ms: u64) -> String {
    match latency_ms {
        0..=100 => "0-100ms".to_string(),
        101..=500 => "100-500ms".to_string(),
        501..=1000 => "500ms-1s".to_string(),
        1001..=5000 => "1-5s".to_string(),
        5001..=10000 => "5-10s".to_string(),
        10001..=30000 => "10-30s".to_string(),
        _ => ">30s".to_string(),
    }
}

/// Bucket duration values to protect exact session information
fn bucket_duration(duration_secs: u64) -> String {
    match duration_secs {
        0..=60 => "0-1min".to_string(),
        61..=300 => "1-5min".to_string(),
        301..=900 => "5-15min".to_string(),
        901..=1800 => "15-30min".to_string(),
        1801..=3600 => "30min-1h".to_string(),
        3601..=7200 => "1-2h".to_string(),
        _ => ">2h".to_string(),
    }
}

/// Anonymize model names to remove any custom/personal identifiers
fn anonymize_model_name(model: &str) -> String {
    // Known model patterns - keep only the base model name
    // IMPORTANT: More specific (longer) patterns must come first to ensure correct matching
    // e.g., "gpt-4-turbo" must be checked before "gpt-4"
    let known_models = [
        "gpt-4-turbo",
        "gpt-4o",
        "gpt-4",
        "gpt-3.5",
        "claude-instant",
        "claude-3",
        "claude-2",
        "gemini-ultra",
        "gemini-pro",
        "codellama",
        "llama",
        "mixtral",
        "mistral",
        "deepseek",
        "qwen",
        "phi",
    ];

    let model_lower = model.to_lowercase();

    for known in known_models {
        if model_lower.contains(known) {
            return known.to_string();
        }
    }

    // For unknown models, just return "custom"
    "custom".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_latency() {
        assert_eq!(bucket_latency(50), "0-100ms");
        assert_eq!(bucket_latency(200), "100-500ms");
        assert_eq!(bucket_latency(750), "500ms-1s");
        assert_eq!(bucket_latency(3000), "1-5s");
        assert_eq!(bucket_latency(60000), ">30s");
    }

    #[test]
    fn test_bucket_duration() {
        assert_eq!(bucket_duration(30), "0-1min");
        assert_eq!(bucket_duration(120), "1-5min");
        assert_eq!(bucket_duration(600), "5-15min");
        assert_eq!(bucket_duration(5000), "1-2h");
    }

    #[test]
    fn test_anonymize_model_name() {
        assert_eq!(anonymize_model_name("gpt-4-turbo-preview"), "gpt-4-turbo");
        assert_eq!(anonymize_model_name("claude-3-sonnet-20240229"), "claude-3");
        assert_eq!(anonymize_model_name("my-custom-finetuned-model"), "custom");
        assert_eq!(anonymize_model_name("llama-3-70b-chat"), "llama");
    }

    #[test]
    fn test_event_creation() {
        let event = TelemetryEvent::command_used("chat");
        assert_eq!(event.category, EventCategory::Usage);
        assert_eq!(event.event_type, "command_used");

        let event = TelemetryEvent::provider_used("openai", "gpt-4-turbo-preview");
        assert_eq!(event.category, EventCategory::Provider);
        if let EventData::ProviderUsed { model, .. } = event.data {
            assert_eq!(model, "gpt-4-turbo");
        }
    }
}
