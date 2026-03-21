//! Auto-Refactoring Module
//!
//! AI-powered refactoring suggestions and automatic code transformations.

use crate::analyzer::{CodeSmell, CodeSymbol, FileAnalysis, Language, SymbolKind};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Types of refactoring operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefactoringKind {
    /// Extract a piece of code into a new function
    ExtractFunction,
    /// Extract a piece of code into a new method
    ExtractMethod,
    /// Extract a variable from an expression
    ExtractVariable,
    /// Extract a constant from a literal
    ExtractConstant,
    /// Inline a variable/function
    Inline,
    /// Rename a symbol
    Rename,
    /// Move code to a different file/module
    Move,
    /// Extract code into a new class/struct
    ExtractClass,
    /// Extract an interface/trait from a class
    ExtractInterface,
    /// Convert anonymous function to named
    ConvertToNamedFunction,
    /// Simplify conditional expression
    SimplifyConditional,
    /// Remove dead code
    RemoveDeadCode,
    /// Replace magic number with constant
    ReplaceMagicNumber,
    /// Add error handling
    AddErrorHandling,
    /// Convert to async
    ConvertToAsync,
    /// Apply early return pattern
    EarlyReturn,
    /// Split large function
    SplitFunction,
    /// Merge duplicate code
    MergeDuplicates,
    /// Convert loop to functional style
    ConvertToFunctional,
    /// Add type annotations
    AddTypeAnnotations,
    /// Remove unused imports
    RemoveUnusedImports,
    /// Organize imports
    OrganizeImports,
}

/// Confidence level of a refactoring suggestion
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Confidence {
    Low,
    Medium,
    High,
    VeryHigh,
}

/// A refactoring suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactoringSuggestion {
    /// Type of refactoring
    pub kind: RefactoringKind,
    /// Confidence level
    pub confidence: Confidence,
    /// Human-readable description
    pub description: String,
    /// Detailed rationale
    pub rationale: String,
    /// Affected file
    pub file: PathBuf,
    /// Start line
    pub start_line: u32,
    /// End line
    pub end_line: u32,
    /// Original code snippet
    pub original_code: String,
    /// Suggested refactored code
    pub suggested_code: String,
    /// Estimated impact (complexity reduction, readability improvement)
    pub impact: RefactoringImpact,
    /// Prerequisites (other refactorings that should be done first)
    pub prerequisites: Vec<String>,
    /// Breaking changes (if any)
    pub breaking_changes: Vec<String>,
}

/// Impact assessment of a refactoring
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RefactoringImpact {
    /// Estimated complexity reduction (negative = increase)
    pub complexity_change: i32,
    /// Readability improvement (0-100)
    pub readability_improvement: u32,
    /// Maintainability improvement (0-100)
    pub maintainability_improvement: u32,
    /// Test coverage impact
    pub test_impact: TestImpact,
    /// Lines of code change (negative = reduction)
    pub loc_change: i32,
}

/// Test impact assessment
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum TestImpact {
    #[default]
    None,
    /// Some tests may need updates
    MinorUpdates,
    /// Tests will need significant updates
    MajorUpdates,
    /// New tests needed
    NewTestsNeeded,
}

/// A code change/edit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEdit {
    /// File path
    pub file: PathBuf,
    /// Start line (1-indexed)
    pub start_line: u32,
    /// Start column (1-indexed)
    pub start_column: u32,
    /// End line (1-indexed)
    pub end_line: u32,
    /// End column (1-indexed)
    pub end_column: u32,
    /// New text to insert
    pub new_text: String,
}

/// Result of applying a refactoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactoringResult {
    /// Whether the refactoring was successful
    pub success: bool,
    /// Edits to apply
    pub edits: Vec<CodeEdit>,
    /// New files to create
    pub new_files: Vec<(PathBuf, String)>,
    /// Files to delete
    pub delete_files: Vec<PathBuf>,
    /// Warning messages
    pub warnings: Vec<String>,
    /// Error message if failed
    pub error: Option<String>,
}

