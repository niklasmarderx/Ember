//! Model Scoring for Intelligent Model Selection
//!
//! This module provides model scoring capabilities based on task requirements,
//! cost, latency, and quality metrics.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::analyzer::{TaskAnalysis, TaskComplexity, TaskType};

/// Capabilities and characteristics of a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Model identifier (e.g., "gpt-4", "claude-3-opus").
    pub name: String,
    /// Provider name (e.g., "openai", "anthropic").
    pub provider: String,
    /// Maximum context length in tokens.
    pub context_length: usize,
    /// Cost per 1000 input tokens in USD.
    pub cost_per_1k_input: f64,
    /// Cost per 1000 output tokens in USD.
    pub cost_per_1k_output: f64,
    /// Average latency in milliseconds.
    pub latency_ms: u32,
    /// Quality scores by task type (0.0 - 1.0).
    pub quality_scores: HashMap<TaskType, f32>,
    /// Whether the model supports tool/function calling.
    pub supports_tools: bool,
    /// Whether the model supports streaming.
    pub supports_streaming: bool,
    /// Whether the model is available (not rate-limited, etc.).
    pub available: bool,
}

impl ModelCapabilities {
    /// Create a new model capabilities definition.
    pub fn new(name: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: provider.into(),
            context_length: 8192,
            cost_per_1k_input: 0.01,
            cost_per_1k_output: 0.03,
            latency_ms: 1000,
            quality_scores: HashMap::new(),
            supports_tools: false,
            supports_streaming: true,
            available: true,
        }
    }

    /// Set context length.
    pub fn with_context_length(mut self, length: usize) -> Self {
        self.context_length = length;
        self
    }

    /// Set pricing.
    pub fn with_pricing(mut self, input: f64, output: f64) -> Self {
        self.cost_per_1k_input = input;
        self.cost_per_1k_output = output;
        self
    }

    /// Set latency.
    pub fn with_latency(mut self, latency_ms: u32) -> Self {
        self.latency_ms = latency_ms;
        self
    }

    /// Add a quality score for a task type.
    pub fn with_quality(mut self, task_type: TaskType, score: f32) -> Self {
        self.quality_scores.insert(task_type, score.clamp(0.0, 1.0));
        self
    }

    /// Set tool support.
    pub fn with_tools(mut self, supports: bool) -> Self {
        self.supports_tools = supports;
        self
    }

    /// Set streaming support.
    pub fn with_streaming(mut self, supports: bool) -> Self {
        self.supports_streaming = supports;
        self
    }

    /// Get quality score for a task type.
    pub fn get_quality(&self, task_type: &TaskType) -> f32 {
        *self.quality_scores.get(task_type).unwrap_or(&0.5)
    }
}

/// User preferences for model selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    /// Weight for quality (0.0 - 1.0).
    pub quality_weight: f64,
    /// Weight for cost (0.0 - 1.0).
    pub cost_weight: f64,
    /// Weight for latency (0.0 - 1.0).
    pub latency_weight: f64,
    /// Maximum cost per request in USD.
    pub max_cost_per_request: Option<f64>,
    /// Maximum acceptable latency in milliseconds.
    pub max_latency_ms: Option<u32>,
    /// Preferred providers (empty = no preference).
    pub preferred_providers: Vec<String>,
    /// Blocked models.
    pub blocked_models: Vec<String>,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            quality_weight: 0.5,
            cost_weight: 0.3,
            latency_weight: 0.2,
            max_cost_per_request: None,
            max_latency_ms: None,
            preferred_providers: Vec::new(),
            blocked_models: Vec::new(),
        }
    }
}

impl UserPreferences {
    /// Create preferences optimized for quality.
    pub fn quality_first() -> Self {
        Self {
            quality_weight: 0.7,
            cost_weight: 0.2,
            latency_weight: 0.1,
            ..Default::default()
        }
    }

