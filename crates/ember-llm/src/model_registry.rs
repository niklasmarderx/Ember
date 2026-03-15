//! Model Registry - Central repository for all model information and pricing
//!
//! Provides comprehensive information about available models including:
//! - Context windows and output limits
//! - Pricing (input/output per 1K tokens)
//! - Capabilities (tools, vision, reasoning)
//! - Provider information

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Information about a specific model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    /// Model identifier (e.g., "gpt-4o", "claude-3.5-sonnet")
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Provider name
    pub provider: String,
    /// Optional description
    pub description: Option<String>,
    /// Context window size in tokens
    pub context_window: u32,
    /// Maximum output tokens
    pub max_output_tokens: u32,
    /// Price per 1K input tokens in USD
    pub input_price_per_1k: f64,
    /// Price per 1K output tokens in USD
    pub output_price_per_1k: f64,
    /// Price per 1K cached input tokens (if supported)
    pub cached_input_price_per_1k: Option<f64>,
    /// Model capabilities
    pub capabilities: ModelCapabilities,
    /// Release date (YYYY-MM-DD format)
    pub released: Option<String>,
    /// Whether the model is deprecated
    pub deprecated: bool,
    /// Suggested replacement model if deprecated
    pub replacement: Option<String>,
}

/// Model capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Supports function/tool calling
    pub tools: bool,
    /// Supports vision/image input
    pub vision: bool,
    /// Supports audio input
    pub audio: bool,
    /// Supports streaming
    pub streaming: bool,
    /// Supports JSON mode
    pub json_mode: bool,
    /// Advanced reasoning (like o1, DeepSeek R1)
    pub reasoning: bool,
    /// Supports code execution
    pub code_execution: bool,
}

