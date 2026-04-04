//! # Tool Selector
//!
//! Intelligent tool selection and ranking for agents.
//!
//! Features:
//! - Semantic matching of user intent to tools
//! - Tool capability scoring
//! - Context-aware tool recommendation
//! - Tool usage history tracking

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

/// Tool selector for intelligent tool selection.
#[derive(Debug, Clone)]
pub struct ToolSelector {
    /// Registered tools with metadata
    tools: HashMap<String, ToolMetadata>,
    /// Tool usage history
    usage_history: Vec<ToolUsageRecord>,
    /// Configuration
    config: ToolSelectorConfig,
}

/// Configuration for tool selector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSelectorConfig {
    /// Maximum tools to recommend
    pub max_recommendations: usize,
    /// Minimum relevance score (0.0-1.0)
    pub min_relevance_score: f64,
    /// Weight for semantic matching
    pub semantic_weight: f64,
    /// Weight for usage history
    pub history_weight: f64,
    /// Weight for capability match
    pub capability_weight: f64,
    /// Enable learning from usage
    pub enable_learning: bool,
}

impl Default for ToolSelectorConfig {
    fn default() -> Self {
        Self {
            max_recommendations: 5,
            min_relevance_score: 0.3,
            semantic_weight: 0.5,
            history_weight: 0.2,
            capability_weight: 0.3,
            enable_learning: true,
        }
    }
}

/// Metadata about a tool for selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Keywords for matching
    pub keywords: Vec<String>,
    /// Capabilities (read, write, network, etc.)
    pub capabilities: Vec<ToolCapability>,
    /// Example use cases
    pub examples: Vec<String>,
    /// Required parameters
    pub required_params: Vec<String>,
    /// Risk level (0.0-1.0)
    pub risk_level: f64,
    /// Average execution time in ms
    pub avg_execution_time_ms: Option<u64>,
    /// Success rate (0.0-1.0)
    pub success_rate: Option<f64>,
}

/// Tool capability categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCapability {
    /// Read data from system
    Read,
    /// Write data to system
    Write,
    /// Execute commands
    Execute,
    /// Network operations
    Network,
    /// File system access
    FileSystem,
    /// Database operations
    Database,
    /// Image processing
    Image,
    /// Code execution
    CodeExecution,
    /// Web browsing
    Browser,
    /// API calls
    Api,
    /// Git operations
    Git,
    /// Search functionality
    Search,
}

/// Record of tool usage for learning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUsageRecord {
    /// Tool name
    pub tool_name: String,
    /// Query that triggered the tool
    pub query: String,
    /// Was the usage successful
    pub success: bool,
    /// Execution time in ms
    pub execution_time_ms: u64,
    /// Timestamp
    pub timestamp: u64,
    /// Context keywords
    pub context_keywords: Vec<String>,
}

/// Tool recommendation with scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRecommendation {
    /// Tool name
    pub tool_name: String,
    /// Relevance score (0.0-1.0)
    pub relevance_score: f64,
    /// Confidence score (0.0-1.0)
    pub confidence: f64,
    /// Reason for recommendation
    pub reason: String,
    /// Suggested parameters
    pub suggested_params: HashMap<String, String>,
    /// Alternative tools
    pub alternatives: Vec<String>,
}

impl ToolSelector {
    /// Create a new tool selector with default configuration.
    pub fn new() -> Self {
        Self::with_config(ToolSelectorConfig::default())
    }

    /// Create with custom configuration.
    pub fn with_config(config: ToolSelectorConfig) -> Self {
        Self {
            tools: HashMap::new(),
            usage_history: Vec::new(),
            config,
        }
    }

    /// Register a tool.
    pub fn register_tool(&mut self, metadata: ToolMetadata) {
        debug!("Registering tool: {}", metadata.name);
        self.tools.insert(metadata.name.clone(), metadata);
    }

    /// Register multiple tools.
    pub fn register_tools(&mut self, tools: Vec<ToolMetadata>) {
        for tool in tools {
            self.register_tool(tool);
        }
    }

