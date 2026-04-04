//! Code intelligence CLI commands.
//!
//! Provides AI-powered code analysis, refactoring suggestions, and test generation.

use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};
use colored::Colorize;
use std::path::{Path, PathBuf};

/// Arguments for the code command.
#[derive(Args)]
pub struct CodeArgs {
    #[command(subcommand)]
    pub action: CodeAction,
}

/// Code intelligence actions.
#[derive(Subcommand)]
pub enum CodeAction {
    /// Analyze code for complexity, code smells, and structure.
    ///
    /// Examples:
    ///   ember code analyze src/
    ///   ember code analyze --language rust src/main.rs
    ///   ember code analyze --format json --output report.json .
    #[command(
        about = "Analyze code for complexity, code smells, and structure.",
        after_help = "Examples:
  ember code analyze src/
  ember code analyze --language rust src/main.rs
  ember code analyze --format json --output report.json ."
    )]
    Analyze {
        /// Path to file or directory to analyze
        path: PathBuf,

        /// Language to use (auto-detected if not specified)
        #[arg(short, long)]
        language: Option<LanguageArg>,

        /// Output format
        #[arg(short, long, value_enum, default_value = "pretty")]
        format: OutputFormat,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Maximum cyclomatic complexity threshold
        #[arg(long, default_value = "10")]
        max_complexity: u32,

        /// Include code smells in output
        #[arg(long, default_value = "true")]
        smells: bool,

        /// Include symbols in output
        #[arg(long)]
        symbols: bool,

        /// Recursive analysis for directories
        #[arg(short, long, default_value = "true")]
        recursive: bool,
    },

    /// Generate refactoring suggestions for code.
    ///
    /// Examples:
    ///   ember code refactor src/main.rs
    ///   ember code refactor --min-confidence high src/
    ///   ember code refactor --kind extract-function src/lib.rs
    #[command(
        about = "Generate refactoring suggestions for code.",
        after_help = "Examples:
  ember code refactor src/main.rs
  ember code refactor --min-confidence high src/
  ember code refactor --kind extract-function src/lib.rs"
    )]
    Refactor {
        /// Path to file or directory to analyze
        path: PathBuf,

        /// Language to use (auto-detected if not specified)
        #[arg(short, long)]
        language: Option<LanguageArg>,

        /// Output format
        #[arg(short, long, value_enum, default_value = "pretty")]
        format: OutputFormat,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Minimum confidence level for suggestions
        #[arg(long, value_enum, default_value = "medium")]
        min_confidence: ConfidenceLevel,

        /// Filter by refactoring kind
        #[arg(long)]
        kind: Option<String>,

        /// Apply refactoring automatically (requires confirmation)
        #[arg(long)]
        apply: bool,

        /// Skip confirmation when applying
        #[arg(long)]
        yes: bool,
    },

    /// Generate tests for code.
    ///
    /// Examples:
    ///   ember code testgen src/lib.rs
    ///   ember code testgen --framework pytest src/utils.py
    ///   ember code testgen --type integration --output tests/ src/
    #[command(
        about = "Generate tests for code.",
        after_help = "Examples:
  ember code testgen src/lib.rs
  ember code testgen --framework pytest src/utils.py
  ember code testgen --type integration --output tests/ src/"
    )]
    Testgen {
        /// Path to file or directory to generate tests for
        path: PathBuf,

        /// Language to use (auto-detected if not specified)
        #[arg(short, long)]
        language: Option<LanguageArg>,

        /// Test framework to use
        #[arg(long)]
        framework: Option<String>,

        /// Test type to generate
        #[arg(short, long, value_enum, default_value = "unit")]
        r#type: TestType,

        /// Output directory for generated tests
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Include edge case tests
        #[arg(long, default_value = "true")]
        edge_cases: bool,

        /// Include error handling tests
        #[arg(long, default_value = "true")]
        error_tests: bool,

        /// Generate mocks for dependencies
        #[arg(long)]
        mocks: bool,

        /// Dry run (show tests without writing)
        #[arg(long)]
        dry_run: bool,
    },

    /// Show code statistics and metrics summary.
    ///
    /// Examples:
    ///   ember code stats .
    ///   ember code stats --by-language src/
    #[command(
        about = "Show code statistics and metrics summary.",
        after_help = "Examples:
  ember code stats .
  ember code stats --by-language src/"
    )]
    Stats {
        /// Path to analyze
        path: PathBuf,

        /// Group statistics by language
        #[arg(long)]
        by_language: bool,

        /// Include file-level details
        #[arg(long)]
        detailed: bool,
    },
}

/// Supported languages for code analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LanguageArg {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
}

