//! Preference learning module.
//!
//! Learns user preferences from interactions.

use crate::{EventType, LearningEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Learned user preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PreferenceLearner {
    /// Model preferences by task type.
    pub model_preferences: HashMap<String, ModelPreference>,
    /// Coding style preferences.
    pub coding_style: CodingStylePreference,
    /// Communication preferences.
    pub communication: CommunicationPreference,
    /// Tool preferences.
    pub tool_preferences: HashMap<String, f64>,
    /// Language preferences.
    pub language_preferences: HashMap<String, f64>,
    /// Framework preferences.
    pub framework_preferences: HashMap<String, f64>,
    /// Time-of-day activity patterns.
    pub activity_patterns: ActivityPatterns,
}

impl PreferenceLearner {
    /// Create a new preference learner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from existing profile.
    pub fn from_profile(profile: &crate::UserProfile) -> Self {
        let mut learner = Self::new();

        // Initialize from profile data.
        for (lang, count) in &profile.language_usage {
            learner
                .language_preferences
                .insert(lang.clone(), *count as f64);
        }

        learner
    }

    /// Process a learning event.
    pub fn process_event(&mut self, event: &LearningEvent) {
        match event.event_type {
            EventType::CodeGenerated | EventType::CodeAccepted => {
                self.update_coding_preferences(event);
            }
            EventType::ToolUsed => {
                self.update_tool_preferences(event);
            }
            EventType::MessageSent => {
                self.update_communication_preferences(event);
            }
            EventType::SuggestionAccepted | EventType::SuggestionRejected => {
                self.update_suggestion_preferences(event);
            }
            _ => {}
        }

        // Update activity patterns.
        self.activity_patterns.record_activity(event.timestamp);

        // Update language preferences.
        if let Some(lang) = &event.context.language {
            *self.language_preferences.entry(lang.clone()).or_insert(0.0) += 1.0;
        }
    }

    /// Get count of learned preferences.
    pub fn count(&self) -> usize {
        self.model_preferences.len()
            + self.tool_preferences.len()
            + self.language_preferences.len()
            + self.framework_preferences.len()
            + if self.coding_style.has_preferences() {
                1
            } else {
                0
            }
            + if self.communication.has_preferences() {
                1
            } else {
                0
            }
    }

    /// Get preferred model for a task type.
    pub fn preferred_model(&self, task_type: &str) -> Option<&str> {
        self.model_preferences
            .get(task_type)
            .map(|p| p.preferred_model.as_str())
    }

    /// Get preferred language.
    pub fn preferred_language(&self) -> Option<String> {
        self.language_preferences
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(k, _)| k.clone())
    }

    /// Get top N tools by preference.
    pub fn top_tools(&self, n: usize) -> Vec<(String, f64)> {
        let mut tools: Vec<_> = self
            .tool_preferences
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        tools.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        tools.truncate(n);
        tools
    }

    fn update_coding_preferences(&mut self, event: &LearningEvent) {
        if let Some(style) = event.data.get("style").and_then(|v| v.as_str()) {
            match style {
                "verbose" => self.coding_style.verbosity += 0.1,
                "concise" => self.coding_style.verbosity -= 0.1,
                _ => {}
            }
            self.coding_style.verbosity = self.coding_style.verbosity.clamp(0.0, 1.0);
        }

        if let Some(comments) = event.data.get("has_comments").and_then(|v| v.as_bool()) {
            if comments {
                self.coding_style.comment_density += 0.1;
            } else {
                self.coding_style.comment_density -= 0.05;
            }
            self.coding_style.comment_density = self.coding_style.comment_density.clamp(0.0, 1.0);
        }
    }

    fn update_tool_preferences(&mut self, event: &LearningEvent) {
        if let Some(tool) = event.data.get("tool").and_then(|v| v.as_str()) {
            *self.tool_preferences.entry(tool.to_string()).or_insert(0.0) += 1.0;
        }
    }

    fn update_communication_preferences(&mut self, event: &LearningEvent) {
        if let Some(length) = event.data.get("message_length").and_then(|v| v.as_u64()) {
            let normalized = (length as f64 / 500.0).min(1.0);
            self.communication.message_length_preference =
                self.communication.message_length_preference * 0.9 + normalized * 0.1;
        }

        if let Some(technical) = event.data.get("technical_level").and_then(|v| v.as_f64()) {
            self.communication.technical_level =
                self.communication.technical_level * 0.9 + technical * 0.1;
        }
    }

    fn update_suggestion_preferences(&mut self, event: &LearningEvent) {
        if let Some(model) = &event.context.model {
            let task_type = event
                .context
                .project_type
                .clone()
                .unwrap_or_else(|| "general".to_string());
            let pref = self.model_preferences.entry(task_type).or_default();

            if event.event_type == EventType::SuggestionAccepted {
                pref.success_count += 1;
                if pref.success_count > pref.best_success_count {
                    pref.best_success_count = pref.success_count;
                    pref.preferred_model = model.clone();
                }
            } else {
                pref.rejection_count += 1;
            }
        }
    }
}

/// Model preference for a task type.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelPreference {
    /// Preferred model name.
    pub preferred_model: String,
    /// Number of successful interactions.
    pub success_count: usize,
    /// Number of rejections.
    pub rejection_count: usize,
    /// Best success count (for comparison).
    pub best_success_count: usize,
}