    /// Select best tools for a query.
    pub fn select_tools(&self, query: &str, context: &SelectionContext) -> Vec<ToolRecommendation> {
        let mut recommendations: Vec<_> = self
            .tools
            .values()
            .map(|tool| self.score_tool(tool, query, context))
            .filter(|rec| rec.relevance_score >= self.config.min_relevance_score)
            .collect();

        // Sort by relevance score descending
        recommendations.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit to max recommendations
        recommendations.truncate(self.config.max_recommendations);

        // Add alternatives
        for rec in &mut recommendations {
            rec.alternatives = self.find_alternatives(&rec.tool_name, query);
        }

        recommendations
    }

    /// Score a tool for a query.
    fn score_tool(
        &self,
        tool: &ToolMetadata,
        query: &str,
        context: &SelectionContext,
    ) -> ToolRecommendation {
        let query_lower = query.to_lowercase();
        let mut total_score = 0.0;
        let mut reasons = Vec::new();

        // 1. Semantic matching (keyword matching)
        let semantic_score = self.compute_semantic_score(tool, &query_lower);
        total_score += semantic_score * self.config.semantic_weight;
        if semantic_score > 0.5 {
            reasons.push(format!("Keywords match: {:.0}%", semantic_score * 100.0));
        }

        // 2. Capability matching
        let capability_score = self.compute_capability_score(tool, context);
        total_score += capability_score * self.config.capability_weight;
        if capability_score > 0.5 {
            reasons.push(format!(
                "Capabilities match: {:.0}%",
                capability_score * 100.0
            ));
        }

        // 3. Historical success rate
        let history_score = self.compute_history_score(&tool.name, &query_lower);
        total_score += history_score * self.config.history_weight;
        if history_score > 0.5 {
            reasons.push(format!("Historical success: {:.0}%", history_score * 100.0));
        }

        // Compute confidence based on data availability
        let confidence = self.compute_confidence(tool, context);

        ToolRecommendation {
            tool_name: tool.name.clone(),
            relevance_score: total_score.min(1.0),
            confidence,
            reason: if reasons.is_empty() {
                "General match".to_string()
            } else {
                reasons.join("; ")
            },
            suggested_params: self.suggest_params(tool, query, context),
            alternatives: vec![],
        }
    }

    /// Compute semantic matching score.
    fn compute_semantic_score(&self, tool: &ToolMetadata, query: &str) -> f64 {
        let mut score = 0.0;
        let query_words: Vec<&str> = query.split_whitespace().collect();

        // Check tool name
        if query.contains(&tool.name.to_lowercase()) {
            score += 0.5;
        }

        // Check keywords
        let keyword_matches = tool
            .keywords
            .iter()
            .filter(|kw| query.contains(&kw.to_lowercase()))
            .count();
        score += (keyword_matches as f64 / tool.keywords.len().max(1) as f64) * 0.3;

        // Check description words
        let desc_lower = tool.description.to_lowercase();
        let desc_matches = query_words
            .iter()
            .filter(|w| desc_lower.contains(*w))
            .count();
        score += (desc_matches as f64 / query_words.len().max(1) as f64) * 0.2;

        // Check examples
        for example in &tool.examples {
            if query.contains(&example.to_lowercase()) {
                score += 0.1;
                break;
            }
        }

        score.min(1.0)
    }

    /// Compute capability matching score.
    fn compute_capability_score(&self, tool: &ToolMetadata, context: &SelectionContext) -> f64 {
        if context.required_capabilities.is_empty() {
            return 0.5; // Neutral if no requirements
        }

        let matches = context
            .required_capabilities
            .iter()
            .filter(|cap| tool.capabilities.contains(cap))
            .count();

        matches as f64 / context.required_capabilities.len() as f64
    }

    /// Compute history-based score.
    fn compute_history_score(&self, tool_name: &str, query: &str) -> f64 {
        let relevant_history: Vec<_> = self
            .usage_history
            .iter()
            .filter(|h| h.tool_name == tool_name)
            .collect();

        if relevant_history.is_empty() {
            return 0.5; // Neutral if no history
        }

        // Compute success rate
        let successes = relevant_history.iter().filter(|h| h.success).count();
        let success_rate = successes as f64 / relevant_history.len() as f64;

        // Bonus for similar queries
        let query_words: Vec<&str> = query.split_whitespace().collect();
        let similar_queries = relevant_history
            .iter()
            .filter(|h| {
                query_words
                    .iter()
                    .any(|w| h.query.to_lowercase().contains(*w))
            })
            .count();
        let similarity_bonus = (similar_queries as f64 / relevant_history.len() as f64) * 0.2;

        (success_rate + similarity_bonus).min(1.0)
    }

