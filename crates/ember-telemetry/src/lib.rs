//! Ember Telemetry - Privacy-First, Opt-In Usage Analytics
//!
//! This crate provides anonymous, opt-in telemetry for Ember AI.
//!
//! ## Privacy Principles
//!
//! 1. **Opt-in by default** - Telemetry is disabled unless explicitly enabled
//! 2. **No PII collection** - No personal identifiable information is ever collected
//! 3. **Local-first** - All data is stored locally first, remote reporting is optional
//! 4. **Transparent** - Users can view all collected data at any time
//! 5. **Minimal data** - Only collect what's necessary for improving the product
//! 6. **User control** - Users can delete their data at any time
//!
//! ## What We Collect (when enabled)
//!
//! - Anonymous session IDs (hashed, not trackable)
//! - Command usage counts (e.g., "chat", "agent", "config")
//! - Provider usage (which LLM providers are used, not API keys)
//! - Error types (not error messages or stack traces)
//! - Feature usage (which features are popular)
//! - Performance metrics (latency buckets, not exact times)
//! - Version information
//!
//! ## What We Never Collect
//!
//! - Prompts or messages
//! - API keys or credentials
//! - File contents or paths
//! - IP addresses
//! - Personal information
//! - Conversation history

pub mod anonymizer;
pub mod collector;
pub mod error;
pub mod events;
pub mod storage;

#[cfg(feature = "remote")]
pub mod reporter;

pub use collector::TelemetryCollector;
pub use error::TelemetryError;
pub use events::*;

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Telemetry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Whether telemetry is enabled (default: false)
    pub enabled: bool,

    /// Whether to report to remote server (default: false)
    #[serde(default)]
    pub remote_reporting: bool,

    /// Remote endpoint URL (if remote_reporting is enabled)
    #[serde(default)]
    pub remote_endpoint: Option<String>,

    /// How often to flush events to storage (in seconds)
    #[serde(default = "default_flush_interval")]
    pub flush_interval_secs: u64,

    /// Maximum events to keep locally
    #[serde(default = "default_max_events")]
    pub max_local_events: usize,

    /// Event categories to collect
    #[serde(default = "default_categories")]
    pub categories: Vec<EventCategory>,
}

fn default_flush_interval() -> u64 {
    60 // 1 minute
}

fn default_max_events() -> usize {
    10000
}

fn default_categories() -> Vec<EventCategory> {
    vec![
        EventCategory::Usage,
        EventCategory::Error,
        EventCategory::Performance,
    ]
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default!
            remote_reporting: false,
            remote_endpoint: None,
            flush_interval_secs: default_flush_interval(),
            max_local_events: default_max_events(),
            categories: default_categories(),
        }
    }
}

/// Event categories for filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventCategory {
    /// Command and feature usage
    Usage,
    /// Error occurrences (types only)
    Error,
    /// Performance metrics
    Performance,
    /// Session lifecycle
    Session,
    /// Provider usage
    Provider,
}

/// The main telemetry service
pub struct Telemetry {
    config: Arc<RwLock<TelemetryConfig>>,
    collector: Arc<TelemetryCollector>,
}

