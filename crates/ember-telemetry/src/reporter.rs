//! Remote telemetry reporting
//!
//! This module handles sending anonymized telemetry data to the remote server
//! when the user has explicitly opted in to remote reporting.

use crate::events::TelemetryEvent;
use crate::TelemetryError;

/// Remote telemetry reporter
pub struct TelemetryReporter {
    /// Remote endpoint URL
    endpoint: String,
    /// HTTP client
    client: reqwest::Client,
    /// Whether reporting is enabled
    enabled: bool,
}

impl TelemetryReporter {
    /// Create a new telemetry reporter
    pub fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            client: reqwest::Client::new(),
            enabled: true,
        }
    }

    /// Set whether reporting is enabled
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if reporting is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Report a batch of events to the remote server
    pub async fn report_batch(&self, events: &[TelemetryEvent]) -> Result<(), TelemetryError> {
        if !self.enabled || events.is_empty() {
            return Ok(());
        }

        let response = self
            .client
            .post(&self.endpoint)
            .json(events)
            .send()
            .await
            .map_err(|e| TelemetryError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(TelemetryError::Network(format!(
                "Server returned status: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Report a single event to the remote server
    pub async fn report(&self, event: &TelemetryEvent) -> Result<(), TelemetryError> {
        self.report_batch(&[event.clone()]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reporter_creation() {
        let reporter = TelemetryReporter::new("https://example.com/telemetry".to_string());
        assert!(reporter.is_enabled());
    }

    #[test]
    fn test_reporter_disable() {
        let mut reporter = TelemetryReporter::new("https://example.com/telemetry".to_string());
        reporter.set_enabled(false);
        assert!(!reporter.is_enabled());
    }
}
