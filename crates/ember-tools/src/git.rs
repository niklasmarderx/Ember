//! Git operations tool for repository management.
//!
//! This module provides a comprehensive Git tool for:
//! - Repository status and information
//! - Staging and committing changes
//! - Branch management
//! - Viewing diffs and logs
//! - File operations (add, restore, etc.)

use crate::{registry::ToolOutput, Error, Result, ToolDefinition, ToolHandler};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tracing::debug;

/// Configuration for the Git tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfig {
    /// Default repository path (if not specified in each call)
    pub default_repo: Option<PathBuf>,

    /// Maximum output size in bytes
    pub max_output_bytes: usize,

    /// Timeout for git operations in seconds
    pub timeout_secs: u64,

    /// Whether to allow destructive operations (reset --hard, etc.)
    pub allow_destructive: bool,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            default_repo: None,
            max_output_bytes: 1024 * 1024, // 1MB
            timeout_secs: 60,
            allow_destructive: false,
        }
    }
}

/// Git operations tool.
pub struct GitTool {
    config: GitConfig,
    enabled: bool,
}

impl GitTool {
    /// Create a new Git tool with default configuration.
    pub fn new() -> Self {
        Self {
            config: GitConfig::default(),
            enabled: true,
        }
    }

    /// Create a Git tool with custom configuration.
    pub fn with_config(config: GitConfig) -> Self {
        Self {
            config,
            enabled: true,
        }
    }

    /// Set the default repository path.
    pub fn default_repo(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.default_repo = Some(path.into());
        self
    }

    /// Allow destructive operations.
    pub fn allow_destructive(mut self) -> Self {
        self.config.allow_destructive = true;
        self
    }

    /// Enable or disable the tool.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Run a git command and return the output.
    async fn run_git(&self, args: &[&str], repo_path: Option<&Path>) -> Result<GitOutput> {
        let repo = repo_path
            .map(PathBuf::from)
            .or_else(|| self.config.default_repo.clone())
            .unwrap_or_else(|| PathBuf::from("."));

        debug!(
            repo = %repo.display(),
            args = ?args,
            "Running git command"
        );

        let mut cmd = Command::new("git");
        cmd.args(args)
            .current_dir(&repo)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(self.config.timeout_secs),
            cmd.output(),
        )
        .await
        .map_err(|_| Error::execution_failed("git", "Operation timed out"))?
        .map_err(|e| Error::execution_failed("git", format!("Failed to execute git: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        Ok(GitOutput {
            stdout,
            stderr,
            exit_code,
            success: output.status.success(),
        })
    }

    /// Get repository status.
    pub async fn status(&self, repo_path: Option<&Path>) -> Result<String> {
        let output = self
            .run_git(&["status", "--porcelain=v2", "--branch"], repo_path)
            .await?;

        if !output.success {
            return Err(Error::execution_failed("git", output.stderr));
        }

        // Parse and format the status output
        let mut result = String::new();
        let mut branch_info = String::new();
        let mut staged = Vec::new();
        let mut modified = Vec::new();
        let mut untracked = Vec::new();

        for line in output.stdout.lines() {
            if line.starts_with("# branch.head ") {
                branch_info = format!("On branch: {}", &line[14..]);
            } else if let Some(upstream) = line.strip_prefix("# branch.upstream ") {
                branch_info.push_str(&format!(" (tracking: {})", upstream));
            } else if line.starts_with("# branch.ab ") {
                let parts: Vec<&str> = line[12..].split_whitespace().collect();
                if parts.len() >= 2 {
                    let ahead = parts[0].trim_start_matches('+');
                    let behind = parts[1].trim_start_matches('-');
                    if ahead != "0" || behind != "0" {
                        branch_info.push_str(&format!(" [ahead {}, behind {}]", ahead, behind));
                    }
                }
            } else if line.starts_with("1 ") || line.starts_with("2 ") {
                // Changed entry
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 9 {
                    let xy = parts[1];
                    let path = parts[8];

                    let index = xy.chars().next().unwrap_or(' ');
                    let worktree = xy.chars().nth(1).unwrap_or(' ');

                    if index != '.' && index != ' ' {
                        staged.push(format!("{}: {}", index, path));
                    }
                    if worktree != '.' && worktree != ' ' {
                        modified.push(format!("{}: {}", worktree, path));
                    }
                }
            } else if let Some(path) = line.strip_prefix("? ") {
                // Untracked
                untracked.push(path.to_string());
            }
        }

        result.push_str(&branch_info);
        result.push('\n');

        if !staged.is_empty() {
            result.push_str("\nStaged changes:\n");
            for item in &staged {
                result.push_str(&format!("  {}\n", item));
            }
        }

        if !modified.is_empty() {
            result.push_str("\nModified (not staged):\n");
            for item in &modified {
                result.push_str(&format!("  {}\n", item));
            }
        }

        if !untracked.is_empty() {
            result.push_str("\nUntracked files:\n");
            for item in &untracked {
                result.push_str(&format!("  {}\n", item));
            }
        }

        if staged.is_empty() && modified.is_empty() && untracked.is_empty() {
            result.push_str("\nWorking tree clean");
        }

        Ok(result)
    }

