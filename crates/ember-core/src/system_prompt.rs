//! System prompt engineering for Ember.
//!
//! Builds rich, context-aware system prompts that turn the LLM into a
//! powerful coding assistant — similar to what makes Claude Code / Cline
//! effective. The prompt encodes:
//!
//! - Role identity and behavioural rules
//! - Available tools with usage instructions
//! - Project context (language, framework, conventions)
//! - File-editing protocol (SEARCH/REPLACE)
//! - Safety tiers and approval rules
//!
//! # Example
//!
//! ```rust,no_run
//! use ember_core::system_prompt::{SystemPromptBuilder, ProjectKind};
//!
//! let prompt = SystemPromptBuilder::new()
//!     .project_kind(ProjectKind::Rust)
//!     .cwd("/home/user/my-project")
//!     .tool_names(&["shell", "file_edit", "file_read", "git", "web_fetch"])
//!     .auto_approve(false)
//!     .build();
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

// ---------------------------------------------------------------------------
// Project kind detection
// ---------------------------------------------------------------------------

/// Detected project language/framework.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectKind {
    /// Rust project (Cargo.toml detected)
    Rust,
    /// JavaScript project (package.json detected)
    JavaScript,
    /// TypeScript project (tsconfig.json detected)
    TypeScript,
    /// Python project (pyproject.toml / setup.py / requirements.txt)
    Python,
    /// Go project (go.mod detected)
    Go,
    /// Java project (pom.xml / build.gradle)
    Java,
    /// C# project (.csproj detected)
    CSharp,
    /// Ruby project (Gemfile detected)
    Ruby,
    /// Elixir project (mix.exs detected)
    Elixir,
    /// Swift project (Package.swift detected)
    Swift,
    /// Kotlin project (build.gradle.kts detected)
    Kotlin,
    /// C++ project (CMakeLists.txt detected)
    Cpp,
    /// No recognisable project markers found
    Unknown,
}

impl fmt::Display for ProjectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rust => write!(f, "Rust"),
            Self::JavaScript => write!(f, "JavaScript"),
            Self::TypeScript => write!(f, "TypeScript"),
            Self::Python => write!(f, "Python"),
            Self::Go => write!(f, "Go"),
            Self::Java => write!(f, "Java"),
            Self::CSharp => write!(f, "C#"),
            Self::Ruby => write!(f, "Ruby"),
            Self::Elixir => write!(f, "Elixir"),
            Self::Swift => write!(f, "Swift"),
            Self::Kotlin => write!(f, "Kotlin"),
            Self::Cpp => write!(f, "C++"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Detect the primary project language from filesystem markers.
pub fn detect_project_kind(root: &Path) -> ProjectKind {
    let markers: &[(&str, ProjectKind)] = &[
        ("Cargo.toml", ProjectKind::Rust),
        ("tsconfig.json", ProjectKind::TypeScript),
        ("package.json", ProjectKind::JavaScript),
        ("pyproject.toml", ProjectKind::Python),
        ("setup.py", ProjectKind::Python),
        ("requirements.txt", ProjectKind::Python),
        ("go.mod", ProjectKind::Go),
        ("pom.xml", ProjectKind::Java),
        ("build.gradle", ProjectKind::Java),
        ("build.gradle.kts", ProjectKind::Kotlin),
        ("*.csproj", ProjectKind::CSharp),
        ("Gemfile", ProjectKind::Ruby),
        ("mix.exs", ProjectKind::Elixir),
        ("Package.swift", ProjectKind::Swift),
        ("CMakeLists.txt", ProjectKind::Cpp),
    ];

    for (marker, kind) in markers {
        if marker.starts_with('*') {
            // Glob — check if any file matches
            let ext = &marker[1..];
            if let Ok(entries) = std::fs::read_dir(root) {
                for entry in entries.flatten() {
                    if entry.file_name().to_string_lossy().ends_with(ext) {
                        return *kind;
                    }
                }
            }
        } else if root.join(marker).exists() {
            return *kind;
        }
    }

    ProjectKind::Unknown
}

// ---------------------------------------------------------------------------
// Risk tier
// ---------------------------------------------------------------------------

/// Risk classification for tool operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskTier {
    /// Read-only, no side effects (file_read, glob, web_fetch)
    Safe,
    /// May modify the project (file_edit, file_write, git commit)
    Moderate,
    /// System-level side effects (shell, package install, network ops)
    Dangerous,
}

impl fmt::Display for RiskTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Safe => write!(f, "safe"),
            Self::Moderate => write!(f, "moderate"),
            Self::Dangerous => write!(f, "dangerous"),
        }
    }
}

