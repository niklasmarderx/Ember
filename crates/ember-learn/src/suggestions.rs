//! Adaptive suggestion engine.
//!
//! Generates personalized suggestions based on learned patterns.

use crate::{EventContext, PatternRecognizer, PreferenceLearner, UserProfile};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Suggestion engine that generates context-aware recommendations.
#[derive(Debug, Clone, Default)]
pub struct SuggestionEngine {
    /// Cached suggestions.
    cache: Vec<CachedSuggestion>,
    /// Configuration.
    config: SuggestionConfig,
}

impl SuggestionEngine {
    /// Create a new suggestion engine.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with custom configuration.
    pub fn with_config(config: SuggestionConfig) -> Self {
        Self {
            cache: Vec::new(),
            config,
        }
    }

    /// Generate suggestions for the current context.
    pub fn generate(
        &self,
        profile: &UserProfile,
        preferences: &PreferenceLearner,
        patterns: &PatternRecognizer,
        context: &EventContext,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        // Model suggestions.
        if let Some(model) = self.suggest_model(preferences, context) {
            suggestions.push(model);
        }

        // Tool suggestions.
        suggestions.extend(self.suggest_tools(preferences, context));

        // Workflow suggestions.
        suggestions.extend(self.suggest_workflows(patterns, context));

        // Code pattern suggestions.
        suggestions.extend(self.suggest_code_patterns(patterns, context));

        // Productivity tips.
        if self.config.include_tips {
            suggestions.extend(self.suggest_tips(profile, preferences));
        }

        // Sort by relevance and limit.
        suggestions.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap());
        suggestions.truncate(self.config.max_suggestions);

        suggestions
    }

    fn suggest_model(
        &self,
        preferences: &PreferenceLearner,
        context: &EventContext,
    ) -> Option<Suggestion> {
        let task_type = context.project_type.as_deref().unwrap_or("general");

        if let Some(model) = preferences.preferred_model(task_type) {
            if !model.is_empty() {
                return Some(Suggestion {
                    id: Uuid::new_v4(),
                    suggestion_type: SuggestionType::Model,
                    title: format!("Use {} for this task", model),
                    description: format!(
                        "Based on your history, {} works well for {} tasks.",
                        model, task_type
                    ),
                    action: SuggestionAction::SetModel(model.to_string()),
                    relevance: 0.8,
                    confidence: 0.7,
                });
            }
        }
        None
    }

    fn suggest_tools(
        &self,
        preferences: &PreferenceLearner,
        context: &EventContext,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        let top_tools = preferences.top_tools(3);
        for (tool, score) in top_tools {
            // Only suggest if tool is relevant to context.
            if self.is_tool_relevant(&tool, context) {
                let relevance = (score / 100.0).min(1.0);
                if relevance > 0.3 {
                    suggestions.push(Suggestion {
                        id: Uuid::new_v4(),
                        suggestion_type: SuggestionType::Tool,
                        title: format!("Enable {} tool", tool),
                        description: format!("You frequently use the {} tool.", tool),
                        action: SuggestionAction::EnableTool(tool),
                        relevance,
                        confidence: 0.6,
                    });
                }
            }
        }

        suggestions
    }

    fn suggest_workflows(
        &self,
        patterns: &PatternRecognizer,
        context: &EventContext,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        for workflow in patterns.matching_workflows(context) {
            if workflow.confidence > 0.5 {
                suggestions.push(Suggestion {
                    id: Uuid::new_v4(),
                    suggestion_type: SuggestionType::Workflow,
                    title: format!("Start '{}' workflow", workflow.name),
                    description: workflow.description.clone(),
                    action: SuggestionAction::StartWorkflow(workflow.name.clone()),
                    relevance: workflow.confidence,
                    confidence: workflow.confidence,
                });
            }
        }

        suggestions
    }

    fn suggest_code_patterns(
        &self,
        patterns: &PatternRecognizer,
        context: &EventContext,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        for pattern in &patterns.code_patterns {
            // Check if language matches.
            let lang_match = pattern.language.is_none() || pattern.language == context.language;

            if lang_match && pattern.acceptance_rate > 0.7 && pattern.usage_count >= 3 {
                suggestions.push(Suggestion {
                    id: Uuid::new_v4(),
                    suggestion_type: SuggestionType::CodePattern,
                    title: format!("Use {} pattern", pattern.pattern_type),
                    description: format!(
                        "This pattern has a {}% acceptance rate in your code.",
                        (pattern.acceptance_rate * 100.0) as u32
                    ),
                    action: SuggestionAction::ApplyPattern(pattern.pattern_type.clone()),
                    relevance: pattern.acceptance_rate * 0.8,
                    confidence: pattern.acceptance_rate,
                });
            }
        }

        suggestions
    }

    fn suggest_tips(
        &self,
        profile: &UserProfile,
        preferences: &PreferenceLearner,
    ) -> Vec<Suggestion> {
        let mut tips = Vec::new();

        // Suggest keyboard shortcuts if user types a lot.
        if profile.total_messages > 50 {
            tips.push(Suggestion {
                id: Uuid::new_v4(),
                suggestion_type: SuggestionType::Tip,
                title: "Use keyboard shortcuts".to_string(),
                description: "Press Ctrl+Enter to send, Ctrl+K for quick actions.".to_string(),
                action: SuggestionAction::ShowHelp("shortcuts".to_string()),
                relevance: 0.4,
                confidence: 0.9,
            });
        }

        // Suggest exploring features based on usage patterns.
        let peak_hours = preferences.activity_patterns.peak_hours();
        if !peak_hours.is_empty() {
            let hour = peak_hours[0];
            tips.push(Suggestion {
                id: Uuid::new_v4(),
                suggestion_type: SuggestionType::Tip,
                title: "Your peak productivity time".to_string(),
                description: format!(
                    "You're most active around {}:00. Consider scheduling complex tasks then.",
                    hour
                ),
                action: SuggestionAction::None,
                relevance: 0.3,
                confidence: 0.8,
            });
        }

        // Suggest model exploration.
        if preferences.model_preferences.len() <= 1 {
            tips.push(Suggestion {
                id: Uuid::new_v4(),
                suggestion_type: SuggestionType::Tip,
                title: "Explore different models".to_string(),
                description: "Try Claude for reasoning, GPT-4 for code, Gemini for long context."
                    .to_string(),
                action: SuggestionAction::ShowHelp("models".to_string()),
                relevance: 0.35,
                confidence: 0.9,
            });
        }

        tips
    }

    fn is_tool_relevant(&self, tool: &str, context: &EventContext) -> bool {
        match tool {
            "shell" => true,
            "filesystem" => true,
            "git" => context.project_type.is_some(),
            "web" => context
                .tags
                .iter()
                .any(|t| t.contains("web") || t.contains("api")),
            "browser" => context
                .tags
                .iter()
                .any(|t| t.contains("web") || t.contains("scrape")),
            "code" => context.language.is_some(),
            _ => true,
        }
    }
}

