//! Slash command system for the Ember REPL.
//!
//! Provides parsing, a command registry with descriptions, and a completion
//! helper so the interactive chat loop can handle `/help`, `/model`, etc.
//!
//! # Example
//!
//! ```rust
//! use ember_cli::commands::slash::SlashCommand;
//!
//! let cmd = SlashCommand::parse("/model gpt-4o");
//! assert_eq!(cmd, Some(SlashCommand::Model { model: Some("gpt-4o".into()) }));
//! ```

#![allow(dead_code)]

// ──────────────────────────────────────────────────────────────────────────────
// SlashCommand enum
// ──────────────────────────────────────────────────────────────────────────────

/// A parsed slash command from user input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    /// Show all commands or help for a specific command.
    Help,
    /// Show session stats (turns, tokens, cost).
    Status,
    /// Force session compaction.
    Compact,
    /// Show or change the current model.
    Model { model: Option<String> },
    /// Show or change the permission mode.
    Permissions { mode: Option<String> },
    /// Show configuration, optionally filtered to a section.
    Config { section: Option<String> },
    /// Show memory / context usage.
    Memory,
    /// Clear the conversation. `confirm` is `true` when the user already
    /// confirmed (e.g. `/clear --yes`).
    Clear { confirm: bool },
    /// Show cost breakdown.
    Cost,
    /// Fork the current session, optionally with a name.
    Fork { name: Option<String> },
    /// List all session forks.
    Forks,
    /// Restore to a named / numbered fork point.
    Restore { fork_id: String },
    /// Compare two providers on the same prompt.
    /// `provider1` and `provider2` are optional; when absent, uses current +
    /// next-cheapest.  `prompt` is the text to send.
    Compare {
        provider1: Option<String>,
        provider2: Option<String>,
        prompt: String,
    },
    /// Show or clear the semantic cache.
    /// When `subcommand` is `Some("clear")`, clears the cache.
    Cache { subcommand: Option<String> },
    /// Undo the last file write performed by a tool.
    Undo,
    /// Create a git commit. `message` is the commit message; if `None` an
    /// auto-generated message is used.
    Commit { message: Option<String> },
    /// Show the current `git diff` of unstaged (or staged) changes.
    Diff { staged: bool },
    /// Toggle Plan Mode (read-only: propose changes without executing).
    Plan,
    /// Execute the last proposed plan.
    Execute,
    /// Save a named checkpoint of the conversation state.
    Checkpoint { name: Option<String> },
    /// List all checkpoints.
    Checkpoints,
    /// Show a summary of the current session as a replayable log.
    Replay,
    /// Run a built-in benchmark against multiple providers/models.
    Bench { task: Option<String> },
    /// Show what Ember has learned about your coding preferences.
    Learn { subcommand: Option<String> },
    /// Switch terminal color theme: dark, light, neon.
    Theme { theme: Option<String> },
    /// Show buddy stats and XP progress.
    Buddy,
    /// Auto-generate EMBER.md project context file.
    Init,
    /// Add files to conversation context.
    Add { paths: Vec<String> },
    /// An unrecognised slash command — stores the raw name.
    Unknown(String),
}

impl SlashCommand {
    /// Parse a slash command from a line of user input.
    ///
    /// Returns `None` when the input does not start with `/`.
    /// Returns `Some(SlashCommand::Unknown(_))` for unrecognised commands.
    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        if !input.starts_with('/') {
            return None;
        }

        // Split into command name and optional rest.
        let without_slash = &input[1..];
        let (name, rest) = match without_slash.find(char::is_whitespace) {
            Some(pos) => (&without_slash[..pos], without_slash[pos..].trim()),
            None => (without_slash, ""),
        };

        let arg = if rest.is_empty() { None } else { Some(rest) };