/// Configuration for the refactoring engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactoringConfig {
    /// Minimum confidence to suggest
    pub min_confidence: Confidence,
    /// Maximum suggestions per file
    pub max_suggestions_per_file: usize,
    /// Enable breaking changes
    pub allow_breaking_changes: bool,
    /// Preserve comments
    pub preserve_comments: bool,
    /// Auto-fix imports
    pub auto_fix_imports: bool,
    /// Formatting style
    pub formatting_style: FormattingStyle,
}

impl Default for RefactoringConfig {
    fn default() -> Self {
        Self {
            min_confidence: Confidence::Medium,
            max_suggestions_per_file: 10,
            allow_breaking_changes: false,
            preserve_comments: true,
            auto_fix_imports: true,
            formatting_style: FormattingStyle::default(),
        }
    }
}

/// Formatting style preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattingStyle {
    /// Indentation (spaces)
    pub indent_size: usize,
    /// Use tabs instead of spaces
    pub use_tabs: bool,
    /// Max line length
    pub max_line_length: usize,
    /// Trailing commas
    pub trailing_commas: bool,
    /// Semicolons (for JS/TS)
    pub semicolons: bool,
    /// Single quotes (for JS/TS)
    pub single_quotes: bool,
}

impl Default for FormattingStyle {
    fn default() -> Self {
        Self {
            indent_size: 4,
            use_tabs: false,
            max_line_length: 100,
            trailing_commas: true,
            semicolons: true,
            single_quotes: false,
        }
    }
}

/// The refactoring engine
pub struct RefactoringEngine {
    config: RefactoringConfig,
}