/// Central registry for all supported models
#[derive(Debug, Clone)]
pub struct ModelRegistry {
    models: HashMap<String, ModelMetadata>,
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRegistry {
    /// Create a new model registry with all known models
    pub fn new() -> Self {
        let mut registry = Self {
            models: HashMap::new(),
        };
        registry.register_openai_models();
        registry.register_anthropic_models();
        registry.register_google_models();
        registry.register_mistral_models();
        registry.register_deepseek_models();
        registry.register_groq_models();
        registry.register_xai_models();
        registry.register_ollama_models();
        registry
    }

    /// Get model metadata by ID
    pub fn get(&self, model_id: &str) -> Option<&ModelMetadata> {
        self.models.get(model_id)
    }

    /// Get all models for a specific provider
    pub fn get_by_provider(&self, provider: &str) -> Vec<&ModelMetadata> {
        self.models
            .values()
            .filter(|m| m.provider == provider)
            .collect()
    }

    /// Get all models with a specific capability
    pub fn get_by_capability(&self, capability: &str) -> Vec<&ModelMetadata> {
        self.models
            .values()
            .filter(|m| match capability {
                "tools" => m.capabilities.tools,
                "vision" => m.capabilities.vision,
                "audio" => m.capabilities.audio,
                "reasoning" => m.capabilities.reasoning,
                "json_mode" => m.capabilities.json_mode,
                _ => false,
            })
            .collect()
    }

    /// Get all available models
    pub fn all(&self) -> Vec<&ModelMetadata> {
        self.models.values().collect()
    }

    /// Estimate cost for a request
    pub fn estimate_cost(
        &self,
        model_id: &str,
        input_tokens: u32,
        output_tokens: u32,
    ) -> Option<CostEstimate> {
        let model = self.get(model_id)?;
        let input_cost = (input_tokens as f64 / 1000.0) * model.input_price_per_1k;
        let output_cost = (output_tokens as f64 / 1000.0) * model.output_price_per_1k;

        Some(CostEstimate {
            model_id: model_id.to_string(),
            input_tokens,
            output_tokens,
            input_cost,
            output_cost,
            total_cost: input_cost + output_cost,
            input_price_per_1k: model.input_price_per_1k,
            output_price_per_1k: model.output_price_per_1k,
        })
    }

    /// Register a custom model
    pub fn register(&mut self, model: ModelMetadata) {
        self.models.insert(model.id.clone(), model);
    }

    fn register_openai_models(&mut self) {
        // GPT-4o models
        self.register(ModelMetadata {
            id: "gpt-4o".to_string(),
            name: "GPT-4o".to_string(),
            provider: "openai".to_string(),
            description: Some("Most capable GPT-4 model, multimodal".to_string()),
            context_window: 128000,
            max_output_tokens: 16384,
            input_price_per_1k: 0.0025,
            output_price_per_1k: 0.01,
            cached_input_price_per_1k: Some(0.00125),
            capabilities: ModelCapabilities {
                tools: true,
                vision: true,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: false,
            },
            released: Some("2024-05-13".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "gpt-4o-mini".to_string(),
            name: "GPT-4o Mini".to_string(),
            provider: "openai".to_string(),
            description: Some("Affordable, fast GPT-4o variant".to_string()),
            context_window: 128000,
            max_output_tokens: 16384,
            input_price_per_1k: 0.00015,
            output_price_per_1k: 0.0006,
            cached_input_price_per_1k: Some(0.000075),
            capabilities: ModelCapabilities {
                tools: true,
                vision: true,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: false,
            },
            released: Some("2024-07-18".to_string()),
            deprecated: false,
            replacement: None,
        });

        // o1 reasoning models
        self.register(ModelMetadata {
            id: "o1".to_string(),
            name: "o1".to_string(),
            provider: "openai".to_string(),
            description: Some("Advanced reasoning model".to_string()),
            context_window: 200000,
            max_output_tokens: 100000,
            input_price_per_1k: 0.015,
            output_price_per_1k: 0.06,
            cached_input_price_per_1k: Some(0.0075),
            capabilities: ModelCapabilities {
                tools: true,
                vision: true,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: true,
                code_execution: false,
            },
            released: Some("2024-12-17".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "o1-mini".to_string(),
            name: "o1 Mini".to_string(),
            provider: "openai".to_string(),
            description: Some("Fast reasoning model for STEM".to_string()),
            context_window: 128000,
            max_output_tokens: 65536,
            input_price_per_1k: 0.003,
            output_price_per_1k: 0.012,
            cached_input_price_per_1k: Some(0.0015),
            capabilities: ModelCapabilities {
                tools: false,
                vision: false,
                audio: false,
                streaming: true,
                json_mode: false,
                reasoning: true,
                code_execution: false,
            },
            released: Some("2024-09-12".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "o3-mini".to_string(),
            name: "o3 Mini".to_string(),
            provider: "openai".to_string(),
            description: Some("Latest reasoning model, cost-effective".to_string()),
            context_window: 200000,
            max_output_tokens: 100000,
            input_price_per_1k: 0.0011,
            output_price_per_1k: 0.0044,
            cached_input_price_per_1k: Some(0.00055),
            capabilities: ModelCapabilities {
                tools: true,
                vision: false,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: true,
                code_execution: false,
            },
            released: Some("2025-01-31".to_string()),
            deprecated: false,
            replacement: None,
        });
    }

    fn register_anthropic_models(&mut self) {
        self.register(ModelMetadata {
            id: "claude-3-5-sonnet-20241022".to_string(),
            name: "Claude 3.5 Sonnet".to_string(),
            provider: "anthropic".to_string(),
            description: Some("Best balance of intelligence and speed".to_string()),
            context_window: 200000,
            max_output_tokens: 8192,
            input_price_per_1k: 0.003,
            output_price_per_1k: 0.015,
            cached_input_price_per_1k: Some(0.0003),
            capabilities: ModelCapabilities {
                tools: true,
                vision: true,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: true,
            },
            released: Some("2024-10-22".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "claude-3-5-haiku-20241022".to_string(),
            name: "Claude 3.5 Haiku".to_string(),
            provider: "anthropic".to_string(),
            description: Some("Fastest Claude model".to_string()),
            context_window: 200000,
            max_output_tokens: 8192,
            input_price_per_1k: 0.0008,
            output_price_per_1k: 0.004,
            cached_input_price_per_1k: Some(0.00008),
            capabilities: ModelCapabilities {
                tools: true,
                vision: true,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: false,
            },
            released: Some("2024-10-22".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "claude-3-opus-20240229".to_string(),
            name: "Claude 3 Opus".to_string(),
            provider: "anthropic".to_string(),
            description: Some("Most capable Claude model".to_string()),
            context_window: 200000,
            max_output_tokens: 4096,
            input_price_per_1k: 0.015,
            output_price_per_1k: 0.075,
            cached_input_price_per_1k: Some(0.0015),
            capabilities: ModelCapabilities {
                tools: true,
                vision: true,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: false,
            },
            released: Some("2024-02-29".to_string()),
            deprecated: false,
            replacement: None,
        });
    }

    fn register_google_models(&mut self) {
        self.register(ModelMetadata {
            id: "gemini-2.0-flash-exp".to_string(),
            name: "Gemini 2.0 Flash".to_string(),
            provider: "gemini".to_string(),
            description: Some("Latest Gemini model, multimodal, fast".to_string()),
            context_window: 1000000,
            max_output_tokens: 8192,
            input_price_per_1k: 0.0,
            output_price_per_1k: 0.0,
            cached_input_price_per_1k: None,
            capabilities: ModelCapabilities {
                tools: true,
                vision: true,
                audio: true,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: true,
            },
            released: Some("2024-12-11".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "gemini-1.5-pro".to_string(),
            name: "Gemini 1.5 Pro".to_string(),
            provider: "gemini".to_string(),
            description: Some("Best Gemini for complex tasks".to_string()),
            context_window: 2000000,
            max_output_tokens: 8192,
            input_price_per_1k: 0.00125,
            output_price_per_1k: 0.005,
            cached_input_price_per_1k: Some(0.0003125),
            capabilities: ModelCapabilities {
                tools: true,
                vision: true,
                audio: true,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: true,
            },
            released: Some("2024-02-15".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "gemini-1.5-flash".to_string(),
            name: "Gemini 1.5 Flash".to_string(),
            provider: "gemini".to_string(),
            description: Some("Fast and efficient Gemini".to_string()),
            context_window: 1000000,
            max_output_tokens: 8192,
            input_price_per_1k: 0.000075,
            output_price_per_1k: 0.0003,
            cached_input_price_per_1k: Some(0.00001875),
            capabilities: ModelCapabilities {
                tools: true,
                vision: true,
                audio: true,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: false,
            },
            released: Some("2024-05-14".to_string()),
            deprecated: false,
            replacement: None,
        });
    }

    fn register_mistral_models(&mut self) {
        self.register(ModelMetadata {
            id: "mistral-large-latest".to_string(),
            name: "Mistral Large".to_string(),
            provider: "mistral".to_string(),
            description: Some("Most capable Mistral model".to_string()),
            context_window: 128000,
            max_output_tokens: 8192,
            input_price_per_1k: 0.002,
            output_price_per_1k: 0.006,
            cached_input_price_per_1k: None,
            capabilities: ModelCapabilities {
                tools: true,
                vision: false,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: false,
            },
            released: Some("2024-11-18".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "mistral-small-latest".to_string(),
            name: "Mistral Small".to_string(),
            provider: "mistral".to_string(),
            description: Some("Cost-effective Mistral model".to_string()),
            context_window: 128000,
            max_output_tokens: 8192,
            input_price_per_1k: 0.0002,
            output_price_per_1k: 0.0006,
            cached_input_price_per_1k: None,
            capabilities: ModelCapabilities {
                tools: true,
                vision: false,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: false,
            },
            released: Some("2024-09-18".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "codestral-latest".to_string(),
            name: "Codestral".to_string(),
            provider: "mistral".to_string(),
            description: Some("Specialized for code generation".to_string()),
            context_window: 32000,
            max_output_tokens: 8192,
            input_price_per_1k: 0.0002,
            output_price_per_1k: 0.0006,
            cached_input_price_per_1k: None,
            capabilities: ModelCapabilities {
                tools: true,
                vision: false,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: true,
            },
            released: Some("2024-05-29".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "pixtral-large-latest".to_string(),
            name: "Pixtral Large".to_string(),
            provider: "mistral".to_string(),
            description: Some("Multimodal Mistral with vision".to_string()),
            context_window: 128000,
            max_output_tokens: 8192,
            input_price_per_1k: 0.002,
            output_price_per_1k: 0.006,
            cached_input_price_per_1k: None,
            capabilities: ModelCapabilities {
                tools: true,
                vision: true,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: false,
            },
            released: Some("2024-11-18".to_string()),
            deprecated: false,
            replacement: None,
        });
    }

    fn register_deepseek_models(&mut self) {
        self.register(ModelMetadata {
            id: "deepseek-chat".to_string(),
            name: "DeepSeek V3".to_string(),
            provider: "deepseek".to_string(),
            description: Some("Most capable DeepSeek model".to_string()),
            context_window: 64000,
            max_output_tokens: 8192,
            input_price_per_1k: 0.00014,
            output_price_per_1k: 0.00028,
            cached_input_price_per_1k: Some(0.000014),
            capabilities: ModelCapabilities {
                tools: true,
                vision: false,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: false,
            },
            released: Some("2024-12-25".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "deepseek-reasoner".to_string(),
            name: "DeepSeek R1".to_string(),
            provider: "deepseek".to_string(),
            description: Some("Advanced reasoning model with chain-of-thought".to_string()),
            context_window: 64000,
            max_output_tokens: 8192,
            input_price_per_1k: 0.00055,
            output_price_per_1k: 0.00219,
            cached_input_price_per_1k: Some(0.00014),
            capabilities: ModelCapabilities {
                tools: false,
                vision: false,
                audio: false,
                streaming: true,
                json_mode: false,
                reasoning: true,
                code_execution: false,
            },
            released: Some("2025-01-20".to_string()),
            deprecated: false,
            replacement: None,
        });
    }

    fn register_groq_models(&mut self) {
        self.register(ModelMetadata {
            id: "llama-3.3-70b-versatile".to_string(),
            name: "Llama 3.3 70B".to_string(),
            provider: "groq".to_string(),
            description: Some("Fast inference via Groq".to_string()),
            context_window: 128000,
            max_output_tokens: 32768,
            input_price_per_1k: 0.00059,
            output_price_per_1k: 0.00079,
            cached_input_price_per_1k: None,
            capabilities: ModelCapabilities {
                tools: true,
                vision: false,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: false,
            },
            released: Some("2024-12-06".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "mixtral-8x7b-32768".to_string(),
            name: "Mixtral 8x7B".to_string(),
            provider: "groq".to_string(),
            description: Some("Fast MoE model via Groq".to_string()),
            context_window: 32768,
            max_output_tokens: 8192,
            input_price_per_1k: 0.00024,
            output_price_per_1k: 0.00024,
            cached_input_price_per_1k: None,
            capabilities: ModelCapabilities {
                tools: true,
                vision: false,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: false,
            },
            released: Some("2023-12-11".to_string()),
            deprecated: false,
            replacement: None,
        });
    }

    fn register_xai_models(&mut self) {
        self.register(ModelMetadata {
            id: "grok-2".to_string(),
            name: "Grok 2".to_string(),
            provider: "xai".to_string(),
            description: Some("Most capable Grok model for complex reasoning".to_string()),
            context_window: 131072,
            max_output_tokens: 8192,
            input_price_per_1k: 0.002,
            output_price_per_1k: 0.01,
            cached_input_price_per_1k: None,
            capabilities: ModelCapabilities {
                tools: true,
                vision: false,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: false,
            },
            released: Some("2024-12-12".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "grok-2-mini".to_string(),
            name: "Grok 2 Mini".to_string(),
            provider: "xai".to_string(),
            description: Some("Fast, efficient Grok model".to_string()),
            context_window: 131072,
            max_output_tokens: 8192,
            input_price_per_1k: 0.0002,
            output_price_per_1k: 0.001,
            cached_input_price_per_1k: None,
            capabilities: ModelCapabilities {
                tools: true,
                vision: false,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: false,
            },
            released: Some("2024-12-12".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "grok-2-vision-1212".to_string(),
            name: "Grok 2 Vision".to_string(),
            provider: "xai".to_string(),
            description: Some("Grok 2 with vision capabilities".to_string()),
            context_window: 32768,
            max_output_tokens: 8192,
            input_price_per_1k: 0.002,
            output_price_per_1k: 0.01,
            cached_input_price_per_1k: None,
            capabilities: ModelCapabilities {
                tools: true,
                vision: true,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: false,
            },
            released: Some("2024-12-12".to_string()),
            deprecated: false,
            replacement: None,
        });
    }

    fn register_ollama_models(&mut self) {
        self.register(ModelMetadata {
            id: "llama3.2".to_string(),
            name: "Llama 3.2".to_string(),
            provider: "ollama".to_string(),
            description: Some("Local Llama 3.2 model".to_string()),
            context_window: 128000,
            max_output_tokens: 4096,
            input_price_per_1k: 0.0,
            output_price_per_1k: 0.0,
            cached_input_price_per_1k: None,
            capabilities: ModelCapabilities {
                tools: true,
                vision: false,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: false,
            },
            released: Some("2024-09-25".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "qwen2.5-coder".to_string(),
            name: "Qwen 2.5 Coder".to_string(),
            provider: "ollama".to_string(),
            description: Some("Local coding model".to_string()),
            context_window: 32000,
            max_output_tokens: 4096,
            input_price_per_1k: 0.0,
            output_price_per_1k: 0.0,
            cached_input_price_per_1k: None,
            capabilities: ModelCapabilities {
                tools: true,
                vision: false,
                audio: false,
                streaming: true,
                json_mode: true,
                reasoning: false,
                code_execution: true,
            },
            released: Some("2024-11-11".to_string()),
            deprecated: false,
            replacement: None,
        });

        self.register(ModelMetadata {
            id: "deepseek-r1".to_string(),
            name: "DeepSeek R1 (Local)".to_string(),
            provider: "ollama".to_string(),
            description: Some("Local DeepSeek R1 reasoning model".to_string()),
            context_window: 64000,
            max_output_tokens: 8192,
            input_price_per_1k: 0.0,
            output_price_per_1k: 0.0,
            cached_input_price_per_1k: None,
            capabilities: ModelCapabilities {
                tools: false,
                vision: false,
                audio: false,
                streaming: true,
                json_mode: false,
                reasoning: true,
                code_execution: false,
            },
            released: Some("2025-01-20".to_string()),
            deprecated: false,
            replacement: None,
        });
    }
}

/// Cost estimate for a model request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    /// Model ID
    pub model_id: String,
    /// Estimated input tokens
    pub input_tokens: u32,
    /// Estimated output tokens
    pub output_tokens: u32,
    /// Cost for input tokens in USD
    pub input_cost: f64,
    /// Cost for output tokens in USD
    pub output_cost: f64,
    /// Total estimated cost in USD
    pub total_cost: f64,
    /// Price per 1K input tokens
    pub input_price_per_1k: f64,
    /// Price per 1K output tokens
    pub output_price_per_1k: f64,
}

impl CostEstimate {
    /// Format cost as human-readable string
    pub fn format(&self) -> String {
        if self.total_cost == 0.0 {
            return "Free (local model)".to_string();
        }

        if self.total_cost < 0.01 {
            format!("${:.4}", self.total_cost)
        } else {
            format!("${:.2}", self.total_cost)
        }
    }
}

/// Global model registry instance
pub static MODEL_REGISTRY: std::sync::LazyLock<ModelRegistry> =
    std::sync::LazyLock::new(ModelRegistry::new);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_contains_models() {
        let registry = ModelRegistry::new();
        assert!(registry.get("gpt-4o").is_some());
        assert!(registry.get("claude-3-5-sonnet-20241022").is_some());
        assert!(registry.get("gemini-2.0-flash-exp").is_some());
        assert!(registry.get("deepseek-chat").is_some());
    }

    #[test]
    fn test_cost_estimation() {
        let registry = ModelRegistry::new();
        let estimate = registry.estimate_cost("gpt-4o-mini", 1000, 500);
        assert!(estimate.is_some());
        let estimate = estimate.unwrap();
        assert!(estimate.total_cost > 0.0);
    }

    #[test]
    fn test_get_by_provider() {
        let registry = ModelRegistry::new();
        let openai_models = registry.get_by_provider("openai");
        assert!(!openai_models.is_empty());
    }

    #[test]
    fn test_get_by_capability() {
        let registry = ModelRegistry::new();
        let vision_models = registry.get_by_capability("vision");
        assert!(!vision_models.is_empty());

        let reasoning_models = registry.get_by_capability("reasoning");
        assert!(!reasoning_models.is_empty());
    }
}
