//! Pattern recognition module.
//!
//! Recognizes patterns in user behavior and code.

use crate::{EventContext, EventType, LearningEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Pattern recognizer for user behavior.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PatternRecognizer {
    /// Workflow patterns.
    pub workflows: Vec<WorkflowPattern>,
    /// Code patterns.
    pub code_patterns: Vec<CodePattern>,
    /// Error patterns (common mistakes).
    pub error_patterns: Vec<ErrorPattern>,
    /// Sequence patterns (common action sequences).
    pub sequences: Vec<SequencePattern>,
    /// Current action buffer for sequence detection.
    #[serde(skip)]
    action_buffer: Vec<ActionRecord>,
}

impl PatternRecognizer {
    /// Create a new pattern recognizer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from event history.
    pub fn from_events(events: &[LearningEvent]) -> Self {
        let mut recognizer = Self::new();
        for event in events {
            recognizer.process_event(event);
        }
        recognizer
    }

    /// Process a learning event.
    pub fn process_event(&mut self, event: &LearningEvent) {
        // Record action.
        self.action_buffer.push(ActionRecord {
            event_type: event.event_type,
            context: event.context.clone(),
            timestamp: event.timestamp,
        });

        // Limit buffer size.
        if self.action_buffer.len() > 100 {
            self.action_buffer.drain(0..50);
        }

        // Detect patterns.
        self.detect_workflow_patterns();
        self.detect_sequence_patterns();

        // Update error patterns.
        if event.event_type == EventType::ErrorOccurred {
            self.record_error_pattern(event);
        }

        // Update code patterns.
        if matches!(
            event.event_type,
            EventType::CodeGenerated | EventType::CodeAccepted | EventType::CodeModified
        ) {
            self.update_code_patterns(event);
        }
    }

    /// Get count of recognized patterns.
    pub fn count(&self) -> usize {
        self.workflows.len()
            + self.code_patterns.len()
            + self.error_patterns.len()
            + self.sequences.len()
    }

    /// Get workflow patterns matching context.
    pub fn matching_workflows(&self, context: &EventContext) -> Vec<&WorkflowPattern> {
        self.workflows
            .iter()
            .filter(|w| w.matches_context(context))
            .collect()
    }

    /// Get common error patterns.
    pub fn common_errors(&self, min_count: usize) -> Vec<&ErrorPattern> {
        self.error_patterns
            .iter()
            .filter(|e| e.occurrence_count >= min_count)
            .collect()
    }

    /// Get predicted next action.
    pub fn predict_next_action(&self, recent_actions: &[EventType]) -> Option<EventType> {
        for sequence in &self.sequences {
            if sequence.matches_prefix(recent_actions) && sequence.confidence > 0.5 {
                return sequence.next_action;
            }
        }
        None
    }

    fn detect_workflow_patterns(&mut self) {
        if self.action_buffer.len() < 3 {
            return;
        }

        // Look for repeated action sequences.
        let recent: Vec<EventType> = self
            .action_buffer
            .iter()
            .rev()
            .take(10)
            .map(|a| a.event_type)
            .collect();

        // Check for common workflows.
        // E.g., MessageSent -> CodeGenerated -> CodeAccepted
        if recent.len() >= 3 {
            let last_three = &recent[0..3];

            // Clone action_buffer to avoid borrow checker issues
            let actions_clone = self.action_buffer.clone();

            // Code generation workflow.
            if last_three[2] == EventType::MessageSent
                && last_three[1] == EventType::CodeGenerated
                && last_three[0] == EventType::CodeAccepted
            {
                self.record_workflow("code_generation", &actions_clone);
            }

            // File editing workflow.
            if last_three.contains(&EventType::FileEdited)
                && last_three.contains(&EventType::CommandExecuted)
            {
                self.record_workflow("file_edit_and_run", &actions_clone);
            }
        }
    }

    fn detect_sequence_patterns(&mut self) {
        if self.action_buffer.len() < 5 {
            return;
        }

        // Build frequency map for action pairs.
        let mut pair_counts: HashMap<(EventType, EventType), usize> = HashMap::new();

        for window in self.action_buffer.windows(2) {
            let pair = (window[0].event_type, window[1].event_type);
            *pair_counts.entry(pair).or_insert(0) += 1;
        }

        // Find frequently occurring sequences.
        for ((first, second), count) in pair_counts {
            if count >= 3 {
                let confidence = count as f64 / self.action_buffer.len() as f64;

                // Update or add sequence pattern.
                if let Some(seq) = self.sequences.iter_mut().find(|s| s.actions == vec![first]) {
                    seq.confidence = (seq.confidence + confidence) / 2.0;
                    seq.next_action = Some(second);
                } else if confidence > 0.1 {
                    self.sequences.push(SequencePattern {
                        actions: vec![first],
                        next_action: Some(second),
                        confidence,
                        occurrence_count: count,
                    });
                }
            }
        }

        // Limit sequences.
        if self.sequences.len() > 50 {
            self.sequences
                .sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
            self.sequences.truncate(30);
        }
    }

    fn record_workflow(&mut self, name: &str, actions: &[ActionRecord]) {
        // Check if workflow exists.
        if let Some(workflow) = self.workflows.iter_mut().find(|w| w.name == name) {
            workflow.occurrence_count += 1;
            workflow.confidence = (workflow.confidence + 0.1).min(1.0);
        } else {
            let context = actions
                .last()
                .map(|a| a.context.clone())
                .unwrap_or_default();
            self.workflows.push(WorkflowPattern {
                name: name.to_string(),
                description: describe_workflow(name),
                steps: actions.iter().map(|a| a.event_type).collect(),
                context_triggers: vec![context.clone()],
                occurrence_count: 1,
                confidence: 0.5,
            });
        }
    }

    fn record_error_pattern(&mut self, event: &LearningEvent) {
        let error_type = event
            .data
            .get("error_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        if let Some(pattern) = self
            .error_patterns
            .iter_mut()
            .find(|p| p.error_type == error_type)
        {
            pattern.occurrence_count += 1;
        } else {
            self.error_patterns.push(ErrorPattern {
                error_type,
                context: event.context.clone(),
                occurrence_count: 1,
                suggested_fix: None,
                prevention_hint: None,
            });
        }
    }

    fn update_code_patterns(&mut self, event: &LearningEvent) {
        if let Some(pattern_type) = event.data.get("pattern").and_then(|v| v.as_str()) {
            if let Some(pattern) = self
                .code_patterns
                .iter_mut()
                .find(|p| p.pattern_type == pattern_type)
            {
                pattern.usage_count += 1;
                if event.event_type == EventType::CodeAccepted {
                    pattern.acceptance_rate =
                        (pattern.acceptance_rate * (pattern.usage_count - 1) as f64 + 1.0)
                            / pattern.usage_count as f64;
                }
            } else {
                self.code_patterns.push(CodePattern {
                    pattern_type: pattern_type.to_string(),
                    language: event.context.language.clone(),
                    usage_count: 1,
                    acceptance_rate: if event.event_type == EventType::CodeAccepted {
                        1.0
                    } else {
                        0.0
                    },
                    examples: vec![],
                });
            }
        }
    }
}

/// A recognized workflow pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPattern {
    /// Pattern name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Steps in the workflow.
    pub steps: Vec<EventType>,
    /// Contexts that trigger this workflow.
    pub context_triggers: Vec<EventContext>,
    /// Number of times observed.
    pub occurrence_count: usize,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f64,
}

impl WorkflowPattern {
    /// Check if pattern matches context.
    pub fn matches_context(&self, context: &EventContext) -> bool {
        for trigger in &self.context_triggers {
            let lang_match = trigger.language.is_none() || trigger.language == context.language;
            let project_match =
                trigger.project_type.is_none() || trigger.project_type == context.project_type;

            if lang_match && project_match {
                return true;
            }
        }
        false
    }
}

/// A recognized code pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePattern {
    /// Pattern type (e.g., "error_handling", "async_await").
    pub pattern_type: String,
    /// Associated language.
    pub language: Option<String>,
    /// How many times used.
    pub usage_count: usize,
    /// Acceptance rate (0.0 - 1.0).
    pub acceptance_rate: f64,
    /// Example code snippets.
    pub examples: Vec<String>,
}