    /// Get the diff of changes.
    pub async fn diff(
        &self,
        staged: bool,
        file: Option<&str>,
        repo_path: Option<&Path>,
    ) -> Result<String> {
        let mut args = vec!["diff"];

        if staged {
            args.push("--cached");
        }

        // Add file if specified
        if let Some(f) = file {
            args.push("--");
            args.push(f);
        }

        let output = self.run_git(&args, repo_path).await?;

        if !output.success && !output.stderr.is_empty() {
            return Err(Error::execution_failed("git", output.stderr));
        }

        if output.stdout.is_empty() {
            Ok("No differences found".to_string())
        } else {
            Ok(output.stdout)
        }
    }

    /// Get commit log.
    pub async fn log(
        &self,
        count: usize,
        oneline: bool,
        repo_path: Option<&Path>,
    ) -> Result<String> {
        let count_str = format!("-{}", count);
        let mut args = vec!["log", &count_str];

        if oneline {
            args.push("--oneline");
        } else {
            args.push("--pretty=format:%h %ad %s (%an)");
            args.push("--date=short");
        }

        let output = self.run_git(&args, repo_path).await?;

        if !output.success {
            return Err(Error::execution_failed("git", output.stderr));
        }

        Ok(output.stdout)
    }

    /// Add files to staging area.
    pub async fn add(&self, files: &[&str], repo_path: Option<&Path>) -> Result<String> {
        let mut args = vec!["add"];
        args.extend(files);

        let output = self.run_git(&args, repo_path).await?;

        if !output.success {
            return Err(Error::execution_failed("git", output.stderr));
        }

        Ok(format!("Added {} file(s) to staging area", files.len()))
    }

    /// Commit staged changes.
    pub async fn commit(&self, message: &str, repo_path: Option<&Path>) -> Result<String> {
        let output = self.run_git(&["commit", "-m", message], repo_path).await?;

        if !output.success {
            return Err(Error::execution_failed("git", output.stderr));
        }

        Ok(output.stdout)
    }

    /// List branches.
    pub async fn branches(&self, all: bool, repo_path: Option<&Path>) -> Result<String> {
        let mut args = vec!["branch"];

        if all {
            args.push("-a");
        }

        args.push("-v");

        let output = self.run_git(&args, repo_path).await?;

        if !output.success {
            return Err(Error::execution_failed("git", output.stderr));
        }

        Ok(output.stdout)
    }

    /// Checkout a branch or file.
    pub async fn checkout(
        &self,
        target: &str,
        create: bool,
        repo_path: Option<&Path>,
    ) -> Result<String> {
        let mut args = vec!["checkout"];

        if create {
            args.push("-b");
        }

        args.push(target);

        let output = self.run_git(&args, repo_path).await?;

        if !output.success {
            return Err(Error::execution_failed("git", output.stderr));
        }

        let result = if output.stdout.is_empty() {
            output.stderr // Git often outputs to stderr for checkout
        } else {
            output.stdout
        };

        Ok(result)
    }

    /// Reset changes.
    pub async fn reset(
        &self,
        hard: bool,
        target: Option<&str>,
        repo_path: Option<&Path>,
    ) -> Result<String> {
        if hard && !self.config.allow_destructive {
            return Err(Error::execution_failed(
                "git",
                "Hard reset is disabled. Enable destructive operations to use this.",
            ));
        }

        let mut args = vec!["reset"];

        if hard {
            args.push("--hard");
        }

        if let Some(t) = target {
            args.push(t);
        }

        let output = self.run_git(&args, repo_path).await?;

        if !output.success {
            return Err(Error::execution_failed("git", output.stderr));
        }

        Ok(if output.stdout.is_empty() {
            "Reset complete".to_string()
        } else {
            output.stdout
        })
    }

    /// Stash changes.
    pub async fn stash(&self, pop: bool, repo_path: Option<&Path>) -> Result<String> {
        let args = if pop {
            vec!["stash", "pop"]
        } else {
            vec!["stash"]
        };

        let output = self.run_git(&args, repo_path).await?;

        if !output.success {
            return Err(Error::execution_failed("git", output.stderr));
        }

        Ok(if output.stdout.is_empty() {
            output.stderr
        } else {
            output.stdout
        })
    }

