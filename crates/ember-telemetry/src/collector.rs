//! Telemetry event collector
//!
//! Collects events in memory and flushes to storage periodically.

use crate::{
    storage::TelemetryStorage, EventData, TelemetryConfig, TelemetryError, TelemetryEvent,
    TelemetryStats,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Collects and manages telemetry events
pub struct TelemetryCollector {
    /// Whether collection is enabled
    enabled: Arc<AtomicBool>,

    /// In-memory event buffer
    buffer: Arc<RwLock<Vec<TelemetryEvent>>>,

    /// Persistent storage
    storage: Arc<RwLock<Option<TelemetryStorage>>>,

    /// Configuration
    config: TelemetryConfig,
}

impl TelemetryCollector {
    /// Create a new collector
    pub async fn new(config: TelemetryConfig) -> Result<Self, TelemetryError> {
        let storage = TelemetryStorage::new().await?;

        Ok(Self {
            enabled: Arc::new(AtomicBool::new(config.enabled)),
            buffer: Arc::new(RwLock::new(Vec::new())),
            storage: Arc::new(RwLock::new(Some(storage))),
            config,
        })
    }

    /// Create a disabled collector (no-op)
    pub fn disabled() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(false)),
            buffer: Arc::new(RwLock::new(Vec::new())),
            storage: Arc::new(RwLock::new(None)),
            config: TelemetryConfig::default(),
        }
    }

    /// Set enabled state
    pub async fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Record an event
    pub async fn record(&self, event: TelemetryEvent) {
        if !self.is_enabled() {
            return;
        }

        // Check if this category is enabled
        if !self.config.categories.contains(&event.category) {
            return;
        }

        let mut buffer = self.buffer.write().await;
        buffer.push(event);

        // Auto-flush if buffer is getting large
        if buffer.len() >= 100 {
            drop(buffer); // Release lock
            let _ = self.flush().await;
        }
    }

    /// Flush events to storage
    pub async fn flush(&self) -> Result<(), TelemetryError> {
        let mut buffer = self.buffer.write().await;
        if buffer.is_empty() {
            return Ok(());
        }

        let events: Vec<_> = buffer.drain(..).collect();
        drop(buffer); // Release lock before storage operation

        let storage_guard = self.storage.read().await;
        if let Some(storage) = storage_guard.as_ref() {
            storage.append_events(&events).await?;
        }

        Ok(())
    }

    /// Get all events (from buffer and storage)
    pub async fn get_all_events(&self) -> Vec<TelemetryEvent> {
        let mut events = Vec::new();

        // Get from storage first
        let storage_guard = self.storage.read().await;
        if let Some(storage) = storage_guard.as_ref() {
            if let Ok(stored) = storage.load_events().await {
                events.extend(stored);
            }
        }
        drop(storage_guard);

        // Add buffered events
        let buffer = self.buffer.read().await;
        events.extend(buffer.iter().cloned());

        events
    }

    /// Delete all collected data
    pub async fn delete_all_data(&self) -> Result<(), TelemetryError> {
        // Clear buffer
        self.buffer.write().await.clear();

        // Clear storage
        let storage_guard = self.storage.read().await;
        if let Some(storage) = storage_guard.as_ref() {
            storage.clear().await?;
        }

        Ok(())
    }

    /// Get aggregated statistics
    pub async fn get_statistics(&self) -> TelemetryStats {
        let events = self.get_all_events().await;

        let mut stats = TelemetryStats {
            total_events: events.len(),
            ..Default::default()
        };

        let mut command_counts: HashMap<String, usize> = HashMap::new();
        let mut provider_counts: HashMap<String, usize> = HashMap::new();
        let mut error_counts: HashMap<String, usize> = HashMap::new();
        let mut latency_totals: HashMap<String, (u64, usize)> = HashMap::new();
        let mut session_count = 0;
        let mut total_session_duration = 0u64;
        let mut session_duration_count = 0;

        for event in &events {
            // Count by category
            let category = format!("{:?}", event.category);
            *stats.events_by_category.entry(category).or_insert(0) += 1;

            // Process event data
            match &event.data {
                EventData::CommandUsed { command } => {
                    *command_counts.entry(command.clone()).or_insert(0) += 1;
                }
                EventData::ProviderUsed { provider, .. } => {
                    *provider_counts.entry(provider.clone()).or_insert(0) += 1;
                }
                EventData::ErrorOccurred { error_type, .. } => {
                    *error_counts.entry(error_type.clone()).or_insert(0) += 1;
                }
                EventData::LatencyRecorded {
                    operation,
                    latency_bucket,
                } => {
                    // Parse bucket to get approximate latency
                    let approx_latency = parse_latency_bucket(latency_bucket);
                    let entry = latency_totals.entry(operation.clone()).or_insert((0, 0));
                    entry.0 += approx_latency;
                    entry.1 += 1;
                }
                EventData::SessionStarted => {
                    session_count += 1;
                }
                EventData::SessionEnded { duration_bucket } => {
                    let approx_duration = parse_duration_bucket(duration_bucket);
                    total_session_duration += approx_duration;
                    session_duration_count += 1;
                }
                _ => {}
            }
        }

        // Sort and take top items
        let mut commands: Vec<_> = command_counts.into_iter().collect();
        commands.sort_by(|a, b| b.1.cmp(&a.1));
        stats.top_commands = commands.into_iter().take(10).collect();

        let mut providers: Vec<_> = provider_counts.into_iter().collect();
        providers.sort_by(|a, b| b.1.cmp(&a.1));
        stats.top_providers = providers.into_iter().take(10).collect();

        stats.error_counts = error_counts;

        // Calculate average latencies
        for (op, (total, count)) in latency_totals {
            if count > 0 {
                stats.avg_latencies.insert(op, total / count as u64);
            }
        }

        stats.session_count = session_count;
        if session_duration_count > 0 {
            stats.avg_session_duration = total_session_duration / session_duration_count as u64;
        }

        stats
    }
}

/// Parse latency bucket to get approximate value in ms
fn parse_latency_bucket(bucket: &str) -> u64 {
    match bucket {
        "0-100ms" => 50,
        "100-500ms" => 300,
        "500ms-1s" => 750,
        "1-5s" => 3000,
        "5-10s" => 7500,
        "10-30s" => 20000,
        ">30s" => 45000,
        _ => 0,
    }
}

/// Parse duration bucket to get approximate value in seconds
fn parse_duration_bucket(bucket: &str) -> u64 {
    match bucket {
        "0-1min" => 30,
        "1-5min" => 180,
        "5-15min" => 600,
        "15-30min" => 1350,
        "30min-1h" => 2700,
        "1-2h" => 5400,
        ">2h" => 10800,
        _ => 0,
    }
}