impl RefactoringEngine {
    /// Create a new refactoring engine
    pub fn new() -> Self {
        Self {
            config: RefactoringConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: RefactoringConfig) -> Self {
        Self { config }
    }

    /// Analyze code and suggest refactorings
    pub fn suggest_refactorings(
        &self,
        analysis: &FileAnalysis,
        content: &str,
    ) -> Vec<RefactoringSuggestion> {
        let mut suggestions = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Check for code smell-based refactorings
        for smell in &analysis.smells {
            if let Some(suggestion) = self.smell_to_refactoring(smell, &lines, analysis.language) {
                if suggestion.confidence >= self.config.min_confidence {
                    suggestions.push(suggestion);
                }
            }
        }

        // Check for pattern-based refactorings
        suggestions.extend(self.find_pattern_refactorings(&lines, analysis));

        // Check for symbol-based refactorings
        for symbol in &analysis.symbols {
            suggestions.extend(self.suggest_symbol_refactorings(symbol, &lines, analysis.language));
        }

        // Sort by confidence and impact
        suggestions.sort_by(|a, b| {
            b.confidence.cmp(&a.confidence).then(
                b.impact
                    .maintainability_improvement
                    .cmp(&a.impact.maintainability_improvement),
            )
        });

        // Limit per file
        suggestions.truncate(self.config.max_suggestions_per_file);

        suggestions
    }

    /// Convert a code smell to a refactoring suggestion
    fn smell_to_refactoring(
        &self,
        smell: &CodeSmell,
        lines: &[&str],
        language: Language,
    ) -> Option<RefactoringSuggestion> {
        let original_code = self.get_code_range(lines, smell.line, smell.end_line);

        match smell.smell_type {
            crate::analyzer::CodeSmellType::LongFunction => Some(RefactoringSuggestion {
                kind: RefactoringKind::SplitFunction,
                confidence: Confidence::High,
                description: "Split long function into smaller, focused functions".to_string(),
                rationale: smell.message.clone(),
                file: smell.file.clone(),
                start_line: smell.line,
                end_line: smell.end_line,
                original_code: original_code.clone(),
                suggested_code: self.generate_split_function(&original_code, language),
                impact: RefactoringImpact {
                    complexity_change: -5,
                    readability_improvement: 40,
                    maintainability_improvement: 35,
                    test_impact: TestImpact::NewTestsNeeded,
                    loc_change: 10,
                },
                prerequisites: vec![],
                breaking_changes: vec![],
            }),
            crate::analyzer::CodeSmellType::DeepNesting => Some(RefactoringSuggestion {
                kind: RefactoringKind::EarlyReturn,
                confidence: Confidence::High,
                description: "Apply early return pattern to reduce nesting".to_string(),
                rationale: smell.message.clone(),
                file: smell.file.clone(),
                start_line: smell.line,
                end_line: smell.end_line,
                original_code: original_code.clone(),
                suggested_code: self.generate_early_return(&original_code, language),
                impact: RefactoringImpact {
                    complexity_change: -3,
                    readability_improvement: 50,
                    maintainability_improvement: 30,
                    test_impact: TestImpact::None,
                    loc_change: 0,
                },
                prerequisites: vec![],
                breaking_changes: vec![],
            }),
            crate::analyzer::CodeSmellType::MagicNumbers => Some(RefactoringSuggestion {
                kind: RefactoringKind::ReplaceMagicNumber,
                confidence: Confidence::VeryHigh,
                description: "Replace magic number with named constant".to_string(),
                rationale: smell.message.clone(),
                file: smell.file.clone(),
                start_line: smell.line,
                end_line: smell.end_line,
                original_code: smell.snippet.clone().unwrap_or_default(),
                suggested_code: self.generate_constant_replacement(&original_code, language),
                impact: RefactoringImpact {
                    complexity_change: 0,
                    readability_improvement: 30,
                    maintainability_improvement: 40,
                    test_impact: TestImpact::None,
                    loc_change: 2,
                },
                prerequisites: vec![],
                breaking_changes: vec![],
            }),
            crate::analyzer::CodeSmellType::MissingDocumentation => Some(RefactoringSuggestion {
                kind: RefactoringKind::AddTypeAnnotations,
                confidence: Confidence::Medium,
                description: "Add documentation to public function".to_string(),
                rationale: smell.message.clone(),
                file: smell.file.clone(),
                start_line: smell.line,
                end_line: smell.line,
                original_code: lines
                    .get((smell.line - 1) as usize)
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                suggested_code: self.generate_documentation(&original_code, language),
                impact: RefactoringImpact {
                    complexity_change: 0,
                    readability_improvement: 20,
                    maintainability_improvement: 25,
                    test_impact: TestImpact::None,
                    loc_change: 5,
                },
                prerequisites: vec![],
                breaking_changes: vec![],
            }),
            _ => None,
        }
    }

    /// Find pattern-based refactorings
    fn find_pattern_refactorings(
        &self,
        lines: &[&str],
        analysis: &FileAnalysis,
    ) -> Vec<RefactoringSuggestion> {
        let mut suggestions = Vec::new();

        // Check for duplicate code patterns
        if let Some(duplicates) = self.find_duplicates(lines) {
            for (start1, end1, start2, end2) in duplicates {
                suggestions.push(RefactoringSuggestion {
                    kind: RefactoringKind::MergeDuplicates,
                    confidence: Confidence::High,
                    description: "Extract duplicate code into a shared function".to_string(),
                    rationale: format!(
                        "Lines {}-{} and {}-{} contain duplicate code",
                        start1, end1, start2, end2
                    ),
                    file: analysis.path.clone(),
                    start_line: start1,
                    end_line: end1,
                    original_code: self.get_code_range(lines, start1, end1),
                    suggested_code: "// Extract to shared function".to_string(),
                    impact: RefactoringImpact {
                        complexity_change: -2,
                        readability_improvement: 30,
                        maintainability_improvement: 50,
                        test_impact: TestImpact::NewTestsNeeded,
                        loc_change: -((end1 - start1) as i32),
                    },
                    prerequisites: vec![],
                    breaking_changes: vec![],
                });
            }
        }

        // Check for loop to functional conversion (JS/TS/Rust)
        for (i, line) in lines.iter().enumerate() {
            if self.is_imperative_loop(line, analysis.language) {
                let (loop_start, loop_end) = self.find_loop_bounds(lines, i);
                let loop_code =
                    self.get_code_range(lines, loop_start as u32 + 1, loop_end as u32 + 1);

                if let Some(functional) = self.convert_to_functional(&loop_code, analysis.language)
                {
                    suggestions.push(RefactoringSuggestion {
                        kind: RefactoringKind::ConvertToFunctional,
                        confidence: Confidence::Medium,
                        description: "Convert imperative loop to functional style".to_string(),
                        rationale: "Functional style is often more readable and less error-prone"
                            .to_string(),
                        file: analysis.path.clone(),
                        start_line: loop_start as u32 + 1,
                        end_line: loop_end as u32 + 1,
                        original_code: loop_code,
                        suggested_code: functional,
                        impact: RefactoringImpact {
                            complexity_change: -1,
                            readability_improvement: 25,
                            maintainability_improvement: 20,
                            test_impact: TestImpact::None,
                            loc_change: -2,
                        },
                        prerequisites: vec![],
                        breaking_changes: vec![],
                    });
                }
            }
        }

        // Check for unused imports
        let unused_imports = self.find_unused_imports(lines, analysis);
        if !unused_imports.is_empty() {
            suggestions.push(RefactoringSuggestion {
                kind: RefactoringKind::RemoveUnusedImports,
                confidence: Confidence::VeryHigh,
                description: format!("Remove {} unused imports", unused_imports.len()),
                rationale: "Unused imports add clutter and may slow compilation".to_string(),
                file: analysis.path.clone(),
                start_line: 1,
                end_line: 1,
                original_code: unused_imports.join("\n"),
                suggested_code: "// Remove these imports".to_string(),
                impact: RefactoringImpact {
                    complexity_change: 0,
                    readability_improvement: 10,
                    maintainability_improvement: 15,
                    test_impact: TestImpact::None,
                    loc_change: -(unused_imports.len() as i32),
                },
                prerequisites: vec![],
                breaking_changes: vec![],
            });
        }

        suggestions
    }

    /// Suggest refactorings for a specific symbol
    fn suggest_symbol_refactorings(
        &self,
        symbol: &CodeSymbol,
        lines: &[&str],
        language: Language,
    ) -> Vec<RefactoringSuggestion> {
        let mut suggestions = Vec::new();

        // Check naming conventions
        if let Some(naming_suggestion) = self.check_naming_convention(symbol, language) {
            suggestions.push(naming_suggestion);
        }

        // Check for async conversion opportunities
        if symbol.kind == SymbolKind::Function {
            let func_code = self.get_code_range(lines, symbol.start_line, symbol.end_line);
            if self.could_be_async(&func_code, language) {
                suggestions.push(RefactoringSuggestion {
                    kind: RefactoringKind::ConvertToAsync,
                    confidence: Confidence::Medium,
                    description: format!("Convert '{}' to async function", symbol.name),
                    rationale: "This function performs I/O operations that could be async"
                        .to_string(),
                    file: PathBuf::new(),
                    start_line: symbol.start_line,
                    end_line: symbol.end_line,
                    original_code: func_code.clone(),
                    suggested_code: self.generate_async_version(&func_code, language),
                    impact: RefactoringImpact {
                        complexity_change: 1,
                        readability_improvement: 0,
                        maintainability_improvement: 10,
                        test_impact: TestImpact::MajorUpdates,
                        loc_change: 2,
                    },
                    prerequisites: vec!["Ensure async runtime is available".to_string()],
                    breaking_changes: vec!["Function signature changes".to_string()],
                });
            }
        }

        suggestions
    }

    /// Get code from a range of lines
    fn get_code_range(&self, lines: &[&str], start: u32, end: u32) -> String {
        let start_idx = (start.saturating_sub(1)) as usize;
        let end_idx = (end as usize).min(lines.len());

        lines[start_idx..end_idx].join("\n")
    }

    /// Generate split function suggestion
    fn generate_split_function(&self, _code: &str, language: Language) -> String {
        match language {
            Language::Rust => {
                format!(
                    "// Consider splitting into:\n\
                     // fn process_input(...) -> Result<...> {{ ... }}\n\
                     // fn validate_data(...) -> Result<...> {{ ... }}\n\
                     // fn transform_output(...) -> Result<...> {{ ... }}\n\n\
                     // Original function would become:\n\
                     // pub fn main_function(...) -> Result<...> {{\n\
                     //     let input = process_input(...)?;\n\
                     //     let validated = validate_data(input)?;\n\
                     //     transform_output(validated)\n\
                     // }}"
                )
            }
            Language::Python => "# Consider splitting into:\n\
                 # def process_input(...): ...\n\
                 # def validate_data(...): ...\n\
                 # def transform_output(...): ...\n\n\
                 # Original function would call these helpers"
                .to_string(),
            Language::JavaScript | Language::TypeScript => "// Consider splitting into:\n\
                 // function processInput(...) { ... }\n\
                 // function validateData(...) { ... }\n\
                 // function transformOutput(...) { ... }\n\n\
                 // Original function would orchestrate these"
                .to_string(),
            _ => "// Consider splitting this function into smaller, focused functions".to_string(),
        }
    }

    /// Generate early return pattern
    fn generate_early_return(&self, _code: &str, language: Language) -> String {
        // Simplified - in real implementation would parse and transform AST
        match language {
            Language::Rust => "// Instead of:\n\
                 // if condition {\n\
                 //     if another {\n\
                 //         // deeply nested\n\
                 //     }\n\
                 // }\n\n\
                 // Use:\n\
                 // if !condition { return early_value; }\n\
                 // if !another { return early_value; }\n\
                 // // main logic here"
                .to_string(),
            Language::Python => "# Instead of:\n\
                 # if condition:\n\
                 #     if another:\n\
                 #         # deeply nested\n\n\
                 # Use:\n\
                 # if not condition:\n\
                 #     return early_value\n\
                 # if not another:\n\
                 #     return early_value\n\
                 # # main logic here"
                .to_string(),
            _ => "// Apply early return pattern to reduce nesting".to_string(),
        }
    }

    /// Generate constant replacement
    fn generate_constant_replacement(&self, code: &str, language: Language) -> String {
        // Extract the magic number from code (simplified)
        let number = code
            .split_whitespace()
            .find(|s| s.parse::<i64>().is_ok())
            .unwrap_or("42");

        match language {
            Language::Rust => {
                format!(
                    "const MEANINGFUL_NAME: i32 = {};\n// Then use MEANINGFUL_NAME instead",
                    number
                )
            }
            Language::Python => {
                format!(
                    "MEANINGFUL_NAME = {}\n# Then use MEANINGFUL_NAME instead",
                    number
                )
            }
            Language::JavaScript | Language::TypeScript => {
                format!(
                    "const MEANINGFUL_NAME = {};\n// Then use MEANINGFUL_NAME instead",
                    number
                )
            }
            _ => format!("// Define constant for {}", number),
        }
    }

    /// Generate documentation
    fn generate_documentation(&self, code: &str, language: Language) -> String {
        match language {
            Language::Rust => {
                format!(
                    "/// Brief description of what this function does.\n\
                     ///\n\
                     /// # Arguments\n\
                     ///\n\
                     /// * `param` - Description of parameter\n\
                     ///\n\
                     /// # Returns\n\
                     ///\n\
                     /// Description of return value\n\
                     ///\n\
                     /// # Examples\n\
                     ///\n\
                     /// ```\n\
                     /// // Example usage\n\
                     /// ```\n\
                     {}",
                    code
                )
            }
            Language::Python => {
                format!(
                    "def function_name(params):\n\
                     \"\"\"Brief description.\n\
                     \n\
                     Args:\n\
                         param: Description of parameter\n\
                     \n\
                     Returns:\n\
                         Description of return value\n\
                     \n\
                     Examples:\n\
                         >>> example_usage()\n\
                     \"\"\"\n\
                     {}",
                    code
                )
            }
            Language::JavaScript | Language::TypeScript => {
                format!(
                    "/**\n\
                     * Brief description.\n\
                     *\n\
                     * @param param - Description of parameter\n\
                     * @returns Description of return value\n\
                     *\n\
                     * @example\n\
                     * // Example usage\n\
                     */\n\
                     {}",
                    code
                )
            }
            _ => format!("// Add documentation\n{}", code),
        }
    }

    /// Find duplicate code patterns
    fn find_duplicates(&self, lines: &[&str]) -> Option<Vec<(u32, u32, u32, u32)>> {
        // Simplified duplicate detection
        // In real implementation, would use more sophisticated algorithms
        let min_lines = 5;
        let mut duplicates = Vec::new();

        for i in 0..lines.len().saturating_sub(min_lines) {
            for j in (i + min_lines)..lines.len().saturating_sub(min_lines) {
                let mut matching = 0;
                while i + matching < j
                    && j + matching < lines.len()
                    && lines[i + matching].trim() == lines[j + matching].trim()
                    && !lines[i + matching].trim().is_empty()
                {
                    matching += 1;
                }

                if matching >= min_lines {
                    duplicates.push((
                        (i + 1) as u32,
                        (i + matching) as u32,
                        (j + 1) as u32,
                        (j + matching) as u32,
                    ));
                }
            }
        }

        if duplicates.is_empty() {
            None
        } else {
            Some(duplicates)
        }
    }

    /// Check if line starts an imperative loop
    fn is_imperative_loop(&self, line: &str, language: Language) -> bool {
        let trimmed = line.trim();
        match language {
            Language::Rust => trimmed.starts_with("for ") && trimmed.contains(" in "),
            Language::JavaScript | Language::TypeScript => {
                trimmed.starts_with("for (") || trimmed.starts_with("for(")
            }
            Language::Python => trimmed.starts_with("for ") && trimmed.contains(" in "),
            _ => false,
        }
    }

    /// Find loop bounds
    fn find_loop_bounds(&self, lines: &[&str], start: usize) -> (usize, usize) {
        let mut brace_count = 0;
        let mut found_start = false;

        for (i, line) in lines.iter().enumerate().skip(start) {
            for ch in line.chars() {
                match ch {
                    '{' => {
                        brace_count += 1;
                        found_start = true;
                    }
                    '}' => {
                        brace_count -= 1;
                        if found_start && brace_count == 0 {
                            return (start, i);
                        }
                    }
                    _ => {}
                }
            }
        }

        (start, lines.len() - 1)
    }

    /// Convert imperative loop to functional style
    fn convert_to_functional(&self, code: &str, language: Language) -> Option<String> {
        // Simplified conversion - real implementation would parse AST
        match language {
            Language::Rust => {
                if code.contains(".push(") {
                    Some(
                        "// Consider: collection.iter().map(|item| transform(item)).collect()"
                            .to_string(),
                    )
                } else if code.contains("if ") {
                    Some(
                        "// Consider: collection.iter().filter(|item| condition(item)).collect()"
                            .to_string(),
                    )
                } else {
                    None
                }
            }
            Language::JavaScript | Language::TypeScript => {
                if code.contains(".push(") {
                    Some("// Consider: array.map(item => transform(item))".to_string())
                } else if code.contains("if (") || code.contains("if(") {
                    Some("// Consider: array.filter(item => condition(item))".to_string())
                } else {
                    None
                }
            }
            Language::Python => {
                if code.contains(".append(") {
                    Some("# Consider: [transform(item) for item in collection]".to_string())
                } else if code.contains("if ") {
                    Some("# Consider: [item for item in collection if condition(item)]".to_string())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Find unused imports
    fn find_unused_imports(&self, lines: &[&str], analysis: &FileAnalysis) -> Vec<String> {
        let mut unused = Vec::new();
        let content = lines.join("\n");

        for import in &analysis.imports {
            // Check if the imported module/items are used
            let mut is_used = false;

            if import.items.is_empty() {
                // Module import - check if module name is used
                let module_name = import
                    .module
                    .split("::")
                    .last()
                    .or_else(|| import.module.split('.').last())
                    .unwrap_or(&import.module);
                is_used = content.matches(module_name).count() > 1; // More than just the import
            } else {
                // Specific items - check each
                for item in &import.items {
                    let item_name = item.split(" as ").last().unwrap_or(item).trim();
                    if content.matches(item_name).count() > 1 {
                        is_used = true;
                        break;
                    }
                }
            }

            if !is_used {
                unused.push(format!("{} (line {})", import.module, import.line));
            }
        }

        unused
    }

    /// Check naming conventions
    fn check_naming_convention(
        &self,
        symbol: &CodeSymbol,
        language: Language,
    ) -> Option<RefactoringSuggestion> {
        let name = &symbol.name;

        let suggestion = match language {
            Language::Rust => match symbol.kind {
                SymbolKind::Function | SymbolKind::Method => {
                    if !self.is_snake_case(name) {
                        Some(format!("Rename to '{}'", self.to_snake_case(name)))
                    } else {
                        None
                    }
                }
                SymbolKind::Struct | SymbolKind::Enum | SymbolKind::Trait => {
                    if !self.is_pascal_case(name) {
                        Some(format!("Rename to '{}'", self.to_pascal_case(name)))
                    } else {
                        None
                    }
                }
                SymbolKind::Constant => {
                    if !self.is_screaming_snake_case(name) {
                        Some(format!(
                            "Rename to '{}'",
                            self.to_screaming_snake_case(name)
                        ))
                    } else {
                        None
                    }
                }
                _ => None,
            },
            Language::JavaScript | Language::TypeScript => match symbol.kind {
                SymbolKind::Function | SymbolKind::Method | SymbolKind::Variable => {
                    if !self.is_camel_case(name) && !name.starts_with('_') {
                        Some(format!("Rename to '{}'", self.to_camel_case(name)))
                    } else {
                        None
                    }
                }
                SymbolKind::Class | SymbolKind::Interface => {
                    if !self.is_pascal_case(name) {
                        Some(format!("Rename to '{}'", self.to_pascal_case(name)))
                    } else {
                        None
                    }
                }
                _ => None,
            },
            Language::Python => match symbol.kind {
                SymbolKind::Function | SymbolKind::Method | SymbolKind::Variable => {
                    if !self.is_snake_case(name) && !name.starts_with('_') {
                        Some(format!("Rename to '{}'", self.to_snake_case(name)))
                    } else {
                        None
                    }
                }
                SymbolKind::Class => {
                    if !self.is_pascal_case(name) {
                        Some(format!("Rename to '{}'", self.to_pascal_case(name)))
                    } else {
                        None
                    }
                }
                SymbolKind::Constant => {
                    if !self.is_screaming_snake_case(name) {
                        Some(format!(
                            "Rename to '{}'",
                            self.to_screaming_snake_case(name)
                        ))
                    } else {
                        None
                    }
                }
                _ => None,
            },
            _ => None,
        };

        suggestion.map(|desc| RefactoringSuggestion {
            kind: RefactoringKind::Rename,
            confidence: Confidence::High,
            description: desc,
            rationale: "Follow language naming conventions for consistency".to_string(),
            file: PathBuf::new(),
            start_line: symbol.start_line,
            end_line: symbol.start_line,
            original_code: name.clone(),
            suggested_code: match language {
                Language::Rust => self.to_snake_case(name),
                Language::JavaScript | Language::TypeScript => self.to_camel_case(name),
                Language::Python => self.to_snake_case(name),
                _ => name.clone(),
            },
            impact: RefactoringImpact {
                complexity_change: 0,
                readability_improvement: 15,
                maintainability_improvement: 10,
                test_impact: TestImpact::MinorUpdates,
                loc_change: 0,
            },
            prerequisites: vec![],
            breaking_changes: if symbol.visibility == crate::analyzer::Visibility::Public {
                vec!["This is a public symbol - renaming may break consumers".to_string()]
            } else {
                vec![]
            },
        })
    }

    /// Check if function could be async
    fn could_be_async(&self, code: &str, language: Language) -> bool {
        let io_patterns = match language {
            Language::Rust => vec!["std::fs::", "std::net::", "reqwest::", "tokio::fs::"],
            Language::JavaScript | Language::TypeScript => {
                vec!["fetch(", "axios.", "fs.read", "http."]
            }
            Language::Python => vec!["open(", "requests.", "urllib.", "socket."],
            _ => vec![],
        };

        io_patterns.iter().any(|pattern| code.contains(pattern))
    }

    /// Generate async version of function
    fn generate_async_version(&self, code: &str, language: Language) -> String {
        match language {
            Language::Rust => code
                .replace("fn ", "async fn ")
                .replace("std::fs::", "tokio::fs::"),
            Language::JavaScript => {
                if code.contains("function ") {
                    code.replace("function ", "async function ")
                } else {
                    format!("async {}", code)
                }
            }
            Language::TypeScript => code.replace("function ", "async function "),
            Language::Python => code.replace("def ", "async def "),
            _ => code.to_string(),
        }
    }

    // Naming convention helpers
    fn is_snake_case(&self, s: &str) -> bool {
        s.chars()
            .all(|c| c.is_lowercase() || c.is_numeric() || c == '_')
    }

    fn is_camel_case(&self, s: &str) -> bool {
        !s.is_empty() && s.chars().next().unwrap().is_lowercase() && !s.contains('_')
    }

    fn is_pascal_case(&self, s: &str) -> bool {
        !s.is_empty() && s.chars().next().unwrap().is_uppercase() && !s.contains('_')
    }

    fn is_screaming_snake_case(&self, s: &str) -> bool {
        s.chars()
            .all(|c| c.is_uppercase() || c.is_numeric() || c == '_')
    }

    fn to_snake_case(&self, s: &str) -> String {
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        }
        result
    }

    fn to_camel_case(&self, s: &str) -> String {
        let mut result = String::new();
        let mut capitalize_next = false;
        for (i, c) in s.chars().enumerate() {
            if c == '_' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(c.to_uppercase().next().unwrap());
                capitalize_next = false;
            } else if i == 0 {
                result.push(c.to_lowercase().next().unwrap());
            } else {
                result.push(c);
            }
        }
        result
    }

    fn to_pascal_case(&self, s: &str) -> String {
        let camel = self.to_camel_case(s);
        let mut chars = camel.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }

    fn to_screaming_snake_case(&self, s: &str) -> String {
        self.to_snake_case(s).to_uppercase()
    }
}

impl Default for RefactoringEngine {
    fn default() -> Self {
        Self::new()
    }
}

// Helper trait for Python compatibility
#[allow(dead_code)]
trait StringExt {
    fn startswith(&self, prefix: &str) -> bool;
}

impl StringExt for String {
    fn startswith(&self, prefix: &str) -> bool {
        self.starts_with(prefix)
    }
}

impl StringExt for &str {
    fn startswith(&self, prefix: &str) -> bool {
        self.starts_with(prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_naming_conventions() {
        let engine = RefactoringEngine::new();

        assert!(engine.is_snake_case("my_function"));
        assert!(!engine.is_snake_case("myFunction"));

        assert!(engine.is_camel_case("myFunction"));
        assert!(!engine.is_camel_case("MyFunction"));

        assert!(engine.is_pascal_case("MyClass"));
        assert!(!engine.is_pascal_case("myClass"));

        assert!(engine.is_screaming_snake_case("MY_CONSTANT"));
        assert!(!engine.is_screaming_snake_case("my_constant"));
    }

    #[test]
    fn test_case_conversion() {
        let engine = RefactoringEngine::new();

        assert_eq!(engine.to_snake_case("myFunction"), "my_function");
        assert_eq!(engine.to_camel_case("my_function"), "myFunction");
        assert_eq!(engine.to_pascal_case("my_function"), "MyFunction");
        assert_eq!(engine.to_screaming_snake_case("myFunction"), "MY_FUNCTION");
    }

    #[test]
    fn test_duplicate_detection() {
        let engine = RefactoringEngine::new();
        let lines = vec![
            "fn a() {",
            "    let x = 1;",
            "    let y = 2;",
            "    let z = x + y;",
            "    println!(\"{}\", z);",
            "}",
            "",
            "fn b() {",
            "    let x = 1;",
            "    let y = 2;",
            "    let z = x + y;",
            "    println!(\"{}\", z);",
            "}",
        ];

        let duplicates = engine.find_duplicates(&lines);
        assert!(duplicates.is_some());
    }
}