/// Classify a tool name into its risk tier.
pub fn classify_tool_risk(tool_name: &str) -> RiskTier {
    match tool_name {
        "file_read" | "glob" | "grep" | "web_fetch" | "list_files"
        | "list_code_definitions" | "search_files" => RiskTier::Safe,

        "file_edit" | "file_write" | "file_create" | "git" => RiskTier::Moderate,

        "shell" | "browser" | "install" => RiskTier::Dangerous,

        _ => RiskTier::Moderate,
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Builds the system prompt that turns the LLM into a powerful coding agent.
///
/// The builder assembles multiple sections (identity, tools, safety rules,
/// coding conventions, environment info) into a single coherent system prompt.
pub struct SystemPromptBuilder {
    project_kind: ProjectKind,
    cwd: String,
    os: String,
    shell: String,
    tool_names: Vec<String>,
    auto_approve: bool,
    extra_rules: Vec<String>,
    extra_context: Vec<(String, String)>,
    user_name: Option<String>,
}

impl SystemPromptBuilder {
    /// Create a new builder with sensible defaults.
    pub fn new() -> Self {
        Self {
            project_kind: ProjectKind::Unknown,
            cwd: String::from("."),
            os: detect_os(),
            shell: detect_shell(),
            tool_names: Vec::new(),
            auto_approve: false,
            extra_rules: Vec::new(),
            extra_context: Vec::new(),
            user_name: None,
        }
    }

    /// Set the detected project language/framework.
    pub fn project_kind(mut self, kind: ProjectKind) -> Self {
        self.project_kind = kind;
        self
    }

    /// Set the current working directory shown to the LLM.
    pub fn cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = cwd.into();
        self
    }

    /// Override the detected operating system name.
    pub fn os(mut self, os: impl Into<String>) -> Self {
        self.os = os.into();
        self
    }

    /// Override the detected shell.
    pub fn shell(mut self, shell: impl Into<String>) -> Self {
        self.shell = shell.into();
        self
    }

    /// Set the list of available tool names. This determines which tool
    /// descriptions and protocols (e.g. file-edit) appear in the prompt.
    pub fn tool_names(mut self, names: &[&str]) -> Self {
        self.tool_names = names.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Set whether auto-approve is enabled.
    pub fn auto_approve(mut self, enabled: bool) -> Self {
        self.auto_approve = enabled;
        self
    }

    /// Add a custom rule to the prompt.
    pub fn add_rule(mut self, rule: impl Into<String>) -> Self {
        self.extra_rules.push(rule.into());
        self
    }

    /// Add a labelled context section (e.g. from EMBER.md or auto-context).
    pub fn add_context(mut self, label: impl Into<String>, content: impl Into<String>) -> Self {
        self.extra_context.push((label.into(), content.into()));
        self
    }

    /// Set the user's name for personalised greetings.
    pub fn user_name(mut self, name: impl Into<String>) -> Self {
        self.user_name = Some(name.into());
        self
    }

    /// Build the full system prompt string.
    pub fn build(&self) -> String {
        let mut sections: Vec<String> = Vec::new();

        sections.push(self.build_identity());

        if !self.tool_names.is_empty() {
            sections.push(self.build_tool_section());
        }

        if self.tool_names.iter().any(|t| t == "file_edit") {
            sections.push(self.build_file_edit_protocol());
        }

        sections.push(self.build_safety_rules());
        sections.push(self.build_coding_rules());
        sections.push(self.build_environment());

        for (label, content) in &self.extra_context {
            sections.push(format!("# {}\n\n{}", label, content));
        }

        if !self.extra_rules.is_empty() {
            let rules = self
                .extra_rules
                .iter()
                .map(|r| format!("- {}", r))
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!("# ADDITIONAL RULES\n\n{}", rules));
        }

        sections.join("\n\n====\n\n")
    }

    // ── Private section builders ──

    fn build_identity(&self) -> String {
        let greeting = if let Some(ref name) = self.user_name {
            format!("You are Ember, {}'s expert AI coding assistant.", name)
        } else {
            "You are Ember, an expert AI coding assistant.".to_string()
        };

        format!(
            r#"# IDENTITY

{}

You are a highly skilled software engineer with deep knowledge of many programming languages, frameworks, design patterns, and best practices. You help the user accomplish coding tasks by:

1. **Understanding** the codebase and the user's intent
2. **Planning** the approach before writing code
3. **Implementing** changes precisely using the available tools
4. **Verifying** your changes compile/work correctly

You are direct, technical, and efficient. You do NOT start messages with "Great", "Certainly", "Sure", or "Of course". You proceed step-by-step, confirming each action before moving on."#,
            greeting
        )
    }

    fn build_tool_section(&self) -> String {
        let mut tools_desc = String::from("# AVAILABLE TOOLS\n\n");
        tools_desc.push_str(
            "You have access to the following tools. Use one tool per message, wait for the result, then proceed.\n\n",
        );

        for name in &self.tool_names {
            let desc = tool_description(name);
            let risk = classify_tool_risk(name);
            tools_desc.push_str(&format!(
                "## `{}`  [risk: {}]\n{}\n\n",
                name, risk, desc
            ));
        }

        tools_desc
    }

    fn build_file_edit_protocol(&self) -> String {
        r#"# FILE EDITING PROTOCOL

When modifying existing files, prefer precise SEARCH/REPLACE edits over rewriting entire files.

Use the `file_edit` tool with SEARCH/REPLACE blocks:

```
------- SEARCH
[exact content to find, including whitespace]
=======
[replacement content]
+++++++ REPLACE
```

Rules:
1. SEARCH content must match the file EXACTLY — character for character
2. Each SEARCH/REPLACE block replaces only the FIRST match
3. Use multiple blocks (in file order) for multiple changes
4. Keep blocks minimal — only include lines that change plus enough context for uniqueness
5. To delete code: use an empty REPLACE section
6. To move code: one block to delete + one to insert at new location

When changes are extensive (>60% of file), rewrite the whole file instead."#
            .to_string()
    }

    fn build_safety_rules(&self) -> String {
        let mut s = String::from("# SAFETY & APPROVAL\n\n");

        if self.auto_approve {
            s.push_str("Auto-approve is ENABLED. Safe and moderate operations will execute without confirmation.\n");
            s.push_str("Dangerous operations (shell commands with side effects, package installs, network ops) still require user approval.\n\n");
        } else {
            s.push_str("Auto-approve is DISABLED. Every tool operation requires user confirmation before execution.\n\n");
        }

        s.push_str("Risk tiers:\n");
        s.push_str("- **Safe**: reading files, searching, listing — no side effects\n");
        s.push_str("- **Moderate**: editing files, writing files, git operations — project modifications\n");
        s.push_str("- **Dangerous**: shell commands, package installs, network operations — system-level effects\n");

        s
    }

    fn build_coding_rules(&self) -> String {
        let mut rules = vec![
            "Work through goals sequentially. Complete and verify each step before moving to the next.",
            "Before editing, read the relevant files to understand existing code structure.",
            "Make targeted, minimal changes. Do not refactor unrelated code.",
            "After making changes, verify they are correct (e.g., run tests, check for compile errors).",
            "When fixing bugs, run the existing test suite — do not modify test assertions to match broken behavior.",
            "Use Markdown formatting only where semantically appropriate (code blocks, lists, tables).",
            "When executing commands, check for errors in the output before proceeding.",
            "Do not ask unnecessary questions — use tools to gather information proactively.",
        ];

        match self.project_kind {
            ProjectKind::Rust => {
                rules.push("Follow Rust idioms: use `Result` for error handling, prefer `&str` over `String` in function params where appropriate.");
                rules.push("Run `cargo check` or `cargo build` after making changes to catch compile errors early.");
                rules.push("Use `cargo clippy` conventions. Prefer `thiserror` for library errors, `anyhow` for applications.");
            }
            ProjectKind::TypeScript | ProjectKind::JavaScript => {
                rules.push("Use modern ES6+ syntax. Prefer `const` over `let` where possible.");
                rules.push("Follow the project's existing style (tabs vs spaces, semicolons, quotes).");
                rules.push("Run the linter/formatter after changes if one is configured.");
            }
            ProjectKind::Python => {
                rules.push("Follow PEP 8 style. Use type hints where the project uses them.");
                rules.push("Prefer f-strings for string formatting.");
                rules.push("Run `python -m py_compile <file>` to check syntax after edits.");
            }
            ProjectKind::Go => {
                rules.push("Follow Go conventions: `gofmt` style, short variable names, error checking.");
                rules.push("Run `go vet` after changes.");
            }
            _ => {}
        }

        let body = rules
            .iter()
            .map(|r| format!("- {}", r))
            .collect::<Vec<_>>()
            .join("\n");

        format!("# CODING RULES\n\n{}", body)
    }

    fn build_environment(&self) -> String {
        format!(
            "# ENVIRONMENT\n\n- **OS**: {}\n- **Shell**: {}\n- **CWD**: {}\n- **Project**: {}",
            self.os, self.shell, self.cwd, self.project_kind
        )
    }
}

impl Default for SystemPromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tool descriptions
// ---------------------------------------------------------------------------

fn tool_description(name: &str) -> &'static str {
    match name {
        "shell" => "Execute a shell command. Provide the command string. Prefer non-interactive commands with flags like `--yes`, `--no-pager`. Each command runs in a new shell instance.",
        "file_read" => "Read the contents of a file at a given path. Returns line-numbered content. Use start_line/end_line for large files.",
        "file_edit" => "Make targeted edits to a file using SEARCH/REPLACE blocks. See FILE EDITING PROTOCOL.",
        "file_write" => "Write complete content to a file (creates or overwrites). Use for new files or full rewrites.",
        "file_create" => "Create a new file with content. Automatically creates parent directories.",
        "git" => "Execute git operations: status, diff, commit, branch, log, etc.",
        "web_fetch" => "Fetch content from a URL. Returns the text body.",
        "browser" => "Control a headless browser for web interaction, screenshots, and testing.",
        "glob" | "list_files" => "List files matching a pattern or in a directory.",
        "grep" | "search_files" => "Search files using regex patterns. Returns matches with surrounding context.",
        "list_code_definitions" => "List function/class/method definitions at the top level of a directory.",
        _ => "A registered tool. Refer to its schema for usage.",
    }
}