        let cmd = match name.to_lowercase().as_str() {
            "help" | "h" => SlashCommand::Help,
            "status" => SlashCommand::Status,
            "compact" => SlashCommand::Compact,
            "model" | "m" => SlashCommand::Model {
                model: arg.map(str::to_owned),
            },
            "permissions" | "perm" => SlashCommand::Permissions {
                mode: arg.map(str::to_owned),
            },
            "config" | "cfg" => SlashCommand::Config {
                section: arg.map(str::to_owned),
            },
            "memory" | "mem" => SlashCommand::Memory,
            "clear" | "c" => {
                let confirm = arg
                    .map(|a| matches!(a.to_lowercase().as_str(), "--yes" | "-y" | "yes"))
                    .unwrap_or(false);
                SlashCommand::Clear { confirm }
            }
            "cost" => SlashCommand::Cost,
            "fork" => SlashCommand::Fork {
                name: arg.map(str::to_owned),
            },
            "forks" => SlashCommand::Forks,
            "restore" => {
                let fork_id = arg.unwrap_or("").to_owned();
                SlashCommand::Restore { fork_id }
            }
            "compare" => {
                // Syntax variants:
                //   /compare "prompt"
                //   /compare provider1 provider2 "prompt"
                //   /compare provider1 provider2 prompt text...
                let rest = arg.unwrap_or("").trim().to_owned();
                let (provider1, provider2, prompt) = parse_compare_args(&rest);
                SlashCommand::Compare {
                    provider1,
                    provider2,
                    prompt,
                }
            }
            "cache" => SlashCommand::Cache {
                subcommand: arg.map(|a| a.trim().to_lowercase()),
            },
            "undo" | "u" => SlashCommand::Undo,
            "commit" => SlashCommand::Commit {
                message: arg.map(str::to_owned),
            },
            "diff" => {
                let staged = arg
                    .map(|a| {
                        matches!(
                            a.to_lowercase().as_str(),
                            "--staged" | "--cached" | "staged" | "cached"
                        )
                    })
                    .unwrap_or(false);
                SlashCommand::Diff { staged }
            }
            "plan" => SlashCommand::Plan,
            "execute" | "exec" | "x" => SlashCommand::Execute,
            "checkpoint" | "cp" => SlashCommand::Checkpoint {
                name: arg.map(str::to_owned),
            },
            "checkpoints" | "cps" => SlashCommand::Checkpoints,
            "replay" => SlashCommand::Replay,
            "bench" | "benchmark" => SlashCommand::Bench {
                task: arg.map(str::to_owned),
            },
            "learn" => SlashCommand::Learn {
                subcommand: arg.map(|a| a.trim().to_lowercase()),
            },
            "theme" => SlashCommand::Theme {
                theme: arg.map(|a| a.trim().to_lowercase()),
            },
            "buddy" | "pet" | "xp" => SlashCommand::Buddy,
            "init" => SlashCommand::Init,
            "add" | "a" => {
                let paths = arg
                    .map(|a| a.split_whitespace().map(str::to_owned).collect())
                    .unwrap_or_default();
                SlashCommand::Add { paths }
            }
            other => SlashCommand::Unknown(other.to_owned()),
        };

        Some(cmd)
    }

    /// Return the canonical names of all known commands (without the `/`
    /// prefix), suitable for tab-completion lookups.
    pub fn all_commands() -> Vec<&'static str> {
        vec![
            "help",
            "status",
            "compact",
            "model",
            "permissions",
            "config",
            "memory",
            "clear",
            "cost",
            "fork",
            "forks",
            "restore",
            "compare",
            "cache",
            "undo",
            "commit",
            "diff",
            "plan",
            "execute",
            "checkpoint",
            "checkpoints",
            "replay",
            "bench",
            "learn",
            "theme",
            "buddy",
            "init",
            "add",
        ]
    }

    /// A short one-line description of what this command does.
    /// Look up this command's description from the registry (single source of truth).
    pub fn help_text(&self) -> &'static str {
        let name = match self {
            SlashCommand::Help => "help",
            SlashCommand::Status => "status",
            SlashCommand::Compact => "compact",
            SlashCommand::Model { .. } => "model",
            SlashCommand::Permissions { .. } => "permissions",
            SlashCommand::Config { .. } => "config",
            SlashCommand::Memory => "memory",
            SlashCommand::Clear { .. } => "clear",
            SlashCommand::Cost => "cost",
            SlashCommand::Fork { .. } => "fork",
            SlashCommand::Forks => "forks",
            SlashCommand::Restore { .. } => "restore",
            SlashCommand::Compare { .. } => "compare",
            SlashCommand::Cache { .. } => "cache",
            SlashCommand::Undo => "undo",
            SlashCommand::Commit { .. } => "commit",
            SlashCommand::Diff { .. } => "diff",
            SlashCommand::Plan => "plan",
            SlashCommand::Execute => "execute",
            SlashCommand::Checkpoint { .. } => "checkpoint",
            SlashCommand::Checkpoints => "checkpoints",
            SlashCommand::Replay => "replay",
            SlashCommand::Bench { .. } => "bench",
            SlashCommand::Learn { .. } => "learn",
            SlashCommand::Theme { .. } => "theme",
            SlashCommand::Buddy => "buddy",
            SlashCommand::Init => "init",
            SlashCommand::Add { .. } => "add",
            SlashCommand::Unknown(_) => return "Unknown command",
        };
        // Use a leaked registry lookup so we can return &'static str.
        // The registry is small and this is only called for help display.
        let reg = SlashCommandRegistry::new();
        reg.find(name)
            .map(|e| e.description)
            .unwrap_or("No description")
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// parse_compare_args helper
// ──────────────────────────────────────────────────────────────────────────────

