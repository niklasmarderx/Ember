//! Cost Predictor - Budget management and cost estimation for LLM API calls
//!
//! Provides:
//! - Pre-request cost estimation
//! - Budget tracking and alerts
//! - Usage analytics and reporting
//! - Cost optimization recommendations

use ember_llm::model_registry::{CostEstimate, ModelMetadata, MODEL_REGISTRY};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use tokio::sync::broadcast;

/// Budget alert thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    /// Maximum allowed cost per request in USD
    pub max_cost_per_request: Option<f64>,
    /// Maximum allowed cost per hour in USD
    pub max_cost_per_hour: Option<f64>,
    /// Maximum allowed cost per day in USD
    pub max_cost_per_day: Option<f64>,
    /// Maximum allowed total cost in USD (lifetime)
    pub max_total_cost: Option<f64>,
    /// Alert threshold as percentage of budget (0.0 - 1.0)
    pub alert_threshold: f64,
    /// Whether to block requests that exceed budget
    pub enforce_limits: bool,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            max_cost_per_request: None,
            max_cost_per_hour: None,
            max_cost_per_day: Some(10.0), // Default $10/day limit
            max_total_cost: None,
            alert_threshold: 0.8, // Alert at 80% of budget
            enforce_limits: false,
        }
    }
}

/// Types of budget alerts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BudgetAlert {
    /// Request would exceed per-request limit
    RequestLimitExceeded {
        /// Estimated cost
        estimated_cost: f64,
        /// Limit
        limit: f64,
    },
    /// Approaching hourly budget
    HourlyBudgetWarning {
        /// Current spend this hour
        current_spend: f64,
        /// Hourly limit
        limit: f64,
        /// Percentage used
        percentage: f64,
    },
    /// Approaching daily budget
    DailyBudgetWarning {
        /// Current spend today
        current_spend: f64,
        /// Daily limit
        limit: f64,
        /// Percentage used
        percentage: f64,
    },
    /// Approaching total budget
    TotalBudgetWarning {
        /// Total spend
        total_spend: f64,
        /// Total limit
        limit: f64,
        /// Percentage used
        percentage: f64,
    },
    /// Budget exceeded - request blocked
    BudgetExceeded {
        /// Type of budget exceeded
        budget_type: String,
        /// Current spend
        current_spend: f64,
        /// Limit
        limit: f64,
    },
}

/// Usage record for a single request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    /// Timestamp of the request
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Model used
    pub model_id: String,
    /// Input tokens
    pub input_tokens: u32,
    /// Output tokens
    pub output_tokens: u32,
    /// Actual cost in USD
    pub cost: f64,
    /// Request ID for correlation
    pub request_id: Option<String>,
}

/// Aggregated usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageStats {
    /// Total requests made
    pub total_requests: u64,
    /// Total input tokens processed
    pub total_input_tokens: u64,
    /// Total output tokens generated
    pub total_output_tokens: u64,
    /// Total cost in USD
    pub total_cost: f64,
    /// Cost breakdown by model
    pub cost_by_model: HashMap<String, f64>,
    /// Cost breakdown by provider
    pub cost_by_provider: HashMap<String, f64>,
    /// Requests per model
    pub requests_by_model: HashMap<String, u64>,
    /// Average cost per request
    pub avg_cost_per_request: f64,
    /// Average tokens per request
    pub avg_tokens_per_request: f64,
}

/// Cost optimization recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostRecommendation {
    /// Description of the recommendation
    pub description: String,
    /// Potential savings in USD
    pub potential_savings: f64,
    /// Alternative model suggestion
    pub alternative_model: Option<String>,
    /// Priority level (1 = high, 3 = low)
    pub priority: u8,
}

/// Result of cost prediction
#[derive(Debug, Clone)]
pub struct PredictionResult {
    /// The cost estimate
    pub estimate: CostEstimate,
    /// Any budget alerts triggered
    pub alerts: Vec<BudgetAlert>,
    /// Whether the request is allowed
    pub allowed: bool,
    /// Optimization recommendations
    pub recommendations: Vec<CostRecommendation>,
}

/// Cost Predictor for managing LLM API costs
pub struct CostPredictor {
    config: RwLock<BudgetConfig>,
    usage_history: RwLock<Vec<UsageRecord>>,
    total_cost_micros: AtomicU64, // Store as microdollars for atomic ops
    alert_sender: broadcast::Sender<BudgetAlert>,
}

impl Default for CostPredictor {
    fn default() -> Self {
        Self::new(BudgetConfig::default())
    }
}