    /// Compute confidence level.
    fn compute_confidence(&self, tool: &ToolMetadata, _context: &SelectionContext) -> f64 {
        let mut confidence = 0.5; // Base confidence

        // Higher confidence if we have success rate data
        if let Some(rate) = tool.success_rate {
            confidence += rate * 0.3;
        }

        // Historical usage increases confidence
        let usage_count = self
            .usage_history
            .iter()
            .filter(|h| h.tool_name == tool.name)
            .count();
        confidence += (usage_count as f64 / 100.0).min(0.2);

        confidence.min(1.0)
    }

    /// Suggest parameters based on query and context.
    fn suggest_params(
        &self,
        tool: &ToolMetadata,
        query: &str,
        context: &SelectionContext,
    ) -> HashMap<String, String> {
        let mut params = HashMap::new();

        // Extract potential parameter values from query
        // This is a simple heuristic - could be enhanced with NLP

        // Check for file paths
        if tool.capabilities.contains(&ToolCapability::FileSystem) {
            if let Some(path) = extract_path(query) {
                params.insert("path".to_string(), path);
            }
        }

        // Check for URLs
        if tool.capabilities.contains(&ToolCapability::Network)
            || tool.capabilities.contains(&ToolCapability::Api)
        {
            if let Some(url) = extract_url(query) {
                params.insert("url".to_string(), url);
            }
        }

        // Add context parameters
        for (key, value) in &context.parameters {
            if tool.required_params.contains(key) {
                params.insert(key.clone(), value.clone());
            }
        }

        params
    }

    /// Find alternative tools.
    fn find_alternatives(&self, tool_name: &str, _query: &str) -> Vec<String> {
        let current_tool = match self.tools.get(tool_name) {
            Some(t) => t,
            None => return vec![],
        };

        self.tools
            .values()
            .filter(|t| {
                t.name != tool_name
                    && t.capabilities
                        .iter()
                        .any(|c| current_tool.capabilities.contains(c))
            })
            .map(|t| t.name.clone())
            .take(3)
            .collect()
    }

    /// Record tool usage for learning.
    pub fn record_usage(&mut self, record: ToolUsageRecord) {
        if self.config.enable_learning {
            self.usage_history.push(record);

            // Limit history size
            if self.usage_history.len() > 10000 {
                self.usage_history.drain(0..1000);
            }
        }
    }

    /// Update tool metadata based on usage.
    pub fn update_tool_stats(&mut self, tool_name: &str) {
        let records: Vec<_> = self
            .usage_history
            .iter()
            .filter(|h| h.tool_name == tool_name)
            .collect();

        if records.is_empty() {
            return;
        }

        let success_count = records.iter().filter(|r| r.success).count();
        let avg_time: u64 =
            records.iter().map(|r| r.execution_time_ms).sum::<u64>() / records.len() as u64;

        if let Some(tool) = self.tools.get_mut(tool_name) {
            tool.success_rate = Some(success_count as f64 / records.len() as f64);
            tool.avg_execution_time_ms = Some(avg_time);
        }
    }

    /// Get tool by name.
    pub fn get_tool(&self, name: &str) -> Option<&ToolMetadata> {
        self.tools.get(name)
    }

    /// List all registered tools.
    pub fn list_tools(&self) -> Vec<&ToolMetadata> {
        self.tools.values().collect()
    }
}

impl Default for ToolSelector {
    fn default() -> Self {
        Self::new()
    }
}

/// Context for tool selection.
#[derive(Debug, Clone, Default)]
pub struct SelectionContext {
    /// Required capabilities
    pub required_capabilities: Vec<ToolCapability>,
    /// Context parameters
    pub parameters: HashMap<String, String>,
    /// Previous tools used in this session
    pub previous_tools: Vec<String>,
    /// Current working directory
    pub working_directory: Option<String>,
    /// User preferences
    pub preferences: HashMap<String, String>,
}

impl SelectionContext {
    /// Create a new selection context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add required capability.
    pub fn require_capability(mut self, capability: ToolCapability) -> Self {
        self.required_capabilities.push(capability);
        self
    }