/// Output format for reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable colored output
    #[default]
    Pretty,
    /// JSON format for tooling
    Json,
    /// Markdown format for documentation
    Markdown,
    /// CSV format for spreadsheets
    Csv,
}

/// Confidence level for refactoring suggestions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum ConfidenceLevel {
    /// Low confidence - might be wrong
    Low,
    /// Medium confidence
    #[default]
    Medium,
    /// High confidence - likely correct
    High,
    /// Very high confidence - almost certain
    VeryHigh,
}

/// Test type to generate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum TestType {
    /// Unit tests for individual functions
    #[default]
    Unit,
    /// Integration tests
    Integration,
    /// Property-based tests
    Property,
    /// Snapshot tests
    Snapshot,
    /// Benchmark tests
    Benchmark,
}

/// Execute the code command.
pub async fn execute(args: CodeArgs) -> Result<()> {
    match args.action {
        CodeAction::Analyze {
            path,
            language,
            format,
            output,
            max_complexity,
            smells,
            symbols,
            recursive,
        } => {
            analyze_code(
                path,
                language,
                format,
                output,
                max_complexity,
                smells,
                symbols,
                recursive,
            )
            .await
        }
        CodeAction::Refactor {
            path,
            language,
            format,
            output,
            min_confidence,
            kind,
            apply,
            yes,
        } => {
            suggest_refactoring(
                path,
                language,
                format,
                output,
                min_confidence,
                kind,
                apply,
                yes,
            )
            .await
        }
        CodeAction::Testgen {
            path,
            language,
            framework,
            r#type,
            output,
            edge_cases,
            error_tests,
            mocks,
            dry_run,
        } => {
            generate_tests(
                path,
                language,
                framework,
                r#type,
                output,
                edge_cases,
                error_tests,
                mocks,
                dry_run,
            )
            .await
        }
        CodeAction::Stats {
            path,
            by_language,
            detailed,
        } => show_stats(path, by_language, detailed).await,
    }
}