impl CostPredictor {
    /// Create a new cost predictor with the given budget configuration
    pub fn new(config: BudgetConfig) -> Self {
        let (alert_sender, _) = broadcast::channel(100);
        Self {
            config: RwLock::new(config),
            usage_history: RwLock::new(Vec::new()),
            total_cost_micros: AtomicU64::new(0),
            alert_sender,
        }
    }

    /// Subscribe to budget alerts
    pub fn subscribe_alerts(&self) -> broadcast::Receiver<BudgetAlert> {
        self.alert_sender.subscribe()
    }

    /// Update budget configuration
    pub fn set_config(&self, config: BudgetConfig) {
        let mut cfg = self.config.write().expect("config lock poisoned");
        *cfg = config;
    }

    /// Get current budget configuration
    pub fn config(&self) -> BudgetConfig {
        self.config.read().expect("config lock poisoned").clone()
    }

    /// Estimate cost for a request before making it
    pub fn estimate(
        &self,
        model_id: &str,
        input_tokens: u32,
        output_tokens: u32,
    ) -> Option<CostEstimate> {
        MODEL_REGISTRY.estimate_cost(model_id, input_tokens, output_tokens)
    }

    /// Predict cost and check against budget
    pub fn predict(
        &self,
        model_id: &str,
        estimated_input_tokens: u32,
        estimated_output_tokens: u32,
    ) -> PredictionResult {
        let estimate = self
            .estimate(model_id, estimated_input_tokens, estimated_output_tokens)
            .unwrap_or_else(|| CostEstimate {
                model_id: model_id.to_string(),
                input_tokens: estimated_input_tokens,
                output_tokens: estimated_output_tokens,
                input_cost: 0.0,
                output_cost: 0.0,
                total_cost: 0.0,
                input_price_per_1k: 0.0,
                output_price_per_1k: 0.0,
            });

        let config = self.config.read().expect("config lock poisoned");
        let mut alerts = Vec::new();
        let mut allowed = true;

        // Check per-request limit
        if let Some(limit) = config.max_cost_per_request {
            if estimate.total_cost > limit {
                alerts.push(BudgetAlert::RequestLimitExceeded {
                    estimated_cost: estimate.total_cost,
                    limit,
                });
                if config.enforce_limits {
                    allowed = false;
                }
            }
        }

        // Check hourly budget
        let hourly_spend = self.get_hourly_spend();
        if let Some(limit) = config.max_cost_per_hour {
            let projected = hourly_spend + estimate.total_cost;
            let percentage = projected / limit;

            if projected > limit {
                alerts.push(BudgetAlert::BudgetExceeded {
                    budget_type: "hourly".to_string(),
                    current_spend: hourly_spend,
                    limit,
                });
                if config.enforce_limits {
                    allowed = false;
                }
            } else if percentage >= config.alert_threshold {
                alerts.push(BudgetAlert::HourlyBudgetWarning {
                    current_spend: hourly_spend,
                    limit,
                    percentage,
                });
            }
        }

        // Check daily budget
        let daily_spend = self.get_daily_spend();
        if let Some(limit) = config.max_cost_per_day {
            let projected = daily_spend + estimate.total_cost;
            let percentage = projected / limit;

            if projected > limit {
                alerts.push(BudgetAlert::BudgetExceeded {
                    budget_type: "daily".to_string(),
                    current_spend: daily_spend,
                    limit,
                });
                if config.enforce_limits {
                    allowed = false;
                }
            } else if percentage >= config.alert_threshold {
                alerts.push(BudgetAlert::DailyBudgetWarning {
                    current_spend: daily_spend,
                    limit,
                    percentage,
                });
            }
        }

        // Check total budget
        let total_spend = self.get_total_spend();
        if let Some(limit) = config.max_total_cost {
            let projected = total_spend + estimate.total_cost;
            let percentage = projected / limit;

            if projected > limit {
                alerts.push(BudgetAlert::BudgetExceeded {
                    budget_type: "total".to_string(),
                    current_spend: total_spend,
                    limit,
                });
                if config.enforce_limits {
                    allowed = false;
                }
            } else if percentage >= config.alert_threshold {
                alerts.push(BudgetAlert::TotalBudgetWarning {
                    total_spend,
                    limit,
                    percentage,
                });
            }
        }

        // Get recommendations
        let recommendations = self.get_recommendations(model_id, &estimate);

        // Send alerts
        for alert in &alerts {
            let _ = self.alert_sender.send(alert.clone());
        }

        PredictionResult {
            estimate,
            alerts,
            allowed,
            recommendations,
        }
    }