    /// Add parameter.
    pub fn with_parameter(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.parameters.insert(key.into(), value.into());
        self
    }

    /// Set working directory.
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_directory = Some(dir.into());
        self
    }
}

/// Extract file path from query.
fn extract_path(query: &str) -> Option<String> {
    // Simple heuristic: look for path-like patterns
    let words: Vec<&str> = query.split_whitespace().collect();
    for word in words {
        if word.starts_with('/') || word.starts_with("./") || word.starts_with("~/") {
            return Some(word.to_string());
        }
        if word.contains('/') && !word.contains("://") {
            return Some(word.to_string());
        }
    }
    None
}

/// Extract URL from query.
fn extract_url(query: &str) -> Option<String> {
    let words: Vec<&str> = query.split_whitespace().collect();
    for word in words {
        if word.starts_with("http://") || word.starts_with("https://") {
            return Some(word.to_string());
        }
    }
    None
}

/// Create default tool metadata for built-in tools.
#[allow(dead_code)]
pub fn builtin_tool_metadata() -> Vec<ToolMetadata> {
    vec![
        ToolMetadata {
            name: "shell".to_string(),
            description: "Execute shell commands".to_string(),
            keywords: vec![
                "run".to_string(),
                "execute".to_string(),
                "command".to_string(),
                "terminal".to_string(),
                "bash".to_string(),
                "script".to_string(),
            ],
            capabilities: vec![ToolCapability::Execute],
            examples: vec![
                "run ls".to_string(),
                "execute npm install".to_string(),
                "run cargo build".to_string(),
            ],
            required_params: vec!["command".to_string()],
            risk_level: 0.7,
            avg_execution_time_ms: None,
            success_rate: None,
        },
        ToolMetadata {
            name: "filesystem".to_string(),
            description: "Read, write, and manage files".to_string(),
            keywords: vec![
                "file".to_string(),
                "read".to_string(),
                "write".to_string(),
                "create".to_string(),
                "delete".to_string(),
                "list".to_string(),
                "directory".to_string(),
            ],
            capabilities: vec![
                ToolCapability::Read,
                ToolCapability::Write,
                ToolCapability::FileSystem,
            ],
            examples: vec![
                "read file.txt".to_string(),
                "create new file".to_string(),
                "list directory contents".to_string(),
            ],
            required_params: vec!["operation".to_string(), "path".to_string()],
            risk_level: 0.5,
            avg_execution_time_ms: None,
            success_rate: None,
        },
        ToolMetadata {
            name: "web".to_string(),
            description: "Make HTTP requests".to_string(),
            keywords: vec![
                "http".to_string(),
                "request".to_string(),
                "fetch".to_string(),
                "get".to_string(),
                "post".to_string(),
                "api".to_string(),
                "url".to_string(),
            ],
            capabilities: vec![ToolCapability::Network, ToolCapability::Api],
            examples: vec![
                "fetch https://api.example.com".to_string(),
                "GET request to URL".to_string(),
            ],
            required_params: vec!["url".to_string()],
            risk_level: 0.3,
            avg_execution_time_ms: None,
            success_rate: None,
        },
        ToolMetadata {
            name: "git".to_string(),
            description: "Git version control operations".to_string(),
            keywords: vec![
                "git".to_string(),
                "commit".to_string(),
                "push".to_string(),
                "pull".to_string(),
                "branch".to_string(),
                "merge".to_string(),
                "clone".to_string(),
            ],
            capabilities: vec![ToolCapability::Git, ToolCapability::FileSystem],
            examples: vec![
                "git commit".to_string(),
                "create branch".to_string(),
                "push changes".to_string(),
            ],
            required_params: vec!["operation".to_string()],
            risk_level: 0.4,
            avg_execution_time_ms: None,
            success_rate: None,
        },
        ToolMetadata {
            name: "database".to_string(),
            description: "Execute database queries".to_string(),
            keywords: vec![
                "sql".to_string(),
                "query".to_string(),
                "database".to_string(),
                "select".to_string(),
                "insert".to_string(),
                "update".to_string(),
                "delete".to_string(),
                "table".to_string(),
            ],
            capabilities: vec![
                ToolCapability::Database,
                ToolCapability::Read,
                ToolCapability::Write,
            ],
            examples: vec![
                "query database".to_string(),
                "SELECT * FROM users".to_string(),
            ],
            required_params: vec!["query".to_string()],
            risk_level: 0.6,
            avg_execution_time_ms: None,
            success_rate: None,
        },
        ToolMetadata {
            name: "image".to_string(),
            description: "Process and transform images".to_string(),
            keywords: vec![
                "image".to_string(),
                "resize".to_string(),
                "convert".to_string(),
                "crop".to_string(),
                "rotate".to_string(),
                "thumbnail".to_string(),
                "picture".to_string(),
                "photo".to_string(),
            ],
            capabilities: vec![
                ToolCapability::Image,
                ToolCapability::Read,
                ToolCapability::Write,
            ],
            examples: vec![
                "resize image".to_string(),
                "convert to PNG".to_string(),
                "create thumbnail".to_string(),
            ],
            required_params: vec!["operation".to_string(), "source".to_string()],
            risk_level: 0.2,
            avg_execution_time_ms: None,
            success_rate: None,
        },
        ToolMetadata {
            name: "code_execution".to_string(),
            description: "Execute code in various languages".to_string(),
            keywords: vec![
                "code".to_string(),
                "run".to_string(),
                "execute".to_string(),
                "python".to_string(),
                "javascript".to_string(),
                "rust".to_string(),
                "evaluate".to_string(),
            ],
            capabilities: vec![ToolCapability::CodeExecution, ToolCapability::Execute],
            examples: vec![
                "run python code".to_string(),
                "execute javascript".to_string(),
            ],
            required_params: vec!["language".to_string(), "code".to_string()],
            risk_level: 0.8,
            avg_execution_time_ms: None,
            success_rate: None,
        },
        ToolMetadata {
            name: "browser".to_string(),
            description: "Automated web browser interactions".to_string(),
            keywords: vec![
                "browser".to_string(),
                "web".to_string(),
                "page".to_string(),
                "click".to_string(),
                "navigate".to_string(),
                "screenshot".to_string(),
                "scrape".to_string(),
            ],
            capabilities: vec![ToolCapability::Browser, ToolCapability::Network],
            examples: vec![
                "open webpage".to_string(),
                "take screenshot".to_string(),
                "click button".to_string(),
            ],
            required_params: vec!["action".to_string()],
            risk_level: 0.4,
            avg_execution_time_ms: None,
            success_rate: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_selection() {
        let mut selector = ToolSelector::new();
        selector.register_tools(builtin_tool_metadata());

        let context = SelectionContext::new();
        let recommendations = selector.select_tools("read file content", &context);

        assert!(!recommendations.is_empty());
        // Filesystem tool should be recommended
        assert!(recommendations.iter().any(|r| r.tool_name == "filesystem"));
    }

    #[test]
    fn test_capability_matching() {
        let mut selector = ToolSelector::new();
        selector.register_tools(builtin_tool_metadata());

        let context = SelectionContext::new().require_capability(ToolCapability::Database);

        let recommendations = selector.select_tools("query data", &context);

        // Database tool should rank high
        if !recommendations.is_empty() {
            assert!(recommendations.iter().any(|r| r.tool_name == "database"));
        }
    }

    #[test]
    fn test_usage_recording() {
        let mut selector = ToolSelector::new();
        selector.register_tools(builtin_tool_metadata());

        selector.record_usage(ToolUsageRecord {
            tool_name: "filesystem".to_string(),
            query: "read file".to_string(),
            success: true,
            execution_time_ms: 100,
            timestamp: 1000,
            context_keywords: vec!["file".to_string()],
        });

        selector.update_tool_stats("filesystem");

        let tool = selector.get_tool("filesystem").unwrap();
        assert!(tool.success_rate.is_some());
    }

    #[test]
    fn test_extract_path() {
        assert_eq!(
            extract_path("read /home/user/file.txt"),
            Some("/home/user/file.txt".to_string())
        );
        assert_eq!(
            extract_path("open ./relative/path"),
            Some("./relative/path".to_string())
        );
        assert_eq!(extract_path("no path here"), None);
    }

    #[test]
    fn test_extract_url() {
        assert_eq!(
            extract_url("fetch https://api.example.com/data"),
            Some("https://api.example.com/data".to_string())
        );
        assert_eq!(extract_url("no url here"), None);
    }
}