impl Telemetry {
    /// Create a new telemetry instance with the given configuration
    pub async fn new(config: TelemetryConfig) -> Result<Self, TelemetryError> {
        let collector = TelemetryCollector::new(config.clone()).await?;

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            collector: Arc::new(collector),
        })
    }

    /// Create a disabled telemetry instance (no-op)
    pub fn disabled() -> Self {
        // Create a minimal instance that does nothing
        Self {
            config: Arc::new(RwLock::new(TelemetryConfig::default())),
            collector: Arc::new(TelemetryCollector::disabled()),
        }
    }

    /// Check if telemetry is enabled
    pub async fn is_enabled(&self) -> bool {
        self.config.read().await.enabled
    }

    /// Enable telemetry
    pub async fn enable(&self) {
        self.config.write().await.enabled = true;
        self.collector.set_enabled(true).await;
        tracing::info!("Telemetry enabled");
    }

    /// Disable telemetry
    pub async fn disable(&self) {
        self.config.write().await.enabled = false;
        self.collector.set_enabled(false).await;
        tracing::info!("Telemetry disabled");
    }

    /// Record a command usage event
    pub async fn record_command(&self, command: &str) {
        if !self.is_enabled().await {
            return;
        }

        self.collector
            .record(TelemetryEvent::command_used(command))
            .await;
    }

    /// Record a provider usage event
    pub async fn record_provider_used(&self, provider: &str, model: &str) {
        if !self.is_enabled().await {
            return;
        }

        self.collector
            .record(TelemetryEvent::provider_used(provider, model))
            .await;
    }

    /// Record an error occurrence
    pub async fn record_error(&self, error_type: &str, recoverable: bool) {
        if !self.is_enabled().await {
            return;
        }

        self.collector
            .record(TelemetryEvent::error_occurred(error_type, recoverable))
            .await;
    }

    /// Record a latency metric
    pub async fn record_latency(&self, operation: &str, latency_ms: u64) {
        if !self.is_enabled().await {
            return;
        }

        self.collector
            .record(TelemetryEvent::latency_recorded(operation, latency_ms))
            .await;
    }

    /// Record a feature usage
    pub async fn record_feature(&self, feature: &str) {
        if !self.is_enabled().await {
            return;
        }

        self.collector
            .record(TelemetryEvent::feature_used(feature))
            .await;
    }

    /// Record a session start
    pub async fn record_session_start(&self) {
        if !self.is_enabled().await {
            return;
        }

        self.collector
            .record(TelemetryEvent::session_started())
            .await;
    }

    /// Record a session end
    pub async fn record_session_end(&self, duration_secs: u64) {
        if !self.is_enabled().await {
            return;
        }

        self.collector
            .record(TelemetryEvent::session_ended(duration_secs))
            .await;
    }

    /// Get collected statistics
    pub async fn get_statistics(&self) -> TelemetryStats {
        self.collector.get_statistics().await
    }

    /// Get all collected events (for transparency)
    pub async fn get_all_events(&self) -> Vec<TelemetryEvent> {
        self.collector.get_all_events().await
    }

    /// Delete all collected telemetry data
    pub async fn delete_all_data(&self) -> Result<(), TelemetryError> {
        self.collector.delete_all_data().await
    }

    /// Export telemetry data as JSON
    pub async fn export_data(&self) -> Result<String, TelemetryError> {
        let events = self.get_all_events().await;
        serde_json::to_string_pretty(&events)
            .map_err(|e| TelemetryError::Serialization(e.to_string()))
    }

    /// Flush pending events to storage
    pub async fn flush(&self) -> Result<(), TelemetryError> {
        self.collector.flush().await
    }
}

/// Aggregated telemetry statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryStats {
    /// Total number of events collected
    pub total_events: usize,

    /// Events by category
    pub events_by_category: std::collections::HashMap<String, usize>,

    /// Most used commands
    pub top_commands: Vec<(String, usize)>,

    /// Most used providers
    pub top_providers: Vec<(String, usize)>,

    /// Error counts by type
    pub error_counts: std::collections::HashMap<String, usize>,

    /// Average latencies by operation (in ms)
    pub avg_latencies: std::collections::HashMap<String, u64>,

    /// Session count
    pub session_count: usize,

    /// Average session duration (in seconds)
    pub avg_session_duration: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_telemetry_disabled_by_default() {
        let telemetry = Telemetry::disabled();
        assert!(!telemetry.is_enabled().await);
    }

    #[tokio::test]
    async fn test_telemetry_enable_disable() {
        let config = TelemetryConfig::default();
        let telemetry = Telemetry::new(config).await.unwrap();

        assert!(!telemetry.is_enabled().await);

        telemetry.enable().await;
        assert!(telemetry.is_enabled().await);

        telemetry.disable().await;
        assert!(!telemetry.is_enabled().await);
    }
}
