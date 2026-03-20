//! Local telemetry storage
//!
//! Stores events locally in a JSON file. Data is always stored locally first,
//! giving users full control over their telemetry data.

use crate::{TelemetryError, TelemetryEvent};
use directories::ProjectDirs;
use std::path::PathBuf;
use tokio::fs;
use tokio::sync::RwLock;

/// Local file-based telemetry storage
pub struct TelemetryStorage {
    /// Path to the storage file
    path: PathBuf,

    /// In-memory cache
    cache: RwLock<Vec<TelemetryEvent>>,
}

impl TelemetryStorage {
    /// Create a new storage instance
    pub async fn new() -> Result<Self, TelemetryError> {
        let path = Self::get_storage_path()?;

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Load existing events
        let events = if path.exists() {
            let content = fs::read_to_string(&path).await?;
            serde_json::from_str(&content).unwrap_or_else(|_| Vec::new())
        } else {
            Vec::new()
        };

        Ok(Self {
            path,
            cache: RwLock::new(events),
        })
    }

    /// Get the default storage path
    fn get_storage_path() -> Result<PathBuf, TelemetryError> {
        let proj_dirs = ProjectDirs::from("dev", "ember", "ember-ai")
            .ok_or_else(|| TelemetryError::Config("Could not determine data directory".into()))?;

        Ok(proj_dirs.data_dir().join("telemetry.json"))
    }

    /// Append events to storage
    pub async fn append_events(&self, events: &[TelemetryEvent]) -> Result<(), TelemetryError> {
        let mut cache = self.cache.write().await;
        cache.extend(events.iter().cloned());

        // Write to file
        self.save_to_file(&cache).await?;

        Ok(())
    }

    /// Load all events from storage
    pub async fn load_events(&self) -> Result<Vec<TelemetryEvent>, TelemetryError> {
        let cache = self.cache.read().await;
        Ok(cache.clone())
    }

    /// Clear all stored events
    pub async fn clear(&self) -> Result<(), TelemetryError> {
        let mut cache = self.cache.write().await;
        cache.clear();

        // Remove file
        if self.path.exists() {
            fs::remove_file(&self.path).await?;
        }

        Ok(())
    }

    /// Get storage path (for user transparency)
    pub fn storage_path(&self) -> &PathBuf {
        &self.path
    }

    /// Get storage size in bytes
    pub async fn storage_size(&self) -> Result<u64, TelemetryError> {
        if self.path.exists() {
            let metadata = fs::metadata(&self.path).await?;
            Ok(metadata.len())
        } else {
            Ok(0)
        }
    }

    /// Save events to file
    async fn save_to_file(&self, events: &[TelemetryEvent]) -> Result<(), TelemetryError> {
        let content = serde_json::to_string_pretty(events)
            .map_err(|e| TelemetryError::Serialization(e.to_string()))?;

        fs::write(&self.path, content).await?;

        Ok(())
    }
}