    /// Create preferences optimized for cost.
    pub fn cost_optimized() -> Self {
        Self {
            quality_weight: 0.3,
            cost_weight: 0.5,
            latency_weight: 0.2,
            ..Default::default()
        }
    }

    /// Create preferences optimized for speed.
    pub fn speed_first() -> Self {
        Self {
            quality_weight: 0.3,
            cost_weight: 0.2,
            latency_weight: 0.5,
            ..Default::default()
        }
    }

    /// Normalize weights to sum to 1.0.
    pub fn normalize(&mut self) {
        let total = self.quality_weight + self.cost_weight + self.latency_weight;
        if total > 0.0 {
            self.quality_weight /= total;
            self.cost_weight /= total;
            self.latency_weight /= total;
        }
    }
}

/// Result of scoring a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelScore {
    /// Model name.
    pub model: String,
    /// Provider name.
    pub provider: String,
    /// Total weighted score (0.0 - 1.0).
    pub total_score: f64,
    /// Quality component score.
    pub quality_score: f64,
    /// Cost component score.
    pub cost_score: f64,
    /// Latency component score.
    pub latency_score: f64,
    /// Estimated cost for this request.
    pub estimated_cost: f64,
    /// Whether the model is compatible with the task.
    pub is_compatible: bool,
    /// Reason if not compatible.
    pub incompatibility_reason: Option<String>,
}

/// Model scorer for intelligent model selection.
#[derive(Debug, Clone)]
pub struct ModelScorer {
    /// Available models.
    models: Vec<ModelCapabilities>,
    /// User preferences.
    preferences: UserPreferences,
}

impl ModelScorer {
    /// Create a new scorer with given models and preferences.
    pub fn new(models: Vec<ModelCapabilities>, preferences: UserPreferences) -> Self {
        let mut preferences = preferences;
        preferences.normalize();
        Self {
            models,
            preferences,
        }
    }

    /// Create a scorer with default models.
    pub fn with_defaults() -> Self {
        Self::new(Self::default_models(), UserPreferences::default())
    }

    /// Get default model configurations.
    pub fn default_models() -> Vec<ModelCapabilities> {
        vec![
            // OpenAI models
            ModelCapabilities::new("gpt-4o", "openai")
                .with_context_length(128000)
                .with_pricing(0.005, 0.015)
                .with_latency(800)
                .with_tools(true)
                .with_quality(TaskType::Chat, 0.95)
                .with_quality(TaskType::CodeGeneration, 0.95)
                .with_quality(TaskType::CodeReview, 0.95)
                .with_quality(TaskType::Analysis, 0.95)
                .with_quality(TaskType::Math, 0.90),
            ModelCapabilities::new("gpt-4o-mini", "openai")
                .with_context_length(128000)
                .with_pricing(0.00015, 0.0006)
                .with_latency(400)
                .with_tools(true)
                .with_quality(TaskType::Chat, 0.85)
                .with_quality(TaskType::CodeGeneration, 0.80)
                .with_quality(TaskType::Analysis, 0.80),
            ModelCapabilities::new("gpt-3.5-turbo", "openai")
                .with_context_length(16384)
                .with_pricing(0.0005, 0.0015)
                .with_latency(300)
                .with_tools(true)
                .with_quality(TaskType::Chat, 0.75)
                .with_quality(TaskType::CodeGeneration, 0.70),
            // Anthropic models
            ModelCapabilities::new("claude-3-opus", "anthropic")
                .with_context_length(200000)
                .with_pricing(0.015, 0.075)
                .with_latency(1200)
                .with_tools(true)
                .with_quality(TaskType::Chat, 0.98)
                .with_quality(TaskType::CodeGeneration, 0.98)
                .with_quality(TaskType::CodeReview, 0.98)
                .with_quality(TaskType::Analysis, 0.98)
                .with_quality(TaskType::Creative, 0.95),
            ModelCapabilities::new("claude-3-5-sonnet", "anthropic")
                .with_context_length(200000)
                .with_pricing(0.003, 0.015)
                .with_latency(600)
                .with_tools(true)
                .with_quality(TaskType::Chat, 0.95)
                .with_quality(TaskType::CodeGeneration, 0.95)
                .with_quality(TaskType::Analysis, 0.93),
            ModelCapabilities::new("claude-3-haiku", "anthropic")
                .with_context_length(200000)
                .with_pricing(0.00025, 0.00125)
                .with_latency(250)
                .with_tools(true)
                .with_quality(TaskType::Chat, 0.80)
                .with_quality(TaskType::Summarization, 0.85),
            // Google models
            ModelCapabilities::new("gemini-1.5-pro", "google")
                .with_context_length(1000000)
                .with_pricing(0.00125, 0.005)
                .with_latency(700)
                .with_tools(true)
                .with_quality(TaskType::Chat, 0.92)
                .with_quality(TaskType::Analysis, 0.90)
                .with_quality(TaskType::CodeGeneration, 0.88),
            ModelCapabilities::new("gemini-1.5-flash", "google")
                .with_context_length(1000000)
                .with_pricing(0.000075, 0.0003)
                .with_latency(200)
                .with_tools(true)
                .with_quality(TaskType::Chat, 0.82)
                .with_quality(TaskType::Summarization, 0.85),
            // Local models
            ModelCapabilities::new("llama3-70b", "ollama")
                .with_context_length(8192)
                .with_pricing(0.0, 0.0) // Free (local)
                .with_latency(2000)
                .with_tools(false)
                .with_quality(TaskType::Chat, 0.80)
                .with_quality(TaskType::CodeGeneration, 0.75),
            ModelCapabilities::new("llama3-8b", "ollama")
                .with_context_length(8192)
                .with_pricing(0.0, 0.0) // Free (local)
                .with_latency(500)
                .with_tools(false)
                .with_quality(TaskType::Chat, 0.70)
                .with_quality(TaskType::CodeGeneration, 0.65),
        ]
    }

