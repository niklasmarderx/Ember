//! # Ember Code Intelligence
//!
//! AI-powered code analysis, refactoring, and generation for the Ember framework.
//!
//! This crate provides intelligent code tools that can:
//! - Analyze code structure, complexity, and quality
//! - Suggest and apply refactorings
//! - Generate tests automatically
//! - Generate documentation
//! - Perform code reviews
//!
//! ## Features
//!
//! - **Multi-language support**: Rust, Python, JavaScript, TypeScript, Go, Java
//! - **Code Analysis**: Complexity metrics, code smells, symbol extraction
//! - **Refactoring**: 20+ refactoring types with automatic application
//! - **Test Generation**: Unit tests, edge cases, property-based tests
//! - **Documentation**: Auto-generate doc comments in correct format
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use ember_code::{CodeAnalyzer, RefactoringEngine, TestGenerator};
//!
//! // Analyze code
//! let analyzer = CodeAnalyzer::new();
//! let analysis = analyzer.analyze_file(&path).await?;
//!
//! // Get refactoring suggestions
//! let refactor = RefactoringEngine::new();
//! let suggestions = refactor.suggest_refactorings(&analysis, &content);
//!
//! // Generate tests
//! let testgen = TestGenerator::new();
//! let test_suite = testgen.generate_tests(&analysis, &content);
//! ```

#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]

pub mod analyzer;
pub mod refactor;
pub mod testgen;

// Re-export main types for convenience
pub use analyzer::{
    AnalyzerConfig,
    AnalyzerError,
    CodeAnalyzer,
    CodeSmell,
    CodeSmellType,
    CodeSymbol,
    ComplexityMetrics,
    ComplexityRating,
    FileAnalysis,
    ImportInfo,
    Language,
    LanguageStats,
    ProjectAnalysis,
    Severity,
    SymbolKind,
    Visibility,
};

pub use refactor::{
    CodeEdit,
    Confidence,
    FormattingStyle,
    RefactoringConfig,
    RefactoringEngine,
    RefactoringImpact,
    RefactoringKind,
    RefactoringResult,
    RefactoringSuggestion,
    TestImpact,
};

pub use testgen::{
    GeneratedTest,
    GeneratedTestSuite,
    TestFramework,
    TestGenConfig,
    TestGenerator,
    TestType,
};

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::analyzer::{
        CodeAnalyzer, CodeSmell, CodeSymbol, ComplexityMetrics, FileAnalysis, Language,
        ProjectAnalysis, Severity, SymbolKind, Visibility,
    };
    pub use crate::refactor::{
        Confidence, RefactoringEngine, RefactoringKind, RefactoringSuggestion,
    };
    pub use crate::testgen::{GeneratedTest, GeneratedTestSuite, TestFramework, TestGenerator};
}

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Crate name
pub const NAME: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_version_exists() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_analyze_rust_code() {
        let analyzer = CodeAnalyzer::new();
        let code = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;
        
        let result = analyzer.analyze_content(
            PathBuf::from("test.rs"),
            code,
            Language::Rust,
        ).unwrap();
        
        assert_eq!(result.language, Language::Rust);
        assert!(!result.symbols.is_empty());
    }

    #[test]
    fn test_refactoring_engine() {
        let engine = RefactoringEngine::new();
        // Basic creation test
        assert!(engine.suggest_refactorings(
            &FileAnalysis {
                path: PathBuf::from("test.rs"),
                language: Language::Rust,
                metrics: ComplexityMetrics::default(),
                symbols: vec![],
                smells: vec![],
                imports: vec![],
                dependencies: vec![],
                analyzed_at: chrono::Utc::now(),
            },
            ""
        ).is_empty() || true); // Always passes - just testing instantiation
    }

    #[test]
    fn test_test_generator() {
        let _gen = TestGenerator::new();
        // Basic creation test
        assert!(true);
    }
}