/// A generated suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    /// Unique suggestion ID.
    pub id: Uuid,
    /// Type of suggestion.
    pub suggestion_type: SuggestionType,
    /// Short title.
    pub title: String,
    /// Detailed description.
    pub description: String,
    /// Action to take if accepted.
    pub action: SuggestionAction,
    /// Relevance score (0.0 - 1.0).
    pub relevance: f64,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f64,
}

/// Type of suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuggestionType {
    /// Model selection.
    Model,
    /// Tool to enable.
    Tool,
    /// Workflow to start.
    Workflow,
    /// Code pattern to apply.
    CodePattern,
    /// Productivity tip.
    Tip,
    /// Configuration change.
    Config,
    /// Command to run.
    Command,
}

/// Action to take for a suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuggestionAction {
    /// No action.
    None,
    /// Set the model.
    SetModel(String),
    /// Enable a tool.
    EnableTool(String),
    /// Start a workflow.
    StartWorkflow(String),
    /// Apply a code pattern.
    ApplyPattern(String),
    /// Show help topic.
    ShowHelp(String),
    /// Run a command.
    RunCommand(String),
    /// Change configuration.
    SetConfig { key: String, value: String },
}

/// Suggestion engine configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionConfig {
    /// Maximum number of suggestions to return.
    pub max_suggestions: usize,
    /// Include productivity tips.
    pub include_tips: bool,
    /// Minimum relevance score.
    pub min_relevance: f64,
    /// Minimum confidence score.
    pub min_confidence: f64,
}

impl Default for SuggestionConfig {
    fn default() -> Self {
        Self {
            max_suggestions: 5,
            include_tips: true,
            min_relevance: 0.3,
            min_confidence: 0.5,
        }
    }
}

/// Cached suggestion for quick retrieval.
#[derive(Debug, Clone)]
struct CachedSuggestion {
    suggestion: Suggestion,
    context_hash: u64,
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suggestion_engine_creation() {
        let engine = SuggestionEngine::new();
        assert_eq!(engine.config.max_suggestions, 5);
    }

    #[test]
    fn test_generate_suggestions() {
        let engine = SuggestionEngine::new();
        let profile = UserProfile::new();
        let preferences = PreferenceLearner::new();
        let patterns = PatternRecognizer::new();
        let context = EventContext::default();

        let suggestions = engine.generate(&profile, &preferences, &patterns, &context);
        // With empty profile, we might get some tips.
        assert!(suggestions.len() <= engine.config.max_suggestions);
    }

    #[test]
    fn test_suggestion_types() {
        let suggestion = Suggestion {
            id: Uuid::new_v4(),
            suggestion_type: SuggestionType::Model,
            title: "Test".to_string(),
            description: "Test description".to_string(),
            action: SuggestionAction::SetModel("gpt-4".to_string()),
            relevance: 0.8,
            confidence: 0.9,
        };

        assert_eq!(suggestion.suggestion_type, SuggestionType::Model);
    }
}