    /// Score all models for a given task.
    pub fn score_models(&self, task: &TaskAnalysis) -> Vec<ModelScore> {
        let mut scores: Vec<ModelScore> = self
            .models
            .iter()
            .map(|model| self.score_model(model, task))
            .collect();

        // Sort by total score (descending)
        scores.sort_by(|a, b| {
            b.total_score
                .partial_cmp(&a.total_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        scores
    }

    /// Score a single model for a task.
    fn score_model(&self, model: &ModelCapabilities, task: &TaskAnalysis) -> ModelScore {
        // Check compatibility first
        let (is_compatible, incompatibility_reason) = self.check_compatibility(model, task);

        if !is_compatible {
            return ModelScore {
                model: model.name.clone(),
                provider: model.provider.clone(),
                total_score: 0.0,
                quality_score: 0.0,
                cost_score: 0.0,
                latency_score: 0.0,
                estimated_cost: 0.0,
                is_compatible: false,
                incompatibility_reason,
            };
        }

        // Calculate quality score
        let quality_score = self.calculate_quality_score(model, task);

        // Calculate cost score
        let (cost_score, estimated_cost) = self.calculate_cost_score(model, task);

        // Calculate latency score
        let latency_score = self.calculate_latency_score(model);

        // Calculate total weighted score
        let total_score = quality_score * self.preferences.quality_weight
            + cost_score * self.preferences.cost_weight
            + latency_score * self.preferences.latency_weight;

        // Apply provider preference bonus
        let total_score = if self.preferences.preferred_providers.is_empty()
            || self
                .preferences
                .preferred_providers
                .contains(&model.provider)
        {
            total_score
        } else {
            total_score * 0.9 // 10% penalty for non-preferred providers
        };

        ModelScore {
            model: model.name.clone(),
            provider: model.provider.clone(),
            total_score,
            quality_score,
            cost_score,
            latency_score,
            estimated_cost,
            is_compatible: true,
            incompatibility_reason: None,
        }
    }

    /// Check if a model is compatible with the task.
    fn check_compatibility(
        &self,
        model: &ModelCapabilities,
        task: &TaskAnalysis,
    ) -> (bool, Option<String>) {
        // Check if model is available
        if !model.available {
            return (false, Some("Model is not available".to_string()));
        }

        // Check if model is blocked
        if self.preferences.blocked_models.contains(&model.name) {
            return (false, Some("Model is blocked by user".to_string()));
        }

        // Check context length
        if task.context_length > model.context_length {
            return (
                false,
                Some(format!(
                    "Context length {} exceeds model limit {}",
                    task.context_length, model.context_length
                )),
            );
        }

        // Check tool support
        if task.requires_tools && !model.supports_tools {
            return (false, Some("Model does not support tools".to_string()));
        }

        // Check cost limit
        if let Some(max_cost) = self.preferences.max_cost_per_request {
            let estimated_cost = self.estimate_cost(model, task);
            if estimated_cost > max_cost {
                return (
                    false,
                    Some(format!(
                        "Estimated cost ${:.4} exceeds limit ${:.4}",
                        estimated_cost, max_cost
                    )),
                );
            }
        }

        // Check latency limit
        if let Some(max_latency) = self.preferences.max_latency_ms {
            if model.latency_ms > max_latency {
                return (
                    false,
                    Some(format!(
                        "Latency {}ms exceeds limit {}ms",
                        model.latency_ms, max_latency
                    )),
                );
            }
        }

        (true, None)
    }

    /// Calculate quality score (0.0 - 1.0).
    fn calculate_quality_score(&self, model: &ModelCapabilities, task: &TaskAnalysis) -> f64 {
        let base_quality = model.get_quality(&task.task_type) as f64;

        // Adjust for complexity
        let complexity_factor = match task.complexity {
            TaskComplexity::Simple => 1.0,
            TaskComplexity::Moderate => 0.95,
            TaskComplexity::Complex => 0.9,
            TaskComplexity::Specialized => 0.85,
        };

        // Higher quality models handle complex tasks better
        if task.complexity == TaskComplexity::Complex
            || task.complexity == TaskComplexity::Specialized
        {
            if base_quality >= 0.9 {
                return base_quality; // Top-tier models maintain quality
            }
        }

        base_quality * complexity_factor
    }

    /// Calculate cost score (0.0 - 1.0, higher = cheaper).
    fn calculate_cost_score(&self, model: &ModelCapabilities, task: &TaskAnalysis) -> (f64, f64) {
        let estimated_cost = self.estimate_cost(model, task);

        // Normalize cost score: $0.10 per request as baseline
        let max_cost = 0.10;
        let cost_score = 1.0 - (estimated_cost / max_cost).min(1.0);

        (cost_score, estimated_cost)
    }

    /// Estimate cost for a request.
    fn estimate_cost(&self, model: &ModelCapabilities, task: &TaskAnalysis) -> f64 {
        let input_tokens = task.estimated_input_tokens as f64;
        let output_tokens = task.estimated_output_tokens as f64;

        (input_tokens / 1000.0) * model.cost_per_1k_input
            + (output_tokens / 1000.0) * model.cost_per_1k_output
    }

    /// Calculate latency score (0.0 - 1.0, higher = faster).
    fn calculate_latency_score(&self, model: &ModelCapabilities) -> f64 {
        // 5 seconds as baseline for worst acceptable latency
        let max_latency = 5000.0;
        1.0 - (model.latency_ms as f64 / max_latency).min(1.0)
    }

    /// Get the best model for a task.
    pub fn get_best_model(&self, task: &TaskAnalysis) -> Option<ModelScore> {
        self.score_models(task)
            .into_iter()
            .find(|s| s.is_compatible)
    }

    /// Get top N models for a task.
    pub fn get_top_models(&self, task: &TaskAnalysis, n: usize) -> Vec<ModelScore> {
        self.score_models(task)
            .into_iter()
            .filter(|s| s.is_compatible)
            .take(n)
            .collect()
    }

    /// Update preferences.
    pub fn set_preferences(&mut self, preferences: UserPreferences) {
        self.preferences = preferences;
        self.preferences.normalize();
    }

    /// Add a model.
    pub fn add_model(&mut self, model: ModelCapabilities) {
        self.models.push(model);
    }

    /// Set model availability.
    pub fn set_model_available(&mut self, model_name: &str, available: bool) {
        if let Some(model) = self.models.iter_mut().find(|m| m.name == model_name) {
            model.available = available;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(task_type: TaskType, complexity: TaskComplexity) -> TaskAnalysis {
        TaskAnalysis {
            complexity,
            task_type,
            estimated_input_tokens: 100,
            estimated_output_tokens: 200,
            requires_tools: false,
            context_length: 1000,
            confidence: 0.8,
            detected_keywords: vec![],
        }
    }

    #[test]
    fn test_model_capabilities() {
        let model = ModelCapabilities::new("test-model", "test-provider")
            .with_context_length(8192)
            .with_pricing(0.01, 0.02)
            .with_latency(500)
            .with_tools(true)
            .with_quality(TaskType::Chat, 0.9);

        assert_eq!(model.name, "test-model");
        assert_eq!(model.context_length, 8192);
        assert_eq!(model.get_quality(&TaskType::Chat), 0.9);
        assert_eq!(model.get_quality(&TaskType::Math), 0.5); // Default
    }

    #[test]
    fn test_user_preferences_normalize() {
        let mut prefs = UserPreferences {
            quality_weight: 1.0,
            cost_weight: 1.0,
            latency_weight: 1.0,
            ..Default::default()
        };
        prefs.normalize();

        let total = prefs.quality_weight + prefs.cost_weight + prefs.latency_weight;
        assert!((total - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_model_scoring() {
        let scorer = ModelScorer::with_defaults();
        let task = make_task(TaskType::Chat, TaskComplexity::Simple);

        let scores = scorer.score_models(&task);
        assert!(!scores.is_empty());

        // Scores should be sorted descending
        for i in 1..scores.len() {
            assert!(scores[i - 1].total_score >= scores[i].total_score);
        }
    }

    #[test]
    fn test_compatibility_check() {
        let scorer = ModelScorer::with_defaults();

        // Task requiring tools
        let mut task = make_task(TaskType::Chat, TaskComplexity::Simple);
        task.requires_tools = true;

        let scores = scorer.score_models(&task);

        // Models without tool support should be incompatible
        for score in &scores {
            if !score.is_compatible {
                assert!(score.incompatibility_reason.is_some());
            }
        }
    }

    #[test]
    fn test_cost_preferences() {
        let models = ModelScorer::default_models();
        let scorer = ModelScorer::new(models, UserPreferences::cost_optimized());

        let task = make_task(TaskType::Chat, TaskComplexity::Simple);
        let best = scorer.get_best_model(&task).unwrap();

        // With cost optimization, cheaper models should rank higher
        assert!(best.estimated_cost < 0.01 || best.cost_score > 0.8);
    }

    #[test]
    fn test_quality_preferences() {
        let models = ModelScorer::default_models();
        let scorer = ModelScorer::new(models, UserPreferences::quality_first());

        let task = make_task(TaskType::CodeGeneration, TaskComplexity::Complex);
        let best = scorer.get_best_model(&task).unwrap();

        // With quality first, high-quality models should rank higher
        assert!(best.quality_score > 0.8);
    }

    #[test]
    fn test_context_length_filter() {
        let scorer = ModelScorer::with_defaults();

        // Task with very large context
        let mut task = make_task(TaskType::Chat, TaskComplexity::Simple);
        task.context_length = 500000; // 500k tokens

        let scores = scorer.score_models(&task);

        // Most models should be incompatible
        let compatible_count = scores.iter().filter(|s| s.is_compatible).count();
        assert!(compatible_count < scores.len());
    }
}
