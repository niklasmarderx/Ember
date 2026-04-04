//! Git integration CLI commands.
//!
//! AI-powered git operations including commit message generation,
//! PR descriptions, branch naming, and code review assistance.

use anyhow::{Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use colored::Colorize;
use std::path::PathBuf;
use std::process::Command;

/// Arguments for the git command.
#[derive(Args)]
pub struct GitArgs {
    #[command(subcommand)]
    pub action: GitAction,
}

/// Git intelligence actions.
#[derive(Subcommand)]
pub enum GitAction {
    /// Generate a smart commit message based on staged changes.
    ///
    /// Examples:
    ///   ember git commit
    ///   ember git commit --style conventional
    ///   ember git commit --scope auth --type feat
    #[command(
        about = "Generate a smart commit message based on staged changes.",
        after_help = "Examples:
  ember git commit
  ember git commit --style conventional
  ember git commit --scope auth --type feat"
    )]
    Commit {
        /// Commit message style
        #[arg(short, long, value_enum, default_value = "conventional")]
        style: CommitStyle,

        /// Commit type (for conventional commits)
        #[arg(short, long)]
        r#type: Option<String>,

        /// Commit scope (for conventional commits)
        #[arg(long)]
        scope: Option<String>,

        /// Include breaking change indicator
        #[arg(long)]
        breaking: bool,

        /// Auto-commit without confirmation
        #[arg(short, long)]
        yes: bool,

        /// Dry run (show message without committing)
        #[arg(long)]
        dry_run: bool,
    },

    /// Generate a PR description from branch changes.
    ///
    /// Examples:
    ///   ember git pr
    ///   ember git pr --template detailed
    ///   ember git pr --base main
    #[command(
        about = "Generate a PR description from branch changes.",
        after_help = "Examples:
  ember git pr
  ember git pr --template detailed
  ember git pr --base main"
    )]
    Pr {
        /// Base branch to compare against
        #[arg(short, long, default_value = "main")]
        base: String,

        /// PR template style
        #[arg(short, long, value_enum, default_value = "standard")]
        template: PrTemplate,

        /// Include test instructions
        #[arg(long)]
        tests: bool,

        /// Include screenshots placeholder
        #[arg(long)]
        screenshots: bool,

        /// Copy to clipboard
        #[arg(short, long)]
        copy: bool,
    },

    /// Suggest a branch name based on description.
    ///
    /// Examples:
    ///   ember git branch "add user authentication"
    ///   ember git branch --prefix feature "implement dark mode"
    #[command(
        about = "Suggest a branch name based on description.",
        after_help = "Examples:
  ember git branch \"add user authentication\"
  ember git branch --prefix feature \"implement dark mode\""
    )]
    Branch {
        /// Description of the feature/fix
        description: String,

        /// Branch prefix
        #[arg(short, long, default_value = "feature")]
        prefix: String,

        /// Use ticket number in name
        #[arg(short, long)]
        ticket: Option<String>,

        /// Create the branch
        #[arg(short, long)]
        create: bool,
    },

    /// Analyze and help resolve merge conflicts.
    ///
    /// Examples:
    ///   ember git resolve
    ///   ember git resolve --file src/main.rs
    #[command(
        about = "Analyze and help resolve merge conflicts.",
        after_help = "Examples:
  ember git resolve
  ember git resolve --file src/main.rs"
    )]
    Resolve {
        /// Specific file with conflicts
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Apply suggested resolution
        #[arg(short, long)]
        apply: bool,

        /// Prefer ours in conflicts
        #[arg(long)]
        ours: bool,

        /// Prefer theirs in conflicts
        #[arg(long)]
        theirs: bool,
    },

    /// Generate a code review for changes.
    ///
    /// Examples:
    ///   ember git review
    ///   ember git review --base main
    ///   ember git review --focus security
    #[command(
        about = "Generate a code review for changes.",
        after_help = "Examples:
  ember git review
  ember git review --base main
  ember git review --focus security"
    )]
    Review {
        /// Base branch to compare against
        #[arg(short, long, default_value = "main")]
        base: String,

        /// Review focus area
        #[arg(short, long, value_enum)]
        focus: Option<ReviewFocus>,

        /// Output format
        #[arg(long, value_enum, default_value = "pretty")]
        format: OutputFormat,

        /// Severity threshold
        #[arg(long, value_enum, default_value = "info")]
        severity: Severity,
    },

    /// Show git statistics and insights.
    ///
    /// Examples:
    ///   ember git stats
    ///   ember git stats --author "John Doe"
    ///   ember git stats --since "2024-01-01"
    #[command(
        about = "Show git statistics and insights.",
        after_help = "Examples:
  ember git stats
  ember git stats --author \"John Doe\"
  ember git stats --since \"2024-01-01\""
    )]
    Stats {
        /// Filter by author
        #[arg(short, long)]
        author: Option<String>,

        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        since: Option<String>,

        /// End date (YYYY-MM-DD)
        #[arg(long)]
        until: Option<String>,

        /// Show file statistics
        #[arg(long)]
        files: bool,
    },

    /// Generate release notes from commits.
    ///
    /// Examples:
    ///   ember git changelog
    ///   ember git changelog --from v1.0.0 --to v1.1.0
    #[command(
        about = "Generate release notes from commits.",
        after_help = "Examples:
  ember git changelog
  ember git changelog --from v1.0.0 --to v1.1.0"
    )]
    Changelog {
        /// Start tag/commit
        #[arg(long)]
        from: Option<String>,

        /// End tag/commit
        #[arg(long)]
        to: Option<String>,

        /// Output format
        #[arg(short, long, value_enum, default_value = "markdown")]
        format: ChangelogFormat,

        /// Group by type
        #[arg(long, default_value = "true")]
        grouped: bool,
    },
}

