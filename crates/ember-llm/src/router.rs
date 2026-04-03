//! Multi-model router for intelligent provider selection.
//!
//! Provides two complementary routing strategies:
//!
//! - **`LLMRouter`** – pattern-based routing: routes requests to providers based
//!   on regex rules applied to the last user message.
//!
//! - **`FallbackRouter`** – alias-based routing with automatic failover: resolves
//!   human-readable model aliases (`fast`, `smart`, `code`, `local`) to an ordered
//!   list of `ModelCandidate` entries and retries the next candidate whenever a
//!   request fails with a retryable error (rate-limit / server error).

use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, warn};

use crate::{CompletionRequest, CompletionResponse, Error, LLMProvider, Result};

// ──────────────────────────────────────────────────────────────────────────────
// Model aliases and candidates
// ──────────────────────────────────────────────────────────────────────────────

/// A single model/provider option within an alias group.
///
/// Candidates within an alias are tried in priority order (lowest index first).
/// The `cost_per_million_input` field is informational; the ordering in the
/// alias definition controls the actual preference.
#[derive(Debug, Clone)]
pub struct ModelCandidate {
    /// Provider name (must match a key in the registry you build at runtime,
    /// or be handled by [`FallbackRouter`]).
    pub provider: &'static str,
    /// Concrete model identifier forwarded in the API request.
    pub model: String,
    /// Approximate cost in USD per 1 million input tokens (0.0 = unknown/local).
    pub cost_per_million_input: f64,
}

impl ModelCandidate {
    /// Convenience constructor.
    pub fn new(
        provider: &'static str,
        model: impl Into<String>,
        cost_per_million_input: f64,
    ) -> Self {
        Self {
            provider,
            model: model.into(),
            cost_per_million_input,
        }
    }
}

/// A named alias that expands to an ordered list of [`ModelCandidate`]s.
#[derive(Debug, Clone)]
pub struct ModelAlias {
    /// The short alias users type, e.g. `"fast"`.
    pub alias: &'static str,
    /// Candidates in priority order (first = preferred).
    pub candidates: Vec<ModelCandidate>,
}

/// Return the built-in model alias table.
///
/// Aliases:
/// - `fast`  – cheapest models first (haiku → gpt-4o-mini → gemini-flash)
/// - `smart` – highest-quality models first (opus → gpt-4o → gemini-pro)
/// - `code`  – code-optimised models (sonnet → gpt-4o → deepseek-coder)
/// - `local` – locally-served Ollama models (no API cost)
pub fn builtin_aliases() -> Vec<ModelAlias> {
    vec![
        ModelAlias {
            alias: "fast",
            candidates: vec![
                ModelCandidate::new("anthropic", "claude-3-haiku-20240307", 1.0),
                ModelCandidate::new("openai", "gpt-4o-mini", 0.15),
                ModelCandidate::new("gemini", "gemini-1.5-flash", 0.075),
            ],
        },
        ModelAlias {
            alias: "smart",
            candidates: vec![
                ModelCandidate::new("anthropic", "claude-3-opus-20240229", 15.0),
                ModelCandidate::new("openai", "gpt-4o", 2.50),
                ModelCandidate::new("gemini", "gemini-1.5-pro", 3.50),
            ],
        },
        ModelAlias {
            alias: "code",
            candidates: vec![
                ModelCandidate::new("anthropic", "claude-3-5-sonnet-20241022", 3.0),
                ModelCandidate::new("openai", "gpt-4o", 2.50),
                ModelCandidate::new("deepseek", "deepseek-coder", 0.14),
            ],
        },
        ModelAlias {
            alias: "local",
            candidates: vec![
                ModelCandidate::new("ollama", "llama3.2", 0.0),
                ModelCandidate::new("ollama", "qwen2.5-coder", 0.0),
                ModelCandidate::new("ollama", "deepseek-r1:8b", 0.0),
            ],
        },
    ]
}

/// Resolve a model alias string to its ordered candidate list.
///
/// Returns an empty `Vec` when `alias` is not a known alias — the caller
/// should then treat the input as a literal model name.
pub fn resolve_model_alias(alias: &str) -> Vec<ModelCandidate> {
    let lower = alias.to_lowercase();
    builtin_aliases()
        .into_iter()
        .find(|a| a.alias == lower.as_str())
        .map(|a| a.candidates)
        .unwrap_or_default()
}

/// Return `true` when a string is a known built-in alias.
pub fn is_model_alias(name: &str) -> bool {
    let lower = name.to_lowercase();
    builtin_aliases().iter().any(|a| a.alias == lower.as_str())
}

// ──────────────────────────────────────────────────────────────────────────────
// FallbackRouter
// ──────────────────────────────────────────────────────────────────────────────

