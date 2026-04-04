//! Ember's persistent learning memory system.
//!
//! Stores observations, corrections, and learned preferences from
//! conversations. The memory is periodically consolidated and injected
//! into the system prompt so Ember continuously improves.
//!
//! Storage: `~/.ember/memory/`
//!   - observations.jsonl  — raw observations (append-only)
//!   - consolidated.md     — summarised knowledge (periodically rebuilt)
//!   - stats.toml          — memory statistics

use anyhow::Result;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Maximum observations before auto-consolidation.
const MAX_OBSERVATIONS_BEFORE_CONSOLIDATE: usize = 50;

// ──────────────────────────────────────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────────────────────────────────────

/// A single memory observation from a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// ISO-8601 timestamp.
    pub timestamp: String,
    /// Category of the observation.
    pub category: ObservationCategory,
    /// The content / what was learned.
    pub content: String,
    /// Confidence level (0.0 - 1.0).
    pub confidence: f32,
    /// Source context (e.g. what the user said).
    pub context: String,
}

/// Categories of things Ember can learn.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ObservationCategory {
    /// User corrected Ember's output.
    Correction,
    /// User expressed a preference (style, format, etc.).
    Preference,
    /// A coding pattern the user likes.
    CodingPattern,
    /// A tool/framework preference.
    ToolPreference,
    /// Communication style feedback.
    StyleFeedback,
    /// Project-specific knowledge.
    ProjectKnowledge,
    /// General fact about the user.
    UserFact,
}

impl std::fmt::Display for ObservationCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Correction => write!(f, "correction"),
            Self::Preference => write!(f, "preference"),
            Self::CodingPattern => write!(f, "coding_pattern"),
            Self::ToolPreference => write!(f, "tool_preference"),
            Self::StyleFeedback => write!(f, "style_feedback"),
            Self::ProjectKnowledge => write!(f, "project_knowledge"),
            Self::UserFact => write!(f, "user_fact"),
        }
    }
}

/// Memory statistics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryStats {
    pub total_observations: usize,
    pub corrections: usize,
    pub preferences: usize,
    pub patterns: usize,
    pub last_consolidated: String,
    pub consolidation_count: usize,
}

// ──────────────────────────────────────────────────────────────────────────────
// Memory Manager
// ──────────────────────────────────────────────────────────────────────────────

/// The main memory manager that handles storing and retrieving learned knowledge.
pub struct MemoryManager {
    memory_dir: PathBuf,
}

impl MemoryManager {
    /// Create a new memory manager, initializing the storage directory.
    pub fn new() -> Option<Self> {
        let home = dirs::home_dir()?;
        let memory_dir = home.join(".ember").join("memory");
        std::fs::create_dir_all(&memory_dir).ok()?;
        Some(Self { memory_dir })
    }

    /// Path to the observations file.
    fn observations_path(&self) -> PathBuf {
        self.memory_dir.join("observations.jsonl")
    }

    /// Path to the consolidated knowledge file.
    fn consolidated_path(&self) -> PathBuf {
        self.memory_dir.join("consolidated.md")
    }

    /// Path to the stats file.
    fn stats_path(&self) -> PathBuf {
        self.memory_dir.join("stats.toml")
    }

    /// Record a new observation.
    pub fn observe(
        &self,
        category: ObservationCategory,
        content: &str,
        context: &str,
        confidence: f32,
    ) -> Result<()> {
        let obs = Observation {
            timestamp: now_iso8601(),
            category,
            content: content.to_string(),
            confidence,
            context: context.to_string(),
        };

        // Append to JSONL file
        let line = serde_json::to_string(&obs)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.observations_path())?;
        writeln!(file, "{}", line)?;

        // Update stats
        let mut stats = self.load_stats();
        stats.total_observations += 1;
        match obs.category {
            ObservationCategory::Correction => stats.corrections += 1,
            ObservationCategory::Preference | ObservationCategory::StyleFeedback => {
                stats.preferences += 1
            }
            ObservationCategory::CodingPattern | ObservationCategory::ToolPreference => {
                stats.patterns += 1
            }
            _ => {}
        }
        self.save_stats(&stats)?;

        // Auto-consolidate if needed
        if stats
            .total_observations
            .is_multiple_of(MAX_OBSERVATIONS_BEFORE_CONSOLIDATE)
        {
            self.consolidate()?;
        }