/// Commit message style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum CommitStyle {
    /// Conventional commits (feat:, fix:, etc.)
    #[default]
    Conventional,
    /// Simple descriptive message
    Simple,
    /// Detailed with body
    Detailed,
    /// Emoji prefix style
    Emoji,
}

/// PR template style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum PrTemplate {
    /// Standard template with description
    #[default]
    Standard,
    /// Detailed with sections
    Detailed,
    /// Minimal one-liner
    Minimal,
    /// Bug fix template
    Bugfix,
    /// Feature template
    Feature,
}

/// Review focus area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ReviewFocus {
    /// Security issues
    Security,
    /// Performance issues
    Performance,
    /// Code style
    Style,
    /// Logic errors
    Logic,
    /// All areas
    All,
}

/// Output format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Pretty,
    Json,
    Markdown,
}

/// Severity level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Severity {
    /// Errors only
    Error,
    /// Warnings and errors
    Warning,
    /// Info, warnings, and errors
    #[default]
    Info,
    /// All including hints
    Hint,
}

/// Changelog format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum ChangelogFormat {
    #[default]
    Markdown,
    Json,
    Html,
    Plain,
}

/// Execute the git command.
pub async fn execute(args: GitArgs) -> Result<()> {
    match args.action {
        GitAction::Commit {
            style,
            r#type,
            scope,
            breaking,
            yes,
            dry_run,
        } => generate_commit(style, r#type, scope, breaking, yes, dry_run).await,
        GitAction::Pr {
            base,
            template,
            tests,
            screenshots,
            copy,
        } => generate_pr(base, template, tests, screenshots, copy).await,
        GitAction::Branch {
            description,
            prefix,
            ticket,
            create,
        } => suggest_branch(description, prefix, ticket, create).await,
        GitAction::Resolve {
            file,
            apply,
            ours,
            theirs,
        } => resolve_conflicts(file, apply, ours, theirs).await,
        GitAction::Review {
            base,
            focus,
            format,
            severity,
        } => generate_review(base, focus, format, severity).await,
        GitAction::Stats {
            author,
            since,
            until,
            files,
        } => show_stats(author, since, until, files).await,
        GitAction::Changelog {
            from,
            to,
            format,
            grouped,
        } => generate_changelog(from, to, format, grouped).await,
    }
}

/// Generate a smart commit message.
async fn generate_commit(
    style: CommitStyle,
    commit_type: Option<String>,
    scope: Option<String>,
    breaking: bool,
    auto_commit: bool,
    dry_run: bool,
) -> Result<()> {
    println!("{}", "Analyzing staged changes...".bright_blue());

    // Get staged diff
    let diff_output = Command::new("git")
        .args(["diff", "--cached", "--stat"])
        .output()
        .context("Failed to run git diff")?;

    let diff_stat = String::from_utf8_lossy(&diff_output.stdout);

    if diff_stat.trim().is_empty() {
        println!(
            "{} No staged changes found. Use 'git add' to stage files.",
            "[!]".yellow()
        );
        return Ok(());
    }

    println!();
    println!("{}", "Staged changes:".bright_blue());
    println!("{}", diff_stat);

    // Get file names
    let files_output = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .output()?;
    let files_str = String::from_utf8_lossy(&files_output.stdout);
    let files: Vec<&str> = files_str.lines().take(10).collect();

    // Generate commit message based on style
    let message = match style {
        CommitStyle::Conventional => {
            let t = commit_type.unwrap_or_else(|| infer_commit_type(&files));
            let s = scope.map(|s| format!("({})", s)).unwrap_or_default();
            let bang = if breaking { "!" } else { "" };
            let desc = generate_description(&files);
            format!("{}{}{}: {}", t, s, bang, desc)
        }
        CommitStyle::Simple => generate_description(&files),
        CommitStyle::Detailed => {
            let desc = generate_description(&files);
            let body = generate_body(&files);
            format!("{}\n\n{}", desc, body)
        }
        CommitStyle::Emoji => {
            let emoji = infer_emoji(&files);
            let desc = generate_description(&files);
            format!("{} {}", emoji, desc)
        }
    };

    println!();
    println!("{}", "Generated commit message:".bright_green());
    println!("{}", "=".repeat(50));
    println!("{}", message.bright_yellow());
    println!("{}", "=".repeat(50));

    if dry_run {
        println!();
        println!("{} Dry run - not committing", "[i]".cyan());
        return Ok(());
    }

    if auto_commit {
        let status = Command::new("git")
            .args(["commit", "-m", &message])
            .status()
            .context("Failed to commit")?;

        if status.success() {
            println!();
            println!("{} Committed successfully!", "[OK]".green());
        } else {
            println!("{} Commit failed", "[!]".red());
        }
    } else {
        println!();
        println!(
            "{} Use --yes to auto-commit or copy the message above",
            "[?]".cyan()
        );
    }

    Ok(())
}

/// Generate a PR description.
async fn generate_pr(
    base: String,
    template: PrTemplate,
    include_tests: bool,
    include_screenshots: bool,
    _copy: bool,
) -> Result<()> {
    println!(
        "{} {}",
        "Analyzing changes against:".bright_blue(),
        base.cyan()
    );

    // Get current branch
    let branch_output = Command::new("git")
        .args(["branch", "--show-current"])
        .output()?;
    let current_branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();

    // Get commit log
    let log_output = Command::new("git")
        .args(["log", &format!("{}..HEAD", base), "--oneline"])
        .output()?;
    let commits = String::from_utf8_lossy(&log_output.stdout);

    // Get diff stats
    let diff_output = Command::new("git")
        .args(["diff", &base, "--stat"])
        .output()?;
    let diff_stat = String::from_utf8_lossy(&diff_output.stdout);

    let pr_title = generate_pr_title(&current_branch);
    let pr_body = match template {
        PrTemplate::Minimal => format!("## Summary\n\n{}", pr_title),
        PrTemplate::Standard => {
            let mut body = format!(
                "## Summary\n\n{}\n\n## Changes\n\n{}\n\n## Commits\n\n{}",
                generate_pr_summary(&current_branch),
                format_changes(&diff_stat),
                format_commits(&commits)
            );
            if include_tests {
                body.push_str("\n\n## Testing\n\n- [ ] Unit tests added\n- [ ] Manual testing performed");
            }
            if include_screenshots {
                body.push_str("\n\n## Screenshots\n\n_Add screenshots here if applicable_");
            }
            body
        }
        PrTemplate::Detailed => {
            let mut body = format!(
                "## Summary\n\n{}\n\n## Motivation\n\n_Explain why this change is needed_\n\n## Changes\n\n{}\n\n## Commits\n\n{}\n\n## Checklist\n\n- [ ] Code follows project style\n- [ ] Documentation updated\n- [ ] Tests added/updated\n- [ ] Breaking changes noted",
                generate_pr_summary(&current_branch),
                format_changes(&diff_stat),
                format_commits(&commits)
            );
            if include_tests {
                body.push_str("\n\n## Testing\n\n- [ ] Unit tests\n- [ ] Integration tests\n- [ ] Manual testing");
            }
            if include_screenshots {
                body.push_str("\n\n## Screenshots\n\n| Before | After |\n|--------|-------|\n| _screenshot_ | _screenshot_ |");
            }
            body
        }
        PrTemplate::Bugfix => format!(
            "## Bug Description\n\n_Describe the bug_\n\n## Root Cause\n\n_Explain the root cause_\n\n## Fix\n\n{}\n\n## Testing\n\n- [ ] Bug is fixed\n- [ ] No regression\n\n## Changes\n\n{}",
            generate_pr_summary(&current_branch),
            format_changes(&diff_stat)
        ),
        PrTemplate::Feature => format!(
            "## Feature Description\n\n{}\n\n## Implementation\n\n_Describe the implementation approach_\n\n## Changes\n\n{}\n\n## Testing\n\n- [ ] Feature works as expected\n- [ ] Edge cases handled\n\n## Documentation\n\n- [ ] README updated\n- [ ] API docs updated",
            generate_pr_summary(&current_branch),
            format_changes(&diff_stat)
        ),
    };

    println!();
    println!("{}", "Generated PR Title:".bright_green());
    println!("{}", pr_title.bright_yellow().bold());
    println!();
    println!("{}", "Generated PR Description:".bright_green());
    println!("{}", "=".repeat(60));
    println!("{}", pr_body);
    println!("{}", "=".repeat(60));

    Ok(())
}

/// Suggest a branch name.
async fn suggest_branch(
    description: String,
    prefix: String,
    ticket: Option<String>,
    create: bool,
) -> Result<()> {
    println!(
        "{} {}",
        "Creating branch name for:".bright_blue(),
        description.cyan()
    );

    // Convert description to branch name
    let slug = description
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");

    let branch_name = match ticket {
        Some(t) => format!("{}/{}-{}", prefix, t, slug),
        None => format!("{}/{}", prefix, slug),
    };

    println!();
    println!("{}", "Suggested branch name:".bright_green());
    println!("  {}", branch_name.bright_yellow().bold());

    if create {
        let status = Command::new("git")
            .args(["checkout", "-b", &branch_name])
            .status()
            .context("Failed to create branch")?;

        if status.success() {
            println!();
            println!("{} Branch created and checked out!", "[OK]".green());
        } else {
            println!("{} Failed to create branch", "[!]".red());
        }
    } else {
        println!();
        println!("{} Use --create to create this branch", "[i]".cyan());
        println!("  Or run: git checkout -b {}", branch_name);
    }

    Ok(())
}

/// Help resolve merge conflicts.
async fn resolve_conflicts(
    file: Option<PathBuf>,
    _apply: bool,
    prefer_ours: bool,
    prefer_theirs: bool,
) -> Result<()> {
    println!("{}", "Checking for merge conflicts...".bright_blue());

    // List files with conflicts
    let status_output = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .output()?;
    let conflict_files = String::from_utf8_lossy(&status_output.stdout);

    if conflict_files.trim().is_empty() {
        println!("{} No merge conflicts found!", "[OK]".green());
        return Ok(());
    }

    let files: Vec<&str> = conflict_files.lines().collect();
    println!();
    println!("{} {} files with conflicts:", "[!]".yellow(), files.len());
    for f in &files {
        println!("  {} {}", "-".red(), f);
    }

    if let Some(target_file) = file {
        println!();
        println!(
            "{} {}",
            "Analyzing conflicts in:".bright_blue(),
            target_file.display()
        );

        // Read file content
        let content = std::fs::read_to_string(&target_file)?;
        let conflict_count = content.matches("<<<<<<<").count();

        println!("  Found {} conflict markers", conflict_count);

        if prefer_ours {
            println!("  {} Would keep 'ours' version", "[i]".cyan());
        } else if prefer_theirs {
            println!("  {} Would keep 'theirs' version", "[i]".cyan());
        } else {
            println!();
            println!("{}", "Conflict Resolution Suggestions:".bright_yellow());
            println!("  1. Review each conflict carefully");
            println!("  2. Choose the appropriate version or merge manually");
            println!("  3. Remove conflict markers after resolving");
            println!("  4. Stage the file with 'git add'");
        }
    } else {
        println!();
        println!("{} Use --file to analyze a specific file", "[i]".cyan());
    }

    Ok(())
}

/// Generate a code review.
async fn generate_review(
    base: String,
    focus: Option<ReviewFocus>,
    format: OutputFormat,
    _severity: Severity,
) -> Result<()> {
    println!(
        "{} {}",
        "Reviewing changes against:".bright_blue(),
        base.cyan()
    );

    // Get diff
    let diff_output = Command::new("git").args(["diff", &base]).output()?;
    let diff = String::from_utf8_lossy(&diff_output.stdout);

    if diff.trim().is_empty() {
        println!("{} No changes to review", "[i]".cyan());
        return Ok(());
    }

    // Send diff to LLM for review
    let config = crate::config::AppConfig::load(None).unwrap_or_default();
    let provider = crate::commands::provider_factory::create_provider(&config, &config.provider.default)?;
    let review_prompt = format!(
        "You are a code reviewer. Analyze this git diff and provide review comments.\n\
         For each issue, output EXACTLY this format, one per line:\n\
         FILE:LINE:SEVERITY:MESSAGE:SUGGESTION\n\n\
         SEVERITY must be one of: error, warning, info\n\
         SUGGESTION can be empty if none.\n\
         Only output the review lines, nothing else.\n\n\
         Diff:\n```\n{}\n```",
        &diff[..diff.len().min(8000)] // Truncate very large diffs
    );
    let request = ember_llm::CompletionRequest::new(&config.provider.openai.model)
        .with_message(ember_llm::Message::user(&review_prompt));

    let mut comments: Vec<ReviewComment> = Vec::new();
    match provider.complete(request).await {
        Ok(resp) => {
            for line in resp.content.lines() {
                let parts: Vec<&str> = line.splitn(5, ':').collect();
                if parts.len() >= 4 {
                    comments.push(ReviewComment {
                        file: parts[0].trim().to_string(),
                        line: parts[1].trim().parse().unwrap_or(0),
                        severity: parts[2].trim().to_lowercase(),
                        message: parts[3].trim().to_string(),
                        suggestion: parts
                            .get(4)
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty()),
                    });
                }
            }
            if comments.is_empty() {
                // LLM didn't follow format — show raw response as a single info comment
                comments.push(ReviewComment {
                    file: "(summary)".to_string(),
                    line: 0,
                    severity: "info".to_string(),
                    message: resp.content.lines().take(5).collect::<Vec<_>>().join(" "),
                    suggestion: None,
                });
            }
        }
        Err(e) => {
            eprintln!(
                "{} LLM review failed: {}. Showing basic analysis.",
                "[warn]".bright_yellow(),
                e
            );
            // Fallback: count changed files from diff
            let changed_files: Vec<&str> = diff
                .lines()
                .filter(|l| l.starts_with("+++ b/") || l.starts_with("--- a/"))
                .filter_map(|l| {
                    l.strip_prefix("+++ b/")
                        .or_else(|| l.strip_prefix("--- a/"))
                })
                .collect();
            comments.push(ReviewComment {
                file: "(overview)".to_string(),
                line: 0,
                severity: "info".to_string(),
                message: format!("{} file(s) changed in this diff", changed_files.len() / 2),
                suggestion: None,
            });
        }
    }

    // Filter by focus if specified
    if let Some(f) = focus {
        comments.retain(|c| matches_focus(&c.message, f));
    }

    match format {
        OutputFormat::Pretty => {
            println!();
            println!(
                "{} {}",
                "Code Review Results".bright_green().bold(),
                format!("({} comments)", comments.len()).dimmed()
            );
            println!("{}", "=".repeat(60));

            for comment in &comments {
                let severity_icon = match comment.severity.as_str() {
                    "error" => "[E]".red(),
                    "warning" => "[W]".yellow(),
                    "info" => "[I]".cyan(),
                    _ => "[H]".dimmed(),
                };

                println!();
                println!(
                    "{} {}:{} - {}",
                    severity_icon,
                    comment.file.cyan(),
                    comment.line.to_string().white(),
                    comment.message
                );
                if let Some(ref suggestion) = comment.suggestion {
                    println!(
                        "    {} {}",
                        "Suggestion:".bright_blue(),
                        suggestion.dimmed()
                    );
                }
            }

            println!();
            println!("{}", "Summary:".bright_blue());
            println!(
                "  Errors: {}, Warnings: {}, Info: {}",
                comments.iter().filter(|c| c.severity == "error").count(),
                comments.iter().filter(|c| c.severity == "warning").count(),
                comments.iter().filter(|c| c.severity == "info").count()
            );
        }
        OutputFormat::Json => {
            let result = serde_json::json!({
                "base": base,
                "comments": comments.iter().map(|c| serde_json::json!({
                    "file": c.file,
                    "line": c.line,
                    "severity": c.severity,
                    "message": c.message,
                    "suggestion": c.suggestion
                })).collect::<Vec<_>>()
            });
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        OutputFormat::Markdown => {
            println!("# Code Review\n");
            println!("**Base:** `{}`\n", base);
            println!("## Comments\n");
            for comment in &comments {
                println!(
                    "- **{}:{}** [{}] {}",
                    comment.file, comment.line, comment.severity, comment.message
                );
                if let Some(ref s) = comment.suggestion {
                    println!("  - Suggestion: {}", s);
                }
            }
        }
    }

    Ok(())
}

/// Show git statistics.
async fn show_stats(
    author: Option<String>,
    since: Option<String>,
    until: Option<String>,
    show_files: bool,
) -> Result<()> {
    println!("{}", "Gathering git statistics...".bright_blue());

    let mut args = vec!["log", "--oneline"];
    let mut filters = Vec::new();

    if let Some(ref a) = author {
        filters.push(format!("--author={}", a));
    }
    if let Some(ref s) = since {
        filters.push(format!("--since={}", s));
    }
    if let Some(ref u) = until {
        filters.push(format!("--until={}", u));
    }

    for f in &filters {
        args.push(f);
    }

    let log_output = Command::new("git").args(&args).output()?;
    let commit_count = String::from_utf8_lossy(&log_output.stdout).lines().count();

    // Get contributor stats
    let shortlog = Command::new("git")
        .args(["shortlog", "-sn", "HEAD"])
        .output()?;
    let contributors = String::from_utf8_lossy(&shortlog.stdout);
    let contributor_count = contributors.lines().count();

    println!();
    println!("{}", "Repository Statistics".bright_yellow().bold());
    println!("{}", "=".repeat(50));
    println!("  {} {}", "Total commits:".bright_blue(), commit_count);
    println!("  {} {}", "Contributors:".bright_blue(), contributor_count);

    if show_files {
        let ls_output = Command::new("git").args(["ls-files"]).output()?;
        let file_count = String::from_utf8_lossy(&ls_output.stdout).lines().count();
        println!("  {} {}", "Tracked files:".bright_blue(), file_count);
    }

    println!();
    println!("{}", "Top Contributors:".bright_blue());
    for line in contributors.lines().take(5) {
        println!("  {}", line.trim());
    }

    Ok(())
}

/// Generate changelog.
async fn generate_changelog(
    from: Option<String>,
    to: Option<String>,
    format: ChangelogFormat,
    grouped: bool,
) -> Result<()> {
    let from_ref = from.unwrap_or_else(|| "HEAD~20".to_string());
    let to_ref = to.unwrap_or_else(|| "HEAD".to_string());

    println!(
        "{} {} -> {}",
        "Generating changelog:".bright_blue(),
        from_ref.cyan(),
        to_ref.cyan()
    );

    let log_output = Command::new("git")
        .args([
            "log",
            &format!("{}..{}", from_ref, to_ref),
            "--pretty=format:%s",
        ])
        .output()?;
    let commits_str = String::from_utf8_lossy(&log_output.stdout);
    let commits: Vec<&str> = commits_str.lines().collect();

    if commits.is_empty() || (commits.len() == 1 && commits[0].is_empty()) {
        println!("{} No commits found in range", "[i]".cyan());
        return Ok(());
    }

    // Parse commits by type
    let mut features = Vec::new();
    let mut fixes = Vec::new();
    let mut others = Vec::new();

    for commit in &commits {
        if commit.starts_with("feat") {
            features.push(*commit);
        } else if commit.starts_with("fix") {
            fixes.push(*commit);
        } else {
            others.push(*commit);
        }
    }

    match format {
        ChangelogFormat::Markdown => {
            println!();
            println!("# Changelog\n");
            if grouped {
                if !features.is_empty() {
                    println!("## Features\n");
                    for f in &features {
                        println!("- {}", f);
                    }
                    println!();
                }
                if !fixes.is_empty() {
                    println!("## Bug Fixes\n");
                    for f in &fixes {
                        println!("- {}", f);
                    }
                    println!();
                }
                if !others.is_empty() {
                    println!("## Other Changes\n");
                    for o in &others {
                        println!("- {}", o);
                    }
                }
            } else {
                for c in &commits {
                    println!("- {}", c);
                }
            }
        }
        ChangelogFormat::Json => {
            let result = serde_json::json!({
                "from": from_ref,
                "to": to_ref,
                "features": features,
                "fixes": fixes,
                "others": others
            });
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        ChangelogFormat::Plain => {
            for c in &commits {
                println!("{}", c);
            }
        }
        ChangelogFormat::Html => {
            println!("<h1>Changelog</h1>");
            println!("<ul>");
            for c in &commits {
                println!("  <li>{}</li>", c);
            }
            println!("</ul>");
        }
    }

    Ok(())
}

// Helper types and functions

struct ReviewComment {
    file: String,
    line: u32,
    severity: String,
    message: String,
    suggestion: Option<String>,
}

fn infer_commit_type(files: &[&str]) -> String {
    for f in files {
        if f.contains("test") {
            return "test".to_string();
        }
        if f.contains("doc") || f.ends_with(".md") {
            return "docs".to_string();
        }
        if f.contains("config") || f.ends_with(".toml") || f.ends_with(".json") {
            return "chore".to_string();
        }
    }
    "feat".to_string()
}

fn generate_description(files: &[&str]) -> String {
    if files.is_empty() {
        return "Update files".to_string();
    }
    if files.len() == 1 {
        let file = files[0];
        let name = std::path::Path::new(file)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());
        return format!("update {} module", name);
    }
    format!("update {} files", files.len())
}

fn generate_body(files: &[&str]) -> String {
    let mut body = String::from("Changes:\n");
    for f in files.iter().take(5) {
        body.push_str(&format!("- {}\n", f));
    }
    if files.len() > 5 {
        body.push_str(&format!("- ... and {} more files\n", files.len() - 5));
    }
    body
}

fn infer_emoji(files: &[&str]) -> &'static str {
    for f in files {
        if f.contains("test") {
            return "[test]";
        }
        if f.ends_with(".md") {
            return "[docs]";
        }
        if f.contains("fix") {
            return "[fix]";
        }
    }
    "[feat]"
}

