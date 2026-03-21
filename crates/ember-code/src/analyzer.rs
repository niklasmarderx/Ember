//! Code Analysis Engine
//!
//! AST-based code analysis supporting multiple languages with complexity metrics,
//! code smell detection, and structural analysis.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Supported programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    Unknown,
}

impl Language {
    /// Detect language from file extension
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "rs" => Language::Rust,
            "py" => Language::Python,
            "js" | "mjs" | "cjs" => Language::JavaScript,
            "ts" | "tsx" => Language::TypeScript,
            "go" => Language::Go,
            "java" => Language::Java,
            _ => Language::Unknown,
        }
    }

    /// Get file extensions for this language
    pub fn extensions(&self) -> &[&str] {
        match self {
            Language::Rust => &["rs"],
            Language::Python => &["py", "pyi"],
            Language::JavaScript => &["js", "mjs", "cjs", "jsx"],
            Language::TypeScript => &["ts", "tsx", "mts", "cts"],
            Language::Go => &["go"],
            Language::Java => &["java"],
            Language::Unknown => &[],
        }
    }
}

/// Code complexity metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComplexityMetrics {
    /// Cyclomatic complexity
    pub cyclomatic: u32,
    /// Cognitive complexity (Sonar-style)
    pub cognitive: u32,
    /// Lines of code (excluding blanks/comments)
    pub loc: u32,
    /// Source lines of code
    pub sloc: u32,
    /// Comment lines
    pub comment_lines: u32,
    /// Blank lines
    pub blank_lines: u32,
    /// Nesting depth (max)
    pub max_nesting: u32,
    /// Number of parameters (for functions)
    pub parameter_count: u32,
    /// Maintainability index (0-100)
    pub maintainability_index: f32,
}

impl ComplexityMetrics {
    /// Calculate maintainability index from other metrics
    pub fn calculate_maintainability(&mut self) {
        // Maintainability Index = 171 - 5.2 * ln(Halstead Volume) - 0.23 * Cyclomatic - 16.2 * ln(LOC)
        // Simplified version without Halstead
        let loc_factor = if self.loc > 0 {
            (self.loc as f32).ln()
        } else {
            0.0
        };

        let raw = 171.0 - 0.23 * self.cyclomatic as f32 - 16.2 * loc_factor;
        self.maintainability_index = (raw * 100.0 / 171.0).clamp(0.0, 100.0);
    }

    /// Get complexity rating
    pub fn rating(&self) -> ComplexityRating {
        match self.cyclomatic {
            0..=10 => ComplexityRating::Low,
            11..=20 => ComplexityRating::Moderate,
            21..=50 => ComplexityRating::High,
            _ => ComplexityRating::VeryHigh,
        }
    }
}

/// Complexity rating categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComplexityRating {
    Low,
    Moderate,
    High,
    VeryHigh,
}

/// Types of code smells
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeSmellType {
    LongFunction,
    LongParameterList,
    DeepNesting,
    DuplicateCode,
    LargeClass,
    GodClass,
    FeatureEnvy,
    DataClumps,
    PrimitiveObsession,
    DeadCode,
    CommentedCode,
    MagicNumbers,
    HardcodedStrings,
    MissingDocumentation,
    InconsistentNaming,
    ComplexConditional,
    LongChain,
    TooManyImports,
}

/// Severity levels for issues
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}

/// A detected code smell or issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSmell {
    /// Type of smell
    pub smell_type: CodeSmellType,
    /// Severity level
    pub severity: Severity,
    /// Human-readable message
    pub message: String,
    /// File location
    pub file: PathBuf,
    /// Line number (1-indexed)
    pub line: u32,
    /// Column number (1-indexed)
    pub column: u32,
    /// End line
    pub end_line: u32,
    /// End column
    pub end_column: u32,
    /// Suggested fix
    pub suggestion: Option<String>,
    /// Related code snippet
    pub snippet: Option<String>,
}

/// A code symbol (function, class, variable, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSymbol {
    /// Symbol name
    pub name: String,
    /// Symbol kind
    pub kind: SymbolKind,
    /// Full qualified name
    pub qualified_name: String,
    /// Documentation string
    pub documentation: Option<String>,
    /// Start line
    pub start_line: u32,
    /// End line
    pub end_line: u32,
    /// Visibility
    pub visibility: Visibility,
    /// Complexity metrics (for functions)
    pub metrics: Option<ComplexityMetrics>,
    /// Child symbols
    pub children: Vec<CodeSymbol>,
    /// Type signature
    pub signature: Option<String>,
}