/// Parse the argument string of a `/compare` command into
/// `(provider1, provider2, prompt)`.
///
/// Accepted forms (all tokens separated by whitespace):
/// - `"prompt text"` — no providers, quoted prompt
/// - `prompt text without quotes`
/// - `openai ollama "prompt text"`
/// - `openai ollama prompt text`
fn parse_compare_args(rest: &str) -> (Option<String>, Option<String>, String) {
    // If the whole rest is quoted, it's just a prompt.
    if rest.starts_with('"') {
        let prompt = rest.trim_matches('"').to_owned();
        return (None, None, prompt);
    }

    let parts: Vec<&str> = rest.splitn(3, char::is_whitespace).collect();

    // Known provider names (lowercase).
    let known_providers = [
        "openai",
        "ollama",
        "anthropic",
        "gemini",
        "groq",
        "deepseek",
        "mistral",
    ];

    let looks_like_provider = |s: &str| known_providers.contains(&s.to_lowercase().as_str());

    match parts.as_slice() {
        // No args → empty prompt, caller should warn
        [] => (None, None, String::new()),
        // Single token or single quoted string
        [single] => (None, None, single.trim_matches('"').to_owned()),
        // Two tokens — could be provider + prompt, or just two-word prompt
        [a, b] => {
            if looks_like_provider(a) {
                // One provider + prompt
                (Some((*a).to_owned()), None, b.trim_matches('"').to_owned())
            } else {
                (None, None, format!("{} {}", a, b.trim_matches('"')))
            }
        }
        // Three (or more, due to splitn(3)) tokens
        [a, b, c_rest] => {
            if looks_like_provider(a) && looks_like_provider(b) {
                (
                    Some((*a).to_owned()),
                    Some((*b).to_owned()),
                    c_rest.trim_matches('"').to_owned(),
                )
            } else if looks_like_provider(a) {
                (
                    Some((*a).to_owned()),
                    None,
                    format!("{} {}", b, c_rest.trim_matches('"')),
                )
            } else {
                (
                    None,
                    None,
                    format!("{} {} {}", a, b, c_rest.trim_matches('"')),
                )
            }
        }
        // Fallback for any other shape (shouldn't happen with splitn(3) but
        // keeps the compiler happy).
        _ => (None, None, rest.to_owned()),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Registry
// ──────────────────────────────────────────────────────────────────────────────

/// Metadata entry for a single slash command.
pub struct SlashCommandEntry {
    /// Canonical name without the `/` prefix.
    pub name: &'static str,
    /// One-line description shown in `/help` output.
    pub description: &'static str,
    /// Usage string, e.g. `"/model [name]"`.
    pub usage: &'static str,
    /// Alternative names (without `/`) that map to the same command.
    pub aliases: Vec<&'static str>,
    /// Category for grouping in help output.
    pub category: &'static str,
}

/// Registry of all slash commands, used for display and tab-completion.
pub struct SlashCommandRegistry {
    commands: Vec<SlashCommandEntry>,
}

impl Default for SlashCommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SlashCommandRegistry {
    /// Create a registry pre-populated with all built-in slash commands.
    pub fn new() -> Self {
        let commands = vec![
            // ── Session ──
            SlashCommandEntry {
                name: "help",
                description: "Show all commands",
                usage: "/help [command]",
                aliases: vec!["h"],
                category: "Session",
            },
            SlashCommandEntry {
                name: "status",
                description: "Session stats (turns, tokens, cost)",
                usage: "/status",
                aliases: vec![],
                category: "Session",
            },
            SlashCommandEntry {
                name: "cost",
                description: "Cost breakdown for this session",
                usage: "/cost",
                aliases: vec![],
                category: "Session",
            },
            SlashCommandEntry {
                name: "memory",
                description: "Context window usage",
                usage: "/memory",
                aliases: vec!["mem"],
                category: "Session",
            },
            SlashCommandEntry {
                name: "clear",
                description: "Clear conversation history",
                usage: "/clear [--yes]",
                aliases: vec!["c"],
                category: "Session",
            },
            SlashCommandEntry {
                name: "model",
                description: "Show/change model (fast, smart, code, local)",
                usage: "/model [name]",
                aliases: vec!["m"],
                category: "Session",
            },
            SlashCommandEntry {
                name: "compact",
                description: "Force context compaction",
                usage: "/compact",
                aliases: vec![],
                category: "Session",
            },
            SlashCommandEntry {
                name: "config",
                description: "Display configuration",
                usage: "/config [section]",
                aliases: vec!["cfg"],
                category: "Session",
            },
            SlashCommandEntry {
                name: "permissions",
                description: "Show/change permission mode",
                usage: "/permissions [mode]",
                aliases: vec!["perm"],
                category: "Session",
            },
            // ── Git & Files ──
            SlashCommandEntry {
                name: "undo",
                description: "Undo the last file write",
                usage: "/undo",
                aliases: vec!["u"],
                category: "Git & Files",
            },
            SlashCommandEntry {
                name: "commit",
                description: "Auto git commit",
                usage: "/commit [message]",
                aliases: vec![],
                category: "Git & Files",
            },
            SlashCommandEntry {
                name: "diff",
                description: "Show git diff",
                usage: "/diff [--staged]",
                aliases: vec![],
                category: "Git & Files",
            },
            // ── Planning ──
            SlashCommandEntry {
                name: "plan",
                description: "Toggle plan mode (propose without executing)",
                usage: "/plan",
                aliases: vec![],
                category: "Planning",
            },
            SlashCommandEntry {
                name: "execute",
                description: "Execute the proposed plan",
                usage: "/execute",
                aliases: vec!["exec", "x"],
                category: "Planning",
            },
            // ── Checkpoints ──
            SlashCommandEntry {
                name: "fork",
                description: "Fork the session",
                usage: "/fork [name]",
                aliases: vec![],
                category: "Checkpoints",
            },
            SlashCommandEntry {
                name: "forks",
                description: "List session forks",
                usage: "/forks",
                aliases: vec![],
                category: "Checkpoints",
            },
            SlashCommandEntry {
                name: "restore",
                description: "Restore a fork point",
                usage: "/restore <id>",
                aliases: vec![],
                category: "Checkpoints",
            },
            SlashCommandEntry {
                name: "checkpoint",
                description: "Save a conversation checkpoint",
                usage: "/checkpoint [name]",
                aliases: vec!["cp"],
                category: "Checkpoints",
            },
            SlashCommandEntry {
                name: "checkpoints",
                description: "List saved checkpoints",
                usage: "/checkpoints",
                aliases: vec!["cps"],
                category: "Checkpoints",
            },
            SlashCommandEntry {
                name: "replay",
                description: "Show session replay log",
                usage: "/replay",
                aliases: vec![],
                category: "Checkpoints",
            },
            // ── Advanced ──
            SlashCommandEntry {
                name: "compare",
                description: "Compare two providers side-by-side",
                usage: "/compare [p1] [p2] <prompt>",
                aliases: vec![],
                category: "Advanced",
            },
            SlashCommandEntry {
                name: "cache",
                description: "Semantic cache stats or clear",
                usage: "/cache [clear]",
                aliases: vec![],
                category: "Advanced",
            },
            SlashCommandEntry {
                name: "bench",
                description: "Benchmark across providers",
                usage: "/bench [task]",
                aliases: vec!["benchmark"],
                category: "Advanced",
            },
            SlashCommandEntry {
                name: "learn",
                description: "Show/reset learned preferences",
                usage: "/learn [reset|show]",
                aliases: vec![],
                category: "Advanced",
            },
            SlashCommandEntry {
                name: "theme",
                description: "Switch color theme: dark, light, neon",
                usage: "/theme [dark|light|neon]",
                aliases: vec![],
                category: "Session",
            },
            SlashCommandEntry {
                name: "buddy",
                description: "Show your coding buddy's stats and XP",
                usage: "/buddy",
                aliases: vec!["pet", "xp"],
                category: "Advanced",
            },
            // ── Context ──
            SlashCommandEntry {
                name: "init",
                description: "Auto-generate EMBER.md project rules",
                usage: "/init",
                aliases: vec![],
                category: "Git & Files",
            },
            SlashCommandEntry {
                name: "add",
                description: "Add files to conversation context",
                usage: "/add <file1> [file2] ...",
                aliases: vec!["a"],
                category: "Git & Files",
            },
        ];

        Self { commands }
    }

    /// Return a slice of all registered command entries.
    pub fn commands(&self) -> &[SlashCommandEntry] {
        &self.commands
    }

    /// Find a command entry by its canonical name or any alias.
    /// The lookup is case-insensitive and does **not** require a leading `/`.
    pub fn find(&self, name: &str) -> Option<&SlashCommandEntry> {
        let needle = name.trim_start_matches('/').to_lowercase();
        self.commands.iter().find(|entry| {
            entry.name.eq_ignore_ascii_case(&needle)
                || entry
                    .aliases
                    .iter()
                    .any(|a| a.eq_ignore_ascii_case(&needle))
        })
    }

    /// Return the `/`-prefixed names of all commands that start with `partial`.
    ///
    /// `partial` may or may not include the leading `/`.
    pub fn completions_for(&self, partial: &str) -> Vec<String> {
        let needle = partial.trim_start_matches('/').to_lowercase();
        self.commands
            .iter()
            .filter(|entry| entry.name.starts_with(needle.as_str()))
            .map(|entry| format!("/{}", entry.name))
            .collect()
    }

    /// Render a formatted help string listing all commands grouped by category.
    pub fn format_help(&self) -> String {
        use std::collections::BTreeMap;

        let mut groups: BTreeMap<&str, Vec<&SlashCommandEntry>> = BTreeMap::new();
        for entry in &self.commands {
            groups.entry(entry.category).or_default().push(entry);
        }

        let mut out = String::new();
        for (category, entries) in &groups {
            out.push_str(&format!("  \x1b[1;33m{}\x1b[0m\n", category));
            for entry in entries {
                let aliases = if entry.aliases.is_empty() {
                    String::new()
                } else {
                    format!(
                        " \x1b[2m({})\x1b[0m",
                        entry
                            .aliases
                            .iter()
                            .map(|a| format!("/{a}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                out.push_str(&format!(
                    "    \x1b[36m{:<20}\x1b[0m {}{}\n",
                    entry.usage, entry.description, aliases
                ));
            }
            out.push('\n');
        }
        out
    }

    /// Find the closest command name to `input` for "did you mean?" suggestions.
    pub fn suggest(&self, input: &str) -> Option<String> {
        let needle = input.trim_start_matches('/').to_lowercase();
        if needle.is_empty() {
            return None;
        }

        let mut best: Option<(&str, usize)> = None;
        for entry in &self.commands {
            let dist = edit_distance(&needle, entry.name);
            if dist <= 2 && (best.is_none() || dist < best.unwrap().1) {
                best = Some((entry.name, dist));
            }
            for alias in &entry.aliases {
                let dist = edit_distance(&needle, alias);
                if dist <= 2 && (best.is_none() || dist < best.unwrap().1) {
                    best = Some((entry.name, dist));
                }
            }
        }

        best.map(|(name, _)| format!("/{}", name))
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// SlashCompleter
// ──────────────────────────────────────────────────────────────────────────────

/// Tab-completion helper that integrates with rustyline.
///
/// Implements the rustyline `Completer`, `Hinter`, `Highlighter`, `Validator`,
/// and `Helper` traits so it can be plugged directly into a rustyline `Editor`.
pub struct SlashCompleter {
    registry: SlashCommandRegistry,
}

impl Default for SlashCompleter {
    fn default() -> Self {
        Self::new()
    }
}

impl SlashCompleter {
    /// Create a new completer backed by a default [`SlashCommandRegistry`].
    pub fn new() -> Self {
        Self {
            registry: SlashCommandRegistry::new(),
        }
    }

    /// Return completion candidates for `partial`.
    ///
    /// Only returns results when `partial` starts with `/`; returns an empty
    /// `Vec` otherwise so regular prose is never completed as a command.
    pub fn complete(&self, partial: &str) -> Vec<String> {
        if !partial.starts_with('/') {
            return vec![];
        }
        self.registry.completions_for(partial)
    }
}

// ── rustyline trait implementations ─────────────────────────────────────────

use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::Context;
use rustyline::Helper;

impl Completer for SlashCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        // Only complete when the line starts with '/'
        if !line.starts_with('/') {
            return Ok((0, vec![]));
        }

        // Find the start of the slash command token
        let start = 0;
        let partial = &line[start..pos];

        let candidates: Vec<Pair> = self
            .registry
            .completions_for(partial)
            .into_iter()
            .map(|cmd| Pair {
                display: cmd.clone(),
                replacement: format!("{} ", cmd), // add trailing space
            })
            .collect();

        Ok((start, candidates))
    }
}

impl Hinter for SlashCompleter {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<String> {
        if !line.starts_with('/') || pos == 0 {
            return None;
        }
        let candidates = self.registry.completions_for(line);
        if candidates.len() == 1 {
            let full = &candidates[0];
            if full.len() > pos {
                return Some(full[pos..].to_string());
            }
        }
        None
    }
}

impl Highlighter for SlashCompleter {}
impl Validator for SlashCompleter {}
impl Helper for SlashCompleter {}

// ──────────────────────────────────────────────────────────────────────────────
// Edit distance for fuzzy command suggestions
// ──────────────────────────────────────────────────────────────────────────────

/// Simple Levenshtein edit distance for short strings.
fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let (m, n) = (a_chars.len(), b_chars.len());

    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // 1. Parse /help → Help
    #[test]
    fn parse_help() {
        assert_eq!(SlashCommand::parse("/help"), Some(SlashCommand::Help));
    }

    // 2. Parse /model gpt-4o → Model { model: Some("gpt-4o") }
    #[test]
    fn parse_model_with_arg() {
        assert_eq!(
            SlashCommand::parse("/model gpt-4o"),
            Some(SlashCommand::Model {
                model: Some("gpt-4o".into())
            })
        );
    }

    // 3. Parse /model (no arg) → Model { model: None }
    #[test]
    fn parse_model_no_arg() {
        assert_eq!(
            SlashCommand::parse("/model"),
            Some(SlashCommand::Model { model: None })
        );
    }

    // 4. Parse /clear → Clear { confirm: false }
    #[test]
    fn parse_clear_no_confirm() {
        assert_eq!(
            SlashCommand::parse("/clear"),
            Some(SlashCommand::Clear { confirm: false })
        );
    }

    // 5. Parse /clear --yes → Clear { confirm: true }
    #[test]
    fn parse_clear_with_confirm() {
        assert_eq!(
            SlashCommand::parse("/clear --yes"),
            Some(SlashCommand::Clear { confirm: true })
        );
    }

    // 6. Parse an unknown slash command → Unknown
    #[test]
    fn parse_unknown_command() {
        assert!(matches!(
            SlashCommand::parse("/foobar"),
            Some(SlashCommand::Unknown(_))
        ));
    }

    // 7. Parse a non-slash string → None
    #[test]
    fn parse_non_slash_returns_none() {
        assert_eq!(SlashCommand::parse("hello world"), None);
        assert_eq!(SlashCommand::parse("model gpt-4o"), None);
        assert_eq!(SlashCommand::parse(""), None);
    }

    // 8. all_commands returns the correct count (14 commands)
    #[test]
    fn all_commands_count() {
        assert_eq!(SlashCommand::all_commands().len(), 28);
    }

    // 9. Registry completions for "/he" → ["/help"]
    #[test]
    fn registry_completions_he() {
        let reg = SlashCommandRegistry::new();
        let completions = reg.completions_for("/he");
        assert_eq!(completions, vec!["/help"]);
    }

    // 10. Registry completions for "/m" → ["/model", "/memory"]
    #[test]
    fn registry_completions_m() {
        let reg = SlashCommandRegistry::new();
        let mut completions = reg.completions_for("/m");
        completions.sort();
        assert_eq!(completions, vec!["/memory", "/model"]);
    }

    // 11. format_help is non-empty
    #[test]
    fn registry_format_help_non_empty() {
        let reg = SlashCommandRegistry::new();
        let help = reg.format_help();
        assert!(!help.is_empty());
        // Should contain at least one command name
        assert!(help.contains("/help"));
    }

    // 12. help_text returns non-empty for every variant
    #[test]
    fn help_text_non_empty_for_all_variants() {
        let variants: Vec<SlashCommand> = vec![
            SlashCommand::Help,
            SlashCommand::Status,
            SlashCommand::Compact,
            SlashCommand::Model { model: None },
            SlashCommand::Permissions { mode: None },
            SlashCommand::Config { section: None },
            SlashCommand::Memory,
            SlashCommand::Clear { confirm: false },
            SlashCommand::Cost,
            SlashCommand::Fork { name: None },
            SlashCommand::Forks,
            SlashCommand::Restore {
                fork_id: "1".into(),
            },
            SlashCommand::Compare {
                provider1: None,
                provider2: None,
                prompt: "test".into(),
            },
            SlashCommand::Cache { subcommand: None },
            SlashCommand::Commit { message: None },
            SlashCommand::Diff { staged: false },
            SlashCommand::Plan,
            SlashCommand::Execute,
            SlashCommand::Checkpoint { name: None },
            SlashCommand::Checkpoints,
            SlashCommand::Replay,
            SlashCommand::Bench { task: None },
            SlashCommand::Learn { subcommand: None },
            SlashCommand::Theme { theme: None },
            SlashCommand::Buddy,
            SlashCommand::Undo,
            SlashCommand::Init,
            SlashCommand::Add { paths: vec![] },
            SlashCommand::Unknown("xyz".into()),
        ];
        for variant in &variants {
            assert!(
                !variant.help_text().is_empty(),
                "help_text empty for {:?}",
                variant
            );
        }
    }

    // 13. SlashCompleter returns completions for slash input
    #[test]
    fn slash_completer_returns_completions() {
        let completer = SlashCompleter::new();
        let results = completer.complete("/st");
        assert!(results.contains(&"/status".to_string()));
    }

    // 14. SlashCompleter returns nothing for non-slash input
    #[test]
    fn slash_completer_ignores_non_slash() {
        let completer = SlashCompleter::new();
        assert!(completer.complete("help").is_empty());
        assert!(completer.complete("").is_empty());
    }

    // 15. find() resolves canonical name
    #[test]
    fn registry_find_canonical() {
        let reg = SlashCommandRegistry::new();
        assert!(reg.find("help").is_some());
        assert!(reg.find("/help").is_some());
    }

    // 16. find() resolves alias
    #[test]
    fn registry_find_alias() {
        let reg = SlashCommandRegistry::new();
        let by_alias = reg.find("h");
        let by_name = reg.find("help");
        assert!(by_alias.is_some());
        assert_eq!(by_alias.unwrap().name, by_name.unwrap().name);
    }

    // 17. Parse /fork with name
    #[test]
    fn parse_fork_with_name() {
        assert_eq!(
            SlashCommand::parse("/fork my-fork"),
            Some(SlashCommand::Fork {
                name: Some("my-fork".into())
            })
        );
    }

    // 18. Parse /restore with id
    #[test]
    fn parse_restore_with_id() {
        assert_eq!(
            SlashCommand::parse("/restore abc123"),
            Some(SlashCommand::Restore {
                fork_id: "abc123".into()
            })
        );
    }
}