/// Analyze code for complexity and issues.
async fn analyze_code(
    path: PathBuf,
    _language: Option<LanguageArg>,
    format: OutputFormat,
    output: Option<PathBuf>,
    _max_complexity: u32,
    _show_smells: bool,
    _show_symbols: bool,
    _recursive: bool,
) -> Result<()> {
    println!(
        "{} {}",
        "Analyzing:".bright_blue(),
        path.display().to_string().cyan()
    );

    // Check if path exists
    if !path.exists() {
        anyhow::bail!("Path does not exist: {}", path.display());
    }

    // Collect files to analyze
    let files = collect_files(&path)?;
    println!("  Found {} files to analyze", files.len());

    // Line-count analysis — this is real (counts actual lines).
    // NOTE: Complexity, code smells, and symbol counts require AST analysis
    // (tree-sitter) which is not yet wired into the CLI.
    let mut total_loc = 0;

    for file in &files {
        let loc = estimate_loc(file)?;
        total_loc += loc;
    }

    // Output results
    match format {
        OutputFormat::Json => {
            let result = serde_json::json!({
                "path": path.display().to_string(),
                "files_analyzed": files.len(),
                "total_lines": total_loc,
                "note": "Complexity and code-smell analysis requires AST support (not yet integrated)."
            });

            if let Some(output_path) = output {
                std::fs::write(&output_path, serde_json::to_string_pretty(&result)?)?;
                println!("Results written to {}", output_path.display());
            } else {
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        }
        OutputFormat::Pretty => {
            println!();
            println!("{}", "Analysis Summary".bright_yellow().bold());
            println!("{}", "=".repeat(50));
            println!(
                "  {} {}",
                "Files analyzed:".bright_blue(),
                files.len().to_string().white()
            );
            println!(
                "  {} {}",
                "Total lines:".bright_blue(),
                total_loc.to_string().white()
            );

            println!();
            println!(
                "  {} {}",
                "[info]".yellow(),
                "Complexity, code-smell, and symbol analysis require AST support (tree-sitter).".dimmed()
            );
            println!(
                "  {} {}",
                "".dimmed(),
                "The ember-code crate has the engine — CLI integration coming soon.".dimmed()
            );
        }
        OutputFormat::Markdown => {
            let mut md = String::new();
            md.push_str("# Code Analysis Report\n\n");
            md.push_str(&format!("**Path:** `{}`\n\n", path.display()));
            md.push_str("## Summary\n\n");
            md.push_str("| Metric | Value |\n");
            md.push_str("|--------|-------|\n");
            md.push_str(&format!("| Files analyzed | {} |\n", files.len()));
            md.push_str(&format!("| Total lines | {} |\n", total_loc));
            md.push_str("\n> **Note:** Complexity and code-smell analysis require AST support (not yet integrated).\n");

            if let Some(output_path) = output {
                std::fs::write(&output_path, &md)?;
                println!("Report written to {}", output_path.display());
            } else {
                println!("{}", md);
            }
        }
        OutputFormat::Csv => {
            let mut csv = String::new();
            csv.push_str("file,lines\n");
            for file in &files {
                let loc = estimate_loc(file).unwrap_or(0);
                csv.push_str(&format!(
                    "{},{}\n",
                    file.display(),
                    loc,
                ));
            }

            if let Some(output_path) = output {
                std::fs::write(&output_path, &csv)?;
                println!("CSV written to {}", output_path.display());
            } else {
                println!("{}", csv);
            }
        }
    }

    Ok(())
}

/// Suggest refactoring improvements.
async fn suggest_refactoring(
    path: PathBuf,
    _language: Option<LanguageArg>,
    format: OutputFormat,
    output: Option<PathBuf>,
    _min_confidence: ConfidenceLevel,
    _kind: Option<String>,
    apply: bool,
    _yes: bool,
) -> Result<()> {
    println!(
        "{} {}",
        "Analyzing for refactoring:".bright_blue(),
        path.display().to_string().cyan()
    );

    if !path.exists() {
        anyhow::bail!("Path does not exist: {}", path.display());
    }

    // Refactoring suggestions are not yet implemented — would require
    // AST analysis via tree-sitter (available in ember-code crate).
    println!(
        "{} Refactoring analysis for {}:",
        "[preview]".bright_yellow(),
        path.display().to_string().bright_white()
    );
    println!(
        "  {}",
        "Refactoring suggestions require AST analysis which is not yet wired into the CLI.".dimmed()
    );
    println!(
        "  {}",
        "The ember-code crate has a real RefactoringEngine — integration coming soon.".dimmed()
    );

    let filtered: Vec<RefactoringSuggestion> = vec![];

    match format {
        OutputFormat::Json => {
            let result = serde_json::json!({
                "path": path.display().to_string(),
                "suggestions": filtered.iter().map(|s| serde_json::json!({
                    "file": s.file.display().to_string(),
                    "line": s.line,
                    "kind": s.kind,
                    "title": s.title,
                    "description": s.description,
                    "confidence": s.confidence,
                    "impact": s.impact
                })).collect::<Vec<_>>()
            });

            if let Some(output_path) = output {
                std::fs::write(&output_path, serde_json::to_string_pretty(&result)?)?;
                println!("Results written to {}", output_path.display());
            } else {
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        }
        OutputFormat::Pretty => {
            println!();
            println!(
                "{} {}",
                "Found".bright_green(),
                format!("{} refactoring suggestions", filtered.len()).white()
            );
            println!("{}", "=".repeat(60));

            for (i, suggestion) in filtered.iter().enumerate() {
                println!();
                println!(
                    "{} {} {}",
                    format!("[{}]", i + 1).bright_cyan(),
                    suggestion.title.bright_yellow().bold(),
                    format!("({})", suggestion.kind).dimmed()
                );
                println!(
                    "    {} {}:{}",
                    "Location:".bright_blue(),
                    suggestion.file.display().to_string().cyan(),
                    suggestion.line.to_string().white()
                );
                println!(
                    "    {} {}",
                    "Confidence:".bright_blue(),
                    format_confidence(&suggestion.confidence)
                );
                println!(
                    "    {} {}",
                    "Impact:".bright_blue(),
                    suggestion.impact.white()
                );
                println!("    {}", suggestion.description.dimmed());
            }

            if apply {
                println!();
                println!(
                    "{} No refactoring suggestions available to apply.",
                    "[info]".yellow()
                );
            }
        }
        _ => {
            println!("Format {:?} not yet implemented for refactoring", format);
        }
    }

    Ok(())
}

/// Generate tests for code.
async fn generate_tests(
    path: PathBuf,
    _language: Option<LanguageArg>,
    framework: Option<String>,
    test_type: TestType,
    _output: Option<PathBuf>,
    _include_edge_cases: bool,
    _include_error_tests: bool,
    _mocks: bool,
    _dry_run: bool,
) -> Result<()> {
    println!(
        "{} {}",
        "Generating tests for:".bright_blue(),
        path.display().to_string().cyan()
    );

    if !path.exists() {
        anyhow::bail!("Path does not exist: {}", path.display());
    }

    let framework_name = framework.unwrap_or_else(|| detect_framework(&path));
    println!("  Using framework: {}", framework_name.bright_green());
    println!("  Test type: {:?}", test_type);

    // Test generation requires understanding the actual code (AST analysis or LLM).
    // Generating generic template tests would be dishonest — they don't actually
    // test the target code.
    println!();
    println!(
        "{} Test generation requires AST analysis or LLM integration to produce meaningful tests.",
        "[not implemented]".bright_yellow()
    );
    println!(
        "  {}",
        "Generic template tests that don't reference your actual code would be misleading.".dimmed()
    );
    println!(
        "  {}",
        "The ember-code crate has the foundation — CLI integration coming soon.".dimmed()
    );

    Ok(())
}

/// Show code statistics.
async fn show_stats(path: PathBuf, by_language: bool, detailed: bool) -> Result<()> {
    println!(
        "{} {}",
        "Calculating statistics for:".bright_blue(),
        path.display().to_string().cyan()
    );

    if !path.exists() {
        anyhow::bail!("Path does not exist: {}", path.display());
    }

    let files = collect_files(&path)?;

    let mut stats_by_lang: std::collections::HashMap<String, LangStats> =
        std::collections::HashMap::new();

    for file in &files {
        let ext = file
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown");
        let lang = ext_to_lang(ext);
        let loc = estimate_loc(file).unwrap_or(0);

        let entry = stats_by_lang.entry(lang.to_string()).or_insert(LangStats {
            files: 0,
            lines: 0,
        });
        entry.files += 1;
        entry.lines += loc;
    }

    let total_files: usize = stats_by_lang.values().map(|s| s.files).sum();
    let total_lines: usize = stats_by_lang.values().map(|s| s.lines).sum();

    println!();
    println!("{}", "Code Statistics".bright_yellow().bold());
    println!("{}", "=".repeat(60));

    if by_language {
        println!();
        println!(
            "{:<15} {:>8} {:>10}",
            "Language".bright_blue(),
            "Files".bright_blue(),
            "Lines".bright_blue(),
        );
        println!("{}", "-".repeat(40));

        let mut sorted: Vec<_> = stats_by_lang.iter().collect();
        sorted.sort_by(|a, b| b.1.lines.cmp(&a.1.lines));

        for (lang, stats) in sorted {
            println!(
                "{:<15} {:>8} {:>10}",
                lang.cyan(),
                stats.files.to_string().white(),
                stats.lines.to_string().white(),
            );
        }
        println!("{}", "-".repeat(40));
    }

    println!(
        "{:<15} {:>8} {:>10}",
        "Total".bright_green().bold(),
        total_files.to_string().white().bold(),
        total_lines.to_string().white().bold(),
    );

    if detailed {
        println!();
        println!("{}", "File Details".bright_blue().bold());
        println!("{}", "-".repeat(60));
        for file in files.iter().take(20) {
            let loc = estimate_loc(file).unwrap_or(0);
            println!("  {} {}", loc.to_string().dimmed(), file.display());
        }
        if files.len() > 20 {
            println!("  ... and {} more files", files.len() - 20);
        }
    }

    Ok(())
}

// Helper types and functions

struct RefactoringSuggestion {
    file: PathBuf,
    line: u32,
    kind: String,
    title: String,
    description: String,
    confidence: String,
    impact: String,
}

struct LangStats {
    files: usize,
    lines: usize,
}

fn collect_files(path: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    if path.is_file() {
        files.push(path.clone());
    } else if path.is_dir() {
        for entry in walkdir::WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let p = entry.path();
            if p.is_file() && is_code_file(p) {
                files.push(p.to_path_buf());
            }
        }
    }

    Ok(files)
}

fn is_code_file(path: &std::path::Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(
        ext,
        "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "go" | "java" | "c" | "cpp" | "h" | "hpp"
    )
}

fn estimate_loc(path: &PathBuf) -> Result<usize> {
    let content = std::fs::read_to_string(path)?;
    Ok(content.lines().count())
}

fn format_confidence(confidence: &str) -> String {
    match confidence {
        "VeryHigh" => "Very High".bright_green().to_string(),
        "High" => "High".green().to_string(),
        "Medium" => "Medium".yellow().to_string(),
        "Low" => "Low".red().to_string(),
        _ => confidence.to_string(),
    }
}

fn detect_framework(path: &Path) -> String {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "rs" => "rust-test".to_string(),
        "py" => "pytest".to_string(),
        "js" | "jsx" => "jest".to_string(),
        "ts" | "tsx" => "vitest".to_string(),
        "go" => "go-test".to_string(),
        "java" => "junit".to_string(),
        _ => "generic".to_string(),
    }
}

fn ext_to_lang(ext: &str) -> &str {
    match ext {
        "rs" => "Rust",
        "py" => "Python",
        "js" | "jsx" => "JavaScript",
        "ts" | "tsx" => "TypeScript",
        "go" => "Go",
        "java" => "Java",
        "c" | "h" => "C",
        "cpp" | "hpp" => "C++",
        _ => "Other",
    }
}

