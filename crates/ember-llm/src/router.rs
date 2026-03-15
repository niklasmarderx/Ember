//! Multi-model router for intelligent provider selection

use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{CompletionRequest, CompletionResponse, LLMProvider, Result};

/// Rule for routing requests to specific providers
#[derive(Debug, Clone)]
pub struct RoutingRule {
    /// Regex pattern to match against the message
    pub pattern: Regex,
    /// Provider name to use when pattern matches
    pub provider: String,
    /// Priority (higher = checked first)
    pub priority: i32,
}

impl RoutingRule {
    /// Create a new routing rule
    pub fn new(pattern: &str, provider: impl Into<String>) -> Result<Self> {
        Ok(Self {
            pattern: Regex::new(pattern)
                .map_err(|e| crate::Error::ConfigError(format!("Invalid regex pattern: {}", e)))?,
            provider: provider.into(),
            priority: 0,
        })
    }

    /// Set the priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Check if the message matches this rule
    pub fn matches(&self, message: &str) -> bool {
        self.pattern.is_match(message)
    }
}

/// Router that selects the appropriate LLM provider based on the request
pub struct LLMRouter {
    providers: HashMap<String, Arc<dyn LLMProvider>>,
    rules: Vec<RoutingRule>,
    default_provider: String,
}

impl LLMRouter {
    /// Create a new router with a default provider
    pub fn new(default_provider: impl Into<String>) -> Self {
        Self {
            providers: HashMap::new(),
            rules: Vec::new(),
            default_provider: default_provider.into(),
        }
    }

    /// Add a provider to the router
    pub fn with_provider(
        mut self,
        name: impl Into<String>,
        provider: Arc<dyn LLMProvider>,
    ) -> Self {
        self.providers.insert(name.into(), provider);
        self
    }

    /// Add a routing rule
    pub fn with_rule(mut self, rule: RoutingRule) -> Self {
        self.rules.push(rule);
        // Keep rules sorted by priority (descending)
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
        self
    }

    /// Get the appropriate provider for a message
    pub fn route(&self, message: &str) -> Option<&Arc<dyn LLMProvider>> {
        // Check rules in priority order
        for rule in &self.rules {
            if rule.matches(message) {
                if let Some(provider) = self.providers.get(&rule.provider) {
                    return Some(provider);
                }
            }
        }

        // Fall back to default provider
        self.providers.get(&self.default_provider)
    }

    /// Get a provider by name
    pub fn get_provider(&self, name: &str) -> Option<&Arc<dyn LLMProvider>> {
        self.providers.get(name)
    }

    /// List all available providers
    pub fn list_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    /// Send a completion request using the appropriate provider
    pub async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        // Get the last user message for routing
        let last_message = request
            .messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, crate::Role::User))
            .map(|m| m.content.as_str())
            .unwrap_or("");

        let provider = self
            .route(last_message)
            .ok_or_else(|| crate::Error::ConfigError("No provider available".to_string()))?;

        provider.complete(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing_rule() {
        let rule = RoutingRule::new(r"(?i)code|programming|debug", "anthropic").unwrap();

        assert!(rule.matches("Help me debug this code"));
        assert!(rule.matches("Programming question"));
        assert!(!rule.matches("What's the weather?"));
    }

    #[test]
    fn test_routing_rule_priority() {
        let rule1 = RoutingRule::new(".*", "default").unwrap().with_priority(0);
        let rule2 = RoutingRule::new("code", "anthropic")
            .unwrap()
            .with_priority(10);

        assert_eq!(rule1.priority, 0);
        assert_eq!(rule2.priority, 10);
    }
}