/// Determines whether a provider error is worth retrying on another candidate.
fn is_retryable(err: &Error) -> bool {
    let msg = err.to_string().to_lowercase();
    // Rate-limit (429) or server error (5xx)
    msg.contains("429")
        || msg.contains("rate limit")
        || msg.contains("too many requests")
        || msg.contains("500")
        || msg.contains("502")
        || msg.contains("503")
        || msg.contains("server error")
        || msg.contains("service unavailable")
}

/// A router that resolves model aliases and falls back to the next candidate
/// when a request fails with a retryable error.
///
/// # Example
/// ```rust,no_run
/// use std::sync::Arc;
/// use ember_llm::{OpenAIProvider, OllamaProvider, CompletionRequest, Message};
/// use ember_llm::router::FallbackRouter;
///
/// # async fn example() -> anyhow::Result<()> {
/// let mut router = FallbackRouter::new();
/// // Register providers you actually have configured
/// // router.register("openai", Arc::new(OpenAIProvider::from_env()?));
/// // router.register("ollama", Arc::new(OllamaProvider::new()));
///
/// let request = CompletionRequest::new("gpt-4o")
///     .with_message(Message::user("Hello!"));
/// let response = router.complete(request).await?;
/// println!("{}", response.content);
/// # Ok(())
/// # }
/// ```
pub struct FallbackRouter {
    /// Registered providers, keyed by provider name.
    providers: HashMap<String, Arc<dyn LLMProvider>>,
}

impl Default for FallbackRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl FallbackRouter {
    /// Create an empty router.
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Register a provider under `name`.
    pub fn register(&mut self, name: impl Into<String>, provider: Arc<dyn LLMProvider>) {
        self.providers.insert(name.into(), provider);
    }

    /// Builder-style variant of [`register`].
    pub fn with_provider(mut self, name: impl Into<String>, provider: Arc<dyn LLMProvider>) -> Self {
        self.register(name, provider);
        self
    }

    /// Return all registered provider names.
    pub fn provider_names(&self) -> Vec<&str> {
        self.providers.keys().map(String::as_str).collect()
    }

    /// Resolve an alias to candidates. If the model string is not a known
    /// alias, wrap it in a single-element candidate list with an unknown
    /// provider (`""`).
    fn candidates_for(&self, model: &str) -> Vec<ModelCandidate> {
        let resolved = resolve_model_alias(model);
        if !resolved.is_empty() {
            resolved
        } else {
            // Treat as a literal model name; pick the first registered provider
            // that has a matching provider hint in the model string, or fall
            // back to the first registered provider.
            let provider = self
                .providers
                .keys()
                .find(|p| model.to_lowercase().contains(p.as_str()))
                .cloned()
                .or_else(|| self.providers.keys().next().cloned())
                .unwrap_or_default();
            vec![ModelCandidate::new(
                Box::leak(provider.into_boxed_str()),
                model,
                0.0,
            )]
        }
    }

    /// Send a completion request, automatically failing over to the next
    /// candidate when a retryable error is encountered.
    ///
    /// The model in `request` may be a literal model name or a built-in alias.
    pub async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let alias_or_model = request.model.clone();
        let candidates = self.candidates_for(&alias_or_model);

        if candidates.is_empty() {
            return Err(Error::ConfigError(format!(
                "No candidates found for model '{}'",
                alias_or_model
            )));
        }

        let mut last_err: Option<Error> = None;

