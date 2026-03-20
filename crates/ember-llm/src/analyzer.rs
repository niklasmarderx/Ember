//! Task Analysis for Intelligent Model Selection
//!
//! This module provides task analysis capabilities for automatic model selection,
//! determining task complexity, type, and requirements.

use serde::{Deserialize, Serialize};

use crate::CompletionRequest;

/// Task complexity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskComplexity {
    /// Simple factual questions, greetings, basic tasks.
    Simple,
    /// Multi-step reasoning, summarization, moderate analysis.
    Moderate,
    /// Advanced reasoning, long context, complex analysis.
    Complex,
    /// Specialized tasks: code, math, creative writing.
    Specialized,
}

impl TaskComplexity {
    /// Get a numeric score for this complexity (0.0 - 1.0).
    pub fn score(&self) -> f64 {
        match self {
            Self::Simple => 0.25,
            Self::Moderate => 0.5,
            Self::Complex => 0.75,
            Self::Specialized => 1.0,
        }
    }
}

/// Types of tasks that can be detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskType {
    /// General chat conversation.
    Chat,
    /// Code generation or implementation.
    CodeGeneration,
    /// Code review, debugging, fixing.
    CodeReview,
    /// Summarization of content.
    Summarization,
    /// Language translation.
    Translation,
    /// Mathematical calculations or proofs.
    Math,
    /// Creative writing (stories, poems, etc.).
    Creative,
    /// Data analysis and interpretation.
    Analysis,
    /// Question answering from knowledge.
    QuestionAnswering,
    /// Instruction following.
    InstructionFollowing,
}

impl TaskType {
    /// Get the recommended model specialization for this task type.
    pub fn recommended_specialization(&self) -> &'static str {
        match self {
            Self::Chat => "general",
            Self::CodeGeneration | Self::CodeReview => "code",
            Self::Summarization => "general",
            Self::Translation => "multilingual",
            Self::Math => "reasoning",
            Self::Creative => "creative",
            Self::Analysis => "reasoning",
            Self::QuestionAnswering => "general",
            Self::InstructionFollowing => "instruction",
        }
    }
}

/// Result of task analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAnalysis {
    /// Detected complexity level.
    pub complexity: TaskComplexity,
    /// Detected task type.
    pub task_type: TaskType,
    /// Estimated input tokens.
    pub estimated_input_tokens: usize,
    /// Estimated output tokens.
    pub estimated_output_tokens: usize,
    /// Whether the task likely requires tool usage.
    pub requires_tools: bool,
    /// Total context length needed.
    pub context_length: usize,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,
    /// Detected keywords that influenced the analysis.
    pub detected_keywords: Vec<String>,
}

/// Task analyzer for intelligent model selection.
#[derive(Debug, Clone)]
pub struct TaskAnalyzer {
    /// Keywords for code detection.
    code_keywords: Vec<&'static str>,
    /// Keywords for math detection.
    math_keywords: Vec<&'static str>,
    /// Keywords for creative detection.
    creative_keywords: Vec<&'static str>,
    /// Keywords for analysis detection.
    analysis_keywords: Vec<&'static str>,
    /// Keywords for translation detection.
    translation_keywords: Vec<&'static str>,
    /// Keywords for summarization detection.
    summary_keywords: Vec<&'static str>,
}

impl Default for TaskAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskAnalyzer {
    /// Create a new task analyzer.
    pub fn new() -> Self {
        Self {
            code_keywords: vec![
                "code", "function", "implement", "programming", "debug", "fix", "error",
                "compile", "syntax", "class", "method", "variable", "algorithm", "refactor",
                "typescript", "javascript", "python", "rust", "java", "c++", "golang",
                "api", "endpoint", "database", "sql", "query", "script", "terminal",
                "command", "cli", "git", "docker", "kubernetes", "deploy",
            ],
            math_keywords: vec![
                "calculate", "math", "equation", "formula", "solve", "compute", "derivative",
                "integral", "algebra", "geometry", "statistics", "probability", "matrix",
                "vector", "proof", "theorem", "number", "sum", "product", "average",
            ],
            creative_keywords: vec![
                "story", "creative", "write", "poem", "novel", "fiction", "narrative",
                "character", "plot", "imagination", "fantasy", "describe", "compose",
                "lyrics", "song", "screenplay", "dialogue",
            ],
            analysis_keywords: vec![
                "analyze", "analysis", "compare", "contrast", "evaluate", "assess",
                "examine", "investigate", "research", "study", "review", "critique",
                "interpret", "explain", "why", "how", "reason", "cause", "effect",
            ],
            translation_keywords: vec![
                "translate", "translation", "spanish", "french", "german", "chinese",
                "japanese", "korean", "portuguese", "italian", "russian", "arabic",
                "language", "multilingual",
            ],
            summary_keywords: vec![
                "summarize", "summary", "tldr", "brief", "overview", "key points",
                "main ideas", "condense", "shorten", "recap",
            ],
        }
    }

