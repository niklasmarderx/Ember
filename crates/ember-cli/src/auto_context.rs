//! Smart auto-context: gathers project context with a token budget.
//!
//! Discovers and injects relevant project files (manifests, README, rules,
//! recent git activity) into the system prompt so the LLM has awareness of
//! the codebase without the user having to `/add` files manually.

use std::path::Path;
use tracing::debug;

/// A single piece of context to inject.
pub struct ContextPart {
    pub label: String,
    pub content: String,
    pub priority: u8,
    token_estimate: usize,
}

/// Collected auto-context ready for injection.
pub struct AutoContext {
    pub parts: Vec<ContextPart>,
    #[allow(dead_code)]
    pub total_tokens: usize,
}

impl AutoContext {
    /// Format all parts as a single string for system prompt injection.
    pub fn to_prompt_section(&self) -> String {
        let mut out = String::new();
        for part in &self.parts {
            out.push_str(&format!(
                "\n## Project Context [{}]\n{}\n",
                part.label, part.content
            ));
        }
        out
    }

    /// Labels for display in the startup banner.
    pub fn labels(&self) -> Vec<&str> {
        self.parts.iter().map(|p| p.label.as_str()).collect()
    }
}

/// Builder that gathers context parts and trims to a token budget.
pub struct AutoContextBuilder {
    budget: usize,
    parts: Vec<ContextPart>,
}

impl AutoContextBuilder {
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            parts: Vec::new(),
        }
    }

    fn estimate_tokens(s: &str) -> usize {
        (s.len() + 3) / 4
    }

    fn add(&mut self, label: &str, content: String, priority: u8) {
        if content.trim().is_empty() {
            return;
        }
        let token_estimate = Self::estimate_tokens(&content);
        self.parts.push(ContextPart {
            label: label.to_string(),
            content,
            priority,
            token_estimate,
        });
    }

    /// Gather EMBER.md from the project root (highest priority).
    pub fn gather_ember_md(mut self) -> Self {
        if let Ok(content) = std::fs::read_to_string("EMBER.md") {
            self.add("EMBER.md", content, 0);
        }
        self
    }

    /// Gather .ember/rules/*.md files.
    pub fn gather_rules(mut self) -> Self {
        let rules_dir = Path::new(".ember/rules");
        if rules_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(rules_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "md").unwrap_or(false) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let name = path.file_name().unwrap_or_default().to_string_lossy();
                            self.add(&format!("rules/{}", name), content, 0);
                        }
                    }
                }
            }
        }
        self
    }

    /// Gather project manifest (Cargo.toml, package.json, pyproject.toml).
    pub fn gather_manifest(mut self) -> Self {
        for (file, label) in &[
            ("Cargo.toml", "Cargo.toml"),
            ("package.json", "package.json"),
            ("pyproject.toml", "pyproject.toml"),
            ("go.mod", "go.mod"),
        ] {
            if let Ok(content) = std::fs::read_to_string(file) {
                // Trim to first 100 lines for large manifests
                let trimmed: String = content.lines().take(100).collect::<Vec<_>>().join("\n");
                self.add(label, trimmed, 1);
                break; // Only include the primary manifest
            }
        }
        self
    }

    /// Gather README.md (first 150 lines).
    pub fn gather_readme(mut self) -> Self {
        if let Ok(content) = std::fs::read_to_string("README.md") {
            let trimmed: String = content.lines().take(150).collect::<Vec<_>>().join("\n");
            self.add("README.md", trimmed, 1);
        }
        self
    }

    /// Gather recent git activity.
    pub fn gather_git_context(mut self) -> Self {
        // Recent commits
        if let Ok(output) = std::process::Command::new("git")
            .args(["log", "--oneline", "-10"])
            .output()
        {
            if output.status.success() {
                let log = String::from_utf8_lossy(&output.stdout).to_string();
                if !log.trim().is_empty() {
                    self.add("git:recent", log, 2);
                }
            }
        }

        // Changed files
        if let Ok(output) = std::process::Command::new("git")
            .args(["diff", "--stat", "--no-color"])
            .output()
        {
            if output.status.success() {
                let diff = String::from_utf8_lossy(&output.stdout).to_string();
                if !diff.trim().is_empty() {
                    self.add("git:changes", diff, 2);
                }
            }
        }
        self
    }

    /// Gather top-level directory structure (depth 2).
    pub fn gather_directory_tree(mut self) -> Self {
        let mut tree = String::new();
        if let Ok(entries) = std::fs::read_dir(".") {
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries.iter().take(40) {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                // Skip hidden dirs except .ember
                if name_str.starts_with('.') && name_str != ".ember" {
                    continue;
                }
                if name_str == "target" || name_str == "node_modules" || name_str == ".git" {
                    continue;
                }
                let ft = entry
                    .file_type()
                    .map(|t| if t.is_dir() { "/" } else { "" })
                    .unwrap_or("");
                tree.push_str(&format!("{}{}\n", name_str, ft));
            }
        }
        if !tree.is_empty() {
            self.add("tree", tree, 3);
        }
        self
    }

    /// Build the final context, trimmed to budget.
    pub fn build(mut self) -> AutoContext {
        self.parts.sort_by_key(|p| p.priority);
        let mut included = Vec::new();
        let mut total = 0usize;

        for part in self.parts {
            if self.budget > 0 && total + part.token_estimate > self.budget {
                debug!(
                    "Auto-context budget exceeded, skipping '{}' ({} tokens)",
                    part.label, part.token_estimate
                );
                continue;
            }
            total += part.token_estimate;
            included.push(part);
        }

        debug!("Auto-context: {} parts, ~{} tokens", included.len(), total);
        AutoContext {
            parts: included,
            total_tokens: total,
        }
    }
}