        for candidate in &candidates {
            let provider = match self.providers.get(candidate.provider) {
                Some(p) => p,
                None => {
                    debug!(
                        provider = candidate.provider,
                        "Provider not registered, skipping candidate"
                    );
                    continue;
                }
            };

            // Rewrite the request to use the candidate's concrete model.
            let mut req = request.clone();
            req.model = candidate.model.clone();

            debug!(
                provider = candidate.provider,
                model = %candidate.model,
                "Trying candidate"
            );

            match provider.complete(req).await {
                Ok(response) => return Ok(response),
                Err(e) if is_retryable(&e) => {
                    warn!(
                        provider = candidate.provider,
                        model = %candidate.model,
                        error = %e,
                        "Retryable error, trying next candidate"
                    );
                    last_err = Some(e);
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_err.unwrap_or_else(|| {
            Error::ConfigError(format!(
                "All candidates exhausted for model '{}'",
                alias_or_model
            ))
        }))
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// LLMRouter (pattern-based, unchanged from original + minor additions)
// ──────────────────────────────────────────────────────────────────────────────

/// Rule for routing requests to specific providers based on message content.
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
    /// Create a new routing rule.
    pub fn new(pattern: &str, provider: impl Into<String>) -> Result<Self> {
        Ok(Self {
            pattern: Regex::new(pattern)
                .map_err(|e| crate::Error::ConfigError(format!("Invalid regex pattern: {}", e)))?,
            provider: provider.into(),
            priority: 0,
        })
    }

    /// Set the priority.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Check if the message matches this rule.
    pub fn matches(&self, message: &str) -> bool {
        self.pattern.is_match(message)
    }
}

/// Router that selects the appropriate LLM provider based on the request.
pub struct LLMRouter {
    providers: HashMap<String, Arc<dyn LLMProvider>>,
    rules: Vec<RoutingRule>,
    default_provider: String,
}

impl LLMRouter {
    /// Create a new router with a default provider.
    pub fn new(default_provider: impl Into<String>) -> Self {
        Self {
            providers: HashMap::new(),
            rules: Vec::new(),
            default_provider: default_provider.into(),
        }
    }

    /// Add a provider to the router.
    pub fn with_provider(
        mut self,
        name: impl Into<String>,
        provider: Arc<dyn LLMProvider>,
    ) -> Self {
        self.providers.insert(name.into(), provider);
        self
    }

    /// Add a routing rule.
    pub fn with_rule(mut self, rule: RoutingRule) -> Self {
        self.rules.push(rule);
        // Keep rules sorted by priority (descending)
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
        self
    }

    /// Get the appropriate provider for a message.
    pub fn route(&self, message: &str) -> Option<&Arc<dyn LLMProvider>> {
        for rule in &self.rules {
            if rule.matches(message) {
                if let Some(provider) = self.providers.get(&rule.provider) {
                    return Some(provider);
                }
            }
        }
        self.providers.get(&self.default_provider)
    }

    /// Get a provider by name.
    pub fn get_provider(&self, name: &str) -> Option<&Arc<dyn LLMProvider>> {
        self.providers.get(name)
    }

    /// List all available providers.
    pub fn list_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    /// Send a completion request using the appropriate provider.
    pub async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
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

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // --- RoutingRule ---------------------------------------------------------

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

    // --- resolve_model_alias -------------------------------------------------

    #[test]
    fn resolve_fast_alias() {
        let candidates = resolve_model_alias("fast");
        assert!(!candidates.is_empty());
        // Haiku should be first (lowest cost)
        assert!(candidates[0].model.contains("haiku"));
    }

    #[test]
    fn resolve_smart_alias() {
        let candidates = resolve_model_alias("smart");
        assert!(!candidates.is_empty());
        assert!(candidates[0].model.contains("opus"));
    }

    #[test]
    fn resolve_code_alias() {
        let candidates = resolve_model_alias("code");
        assert!(!candidates.is_empty());
        assert!(candidates[0].model.contains("sonnet"));
    }

    #[test]
    fn resolve_local_alias() {
        let candidates = resolve_model_alias("local");
        assert!(!candidates.is_empty());
        for c in &candidates {
            assert_eq!(c.provider, "ollama");
            assert_eq!(c.cost_per_million_input, 0.0);
        }
    }

    #[test]
    fn resolve_unknown_alias_returns_empty() {
        let candidates = resolve_model_alias("definitely-not-an-alias");
        assert!(candidates.is_empty());
    }

    #[test]
    fn is_model_alias_detects_known() {
        assert!(is_model_alias("fast"));
        assert!(is_model_alias("FAST")); // case-insensitive
        assert!(is_model_alias("smart"));
        assert!(is_model_alias("code"));
        assert!(is_model_alias("local"));
    }

    #[test]
    fn is_model_alias_rejects_literal_names() {
        assert!(!is_model_alias("gpt-4o"));
        assert!(!is_model_alias("claude-3-5-sonnet-20241022"));
        assert!(!is_model_alias("llama3.2"));
    }

    // --- retryable detection -------------------------------------------------

    #[test]
    fn rate_limit_error_is_retryable() {
        let err = Error::ApiError {
            status: 429,
            message: "429 rate limit exceeded".to_string(),
            provider: "openai".to_string(),
        };
        assert!(is_retryable(&err));
    }

    #[test]
    fn server_error_is_retryable() {
        let err = Error::ApiError {
            status: 503,
            message: "503 service unavailable".to_string(),
            provider: "openai".to_string(),
        };
        assert!(is_retryable(&err));
    }

    #[test]
    fn auth_error_is_not_retryable() {
        let err = Error::ApiError {
            status: 401,
            message: "401 unauthorized".to_string(),
            provider: "openai".to_string(),
        };
        assert!(!is_retryable(&err));
    }

    // --- FallbackRouter builder / provider list ------------------------------

    #[test]
    fn fallback_router_provider_registration() {
        use crate::mock::MockProvider;
        let mut router = FallbackRouter::new();
        router.register("mock", Arc::new(MockProvider::new()));
        let names = router.provider_names();
        assert!(names.contains(&"mock"));
    }

    #[test]
    fn fallback_router_with_provider_builder() {
        use crate::mock::MockProvider;
        let router = FallbackRouter::new()
            .with_provider("mock", Arc::new(MockProvider::new()));
        assert!(router.provider_names().contains(&"mock"));
    }
}
