//! Ember Learning System
//!
//! Adaptive learning that improves with usage.
//! Learns user preferences, coding patterns, and workflow habits.

pub mod patterns;
pub mod preferences;
pub mod profile;
pub mod suggestions;

pub use patterns::*;
pub use preferences::*;
pub use profile::*;
pub use suggestions::*;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Learning system errors.
#[derive(Debug, Error)]
pub enum LearningError {
    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Model error: {0}")]
    ModelError(String),
}

/// Result type for learning operations.
pub type Result<T> = std::result::Result<T, LearningError>;

/// A learning event captured from user interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningEvent {
    /// Unique event ID.
    pub id: Uuid,
    /// Event type.
    pub event_type: EventType,
    /// Event context.
    pub context: EventContext,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Associated data.
    pub data: serde_json::Value,
}

impl LearningEvent {
    /// Create a new learning event.
    pub fn new(event_type: EventType, context: EventContext, data: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            context,
            timestamp: Utc::now(),
            data,
        }
    }
}

/// Types of learning events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    /// User sent a message.
    MessageSent,
    /// User accepted a suggestion.
    SuggestionAccepted,
    /// User rejected a suggestion.
    SuggestionRejected,
    /// User modified a suggestion.
    SuggestionModified,
    /// User used a tool.
    ToolUsed,
    /// User changed settings.
    SettingsChanged,
    /// User completed a task.
    TaskCompleted,
    /// User cancelled a task.
    TaskCancelled,
    /// Code was generated.
    CodeGenerated,
    /// Code was accepted.
    CodeAccepted,
    /// Code was rejected.
    CodeRejected,
    /// Code was modified.
    CodeModified,
    /// File was created.
    FileCreated,
    /// File was edited.
    FileEdited,
    /// Command was executed.
    CommandExecuted,
    /// Error occurred.
    ErrorOccurred,
    /// Feedback provided.
    FeedbackProvided,
}

/// Context for a learning event.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventContext {
    /// Current language/framework.
    pub language: Option<String>,
    /// Current project type.
    pub project_type: Option<String>,
    /// Current file path.
    pub file_path: Option<String>,
    /// Current model being used.
    pub model: Option<String>,
    /// Session ID.
    pub session_id: Option<String>,
    /// Tags.
    pub tags: Vec<String>,
}

/// The main learning system.
pub struct LearningSystem {
    /// User profile.
    profile: UserProfile,
    /// Preference learner.
    preferences: PreferenceLearner,
    /// Pattern recognizer.
    patterns: PatternRecognizer,
    /// Suggestion engine.
    suggestions: SuggestionEngine,
    /// Event history.
    events: Vec<LearningEvent>,
}

impl LearningSystem {
    /// Create a new learning system.
    pub fn new() -> Self {
        Self {
            profile: UserProfile::new(),
            preferences: PreferenceLearner::new(),
            patterns: PatternRecognizer::new(),
            suggestions: SuggestionEngine::new(),
            events: Vec::new(),
        }
    }

    /// Load from storage.
    pub fn load(path: &std::path::Path) -> Result<Self> {
        if path.exists() {
            let content =
                std::fs::read_to_string(path).map_err(|e| LearningError::Storage(e.to_string()))?;
            let data: LearningData = serde_json::from_str(&content)
                .map_err(|e| LearningError::InvalidData(e.to_string()))?;

            let mut system = Self::new();
            system.profile = data.profile;
            system.events = data.events;
            system.preferences = PreferenceLearner::from_profile(&system.profile);
            system.patterns = PatternRecognizer::from_events(&system.events);

            Ok(system)
        } else {
            Ok(Self::new())
        }
    }

    /// Save to storage.
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        let data = LearningData {
            profile: self.profile.clone(),
            events: self.events.clone(),
        };

        let content = serde_json::to_string_pretty(&data)
            .map_err(|e| LearningError::Storage(e.to_string()))?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| LearningError::Storage(e.to_string()))?;
        }

        std::fs::write(path, content).map_err(|e| LearningError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Record a learning event.
    pub fn record_event(&mut self, event: LearningEvent) {
        // Update preferences.
        self.preferences.process_event(&event);

        // Update patterns.
        self.patterns.process_event(&event);

        // Update profile.
        self.profile.update_from_event(&event);

        // Store event.
        self.events.push(event);

        // Limit history size.
        if self.events.len() > 10000 {
            self.events.drain(0..1000);
        }
    }

    /// Get suggestions for current context.
    pub fn get_suggestions(&self, context: &EventContext) -> Vec<Suggestion> {
        self.suggestions
            .generate(&self.profile, &self.preferences, &self.patterns, context)
    }

    /// Get the user profile.
    pub fn profile(&self) -> &UserProfile {
        &self.profile
    }

    /// Get learned preferences.
    pub fn preferences(&self) -> &PreferenceLearner {
        &self.preferences
    }

    /// Get recognized patterns.
    pub fn patterns(&self) -> &PatternRecognizer {
        &self.patterns
    }

    /// Get statistics.
    pub fn stats(&self) -> LearningStats {
        LearningStats {
            total_events: self.events.len(),
            events_by_type: self.count_events_by_type(),
            profile_completeness: self.profile.completeness(),
            learned_preferences: self.preferences.count(),
            recognized_patterns: self.patterns.count(),
            suggestion_acceptance_rate: self.calculate_acceptance_rate(),
        }
    }

    fn count_events_by_type(&self) -> std::collections::HashMap<String, usize> {
        let mut counts = std::collections::HashMap::new();
        for event in &self.events {
            let key = format!("{:?}", event.event_type);
            *counts.entry(key).or_insert(0) += 1;
        }
        counts
    }

    fn calculate_acceptance_rate(&self) -> f64 {
        let accepted = self
            .events
            .iter()
            .filter(|e| e.event_type == EventType::SuggestionAccepted)
            .count();
        let rejected = self
            .events
            .iter()
            .filter(|e| e.event_type == EventType::SuggestionRejected)
            .count();
        let total = accepted + rejected;
        if total == 0 {
            0.0
        } else {
            accepted as f64 / total as f64
        }
    }
}

impl Default for LearningSystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable learning data.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LearningData {
    profile: UserProfile,
    events: Vec<LearningEvent>,
}

/// Learning statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningStats {
    /// Total events recorded.
    pub total_events: usize,
    /// Events by type.
    pub events_by_type: std::collections::HashMap<String, usize>,
    /// Profile completeness (0.0 - 1.0).
    pub profile_completeness: f64,
    /// Number of learned preferences.
    pub learned_preferences: usize,
    /// Number of recognized patterns.
    pub recognized_patterns: usize,
    /// Suggestion acceptance rate (0.0 - 1.0).
    pub suggestion_acceptance_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_learning_system_creation() {
        let system = LearningSystem::new();
        assert_eq!(system.events.len(), 0);
    }

    #[test]
    fn test_record_event() {
        let mut system = LearningSystem::new();
        let event = LearningEvent::new(
            EventType::MessageSent,
            EventContext::default(),
            serde_json::json!({"message": "hello"}),
        );
        system.record_event(event);
        assert_eq!(system.events.len(), 1);
    }

    #[test]
    fn test_stats() {
        let system = LearningSystem::new();
        let stats = system.stats();
        assert_eq!(stats.total_events, 0);
    }
}