    /// Record actual usage after a request completes
    pub fn record_usage(
        &self,
        model_id: &str,
        input_tokens: u32,
        output_tokens: u32,
        request_id: Option<String>,
    ) {
        let cost = self
            .estimate(model_id, input_tokens, output_tokens)
            .map_or(0.0, |e| e.total_cost);

        let record = UsageRecord {
            timestamp: chrono::Utc::now(),
            model_id: model_id.to_string(),
            input_tokens,
            output_tokens,
            cost,
            request_id,
        };

        // Add to history
        {
            let mut history = self
                .usage_history
                .write()
                .expect("usage_history lock poisoned");
            history.push(record);
        }

        // Update total cost
        let cost_micros = (cost * 1_000_000.0) as u64;
        self.total_cost_micros
            .fetch_add(cost_micros, Ordering::SeqCst);
    }

    /// Get total spend across all time
    pub fn get_total_spend(&self) -> f64 {
        self.total_cost_micros.load(Ordering::SeqCst) as f64 / 1_000_000.0
    }

    /// Get spend for the current hour
    pub fn get_hourly_spend(&self) -> f64 {
        let now = chrono::Utc::now();
        let hour_ago = now - chrono::Duration::hours(1);

        self.usage_history
            .read()
            .expect("usage_history lock poisoned")
            .iter()
            .filter(|r| r.timestamp > hour_ago)
            .map(|r| r.cost)
            .sum()
    }

    /// Get spend for the current day
    pub fn get_daily_spend(&self) -> f64 {
        let now = chrono::Utc::now();
        let day_ago = now - chrono::Duration::days(1);

        self.usage_history
            .read()
            .expect("usage_history lock poisoned")
            .iter()
            .filter(|r| r.timestamp > day_ago)
            .map(|r| r.cost)
            .sum()
    }

    /// Get aggregated usage statistics
    pub fn get_stats(&self) -> UsageStats {
        let history = self
            .usage_history
            .read()
            .expect("usage_history lock poisoned");

        if history.is_empty() {
            return UsageStats::default();
        }

        let mut stats = UsageStats {
            total_requests: history.len() as u64,
            ..Default::default()
        };

        for record in history.iter() {
            stats.total_input_tokens += record.input_tokens as u64;
            stats.total_output_tokens += record.output_tokens as u64;
            stats.total_cost += record.cost;

            *stats
                .cost_by_model
                .entry(record.model_id.clone())
                .or_insert(0.0) += record.cost;
            *stats
                .requests_by_model
                .entry(record.model_id.clone())
                .or_insert(0) += 1;

            // Get provider from model registry
            if let Some(model) = MODEL_REGISTRY.get(&record.model_id) {
                *stats
                    .cost_by_provider
                    .entry(model.provider.clone())
                    .or_insert(0.0) += record.cost;
            }
        }

        stats.avg_cost_per_request = stats.total_cost / stats.total_requests as f64;
        stats.avg_tokens_per_request = (stats.total_input_tokens + stats.total_output_tokens)
            as f64
            / stats.total_requests as f64;

        stats
    }

    /// Get cost optimization recommendations
    pub fn get_recommendations(
        &self,
        model_id: &str,
        estimate: &CostEstimate,
    ) -> Vec<CostRecommendation> {
        let mut recommendations = Vec::new();

        // Get current model info
        let current_model = match MODEL_REGISTRY.get(model_id) {
            Some(m) => m,
            None => return recommendations,
        };

        // Find cheaper alternatives with similar capabilities
        let alternatives = self.find_cheaper_alternatives(current_model, estimate);
        for (alt_model, savings) in alternatives {
            recommendations.push(CostRecommendation {
                description: format!(
                    "Consider using {} instead of {} for similar capabilities",
                    alt_model.name, current_model.name
                ),
                potential_savings: savings,
                alternative_model: Some(alt_model.id.clone()),
                priority: if savings > 0.1 {
                    1
                } else if savings > 0.01 {
                    2
                } else {
                    3
                },
            });
        }

        // Check if model is overkill for simple tasks
        if current_model.capabilities.reasoning && estimate.input_tokens < 500 {
            recommendations.push(CostRecommendation {
                description:
                    "Reasoning models may be overkill for short prompts. Consider a standard model."
                        .to_string(),
                potential_savings: estimate.total_cost * 0.5,
                alternative_model: Some("gpt-4o-mini".to_string()),
                priority: 2,
            });
        }

        // Check if using caching could help
        if let Some(cached_price) = current_model.cached_input_price_per_1k {
            if estimate.input_tokens > 1000 {
                let potential_savings =
                    estimate.input_cost * (1.0 - cached_price / current_model.input_price_per_1k);
                if potential_savings > 0.001 {
                    recommendations.push(CostRecommendation {
                        description:
                            "Enable prompt caching for repeated prompts to reduce input costs."
                                .to_string(),
                        potential_savings,
                        alternative_model: None,
                        priority: 2,
                    });
                }
            }
        }

        // Recommend local models for high volume
        let stats = self.get_stats();
        if stats.total_requests > 100 && stats.total_cost > 10.0 {
            recommendations.push(CostRecommendation {
                description: "High API usage detected. Consider running local models via Ollama for cost-free inference.".to_string(),
                potential_savings: stats.total_cost * 0.8,
                alternative_model: Some("llama3.2".to_string()),
                priority: 1,
            });
        }

        recommendations
    }