/// A recognized error pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPattern {
    /// Error type.
    pub error_type: String,
    /// Context where error occurred.
    pub context: EventContext,
    /// Number of occurrences.
    pub occurrence_count: usize,
    /// Suggested fix.
    pub suggested_fix: Option<String>,
    /// Prevention hint.
    pub prevention_hint: Option<String>,
}

/// A recognized action sequence pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequencePattern {
    /// Actions in the sequence.
    pub actions: Vec<EventType>,
    /// Predicted next action.
    pub next_action: Option<EventType>,
    /// Confidence score.
    pub confidence: f64,
    /// Occurrence count.
    pub occurrence_count: usize,
}

impl SequencePattern {
    /// Check if the pattern matches a prefix of actions.
    pub fn matches_prefix(&self, actions: &[EventType]) -> bool {
        if actions.len() < self.actions.len() {
            return false;
        }
        let start = actions.len() - self.actions.len();
        &actions[start..] == self.actions.as_slice()
    }
}

/// Internal action record.
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct ActionRecord {
    event_type: EventType,
    context: EventContext,
    timestamp: chrono::DateTime<chrono::Utc>,
}

fn describe_workflow(name: &str) -> String {
    match name {
        "code_generation" => "Generate code from natural language description".to_string(),
        "file_edit_and_run" => "Edit file and run command to test".to_string(),
        "debug_loop" => "Edit, run, check error, repeat".to_string(),
        "refactor" => "Analyze code and apply improvements".to_string(),
        _ => format!("{} workflow", name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_recognizer_creation() {
        let recognizer = PatternRecognizer::new();
        assert_eq!(recognizer.count(), 0);
    }

    #[test]
    fn test_process_event() {
        let mut recognizer = PatternRecognizer::new();
        let event = LearningEvent::new(
            EventType::CodeGenerated,
            EventContext::default(),
            serde_json::json!({}),
        );

        recognizer.process_event(&event);
        assert_eq!(recognizer.action_buffer.len(), 1);
    }

    #[test]
    fn test_sequence_pattern_matching() {
        let pattern = SequencePattern {
            actions: vec![EventType::MessageSent],
            next_action: Some(EventType::CodeGenerated),
            confidence: 0.8,
            occurrence_count: 5,
        };

        assert!(pattern.matches_prefix(&[EventType::MessageSent]));
        assert!(pattern.matches_prefix(&[EventType::ToolUsed, EventType::MessageSent]));
        assert!(!pattern.matches_prefix(&[EventType::CodeGenerated]));
    }
}
