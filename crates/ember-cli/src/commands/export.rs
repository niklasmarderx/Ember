//! Export command for saving conversations in various formats.

use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::Args;
use colored::Colorize;
use ember_core::{Conversation, ExportFormat};
use std::path::PathBuf;

/// Arguments for the export command.
#[derive(Debug, Args)]
pub struct ExportArgs {
    /// Export format: json, markdown (md), or html
    #[arg(short, long, default_value = "json")]
    pub format: String,

    /// Output file path (auto-generated if not provided)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Conversation ID to export (uses current if not specified)
    #[arg(short, long)]
    pub conversation: Option<String>,

    /// Include system prompt in export
    #[arg(long, default_value = "true")]
    pub include_system: bool,

    /// Provider name to include in metadata
    #[arg(long)]
    pub provider: Option<String>,

    /// Model name to include in metadata
    #[arg(long)]
    pub model: Option<String>,
}

/// Execute the export command.
pub fn run(args: ExportArgs) -> Result<()> {
    // Parse format
    let format = ExportFormat::from_str(&args.format)
        .ok_or_else(|| anyhow::anyhow!(
            "Unknown format '{}'. Supported formats: json, markdown (md), html",
            args.format
        ))?;

    // Load conversation
    let conversation = load_conversation(args.conversation.as_deref())?;

    // Generate output filename if not provided
    let output_path = args.output.unwrap_or_else(|| {
        let timestamp = Utc::now().format("%Y-%m-%d_%H%M%S");
        let filename = format!("conversation_{}.{}", timestamp, format.extension());
        PathBuf::from(filename)
    });

    // Export based on format
    let content = match format {
        ExportFormat::Json => conversation.export_json(
            args.provider.as_deref(),
            args.model.as_deref(),
        ),
        ExportFormat::Markdown => conversation.export_markdown(
            args.provider.as_deref(),
            args.model.as_deref(),
        ),
        ExportFormat::Html => conversation.export_html(
            args.provider.as_deref(),
            args.model.as_deref(),
        ),
    };

    // Write to file
    std::fs::write(&output_path, &content)
        .with_context(|| format!("Failed to write to {}", output_path.display()))?;

    // Print success message
    let format_name = match format {
        ExportFormat::Json => "JSON",
        ExportFormat::Markdown => "Markdown",
        ExportFormat::Html => "HTML",
    };

    println!(
        "{} Conversation exported to {} ({})",
        "[OK]".green(),
        output_path.display().to_string().cyan(),
        format_name
    );

    // Print stats
    println!(
        "     {} turns, {} characters",
        conversation.len(),
        content.len()
    );

    Ok(())
}

/// Export a conversation directly (for use in chat mode).
pub fn export_conversation(
    conversation: &Conversation,
    format_str: &str,
    output: Option<PathBuf>,
    provider: Option<&str>,
    model: Option<&str>,
) -> Result<PathBuf> {
    let format = ExportFormat::from_str(format_str)
        .ok_or_else(|| anyhow::anyhow!(
            "Unknown format '{}'. Supported: json, markdown, html",
            format_str
        ))?;

    let output_path = output.unwrap_or_else(|| {
        let timestamp = Utc::now().format("%Y-%m-%d_%H%M%S");
        let filename = format!("conversation_{}.{}", timestamp, format.extension());
        PathBuf::from(filename)
    });

    let content = match format {
        ExportFormat::Json => conversation.export_json(provider, model),
        ExportFormat::Markdown => conversation.export_markdown(provider, model),
        ExportFormat::Html => conversation.export_html(provider, model),
    };

    std::fs::write(&output_path, content)
        .with_context(|| format!("Failed to write to {}", output_path.display()))?;

    Ok(output_path)
}