fn generate_pr_title(branch: &str) -> String {
    let parts: Vec<&str> = branch.split('/').collect();
    if parts.len() >= 2 {
        let desc = parts[1..].join(" ").replace(['-', '_'], " ");
        let first_char = desc.chars().next().unwrap_or(' ').to_uppercase();
        format!("{}{}", first_char, &desc[1..])
    } else {
        branch.replace(['-', '_'], " ")
    }
}

fn generate_pr_summary(branch: &str) -> String {
    let title = generate_pr_title(branch);
    format!(
        "This PR implements {}.\n\n_Add more details about the changes here._",
        title.to_lowercase()
    )
}

fn format_changes(diff_stat: &str) -> String {
    let lines: Vec<&str> = diff_stat.lines().take(10).collect();
    let mut result = String::new();
    for line in lines {
        result.push_str(&format!("- `{}`\n", line.trim()));
    }
    result
}

fn format_commits(commits: &str) -> String {
    let lines: Vec<&str> = commits.lines().take(10).collect();
    let mut result = String::new();
    for line in lines {
        result.push_str(&format!("- {}\n", line));
    }
    result
}

fn matches_focus(message: &str, focus: ReviewFocus) -> bool {
    let msg = message.to_lowercase();
    match focus {
        ReviewFocus::All => true,
        ReviewFocus::Security => {
            msg.contains("unsafe")
                || msg.contains("inject")
                || msg.contains("auth")
                || msg.contains("secret")
                || msg.contains("password")
                || msg.contains("credential")
                || msg.contains("xss")
                || msg.contains("csrf")
                || msg.contains("sanitiz")
                || msg.contains("vulnerab")
        }
        ReviewFocus::Performance => {
            msg.contains("perf")
                || msg.contains("slow")
                || msg.contains("cache")
                || msg.contains("optim")
                || msg.contains("allocat")
                || msg.contains("clone")
                || msg.contains("O(n")
                || msg.contains("loop")
                || msg.contains("batch")
        }
        ReviewFocus::Style => {
            msg.contains("naming")
                || msg.contains("format")
                || msg.contains("style")
                || msg.contains("convention")
                || msg.contains("indent")
                || msg.contains("consistent")
                || msg.contains("readab")
        }
        ReviewFocus::Logic => {
            msg.contains("bug")
                || msg.contains("error")
                || msg.contains("wrong")
                || msg.contains("incorrect")
                || msg.contains("off-by")
                || msg.contains("edge case")
                || msg.contains("null")
                || msg.contains("panic")
                || msg.contains("unwrap")
        }
    }
}
