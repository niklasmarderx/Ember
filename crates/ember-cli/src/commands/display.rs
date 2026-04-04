//! Display helpers for the chat command.
//!
//! Contains formatting for tool calls, tool results, inline diffs,
//! progress indicators, and response statistics.

use colored::Colorize;
use std::io::{self, Write};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Spinner frames for progress indicator.
pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

// ──────────────────────────────────────────────────────────────────────────────
// Response statistics
// ──────────────────────────────────────────────────────────────────────────────

/// Response statistics for token counting and timing.
#[derive(Debug, Default)]
pub struct ResponseStats {
    pub tokens: usize,
    pub duration: Duration,
}

impl ResponseStats {
    pub fn tokens_per_second(&self) -> f64 {
        if self.duration.as_secs_f64() > 0.0 {
            self.tokens as f64 / self.duration.as_secs_f64()
        } else {
            0.0
        }
    }

    pub fn format(&self) -> String {
        format!(
            "[{} tokens, {:.1}s, {:.1} tok/s]",
            self.tokens,
            self.duration.as_secs_f64(),
            self.tokens_per_second()
        )
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Progress indicator
// ──────────────────────────────────────────────────────────────────────────────

/// Progress indicator that shows a spinner while waiting.
pub struct ProgressIndicator {
    message: String,
    stop_tx: Option<mpsc::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl ProgressIndicator {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
            stop_tx: None,
            handle: None,
        }
    }

    pub fn start(&mut self) {
        let (tx, mut rx) = mpsc::channel::<()>(1);
        self.stop_tx = Some(tx);

        let message = self.message.clone();
        let handle = tokio::spawn(async move {
            let mut frame = 0;
            let start = Instant::now();

            loop {
                tokio::select! {
                    _ = rx.recv() => {
                        print!("\r{}\r", " ".repeat(60));
                        let _ = io::stdout().flush();
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(80)) => {
                        let elapsed = start.elapsed().as_secs();
                        let spinner = SPINNER_FRAMES[frame % SPINNER_FRAMES.len()];
                        print!(
                            "\r{} {} {} ({}s)",
                            spinner.bright_cyan(),
                            message.bright_yellow(),
                            ".".repeat((frame / 3) % 4).dimmed(),
                            elapsed
                        );
                        let _ = io::stdout().flush();
                        frame += 1;
                    }
                }
            }
        });

        self.handle = Some(handle);
    }

    pub async fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(()).await;
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Display helpers
// ──────────────────────────────────────────────────────────────────────────────

pub fn print_history(history: &[ember_llm::Message]) {
    if history.len() <= 1 {
        println!("{}", "No conversation history.".bright_yellow());
        return;
    }

    println!();
    println!("{}", "Conversation History:".bright_yellow().bold());

    let mut turn = 0;
    for msg in history.iter().skip(1) {
        match msg.role {
            ember_llm::Role::User => {
                turn += 1;
                println!("{}. {}: {}", turn, "You".bright_green(), msg.content);
            }
            ember_llm::Role::Assistant => {
                let preview: String = msg.content.chars().take(100).collect();
                let suffix = if msg.content.len() > 100 { "..." } else { "" };
                println!("   {}: {}{}", "Ember".bright_blue(), preview, suffix);
            }
            ember_llm::Role::Tool => {
                let preview: String = msg.content.chars().take(60).collect();
                let suffix = if msg.content.len() > 60 { "..." } else { "" };
                println!(
                    "   {}: {}{}",
                    "[tool result]".dimmed(),
                    preview.dimmed(),
                    suffix
                );
            }
            _ => {}
        }
    }
    println!();
}

pub fn print_final_response(content: &str) {
    println!("{}", content);
}

pub fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

pub fn truncate_json(value: &serde_json::Value, max_len: usize) -> String {
    let s = value.to_string();
    truncate_str(&s, max_len)
}

/// Format a tool call for nice terminal display.
pub fn format_tool_call_display(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "shell" => {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
            format!(
                "{} {}",
                "shell".bright_cyan(),
                format!("`{}`", cmd).bright_white()
            )
        }
        "filesystem" => {
            let op = args
                .get("operation")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let icon = match op {
                "read" => "📖",
                "write" => "✏️",
                "list" => "📁",
                "delete" => "🗑️",
                "search" => "🔍",
                "exists" => "❓",
                _ => "📄",
            };
            format!("{} {} {}", icon, op.bright_cyan(), path.bright_white())
        }
        "git" => {
            let op = args
                .get("operation")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("🔀 {} {}", "git".bright_cyan(), op.bright_white())
        }
        "web" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("?");
            format!(
                "🌐 {} {}",
                "fetch".bright_cyan(),
                truncate_str(url, 60).bright_white()
            )
        }
        _ => {
            format!(
                "{} {}",
                tool_name.bright_cyan(),
                truncate_json(args, 60).dimmed()
            )
        }
    }
}