        Ok(())
    }

    /// Load all observations.
    pub fn load_observations(&self) -> Vec<Observation> {
        let path = self.observations_path();
        if !path.exists() {
            return Vec::new();
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        content
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    /// Load the consolidated knowledge (for injection into system prompt).
    pub fn load_consolidated(&self) -> Option<String> {
        let path = self.consolidated_path();
        if !path.exists() {
            return None;
        }
        std::fs::read_to_string(&path).ok()
    }

    /// Load memory stats.
    pub fn load_stats(&self) -> MemoryStats {
        let path = self.stats_path();
        if !path.exists() {
            return MemoryStats::default();
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return MemoryStats::default(),
        };
        toml::from_str(&content).unwrap_or_default()
    }

    /// Save memory stats.
    fn save_stats(&self, stats: &MemoryStats) -> Result<()> {
        let content = toml::to_string_pretty(stats)?;
        std::fs::write(self.stats_path(), content)?;
        Ok(())
    }

    /// Consolidate observations into a summary markdown file.
    /// This groups observations by category and creates a structured knowledge base.
    pub fn consolidate(&self) -> Result<()> {
        let observations = self.load_observations();
        if observations.is_empty() {
            return Ok(());
        }

        let mut corrections: Vec<&Observation> = Vec::new();
        let mut preferences: Vec<&Observation> = Vec::new();
        let mut patterns: Vec<&Observation> = Vec::new();
        let mut facts: Vec<&Observation> = Vec::new();
        let mut project: Vec<&Observation> = Vec::new();

        for obs in &observations {
            match obs.category {
                ObservationCategory::Correction => corrections.push(obs),
                ObservationCategory::Preference | ObservationCategory::StyleFeedback => {
                    preferences.push(obs)
                }
                ObservationCategory::CodingPattern | ObservationCategory::ToolPreference => {
                    patterns.push(obs)
                }
                ObservationCategory::UserFact => facts.push(obs),
                ObservationCategory::ProjectKnowledge => project.push(obs),
            }
        }

        let mut md = String::new();
        md.push_str("# Ember's Learned Knowledge\n\n");
        md.push_str(&format!("_Last consolidated: {}_\n\n", now_iso8601()));

        if !facts.is_empty() {
            md.push_str("## About the User\n\n");
            for obs in &facts {
                md.push_str(&format!("- {}\n", obs.content));
            }
            md.push('\n');
        }

        if !preferences.is_empty() {
            md.push_str("## Preferences & Style\n\n");
            // Deduplicate by taking the latest preference for similar content
            let mut seen = std::collections::HashSet::new();
            for obs in preferences.iter().rev() {
                let key = obs.content.to_lowercase();
                if seen.insert(key) {
                    md.push_str(&format!("- {}\n", obs.content));
                }
            }
            md.push('\n');
        }

        if !patterns.is_empty() {
            md.push_str("## Coding Patterns & Tools\n\n");
            let mut seen = std::collections::HashSet::new();
            for obs in patterns.iter().rev() {
                let key = obs.content.to_lowercase();
                if seen.insert(key) {
                    md.push_str(&format!("- {}\n", obs.content));
                }
            }
            md.push('\n');
        }

        if !corrections.is_empty() {
            md.push_str("## Learned Corrections\n\n");
            md.push_str("_Things I got wrong and should avoid:_\n\n");
            let mut seen = std::collections::HashSet::new();
            for obs in corrections.iter().rev() {
                let key = obs.content.to_lowercase();
                if seen.insert(key) {
                    md.push_str(&format!("- {}\n", obs.content));
                }
            }
            md.push('\n');
        }

        if !project.is_empty() {
            md.push_str("## Project Knowledge\n\n");
            for obs in &project {
                md.push_str(&format!("- {}\n", obs.content));
            }
            md.push('\n');
        }

        std::fs::write(self.consolidated_path(), &md)?;

        // Update stats
        let mut stats = self.load_stats();
        stats.last_consolidated = now_iso8601();
        stats.consolidation_count += 1;
        self.save_stats(&stats)?;

        Ok(())
    }

    /// Generate the memory context for the system prompt.
    /// Returns None if there's nothing learned yet.
    pub fn to_system_context(&self) -> Option<String> {
        // First try consolidated knowledge
        if let Some(consolidated) = self.load_consolidated() {
            if !consolidated.trim().is_empty() {
                return Some(format!(
                    "## Ember's Memory (Learned from past interactions)\n\n{}\n\n\
                     IMPORTANT: Use this knowledge to improve your responses. \
                     If the user corrects you, acknowledge it and adapt.",
                    consolidated
                ));
            }
        }

        // Fall back to recent observations
        let observations = self.load_observations();
        if observations.is_empty() {
            return None;
        }

        // Take the last 20 observations
        let recent: Vec<_> = observations.iter().rev().take(20).collect();
        let mut lines = Vec::new();
        lines.push("## Ember's Memory (Recent observations)".to_string());
        for obs in recent.iter().rev() {
            lines.push(format!("- [{}] {}", obs.category, obs.content));
        }
        lines.push(String::new());
        lines.push("IMPORTANT: Use this knowledge to improve your responses.".to_string());

        Some(lines.join("\n"))
    }

    /// Clear all memory.
    pub fn clear(&self) -> Result<()> {
        let _ = std::fs::remove_file(self.observations_path());
        let _ = std::fs::remove_file(self.consolidated_path());
        let _ = std::fs::remove_file(self.stats_path());
        Ok(())
    }

    /// Print a formatted memory status.
    pub fn print_status(&self) {
        let stats = self.load_stats();
        println!();
        println!("{}", "Ember's Memory".bright_yellow().bold());
        println!(
            "   Total observations: {}",
            stats.total_observations.to_string().bright_green()
        );
        println!(
            "   Corrections:        {}",
            stats.corrections.to_string().bright_cyan()
        );
        println!(
            "   Preferences:        {}",
            stats.preferences.to_string().bright_cyan()
        );
        println!(
            "   Patterns:           {}",
            stats.patterns.to_string().bright_cyan()
        );
        if stats.consolidation_count > 0 {
            println!(
                "   Consolidations:     {}",
                stats.consolidation_count.to_string().bright_blue()
            );
            println!(
                "   Last consolidated:  {}",
                stats.last_consolidated.dimmed()
            );
        }
        println!();
    }
}

/// Auto-detect observations from a conversation exchange.
/// This analyses the user message + assistant response to extract learnable knowledge.
pub fn extract_observations(
    user_msg: &str,
    _assistant_msg: &str,
) -> Vec<(ObservationCategory, String)> {
    let mut observations = Vec::new();
    let lower = user_msg.to_lowercase();

    // Detect corrections
    if lower.contains("no, ")
        || lower.contains("nein,")
        || lower.contains("falsch")
        || lower.contains("wrong")
        || lower.contains("that's not")
        || lower.contains("das stimmt nicht")
        || lower.contains("nicht richtig")
        || lower.starts_with("actually")
    {
        observations.push((
            ObservationCategory::Correction,
            format!("User corrected: {}", truncate(user_msg, 200)),
        ));
    }

    // Detect preferences
    if lower.contains("i prefer")
        || lower.contains("ich bevorzuge")
        || lower.contains("ich mag")
        || lower.contains("i like")
        || lower.contains("please always")
        || lower.contains("bitte immer")
        || lower.contains("i want")
        || lower.contains("ich will")
    {
        observations.push((
            ObservationCategory::Preference,
            format!("User preference: {}", truncate(user_msg, 200)),
        ));
    }

    // Detect style feedback
    if lower.contains("too verbose")
        || lower.contains("zu lang")
        || lower.contains("kürzer")
        || lower.contains("shorter")
        || lower.contains("more detail")
        || lower.contains("mehr detail")
        || lower.contains("too short")
        || lower.contains("zu kurz")
    {
        observations.push((
            ObservationCategory::StyleFeedback,
            format!("Style feedback: {}", truncate(user_msg, 200)),
        ));
    }

    // Detect tool/framework preferences
    if lower.contains("use ")
        && (lower.contains("instead")
            || lower.contains("stattdessen")
            || lower.contains("rather than")
            || lower.contains("anstatt"))
    {
        observations.push((
            ObservationCategory::ToolPreference,
            format!("Tool preference: {}", truncate(user_msg, 200)),
        ));
    }

    observations
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

fn now_iso8601() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let s = secs;
    let sec = (s % 60) as u32;
    let s = s / 60;
    let min = (s % 60) as u32;
    let s = s / 60;
    let hour = (s % 24) as u32;
    let s = s / 24;
    let mut days = s as u32;
    let mut y = 1970u32;
    loop {
        let dy = if (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400) {
            366
        } else {
            365
        };
        if days < dy {
            break;
        }
        days -= dy;
        y += 1;
    }
    let leap = (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
    let md = [
        31u32,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut mo = 0u32;
    for d in &md {
        if days < *d {
            break;
        }
        days -= d;
        mo += 1;
    }
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        mo + 1,
        days + 1,
        hour,
        min,
        sec
    )
}
