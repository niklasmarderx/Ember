//! Export command for saving conversations in various formats.

use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::Args;
use colored::Colorize;
use ember_core::{Conversation, ExportFormat};
use std::path::PathBuf;

use super::session::PersistedSession;

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
    #[arg(long)]
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
    let format = ExportFormat::parse(&args.format).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown format '{}'. Supported formats: json, markdown (md), html",
            args.format
        )
    })?;

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
        ExportFormat::Json => {
            conversation.export_json(args.provider.as_deref(), args.model.as_deref())
        }
        ExportFormat::Markdown => {
            conversation.export_markdown(args.provider.as_deref(), args.model.as_deref())
        }
        ExportFormat::Html => {
            conversation.export_html(args.provider.as_deref(), args.model.as_deref())
        }
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
#[allow(dead_code)]
pub fn export_conversation(
    conversation: &Conversation,
    format_str: &str,
    output: Option<PathBuf>,
    provider: Option<&str>,
    model: Option<&str>,
) -> Result<PathBuf> {
    let format = ExportFormat::parse(format_str).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown format '{}'. Supported: json, markdown, html",
            format_str
        )
    })?;

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
    // Try to load from the sessions directory (where chat saves them)
    let sessions_dir = dirs::home_dir()
        .map(|h| h.join(".ember").join("sessions"))
        .unwrap_or_else(|| PathBuf::from(".ember/sessions"));

    // Also check legacy data directory
    let data_dir = dirs::data_dir()
        .map(|d| d.join("ember").join("conversations"))
        .unwrap_or_else(|| PathBuf::from(".ember/conversations"));

    if let Some(conv_id) = id {
        // Load specific conversation — check sessions dir first, then legacy
        let session_file = sessions_dir.join(format!("{}.json", conv_id));
        let conv_file = data_dir.join(format!("{}.json", conv_id));
        let target = if session_file.exists() {
            session_file
        } else if conv_file.exists() {
            conv_file
        } else {
            bail!("Conversation '{}' not found", conv_id);
        };
        let content = std::fs::read_to_string(&target)
            .with_context(|| format!("Failed to read conversation {}", conv_id))?;
        let conversation = parse_conversation_or_session(&content)
            .with_context(|| format!("Failed to parse conversation {}", conv_id))?;
        return Ok(conversation);
    }

    // Try to load the most recent conversation from sessions dir or legacy dir
    for dir in &[&sessions_dir, &data_dir] {
        if dir.exists() {
            let mut entries: Vec<_> = std::fs::read_dir(dir)?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext == "json")
                        .unwrap_or(false)
                })
                .collect();

            entries.sort_by(|a, b| {
                let a_time = a.metadata().and_then(|m| m.modified()).ok();
                let b_time = b.metadata().and_then(|m| m.modified()).ok();
                b_time.cmp(&a_time)
            });

            if let Some(entry) = entries.first() {
                let content = std::fs::read_to_string(entry.path())?;
                let conversation = parse_conversation_or_session(&content)?;
                return Ok(conversation);
            }
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
    turn.assistant_response =
        "Of course! I'd be happy to help. What do you need assistance with?".to_string();
    turn.complete();

    let turn2 = conv.start_turn("What's the weather like today?");
    turn2.assistant_response = "I don't have access to real-time weather data. However, you can check your local weather service or a weather app for current conditions in your area.".to_string();
    turn2.complete();

    Ok(conv)
}

/// Try to parse a JSON string as a Conversation, falling back to PersistedSession format.
fn parse_conversation_or_session(content: &str) -> Result<Conversation> {
    // Try native Conversation format first
    if let Ok(conv) = serde_json::from_str::<Conversation>(content) {
        return Ok(conv);
    }

    // Fall back to PersistedSession format (used by `ember chat`)
    let session: PersistedSession = serde_json::from_str(content)
        .context("File is neither a Conversation nor a PersistedSession")?;

    // Convert PersistedSession → Conversation
    let system_prompt = session
        .messages
        .iter()
        .find(|m| m.role == "system")
        .map(|m| m.content.clone())
        .unwrap_or_default();

    let mut conv = Conversation::new(&system_prompt);
    conv.title = Some(format!("Session {}", &session.id));

    // Group user/assistant messages into turns
    let mut i = 0;
    let msgs: Vec<_> = session
        .messages
        .iter()
        .filter(|m| m.role != "system")
        .collect();
    while i < msgs.len() {
        if msgs[i].role == "user" {
            let user_msg = &msgs[i].content;
            let assistant_msg = if i + 1 < msgs.len() && msgs[i + 1].role == "assistant" {
                i += 1;
                &msgs[i].content
            } else {
                ""
            };
            let turn = conv.start_turn(user_msg);
            turn.assistant_response = assistant_msg.to_string();
            turn.complete();
        }
        i += 1;
    }

    Ok(conv)
}

/// List available conversations.
#[allow(dead_code)]
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

        let result = export_conversation(&conv, "json", Some(output_path.clone()), None, None);

        assert!(result.is_ok());
        assert!(output_path.exists());

        let content = std::fs::read_to_string(&output_path).unwrap();
        assert!(content.contains("Hello"));
        assert!(content.contains("Hi"));
    }

    #[test]
    fn test_export_format_parsing() {
        assert_eq!(ExportFormat::parse("json"), Some(ExportFormat::Json));
        assert_eq!(ExportFormat::parse("JSON"), Some(ExportFormat::Json));
        assert_eq!(
            ExportFormat::parse("markdown"),
            Some(ExportFormat::Markdown)
        );
        assert_eq!(ExportFormat::parse("md"), Some(ExportFormat::Markdown));
        assert_eq!(ExportFormat::parse("html"), Some(ExportFormat::Html));
        assert_eq!(ExportFormat::parse("HTML"), Some(ExportFormat::Html));
        assert_eq!(ExportFormat::parse("invalid"), None);
    }
}