/// Format tool result for nice terminal display.
pub fn format_tool_result_display(
    tool_name: &str,
    args: &serde_json::Value,
    result: &ember_tools::ToolOutput,
) {
    if result.success {
        match tool_name {
            "filesystem" => {
                let op = args.get("operation").and_then(|v| v.as_str()).unwrap_or("");
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                match op {
                    "read" => {
                        let lines = result.output.lines().count();
                        println!(
                            "  {} Read {} ({} lines)",
                            "✓".bright_green(),
                            path.bright_white(),
                            lines.to_string().bright_green()
                        );
                    }
                    "write" => {
                        println!("  {} Wrote {}", "✓".bright_green(), path.bright_white());
                        // Show inline diff with structured hunk data
                        show_inline_diff(result, path);
                    }
                    "list" => {
                        let entries = result.output.lines().count();
                        println!(
                            "  {} Listed {} ({} entries)",
                            "✓".bright_green(),
                            path.bright_white(),
                            entries.to_string().bright_green()
                        );
                    }
                    "delete" => {
                        println!("  {} Deleted {}", "✓".bright_green(), path.bright_white());
                    }
                    _ => {
                        let preview = truncate_str(&result.output, 80);
                        println!("  {} {}", "✓".bright_green(), preview.dimmed());
                    }
                }
            }
            "shell" => {
                let output = &result.output;
                let lines: Vec<&str> = output.lines().collect();
                let line_count = lines.len();
                if line_count == 0 {
                    println!("  {} (no output)", "✓".bright_green());
                } else if line_count <= 5 {
                    // Show full output for short results
                    for line in &lines {
                        println!("  {} {}", "│".dimmed(), line);
                    }
                } else {
                    // Show first 3 + last 2 lines for longer output
                    for line in &lines[..3] {
                        println!("  {} {}", "│".dimmed(), line);
                    }
                    println!(
                        "  {} {}",
                        "│".dimmed(),
                        format!("... ({} more lines)", line_count - 5).dimmed()
                    );
                    for line in &lines[line_count - 2..] {
                        println!("  {} {}", "│".dimmed(), line);
                    }
                }
            }
            _ => {
                let preview = truncate_str(&result.output, 100);
                println!("  {} {}", "✓".bright_green(), preview.dimmed());
            }
        }
    } else {
        let preview = truncate_str(&result.output, 100);
        println!("  {} {}", "✗".bright_red(), preview);
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Inline diff display for file writes
// ──────────────────────────────────────────────────────────────────────────────

/// Show a colored inline diff when a file is written.
/// Uses structured hunk data from ToolOutput.data when available, falls back to file preview.
pub fn show_inline_diff(tool_output: &ember_tools::ToolOutput, path: &str) {
    // Try to extract structured diff hunks from tool data
    if let Some(ref data) = tool_output.data {
        let lines_added = data
            .get("lines_added")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let lines_removed = data
            .get("lines_removed")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let created = data
            .get("created")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if created {
            println!(
                "  {} new file (+{} lines)",
                "│".dimmed(),
                lines_added.to_string().bright_green()
            );
        } else {
            println!(
                "  {} {} {}",
                "│".dimmed(),
                format!("+{}", lines_added).bright_green(),
                format!("-{}", lines_removed).bright_red(),
            );
        }

        // Render hunks if present
        if let Some(hunks) = data.get("hunks").and_then(|v| v.as_array()) {
            let max_hunks = 8;
            for (i, hunk) in hunks.iter().enumerate() {
                if i >= max_hunks {
                    println!(
                        "  {} {}",
                        "│".dimmed(),
                        format!("... {} more hunk(s)", hunks.len() - max_hunks).dimmed()
                    );
                    break;
                }
                if let Some(lines) = hunk.get("lines").and_then(|v| v.as_array()) {
                    for line in lines {
                        if let Some(obj) = line.as_object() {
                            if let Some(text) = obj.get("Added").and_then(|v| v.as_str()) {
                                let display = if text.len() > 100 { &text[..97] } else { text };
                                println!("  {} {}", "+".bright_green(), display.bright_green());
                            } else if let Some(text) = obj.get("Removed").and_then(|v| v.as_str()) {
                                let display = if text.len() > 100 { &text[..97] } else { text };
                                println!("  {} {}", "-".bright_red(), display.bright_red());
                            }
                            // Skip Context lines to keep output compact
                        }
                    }
                }
            }
        }
        return;
    }

    // Fallback: read the file and show preview
    let written_path = std::path::Path::new(path);
    if let Ok(content) = std::fs::read_to_string(written_path) {
        let line_count = content.lines().count();
        let size = content.len();
        let size_str = if size > 1024 * 1024 {
            format!("{:.1}MB", size as f64 / (1024.0 * 1024.0))
        } else if size > 1024 {
            format!("{:.1}KB", size as f64 / 1024.0)
        } else {
            format!("{}B", size)
        };
        println!(
            "  {} {} lines, {}",
            "│".dimmed(),
            line_count.to_string().bright_cyan(),
            size_str.dimmed()
        );
        let preview_lines: Vec<&str> = content.lines().take(5).collect();
        for line in &preview_lines {
            let display_line = if line.len() > 80 {
                format!("{}...", &line[..77])
            } else {
                line.to_string()
            };
            println!("  {} {}", "+".bright_green(), display_line.bright_green());
        }
        if line_count > 5 {
            println!(
                "  {} {}",
                "│".dimmed(),
                format!("... {} more lines", line_count - 5).dimmed()
            );
        }
    }
}