    fn find_cheaper_alternatives(
        &self,
        current: &ModelMetadata,
        estimate: &CostEstimate,
    ) -> Vec<(&ModelMetadata, f64)> {
        let mut alternatives = Vec::new();

        for model in MODEL_REGISTRY.all() {
            // Skip same model or more expensive models
            if model.id == current.id || model.input_price_per_1k >= current.input_price_per_1k {
                continue;
            }

            // Check if alternative has required capabilities
            let caps_match = (!current.capabilities.tools || model.capabilities.tools)
                && (!current.capabilities.vision || model.capabilities.vision)
                && (!current.capabilities.reasoning || model.capabilities.reasoning)
                && (!current.capabilities.json_mode || model.capabilities.json_mode);

            if !caps_match {
                continue;
            }

            // Calculate potential savings
            let alt_estimate = MODEL_REGISTRY.estimate_cost(
                &model.id,
                estimate.input_tokens,
                estimate.output_tokens,
            );
            if let Some(alt_est) = alt_estimate {
                let savings = estimate.total_cost - alt_est.total_cost;
                if savings > 0.0 {
                    alternatives.push((model, savings));
                }
            }
        }

        // Sort by savings descending
        alternatives.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top 3
        alternatives.into_iter().take(3).collect()
    }

    /// Clear usage history
    pub fn clear_history(&self) {
        let mut history = self
            .usage_history
            .write()
            .expect("usage_history lock poisoned");
        history.clear();
        self.total_cost_micros.store(0, Ordering::SeqCst);
    }

    /// Export usage history to JSON
    pub fn export_history(&self) -> String {
        let history = self
            .usage_history
            .read()
            .expect("usage_history lock poisoned");
        serde_json::to_string_pretty(&*history).unwrap_or_else(|_| "[]".to_string())
    }

    /// Get model information from the registry
    pub fn get_model_info(&self, model_id: &str) -> Option<&'static ModelMetadata> {
        MODEL_REGISTRY.get(model_id)
    }

    /// List all available models
    pub fn list_models(&self) -> Vec<&'static ModelMetadata> {
        MODEL_REGISTRY.all()
    }

    /// List models by provider
    pub fn list_models_by_provider(&self, provider: &str) -> Vec<&'static ModelMetadata> {
        MODEL_REGISTRY.get_by_provider(provider)
    }

    /// List models with specific capability
    pub fn list_models_by_capability(&self, capability: &str) -> Vec<&'static ModelMetadata> {
        MODEL_REGISTRY.get_by_capability(capability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_prediction() {
        let predictor = CostPredictor::default();

        let result = predictor.predict("gpt-4o-mini", 1000, 500);
        assert!(result.allowed);
        assert!(result.estimate.total_cost > 0.0);
    }

    #[test]
    fn test_budget_enforcement() {
        let config = BudgetConfig {
            max_cost_per_request: Some(0.001),
            enforce_limits: true,
            ..Default::default()
        };

        let predictor = CostPredictor::new(config);

        // This should exceed the tiny budget
        let result = predictor.predict("gpt-4o", 10000, 5000);
        assert!(!result.allowed);
        assert!(!result.alerts.is_empty());
    }

    #[test]
    fn test_usage_recording() {
        let predictor = CostPredictor::default();

        predictor.record_usage("gpt-4o-mini", 1000, 500, None);
        predictor.record_usage("gpt-4o-mini", 2000, 1000, None);

        let stats = predictor.get_stats();
        assert_eq!(stats.total_requests, 2);
        assert!(stats.total_cost > 0.0);
    }

    #[test]
    fn test_recommendations() {
        let predictor = CostPredictor::default();

        // Use an expensive model
        let result = predictor.predict("claude-3-opus-20240229", 5000, 2000);

        // Should have recommendations for cheaper alternatives
        assert!(!result.recommendations.is_empty());
    }

    #[test]
    fn test_model_listing() {
        let predictor = CostPredictor::default();

        let all_models = predictor.list_models();
        assert!(!all_models.is_empty());

        let openai_models = predictor.list_models_by_provider("openai");
        assert!(!openai_models.is_empty());

        let vision_models = predictor.list_models_by_capability("vision");
        assert!(!vision_models.is_empty());
    }
}