    /// Analyze a completion request.
    pub fn analyze(&self, request: &CompletionRequest) -> TaskAnalysis {
        // Get the last user message
        let content = request
            .messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, crate::Role::User))
            .map(|m| m.content.as_str())
            .unwrap_or("");

        // Calculate total context
        let total_context: usize = request.messages.iter().map(|m| m.content.len()).sum();

        let complexity = self.estimate_complexity(content, total_context);
        let (task_type, detected_keywords) = self.detect_task_type(content);
        let requires_tools = self.detect_tool_need(content);
        let estimated_input_tokens = self.estimate_tokens(content);
        let estimated_output_tokens = self.estimate_output_tokens(&task_type, estimated_input_tokens);

        TaskAnalysis {
            complexity,
            task_type,
            estimated_input_tokens,
            estimated_output_tokens,
            requires_tools,
            context_length: total_context,
            confidence: self.calculate_confidence(&detected_keywords),
            detected_keywords,
        }
    }

    /// Estimate task complexity based on content.
    fn estimate_complexity(&self, content: &str, context_length: usize) -> TaskComplexity {
        let word_count = content.split_whitespace().count();
        let lower = content.to_lowercase();

        // Check for specialized content
        let has_code = lower.contains("```")
            || lower.contains("fn ")
            || lower.contains("def ")
            || lower.contains("function ")
            || lower.contains("class ");
        let has_math = self.math_keywords.iter().any(|k| lower.contains(k));

        if has_code || has_math {
            return TaskComplexity::Specialized;
        }

        // Check for complex reasoning
        let reasoning_indicators = [
            "explain", "analyze", "compare", "contrast", "evaluate",
            "step by step", "reasoning", "logic", "argument", "evidence",
        ];
        let has_reasoning = reasoning_indicators.iter().any(|k| lower.contains(k));

        // Consider context length
        if context_length > 10000 || (has_reasoning && word_count > 50) {
            return TaskComplexity::Complex;
        }

        if word_count > 30 || has_reasoning {
            return TaskComplexity::Moderate;
        }

        TaskComplexity::Simple
    }

    /// Detect the task type from content.
    fn detect_task_type(&self, content: &str) -> (TaskType, Vec<String>) {
        let lower = content.to_lowercase();
        let mut detected = Vec::new();

        // Check each category
        let code_score = self.count_keyword_matches(&lower, &self.code_keywords, &mut detected);
        let math_score = self.count_keyword_matches(&lower, &self.math_keywords, &mut detected);
        let creative_score = self.count_keyword_matches(&lower, &self.creative_keywords, &mut detected);
        let analysis_score = self.count_keyword_matches(&lower, &self.analysis_keywords, &mut detected);
        let translation_score = self.count_keyword_matches(&lower, &self.translation_keywords, &mut detected);
        let summary_score = self.count_keyword_matches(&lower, &self.summary_keywords, &mut detected);

        // Find the highest scoring category
        let scores = [
            (TaskType::CodeGeneration, code_score),
            (TaskType::Math, math_score),
            (TaskType::Creative, creative_score),
            (TaskType::Analysis, analysis_score),
            (TaskType::Translation, translation_score),
            (TaskType::Summarization, summary_score),
        ];

        let (task_type, max_score) = scores
            .iter()
            .max_by_key(|(_, s)| *s)
            .map(|(t, s)| (*t, *s))
            .unwrap_or((TaskType::Chat, 0));

        // If no strong signal, check for code review specifically
        if max_score == code_score && code_score > 0 {
            let review_keywords = ["review", "bug", "fix", "debug", "error", "issue"];
            if review_keywords.iter().any(|k| lower.contains(k)) {
                return (TaskType::CodeReview, detected);
            }
        }

        // Default to chat if no strong signal
        if max_score < 2 {
            return (TaskType::Chat, detected);
        }

        (task_type, detected)
    }

    /// Count keyword matches in text.
    fn count_keyword_matches(
        &self,
        text: &str,
        keywords: &[&str],
        detected: &mut Vec<String>,
    ) -> usize {
        let mut count = 0;
        for keyword in keywords {
            if text.contains(keyword) {
                count += 1;
                detected.push(keyword.to_string());
            }
        }
        count
    }

    /// Detect if the task likely requires tools.
    fn detect_tool_need(&self, content: &str) -> bool {
        let lower = content.to_lowercase();
        let tool_indicators = [
            "run", "execute", "file", "directory", "folder", "create",
            "delete", "search", "find", "download", "upload", "browse",
            "website", "url", "http", "api call", "shell", "terminal",
            "command", "script",
        ];

        tool_indicators.iter().any(|k| lower.contains(k))
    }

    /// Estimate number of tokens in text.
    fn estimate_tokens(&self, text: &str) -> usize {
        // Rough estimate: ~4 characters per token
        text.len() / 4
    }

    /// Estimate output tokens based on task type.
    fn estimate_output_tokens(&self, task_type: &TaskType, input_tokens: usize) -> usize {
        match task_type {
            TaskType::Chat => input_tokens.max(50).min(200),
            TaskType::CodeGeneration => input_tokens.max(100) * 3,
            TaskType::CodeReview => input_tokens.max(100) * 2,
            TaskType::Summarization => (input_tokens / 4).max(50),
            TaskType::Translation => input_tokens + 50,
            TaskType::Math => input_tokens.max(100) * 2,
            TaskType::Creative => input_tokens.max(200) * 4,
            TaskType::Analysis => input_tokens.max(100) * 2,
            TaskType::QuestionAnswering => input_tokens.max(50).min(500),
            TaskType::InstructionFollowing => input_tokens.max(100) * 2,
        }
    }

    /// Calculate confidence based on detected keywords.
    fn calculate_confidence(&self, detected_keywords: &[String]) -> f32 {
        let count = detected_keywords.len();
        match count {
            0 => 0.3,
            1 => 0.5,
            2 => 0.7,
            3 => 0.8,
            4..=5 => 0.9,
            _ => 0.95,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Message, Role};

    fn make_request(content: &str) -> CompletionRequest {
        CompletionRequest {
            messages: vec![Message {
                role: Role::User,
                content: content.to_string(),
                name: None,
            }],
            model: "gpt-4".to_string(),
            temperature: None,
            max_tokens: None,
            tools: None,
            stream: false,
        }
    }

    #[test]
    fn test_simple_chat() {
        let analyzer = TaskAnalyzer::new();
        let request = make_request("Hello, how are you?");
        let analysis = analyzer.analyze(&request);

        assert_eq!(analysis.complexity, TaskComplexity::Simple);
        assert_eq!(analysis.task_type, TaskType::Chat);
    }

    #[test]
    fn test_code_generation() {
        let analyzer = TaskAnalyzer::new();
        let request = make_request("Write a Python function to calculate fibonacci numbers");
        let analysis = analyzer.analyze(&request);

        assert_eq!(analysis.task_type, TaskType::CodeGeneration);
        assert!(analysis.detected_keywords.contains(&"function".to_string()));
        assert!(analysis.detected_keywords.contains(&"python".to_string()));
    }

    #[test]
    fn test_math_task() {
        let analyzer = TaskAnalyzer::new();
        let request = make_request("Calculate the derivative of x^2 + 3x + 5");
        let analysis = analyzer.analyze(&request);

        assert_eq!(analysis.task_type, TaskType::Math);
        assert_eq!(analysis.complexity, TaskComplexity::Specialized);
    }

    #[test]
    fn test_complex_analysis() {
        let analyzer = TaskAnalyzer::new();
        let request = make_request(
            "Analyze and compare the economic policies of the US and China, \
             explaining their impact on global trade. Provide step by step reasoning.",
        );
        let analysis = analyzer.analyze(&request);

        assert!(analysis.complexity == TaskComplexity::Complex || analysis.complexity == TaskComplexity::Moderate);
        assert_eq!(analysis.task_type, TaskType::Analysis);
    }

    #[test]
    fn test_translation() {
        let analyzer = TaskAnalyzer::new();
        let request = make_request("Translate this text to Spanish: Hello, how are you?");
        let analysis = analyzer.analyze(&request);

        assert_eq!(analysis.task_type, TaskType::Translation);
    }

    #[test]
    fn test_summarization() {
        let analyzer = TaskAnalyzer::new();
        let request = make_request("Summarize the key points of this article");
        let analysis = analyzer.analyze(&request);

        assert_eq!(analysis.task_type, TaskType::Summarization);
    }

    #[test]
    fn test_tool_detection() {
        let analyzer = TaskAnalyzer::new();

        let request1 = make_request("Create a new file called test.txt");
        let analysis1 = analyzer.analyze(&request1);
        assert!(analysis1.requires_tools);

        let request2 = make_request("What is the capital of France?");
        let analysis2 = analyzer.analyze(&request2);
        assert!(!analysis2.requires_tools);
    }
}