// ---------------------------------------------------------------------------
// OS / shell detection helpers
// ---------------------------------------------------------------------------

fn detect_os() -> String {
    #[cfg(target_os = "macos")]
    {
        "macOS".to_string()
    }
    #[cfg(target_os = "linux")]
    {
        "Linux".to_string()
    }
    #[cfg(target_os = "windows")]
    {
        "Windows".to_string()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        "Unknown".to_string()
    }
}

fn detect_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_kind_display() {
        assert_eq!(ProjectKind::Rust.to_string(), "Rust");
        assert_eq!(ProjectKind::TypeScript.to_string(), "TypeScript");
        assert_eq!(ProjectKind::Unknown.to_string(), "Unknown");
    }

    #[test]
    fn test_classify_tool_risk() {
        assert_eq!(classify_tool_risk("file_read"), RiskTier::Safe);
        assert_eq!(classify_tool_risk("file_edit"), RiskTier::Moderate);
        assert_eq!(classify_tool_risk("shell"), RiskTier::Dangerous);
        assert_eq!(classify_tool_risk("unknown_tool"), RiskTier::Moderate);
    }

    #[test]
    fn test_builder_basic() {
        let prompt = SystemPromptBuilder::new()
            .project_kind(ProjectKind::Rust)
            .cwd("/tmp/test")
            .build();

        assert!(prompt.contains("Ember"));
        assert!(prompt.contains("Rust"));
        assert!(prompt.contains("/tmp/test"));
    }

    #[test]
    fn test_builder_with_tools() {
        let prompt = SystemPromptBuilder::new()
            .tool_names(&["shell", "file_edit", "file_read"])
            .build();

        assert!(prompt.contains("shell"));
        assert!(prompt.contains("file_edit"));
        assert!(prompt.contains("FILE EDITING PROTOCOL"));
    }

    #[test]
    fn test_builder_no_file_edit_protocol_without_tool() {
        let prompt = SystemPromptBuilder::new()
            .tool_names(&["shell", "file_read"])
            .build();

        assert!(!prompt.contains("FILE EDITING PROTOCOL"));
    }

    #[test]
    fn test_builder_auto_approve() {
        let prompt = SystemPromptBuilder::new().auto_approve(true).build();
        assert!(prompt.contains("Auto-approve is ENABLED"));

        let prompt = SystemPromptBuilder::new().auto_approve(false).build();
        assert!(prompt.contains("Auto-approve is DISABLED"));
    }

    #[test]
    fn test_builder_user_name() {
        let prompt = SystemPromptBuilder::new().user_name("Alice").build();
        assert!(prompt.contains("Alice's expert AI coding assistant"));
    }

    #[test]
    fn test_builder_extra_rules() {
        let prompt = SystemPromptBuilder::new()
            .add_rule("Always write tests")
            .add_rule("Use snake_case")
            .build();
        assert!(prompt.contains("Always write tests"));
        assert!(prompt.contains("Use snake_case"));
    }

    #[test]
    fn test_detect_project_kind_cargo() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(detect_project_kind(dir.path()), ProjectKind::Rust);
    }

    #[test]
    fn test_detect_project_kind_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert_eq!(detect_project_kind(dir.path()), ProjectKind::JavaScript);
    }

    #[test]
    fn test_detect_project_kind_tsconfig() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();
        assert_eq!(detect_project_kind(dir.path()), ProjectKind::TypeScript);
    }

    #[test]
    fn test_detect_project_kind_unknown() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(detect_project_kind(dir.path()), ProjectKind::Unknown);
    }

    #[test]
    fn test_risk_tier_display() {
        assert_eq!(RiskTier::Safe.to_string(), "safe");
        assert_eq!(RiskTier::Moderate.to_string(), "moderate");
        assert_eq!(RiskTier::Dangerous.to_string(), "dangerous");
    }
}