/// Types of code symbols
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Module,
    Class,
    Struct,
    Enum,
    Interface,
    Trait,
    Function,
    Method,
    Constructor,
    Variable,
    Constant,
    Field,
    Property,
    Parameter,
    TypeParameter,
    Import,
}

/// Visibility levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Protected,
    Private,
    Internal,
    Unknown,
}

/// Analysis result for a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAnalysis {
    /// File path
    pub path: PathBuf,
    /// Detected language
    pub language: Language,
    /// File-level metrics
    pub metrics: ComplexityMetrics,
    /// All symbols in the file
    pub symbols: Vec<CodeSymbol>,
    /// Detected code smells
    pub smells: Vec<CodeSmell>,
    /// Import statements
    pub imports: Vec<ImportInfo>,
    /// Dependencies
    pub dependencies: Vec<String>,
    /// Analysis timestamp
    pub analyzed_at: chrono::DateTime<chrono::Utc>,
}

/// Import information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportInfo {
    /// Module/package being imported
    pub module: String,
    /// Specific items imported (if any)
    pub items: Vec<String>,
    /// Is it a wildcard import
    pub is_wildcard: bool,
    /// Line number
    pub line: u32,
}

/// Project-wide analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectAnalysis {
    /// Project root path
    pub root: PathBuf,
    /// Per-file analysis
    pub files: Vec<FileAnalysis>,
    /// Aggregated metrics
    pub total_metrics: ComplexityMetrics,
    /// All code smells across files
    pub all_smells: Vec<CodeSmell>,
    /// Dependency graph
    pub dependency_graph: HashMap<String, Vec<String>>,
    /// Language statistics
    pub language_stats: HashMap<Language, LanguageStats>,
    /// Analysis duration
    pub duration_ms: u64,
}

/// Statistics per language
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LanguageStats {
    /// Number of files
    pub file_count: u32,
    /// Total lines of code
    pub total_loc: u32,
    /// Total symbols
    pub symbol_count: u32,
    /// Average complexity
    pub avg_complexity: f32,
}

/// Code analyzer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzerConfig {
    /// Maximum cyclomatic complexity before warning
    pub max_complexity: u32,
    /// Maximum function length (lines)
    pub max_function_length: u32,
    /// Maximum parameter count
    pub max_parameters: u32,
    /// Maximum nesting depth
    pub max_nesting: u32,
    /// Maximum file length
    pub max_file_length: u32,
    /// Patterns to ignore
    pub ignore_patterns: Vec<String>,
    /// Languages to analyze
    pub languages: Vec<Language>,
    /// Enable duplicate detection
    pub detect_duplicates: bool,
    /// Minimum duplicate length (tokens)
    pub min_duplicate_tokens: u32,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            max_complexity: 15,
            max_function_length: 50,
            max_parameters: 5,
            max_nesting: 4,
            max_file_length: 500,
            ignore_patterns: vec![
                "**/node_modules/**".to_string(),
                "**/target/**".to_string(),
                "**/.git/**".to_string(),
                "**/vendor/**".to_string(),
                "**/__pycache__/**".to_string(),
            ],
            languages: vec![
                Language::Rust,
                Language::Python,
                Language::JavaScript,
                Language::TypeScript,
                Language::Go,
                Language::Java,
            ],
            detect_duplicates: true,
            min_duplicate_tokens: 50,
        }
    }
}

/// Analyzer errors
#[derive(Debug, Error)]
pub enum AnalyzerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error in {file}: {message}")]
    Parse { file: PathBuf, message: String },

    #[error("Unsupported language: {0:?}")]
    UnsupportedLanguage(Language),

    #[error("Configuration error: {0}")]
    Config(String),
}

/// The main code analyzer
pub struct CodeAnalyzer {
    config: AnalyzerConfig,
}