    /// Show file at a specific revision.
    pub async fn show(
        &self,
        revision: &str,
        file: Option<&str>,
        repo_path: Option<&Path>,
    ) -> Result<String> {
        let target = if let Some(f) = file {
            format!("{}:{}", revision, f)
        } else {
            revision.to_string()
        };

        let output = self.run_git(&["show", &target], repo_path).await?;

        if !output.success {
            return Err(Error::execution_failed("git", output.stderr));
        }

        Ok(output.stdout)
    }
}

impl Default for GitTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolHandler for GitTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "git",
            "Perform Git operations on a repository. Supports status, diff, log, add, commit, branch operations, and more.",
        )
        .add_string_param("operation", "Git operation: status, diff, log, add, commit, branch, checkout, reset, stash, show", true)
        .add_string_param("path", "Repository path (optional, uses current directory if not specified)", false)
        .add_string_param("message", "Commit message (for commit operation)", false)
        .add_string_param("target", "Target branch, file, or revision", false)
        .add_string_param("files", "Comma-separated list of files (for add operation)", false)
        .add_boolean_param("staged", "Show staged changes only (for diff)", false)
        .add_boolean_param("all", "Include all branches (for branch list)", false)
        .add_boolean_param("create", "Create new branch (for checkout)", false)
        .add_boolean_param("hard", "Hard reset (for reset operation)", false)
        .add_boolean_param("pop", "Pop stash (for stash operation)", false)
        .add_boolean_param("oneline", "One line format (for log)", false)
        .add_integer_param("count", "Number of entries (for log, default: 10)", false)
    }

    async fn execute(&self, arguments: Value) -> Result<ToolOutput> {
        let operation = arguments
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::invalid_arguments("git", "Missing 'operation' parameter"))?;

        let repo_path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .map(Path::new);

        let result = match operation {
            "status" => self.status(repo_path).await?,
            "diff" => {
                let staged = arguments
                    .get("staged")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let file = arguments.get("target").and_then(|v| v.as_str());
                self.diff(staged, file, repo_path).await?
            }
            "log" => {
                let count = arguments
                    .get("count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10) as usize;
                let oneline = arguments
                    .get("oneline")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                self.log(count, oneline, repo_path).await?
            }
            "add" => {
                let files: Vec<&str> = arguments
                    .get("files")
                    .and_then(|v| v.as_array())
                    .map_or_else(
                        || {
                            // If no files specified, check for target
                            arguments
                                .get("target")
                                .and_then(|v| v.as_str())
                                .map_or_else(|| vec!["."], |t| vec![t])
                        },
                        |arr| arr.iter().filter_map(|v| v.as_str()).collect(),
                    );
                self.add(&files, repo_path).await?
            }
            "commit" => {
                let message = arguments
                    .get("message")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        Error::invalid_arguments("git", "Commit requires a 'message' parameter")
                    })?;
                self.commit(message, repo_path).await?
            }
            "branch" | "branches" => {
                let all = arguments
                    .get("all")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.branches(all, repo_path).await?
            }
            "checkout" => {
                let target = arguments
                    .get("target")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        Error::invalid_arguments("git", "Checkout requires a 'target' parameter")
                    })?;
                let create = arguments
                    .get("create")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.checkout(target, create, repo_path).await?
            }
            "reset" => {
                let hard = arguments
                    .get("hard")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let target = arguments.get("target").and_then(|v| v.as_str());
                self.reset(hard, target, repo_path).await?
            }
            "stash" => {
                let pop = arguments
                    .get("pop")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.stash(pop, repo_path).await?
            }
            "show" => {
                let revision = arguments
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or("HEAD");
                let file = arguments.get("file").and_then(|v| v.as_str());
                self.show(revision, file, repo_path).await?
            }
            _ => {
                return Err(Error::invalid_arguments(
                    "git",
                    format!("Unknown operation: {}. Use: status, diff, log, add, commit, branch, checkout, reset, stash, show", operation),
                ));
            }
        };

        Ok(ToolOutput::success(result))
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Output from a git command.
#[derive(Debug, Clone)]
struct GitOutput {
    stdout: String,
    stderr: String,
    #[allow(dead_code)]
    exit_code: i32,
    success: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_tool_creation() {
        let tool = GitTool::new();
        assert!(tool.is_enabled());
    }

    #[test]
    fn test_git_config_default() {
        let config = GitConfig::default();
        assert!(!config.allow_destructive);
        assert_eq!(config.timeout_secs, 60);
    }

    #[tokio::test]
    async fn test_git_status() {
        // This test requires a git repository
        let tool = GitTool::new();
        // Only run if we're in a git repo
        if std::path::Path::new(".git").exists() {
            let result = tool.status(None).await;
            assert!(result.is_ok());
        }
    }
}