/// Coding style preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodingStylePreference {
    /// Verbosity preference (0.0 = concise, 1.0 = verbose).
    pub verbosity: f64,
    /// Comment density preference (0.0 = minimal, 1.0 = extensive).
    pub comment_density: f64,
    /// Preferred indentation (spaces).
    pub indentation: usize,
    /// Prefer semicolons (for JS/TS).
    pub prefer_semicolons: bool,
    /// Prefer single quotes.
    pub prefer_single_quotes: bool,
    /// Max line length.
    pub max_line_length: usize,
    /// Trailing commas preference.
    pub trailing_commas: TrailingCommas,
    /// Error handling style.
    pub error_handling_style: ErrorHandlingStyle,
}

impl Default for CodingStylePreference {
    fn default() -> Self {
        Self {
            verbosity: 0.5,
            comment_density: 0.5,
            indentation: 4,
            prefer_semicolons: true,
            prefer_single_quotes: false,
            max_line_length: 100,
            trailing_commas: TrailingCommas::Es5,
            error_handling_style: ErrorHandlingStyle::Explicit,
        }
    }
}

impl CodingStylePreference {
    fn has_preferences(&self) -> bool {
        // Check if any preferences deviate from default.
        (self.verbosity - 0.5).abs() > 0.1 || (self.comment_density - 0.5).abs() > 0.1
    }
}

/// Trailing commas preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TrailingCommas {
    /// No trailing commas.
    None,
    /// ES5 compatible.
    #[default]
    Es5,
    /// All trailing commas.
    All,
}

/// Error handling style preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ErrorHandlingStyle {
    /// Explicit error handling (Result, try/catch).
    #[default]
    Explicit,
    /// Implicit (panics, throws).
    Implicit,
    /// Mixed.
    Mixed,
}

/// Communication preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationPreference {
    /// Preferred message length (0.0 = short, 1.0 = long).
    pub message_length_preference: f64,
    /// Technical level (0.0 = beginner, 1.0 = expert).
    pub technical_level: f64,
    /// Prefer code examples.
    pub prefer_code_examples: bool,
    /// Prefer explanations.
    pub prefer_explanations: bool,
    /// Response format preference.
    pub response_format: ResponseFormat,
}

impl Default for CommunicationPreference {
    fn default() -> Self {
        Self {
            message_length_preference: 0.5,
            technical_level: 0.5,
            prefer_code_examples: true,
            prefer_explanations: true,
            response_format: ResponseFormat::Mixed,
        }
    }
}

impl CommunicationPreference {
    fn has_preferences(&self) -> bool {
        (self.message_length_preference - 0.5).abs() > 0.1
            || (self.technical_level - 0.5).abs() > 0.1
    }
}

/// Response format preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ResponseFormat {
    /// Code only.
    CodeOnly,
    /// Text only.
    TextOnly,
    /// Mixed code and text.
    #[default]
    Mixed,
    /// Structured (bullet points, numbered lists).
    Structured,
}

/// Activity patterns by time.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivityPatterns {
    /// Activity by hour (0-23).
    pub by_hour: [u32; 24],
    /// Activity by day of week (0=Monday, 6=Sunday).
    pub by_day: [u32; 7],
    /// Total activities recorded.
    pub total_activities: u32,
}

impl ActivityPatterns {
    /// Record an activity.
    pub fn record_activity(&mut self, timestamp: chrono::DateTime<chrono::Utc>) {
        let hour = timestamp.hour() as usize;
        let day = timestamp.weekday().num_days_from_monday() as usize;

        self.by_hour[hour] += 1;
        self.by_day[day] += 1;
        self.total_activities += 1;
    }

    /// Get peak hours (hours with above-average activity).
    pub fn peak_hours(&self) -> Vec<usize> {
        if self.total_activities == 0 {
            return Vec::new();
        }

        let avg = self.total_activities as f64 / 24.0;
        self.by_hour
            .iter()
            .enumerate()
            .filter(|(_, &count)| count as f64 > avg)
            .map(|(hour, _)| hour)
            .collect()
    }

    /// Get most active day.
    pub fn most_active_day(&self) -> Option<chrono::Weekday> {
        self.by_day
            .iter()
            .enumerate()
            .max_by_key(|(_, &count)| count)
            .map(|(day, _)| chrono::Weekday::try_from(day as u8).unwrap_or(chrono::Weekday::Mon))
    }
}

use chrono::{Datelike, Timelike};

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_preference_learner_creation() {
        let learner = PreferenceLearner::new();
        assert_eq!(learner.count(), 0);
    }

    #[test]
    fn test_activity_patterns() {
        let mut patterns = ActivityPatterns::default();
        let now = Utc::now();

        patterns.record_activity(now);
        assert_eq!(patterns.total_activities, 1);
    }

    #[test]
    fn test_process_event() {
        let mut learner = PreferenceLearner::new();
        let event = LearningEvent::new(
            EventType::ToolUsed,
            EventContext::default(),
            serde_json::json!({"tool": "shell"}),
        );

        learner.process_event(&event);
        assert!(learner.tool_preferences.contains_key("shell"));
    }
}