impl CodeAnalyzer {
    /// Create a new analyzer with default config
    pub fn new() -> Self {
        Self {
            config: AnalyzerConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: AnalyzerConfig) -> Self {
        Self { config }
    }

    /// Analyze a single file
    pub async fn analyze_file(&self, path: &Path) -> Result<FileAnalysis, AnalyzerError> {
        let content = tokio::fs::read_to_string(path).await?;
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let language = Language::from_extension(extension);

        self.analyze_content(path.to_path_buf(), &content, language)
    }

    /// Analyze code content directly
    pub fn analyze_content(
        &self,
        path: PathBuf,
        content: &str,
        language: Language,
    ) -> Result<FileAnalysis, AnalyzerError> {
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len() as u32;

        // Calculate basic metrics
        let mut metrics = ComplexityMetrics::default();
        let mut blank_lines = 0u32;
        let mut comment_lines = 0u32;
        let mut in_multiline_comment = false;

        for line in &lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                blank_lines += 1;
            } else if self.is_comment_line(trimmed, language, &mut in_multiline_comment) {
                comment_lines += 1;
            }
        }

        metrics.loc = total_lines;
        metrics.sloc = total_lines - blank_lines - comment_lines;
        metrics.blank_lines = blank_lines;
        metrics.comment_lines = comment_lines;

        // Calculate cyclomatic complexity
        metrics.cyclomatic = self.calculate_cyclomatic(content, language);
        metrics.cognitive = self.calculate_cognitive(content, language);
        metrics.max_nesting = self.calculate_max_nesting(content, language);
        metrics.calculate_maintainability();

        // Extract symbols
        let symbols = self.extract_symbols(content, language)?;

        // Detect code smells
        let smells = self.detect_smells(&path, content, language, &metrics, &symbols);

        // Extract imports
        let imports = self.extract_imports(content, language);

