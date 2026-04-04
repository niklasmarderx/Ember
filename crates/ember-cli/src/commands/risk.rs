//! Tool execution risk assessment and confirmation prompts.
//!
//! Classifies tool calls by risk tier (Safe / Moderate / Dangerous) and
//! presents a confirmation prompt for non-safe operations.

use colored::Colorize;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

/// Global auto-approve flag — set by `--yes` or by pressing "a" during confirmation.
pub static AUTO_APPROVE: AtomicBool = AtomicBool::new(false);

/// Classify the effective risk of a tool call, considering the operation type
/// and arguments (not just the tool name).
pub fn classify_call_risk(tool_name: &str, args: &serde_json::Value) -> ember_core::RiskTier {
    use ember_core::RiskTier;

    match tool_name {
        "shell" => {
            // Analyse shell command for risk
            let cmd = args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let lower = cmd.to_lowercase();

            // Read-only commands are Safe
            let safe_prefixes = [
                "ls", "cat", "head", "tail", "wc", "grep", "rg", "find", "which", "echo",
                "pwd", "whoami", "date", "uname", "env", "printenv", "tree", "file",
                "stat", "du", "df",
            ];
            let first_word = cmd.split_whitespace().next().unwrap_or("");
            if safe_prefixes.contains(&first_word) {
                return RiskTier::Safe;
            }

            // Build commands are Moderate
            let build_prefixes = [
                "cargo", "npm", "pnpm", "yarn", "go", "make", "cmake", "pip",
                "bundle", "mvn", "gradle", "rustfmt", "prettier", "eslint",
                "git status", "git diff", "git log", "git branch",
            ];
            if build_prefixes
                .iter()
                .any(|p| lower.starts_with(p))
            {
                return RiskTier::Moderate;
            }

            // Destructive patterns are Dangerous
            if lower.contains("rm ")
                || lower.contains("rm\t")
                || lower.starts_with("rm ")
                || lower.contains("sudo")
                || lower.contains("chmod")
                || lower.contains("chown")
                || lower.contains("dd ")
                || lower.contains("mkfs")
                || lower.contains("> /dev/")
                || lower.contains("curl") && lower.contains("|")
                || lower.contains("wget") && lower.contains("|")
            {
                return RiskTier::Dangerous;
            }

            // Default shell = Dangerous
            RiskTier::Dangerous
        }
        "filesystem" => {
            let op = args
                .get("operation")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match op {
                "read" | "list" | "search" | "glob" | "stat" => RiskTier::Safe,
                "write" | "create" | "edit" | "append" => RiskTier::Moderate,
                "delete" => RiskTier::Dangerous,
                _ => RiskTier::Moderate,
            }
        }
        "git" => {
            let op = args
                .get("operation")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match op {
                "status" | "diff" | "log" | "branch" | "show" => RiskTier::Safe,
                "add" | "commit" | "stash" | "checkout" | "tag" => RiskTier::Moderate,
                "push" | "force-push" | "reset" | "rebase" => RiskTier::Dangerous,
                _ => RiskTier::Moderate,
            }
        }
        "web" | "browser" => RiskTier::Safe,
        _ => ember_core::classify_tool_risk(tool_name),
    }
}

pub fn confirm_tool_execution(tool_name: &str, args: &serde_json::Value) -> bool {
    use ember_core::RiskTier;

    // Check auto-approve first (set by --yes or previous "a" answer)
    if AUTO_APPROVE.load(Ordering::Relaxed) {
        return true;
    }

    let risk = classify_call_risk(tool_name, args);

    // Safe operations: always auto-approve
    if risk == RiskTier::Safe {
        return true;
    }

    // Build description string for confirmation prompt
    let desc = match tool_name {
        "shell" => {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
            format!("Run command: {}", cmd)
        }
        "filesystem" => {
            let op = args
                .get("operation")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            match op {
                "write" | "create" => {
                    // Show diff preview before confirming
                    if let Some(new_content) = args.get("content").and_then(|v| v.as_str()) {
                        let old_content = std::fs::read_to_string(path).unwrap_or_default();
                        let hunks = ember_tools::compute_diff(&old_content, new_content);
                        if !hunks.is_empty() {
                            let diff_str = ember_tools::format_unified_diff(path, &hunks);
                            let lines: Vec<&str> = diff_str.lines().collect();
                            let max_lines = 30;
                            for line in lines.iter().take(max_lines) {
                                if line.starts_with('+') && !line.starts_with("+++") {
                                    eprintln!("    {}", line.bright_green());
                                } else if line.starts_with('-') && !line.starts_with("---") {
                                    eprintln!("    {}", line.bright_red());
                                } else if line.starts_with("@@") {
                                    eprintln!("    {}", line.bright_cyan());
                                } else {
                                    eprintln!("    {}", line.dimmed());
                                }
                            }
                            if lines.len() > max_lines {
                                eprintln!(
                                    "    {} ... {} more lines",
                                    "".dimmed(),
                                    lines.len() - max_lines
                                );
                            }
                        } else if !old_content.is_empty() {
                            eprintln!("    {}", "(no changes)".dimmed());
                        } else {
                            let line_count = new_content.lines().count();
                            eprintln!("    {} new file ({} lines)", "+".bright_green(), line_count);
                        }
                    }
                    format!("Write to file: {}", path)
                }
                "edit" => {
                    if let (Some(old_str), Some(new_str)) = (
                        args.get("old_str").and_then(|v| v.as_str()),
                        args.get("new_str").and_then(|v| v.as_str()),
                    ) {
                        let old_lines: Vec<&str> = old_str.lines().collect();
                        let new_lines: Vec<&str> = new_str.lines().collect();
                        let max_show = 10;
                        for line in old_lines.iter().take(max_show) {
                            eprintln!("    {}{}", "-".bright_red(), line.bright_red());
                        }
                        for line in new_lines.iter().take(max_show) {
                            eprintln!("    {}{}", "+".bright_green(), line.bright_green());
                        }
                        if old_lines.len() > max_show || new_lines.len() > max_show {
                            eprintln!("    {}", "...".dimmed());
                        }
                    }
                    format!("Edit file: {}", path)
                }
                "delete" => format!("Delete: {}", path),
                _ => format!("{}: {}", op, path),
            }
        }
        "git" => {
            let op = args.get("operation").and_then(|v| v.as_str()).unwrap_or("?");
            format!("Git {}", op)
        }
        _ => format!("Execute {} tool", tool_name),
    };

    let risk_badge = match risk {
        RiskTier::Moderate => "moderate".bright_yellow(),
        RiskTier::Dangerous => "DANGEROUS".bright_red().bold(),
        RiskTier::Safe => "safe".bright_green(), // won't reach here
    };

    print!(
        "  {} [{}] {} ({}/{}/{}) ",
        "[confirm]".bright_yellow(),
        risk_badge,
        desc,
        "y".bright_green(),
        "n".bright_red(),
        "a=always".bright_cyan(),
    );
    let _ = io::stdout().flush();

    let mut line = String::new();
    let _ = io::stdin().read_line(&mut line);
    let answer = line.trim().to_lowercase();
    match answer.as_str() {
        "a" | "always" => {
            AUTO_APPROVE.store(true, Ordering::Relaxed);
            println!(
                "  {} Auto-approve enabled for this session.",
                "[info]".bright_blue()
            );
            true
        }
        "y" | "yes" | "" => true,
        _ => false,
    }
}