/// Load a conversation from storage.
fn load_conversation(id: Option<&str>) -> Result<Conversation> {
    // Try to load from the data directory
    let data_dir = dirs::data_dir()
        .map(|d| d.join("ember").join("conversations"))
        .unwrap_or_else(|| PathBuf::from(".ember/conversations"));

    if let Some(conv_id) = id {
        // Load specific conversation
        let conv_file = data_dir.join(format!("{}.json", conv_id));
        if conv_file.exists() {
            let content = std::fs::read_to_string(&conv_file)
                .with_context(|| format!("Failed to read conversation {}", conv_id))?;
            let conversation: Conversation = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse conversation {}", conv_id))?;
            return Ok(conversation);
        }
        bail!("Conversation '{}' not found", conv_id);
    }

    // Try to load the most recent conversation
    if data_dir.exists() {
        let mut entries: Vec<_> = std::fs::read_dir(&data_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "json")
                    .unwrap_or(false)
            })
            .collect();

        // Sort by modification time (most recent first)
        entries.sort_by(|a, b| {
            let a_time = a.metadata().and_then(|m| m.modified()).ok();
            let b_time = b.metadata().and_then(|m| m.modified()).ok();
            b_time.cmp(&a_time)
        });

        if let Some(entry) = entries.first() {
            let content = std::fs::read_to_string(entry.path())?;
            let conversation: Conversation = serde_json::from_str(&content)?;
            return Ok(conversation);
        }
    }

    // No saved conversations, create a demo conversation
    println!(
        "{} No saved conversations found. Creating demo conversation for export.",
        "[Info]".yellow()
    );

    let mut conv = Conversation::new("You are a helpful assistant.");
    conv.title = Some("Demo Conversation".to_string());

    let turn = conv.start_turn("Hello! Can you help me with something?");
    turn.assistant_response = "Of course! I'd be happy to help. What do you need assistance with?".to_string();
    turn.complete();

    let turn2 = conv.start_turn("What's the weather like today?");
    turn2.assistant_response = "I don't have access to real-time weather data. However, you can check your local weather service or a weather app for current conditions in your area.".to_string();
    turn2.complete();

    Ok(conv)
}

/// List available conversations.
pub fn list_conversations() -> Result<Vec<(String, String, chrono::DateTime<Utc>)>> {
    let data_dir = dirs::data_dir()
        .map(|d| d.join("ember").join("conversations"))
        .unwrap_or_else(|| PathBuf::from(".ember/conversations"));

    if !data_dir.exists() {
        return Ok(Vec::new());
    }

    let mut conversations = Vec::new();

    for entry in std::fs::read_dir(&data_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(conv) = serde_json::from_str::<Conversation>(&content) {
                    let title = conv.title.unwrap_or_else(|| "Untitled".to_string());
                    conversations.push((conv.id.to_string(), title, conv.updated_at));
                }
            }
        }
    }

    // Sort by date (most recent first)
    conversations.sort_by(|a, b| b.2.cmp(&a.2));

    Ok(conversations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_export_json() {
        let mut conv = Conversation::new("Test system prompt");
        let turn = conv.start_turn("Hello");
        turn.assistant_response = "Hi!".to_string();
        turn.complete();

        let json = conv.export_json(Some("test-provider"), Some("test-model"));
        assert!(json.contains("\"role\": \"user\""));
        assert!(json.contains("\"role\": \"assistant\""));
        assert!(json.contains("test-provider"));
        assert!(json.contains("test-model"));
    }

    #[test]
    fn test_export_markdown() {
        let mut conv = Conversation::new("System prompt");
        conv.title = Some("Test Chat".to_string());
        let turn = conv.start_turn("What is 2+2?");
        turn.assistant_response = "4".to_string();
        turn.complete();

        let md = conv.export_markdown(Some("openai"), Some("gpt-4"));
        assert!(md.contains("# Chat Conversation"));
        assert!(md.contains("**User:**"));
        assert!(md.contains("**Assistant:**"));
        assert!(md.contains("gpt-4"));
    }

    #[test]
    fn test_export_html() {
        let mut conv = Conversation::new("System");
        let turn = conv.start_turn("Test message");
        turn.assistant_response = "Test response".to_string();
        turn.complete();

        let html = conv.export_html(None, None);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Test message"));
        assert!(html.contains("Test response"));
        assert!(html.contains("user-message"));
        assert!(html.contains("assistant-message"));
    }

    #[test]
    fn test_export_to_file() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("test_export.json");

        let mut conv = Conversation::new("Test");
        let turn = conv.start_turn("Hello");
        turn.assistant_response = "Hi".to_string();
        turn.complete();

        let result = export_conversation(
            &conv,
            "json",
            Some(output_path.clone()),
            None,
            None,
        );

        assert!(result.is_ok());
        assert!(output_path.exists());

        let content = std::fs::read_to_string(&output_path).unwrap();
        assert!(content.contains("Hello"));
        assert!(content.contains("Hi"));
    }

    #[test]
    fn test_export_format_parsing() {
        assert_eq!(ExportFormat::from_str("json"), Some(ExportFormat::Json));
        assert_eq!(ExportFormat::from_str("JSON"), Some(ExportFormat::Json));
        assert_eq!(ExportFormat::from_str("markdown"), Some(ExportFormat::Markdown));
        assert_eq!(ExportFormat::from_str("md"), Some(ExportFormat::Markdown));
        assert_eq!(ExportFormat::from_str("html"), Some(ExportFormat::Html));
        assert_eq!(ExportFormat::from_str("HTML"), Some(ExportFormat::Html));
        assert_eq!(ExportFormat::from_str("invalid"), None);
    }
}