        Ok(FileAnalysis {
            path,
            language,
            metrics,
            symbols,
            smells,
            imports,
            dependencies: vec![],
            analyzed_at: chrono::Utc::now(),
        })
    }

    /// Analyze entire project
    pub async fn analyze_project(&self, root: &Path) -> Result<ProjectAnalysis, AnalyzerError> {
        use std::time::Instant;
        let start = Instant::now();

        let mut files = Vec::new();
        let mut all_smells = Vec::new();
        let mut language_stats: HashMap<Language, LanguageStats> = HashMap::new();
        let mut total_metrics = ComplexityMetrics::default();

        // Collect all source files
        let source_files = self.collect_source_files(root).await?;

        // Analyze each file
        for path in source_files {
            match self.analyze_file(&path).await {
                Ok(analysis) => {
                    // Update language stats
                    let stats = language_stats.entry(analysis.language).or_default();
                    stats.file_count += 1;
                    stats.total_loc += analysis.metrics.sloc;
                    stats.symbol_count += analysis.symbols.len() as u32;

                    // Aggregate metrics
                    total_metrics.loc += analysis.metrics.loc;
                    total_metrics.sloc += analysis.metrics.sloc;
                    total_metrics.comment_lines += analysis.metrics.comment_lines;
                    total_metrics.blank_lines += analysis.metrics.blank_lines;

                    // Collect smells
                    all_smells.extend(analysis.smells.clone());

                    files.push(analysis);
                }
                Err(e) => {
                    tracing::warn!("Failed to analyze {}: {}", path.display(), e);
                }
            }
        }

        // Calculate average complexity per language
        for (_lang, stats) in language_stats.iter_mut() {
            if stats.file_count > 0 {
                stats.avg_complexity = stats.total_loc as f32 / stats.file_count as f32;
            }
        }

        // Calculate total maintainability
        total_metrics.calculate_maintainability();

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ProjectAnalysis {
            root: root.to_path_buf(),
            files,
            total_metrics,
            all_smells,
            dependency_graph: HashMap::new(),
            language_stats,
            duration_ms,
        })
    }

    /// Collect all source files recursively
    async fn collect_source_files(&self, root: &Path) -> Result<Vec<PathBuf>, AnalyzerError> {
        use tokio::fs;

        let mut files = Vec::new();
        let mut stack = vec![root.to_path_buf()];

        while let Some(dir) = stack.pop() {
            let mut entries = fs::read_dir(&dir).await?;

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let metadata = entry.metadata().await?;

                if metadata.is_dir() {
                    // Check ignore patterns
                    let path_str = path.to_string_lossy();
                    let should_ignore = self.config.ignore_patterns.iter().any(|pattern| {
                        glob::Pattern::new(pattern)
                            .map(|p| p.matches(&path_str))
                            .unwrap_or(false)
                    });

                    if !should_ignore {
                        stack.push(path);
                    }
                } else if metadata.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        let lang = Language::from_extension(ext);
                        if lang != Language::Unknown && self.config.languages.contains(&lang) {
                            files.push(path);
                        }
                    }
                }
            }
        }

        Ok(files)
    }

    /// Check if a line is a comment
    fn is_comment_line(&self, line: &str, language: Language, in_multiline: &mut bool) -> bool {
        match language {
            Language::Rust
            | Language::Go
            | Language::Java
            | Language::JavaScript
            | Language::TypeScript => {
                if *in_multiline {
                    if line.contains("*/") {
                        *in_multiline = false;
                    }
                    return true;
                }
                if line.starts_with("//") {
                    return true;
                }
                if line.starts_with("/*") {
                    *in_multiline = !line.contains("*/");
                    return true;
                }
                false
            }
            Language::Python => {
                if *in_multiline {
                    if line.contains("\"\"\"") || line.contains("'''") {
                        *in_multiline = false;
                    }
                    return true;
                }
                if line.starts_with('#') {
                    return true;
                }
                if line.starts_with("\"\"\"") || line.starts_with("'''") {
                    *in_multiline =
                        !(line.matches("\"\"\"").count() >= 2 || line.matches("'''").count() >= 2);
                    return true;
                }
                false
            }
            Language::Unknown => false,
        }
    }

    /// Calculate cyclomatic complexity
    fn calculate_cyclomatic(&self, content: &str, language: Language) -> u32 {
        let mut complexity = 1u32; // Base complexity

        let keywords = match language {
            Language::Rust => vec![
                "if", "else if", "while", "for", "loop", "match", "&&", "||", "?",
            ],
            Language::Python => vec!["if", "elif", "while", "for", "and", "or", "except", "with"],
            Language::JavaScript | Language::TypeScript => vec![
                "if", "else if", "while", "for", "switch", "case", "catch", "&&", "||", "?", "?.",
            ],
            Language::Go => vec![
                "if", "else if", "for", "switch", "case", "select", "&&", "||",
            ],
            Language::Java => vec![
                "if", "else if", "while", "for", "switch", "case", "catch", "&&", "||", "?",
            ],
            Language::Unknown => vec![],
        };

        for keyword in keywords {
            complexity += content.matches(keyword).count() as u32;
        }

        complexity
    }

    /// Calculate cognitive complexity
    fn calculate_cognitive(&self, content: &str, language: Language) -> u32 {
        let mut complexity = 0u32;
        let mut nesting = 0u32;

        let lines: Vec<&str> = content.lines().collect();

        for line in lines {
            let trimmed = line.trim();

            // Track nesting
            let opens = trimmed.matches('{').count() + trimmed.matches('(').count();
            let closes = trimmed.matches('}').count() + trimmed.matches(')').count();

            // Add complexity for control structures with nesting penalty
            let control_keywords = match language {
                Language::Rust => vec!["if ", "else ", "while ", "for ", "loop ", "match "],
                Language::Python => {
                    vec!["if ", "elif ", "else:", "while ", "for ", "try:", "except "]
                }
                Language::JavaScript | Language::TypeScript | Language::Java => vec![
                    "if ", "else ", "while ", "for ", "switch ", "try ", "catch ",
                ],
                Language::Go => vec!["if ", "else ", "for ", "switch ", "select "],
                Language::Unknown => vec![],
            };

            for keyword in control_keywords {
                if trimmed.starts_with(keyword) || trimmed.contains(&format!(" {}", keyword)) {
                    complexity += 1 + nesting; // Base + nesting penalty
                }
            }

            // Update nesting level
            nesting = nesting
                .saturating_add(opens as u32)
                .saturating_sub(closes as u32);
        }

        complexity
    }

    /// Calculate maximum nesting depth
    fn calculate_max_nesting(&self, content: &str, _language: Language) -> u32 {
        let mut max_nesting = 0u32;
        let mut current_nesting = 0u32;

        for ch in content.chars() {
            match ch {
                '{' | '(' | '[' => {
                    current_nesting += 1;
                    max_nesting = max_nesting.max(current_nesting);
                }
                '}' | ')' | ']' => {
                    current_nesting = current_nesting.saturating_sub(1);
                }
                _ => {}
            }
        }

        max_nesting
    }

    /// Extract code symbols from content
    fn extract_symbols(
        &self,
        content: &str,
        language: Language,
    ) -> Result<Vec<CodeSymbol>, AnalyzerError> {
        let mut symbols = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        let function_patterns = match language {
            Language::Rust => vec![
                (r"pub\s+fn\s+(\w+)", Visibility::Public),
                (r"fn\s+(\w+)", Visibility::Private),
                (r"pub\s+async\s+fn\s+(\w+)", Visibility::Public),
                (r"async\s+fn\s+(\w+)", Visibility::Private),
            ],
            Language::Python => vec![
                (r"def\s+(\w+)", Visibility::Public),
                (r"async\s+def\s+(\w+)", Visibility::Public),
            ],
            Language::JavaScript | Language::TypeScript => vec![
                (r"function\s+(\w+)", Visibility::Public),
                (r"async\s+function\s+(\w+)", Visibility::Public),
                (r"export\s+function\s+(\w+)", Visibility::Public),
                (r"const\s+(\w+)\s*=\s*\(", Visibility::Public),
                (r"const\s+(\w+)\s*=\s*async\s*\(", Visibility::Public),
            ],
            Language::Go => vec![
                (r"func\s+(\w+)", Visibility::Unknown),
                (r"func\s+\(\w+\s+\*?\w+\)\s+(\w+)", Visibility::Unknown),
            ],
            Language::Java => vec![
                (r"public\s+\w+\s+(\w+)\s*\(", Visibility::Public),
                (r"private\s+\w+\s+(\w+)\s*\(", Visibility::Private),
                (r"protected\s+\w+\s+(\w+)\s*\(", Visibility::Protected),
            ],
            Language::Unknown => vec![],
        };

        for (line_num, line) in lines.iter().enumerate() {
            for (pattern, visibility) in &function_patterns {
                if let Ok(re) = regex::Regex::new(pattern) {
                    if let Some(caps) = re.captures(line) {
                        if let Some(name) = caps.get(1) {
                            symbols.push(CodeSymbol {
                                name: name.as_str().to_string(),
                                kind: SymbolKind::Function,
                                qualified_name: name.as_str().to_string(),
                                documentation: self.extract_doc_comment(&lines, line_num),
                                start_line: (line_num + 1) as u32,
                                end_line: self.find_function_end(&lines, line_num, language) as u32,
                                visibility: *visibility,
                                metrics: None,
                                children: vec![],
                                signature: Some(line.trim().to_string()),
                            });
                        }
                    }
                }
            }
        }

        Ok(symbols)
    }

    /// Extract documentation comment above a line
    fn extract_doc_comment(&self, lines: &[&str], line_num: usize) -> Option<String> {
        if line_num == 0 {
            return None;
        }

        let mut doc_lines = Vec::new();
        let mut i = line_num - 1;

        loop {
            let line = lines.get(i)?.trim();

            if line.starts_with("///") {
                doc_lines.push(line.trim_start_matches("///").trim());
            } else if line.starts_with("//!") {
                doc_lines.push(line.trim_start_matches("//!").trim());
            } else if line.starts_with('#') {
                doc_lines.push(line.trim_start_matches('#').trim());
            } else if line.starts_with("/**") || line.starts_with("/*") {
                // Multi-line doc comment end
                break;
            } else if !line.is_empty() {
                break;
            }

            if i == 0 {
                break;
            }
            i -= 1;
        }

        if doc_lines.is_empty() {
            None
        } else {
            doc_lines.reverse();
            Some(doc_lines.join("\n"))
        }
    }

    /// Find the end of a function
    fn find_function_end(&self, lines: &[&str], start: usize, language: Language) -> usize {
        match language {
            Language::Python => {
                // Python: track indentation
                let start_indent = lines
                    .get(start)
                    .map(|l| l.len() - l.trim_start().len())
                    .unwrap_or(0);

                for (i, line) in lines.iter().enumerate().skip(start + 1) {
                    let line_indent = line.len() - line.trim_start().len();
                    if !line.trim().is_empty() && line_indent <= start_indent {
                        return i;
                    }
                }
                lines.len()
            }
            _ => {
                // Brace-based languages
                let mut brace_count = 0i32;
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
                                    return i + 1;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                lines.len()
            }
        }
    }

    /// Detect code smells
    fn detect_smells(
        &self,
        path: &Path,
        content: &str,
        _language: Language,
        metrics: &ComplexityMetrics,
        symbols: &[CodeSymbol],
    ) -> Vec<CodeSmell> {
        let mut smells = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Check file length
        if metrics.sloc > self.config.max_file_length {
            smells.push(CodeSmell {
                smell_type: CodeSmellType::LargeClass,
                severity: Severity::Warning,
                message: format!(
                    "File has {} lines of code (max: {})",
                    metrics.sloc, self.config.max_file_length
                ),
                file: path.to_path_buf(),
                line: 1,
                column: 1,
                end_line: metrics.loc,
                end_column: 1,
                suggestion: Some("Consider splitting into smaller modules".to_string()),
                snippet: None,
            });
        }

        // Check each function
        for symbol in symbols {
            if symbol.kind == SymbolKind::Function || symbol.kind == SymbolKind::Method {
                let func_lines = symbol.end_line.saturating_sub(symbol.start_line);

                // Long function
                if func_lines > self.config.max_function_length {
                    smells.push(CodeSmell {
                        smell_type: CodeSmellType::LongFunction,
                        severity: Severity::Warning,
                        message: format!(
                            "Function '{}' has {} lines (max: {})",
                            symbol.name, func_lines, self.config.max_function_length
                        ),
                        file: path.to_path_buf(),
                        line: symbol.start_line,
                        column: 1,
                        end_line: symbol.end_line,
                        end_column: 1,
                        suggestion: Some("Consider extracting smaller functions".to_string()),
                        snippet: lines
                            .get((symbol.start_line - 1) as usize)
                            .map(|s| s.to_string()),
                    });
                }

                // Missing documentation
                if symbol.documentation.is_none() && symbol.visibility == Visibility::Public {
                    smells.push(CodeSmell {
                        smell_type: CodeSmellType::MissingDocumentation,
                        severity: Severity::Info,
                        message: format!("Public function '{}' lacks documentation", symbol.name),
                        file: path.to_path_buf(),
                        line: symbol.start_line,
                        column: 1,
                        end_line: symbol.start_line,
                        end_column: 1,
                        suggestion: Some("Add documentation comment".to_string()),
                        snippet: None,
                    });
                }
            }
        }

        // Check nesting depth
        if metrics.max_nesting > self.config.max_nesting {
            smells.push(CodeSmell {
                smell_type: CodeSmellType::DeepNesting,
                severity: Severity::Warning,
                message: format!(
                    "Maximum nesting depth is {} (max: {})",
                    metrics.max_nesting, self.config.max_nesting
                ),
                file: path.to_path_buf(),
                line: 1,
                column: 1,
                end_line: metrics.loc,
                end_column: 1,
                suggestion: Some("Consider early returns or extracting nested logic".to_string()),
                snippet: None,
            });
        }

        // Check for magic numbers
        let magic_number_re = regex::Regex::new(r"\b\d{2,}\b").unwrap();
        for (i, line) in lines.iter().enumerate() {
            // Skip comments
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("/*") {
                continue;
            }

            // Skip constant definitions
            if trimmed.contains("const ") || trimmed.contains("static ") || trimmed.contains(" = ")
            {
                continue;
            }

            for mat in magic_number_re.find_iter(line) {
                let num: i64 = mat.as_str().parse().unwrap_or(0);
                // Skip common acceptable values
                if num > 1 && num != 10 && num != 100 && num != 1000 {
                    smells.push(CodeSmell {
                        smell_type: CodeSmellType::MagicNumbers,
                        severity: Severity::Info,
                        message: format!("Magic number {} found", num),
                        file: path.to_path_buf(),
                        line: (i + 1) as u32,
                        column: (mat.start() + 1) as u32,
                        end_line: (i + 1) as u32,
                        end_column: (mat.end() + 1) as u32,
                        suggestion: Some("Consider using a named constant".to_string()),
                        snippet: Some(line.to_string()),
                    });
                }
            }
        }

        // Check for TODO/FIXME comments
        for (i, line) in lines.iter().enumerate() {
            if line.contains("TODO") || line.contains("FIXME") || line.contains("HACK") {
                smells.push(CodeSmell {
                    smell_type: CodeSmellType::CommentedCode,
                    severity: Severity::Info,
                    message: "TODO/FIXME comment found".to_string(),
                    file: path.to_path_buf(),
                    line: (i + 1) as u32,
                    column: 1,
                    end_line: (i + 1) as u32,
                    end_column: line.len() as u32,
                    suggestion: Some("Address the TODO or create an issue".to_string()),
                    snippet: Some(line.trim().to_string()),
                });
            }
        }

        smells
    }

    /// Extract import statements
    fn extract_imports(&self, content: &str, language: Language) -> Vec<ImportInfo> {
        let mut imports = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        let import_patterns: Vec<(&str, bool)> = match language {
            Language::Rust => vec![
                (r"use\s+([\w:]+)(?:::\{([^}]+)\})?", false),
                (r"use\s+([\w:]+)::\*", true),
            ],
            Language::Python => vec![
                (r"import\s+([\w.]+)", false),
                (r"from\s+([\w.]+)\s+import\s+(.+)", false),
                (r"from\s+([\w.]+)\s+import\s+\*", true),
            ],
            Language::JavaScript | Language::TypeScript => vec![
                (r#"import\s+.*\s+from\s+['"]([^'"]+)['"]"#, false),
                (r#"import\s+['"]([^'"]+)['"]"#, false),
                (r#"require\s*\(\s*['"]([^'"]+)['"]\s*\)"#, false),
            ],
            Language::Go => vec![
                (r#"import\s+["']([^"']+)["']"#, false),
                (r#"import\s+\w+\s+["']([^"']+)["']"#, false),
            ],
            Language::Java => vec![
                (r"import\s+([\w.]+);", false),
                (r"import\s+([\w.]+)\.\*;", true),
            ],
            Language::Unknown => vec![],
        };

        for (line_num, line) in lines.iter().enumerate() {
            for (pattern, is_wildcard) in &import_patterns {
                if let Ok(re) = regex::Regex::new(pattern) {
                    if let Some(caps) = re.captures(line) {
                        let module = caps
                            .get(1)
                            .map(|m| m.as_str().to_string())
                            .unwrap_or_default();
                        let items = caps
                            .get(2)
                            .map(|m| {
                                m.as_str()
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .collect()
                            })
                            .unwrap_or_default();

                        imports.push(ImportInfo {
                            module,
                            items,
                            is_wildcard: *is_wildcard,
                            line: (line_num + 1) as u32,
                        });
                    }
                }
            }
        }

        imports
    }
}

impl Default for CodeAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detection() {
        assert_eq!(Language::from_extension("rs"), Language::Rust);
        assert_eq!(Language::from_extension("py"), Language::Python);
        assert_eq!(Language::from_extension("js"), Language::JavaScript);
        assert_eq!(Language::from_extension("ts"), Language::TypeScript);
        assert_eq!(Language::from_extension("go"), Language::Go);
        assert_eq!(Language::from_extension("java"), Language::Java);
        assert_eq!(Language::from_extension("txt"), Language::Unknown);
    }

    #[test]
    fn test_complexity_rating() {
        let mut metrics = ComplexityMetrics::default();

        metrics.cyclomatic = 5;
        assert_eq!(metrics.rating(), ComplexityRating::Low);

        metrics.cyclomatic = 15;
        assert_eq!(metrics.rating(), ComplexityRating::Moderate);

        metrics.cyclomatic = 30;
        assert_eq!(metrics.rating(), ComplexityRating::High);

        metrics.cyclomatic = 60;
        assert_eq!(metrics.rating(), ComplexityRating::VeryHigh);
    }

    #[test]
    fn test_rust_analysis() {
        let analyzer = CodeAnalyzer::new();
        let code = r#"
/// A simple function
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn private_fn() {
    if true {
        println!("nested");
    }
}
"#;

        let result = analyzer
            .analyze_content(PathBuf::from("test.rs"), code, Language::Rust)
            .unwrap();

        assert_eq!(result.language, Language::Rust);
        assert!(result.symbols.len() >= 2);
        assert!(result.metrics.sloc > 0);
    }

    #[test]
    fn test_python_analysis() {
        let analyzer = CodeAnalyzer::new();
        let code = r#"
def hello(name):
    """Say hello to someone."""
    print(f"Hello, {name}!")

async def fetch_data():
    pass
"#;

        let result = analyzer
            .analyze_content(PathBuf::from("test.py"), code, Language::Python)
            .unwrap();

        assert_eq!(result.language, Language::Python);
        assert!(!result.symbols.is_empty());
    }
}
