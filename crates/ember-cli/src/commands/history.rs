//! History command for searching and managing conversation history.
//!
//! This command provides functionality to:
//! - Search through past conversations
//! - List recent conversations
//! - View conversation statistics
//! - Prune old conversations

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use colored::Colorize;
use ember_storage::{
    ConversationStats, MessageSearchResult, SearchOptions, SearchSortBy, SqliteConfig,
    SqliteSearchResult, SqliteStorage,
};
use std::path::PathBuf;
use tracing::debug;

/// History command arguments.
#[derive(Debug, Args)]
pub struct HistoryArgs {
    /// Path to the database file
    #[arg(long, short = 'd', default_value = "ember.db")]
    pub database: PathBuf,

    #[command(subcommand)]
    pub command: HistoryCommand,
}

/// History subcommands.
#[derive(Debug, Subcommand)]
pub enum HistoryCommand {
    /// Search conversations and messages
    Search(SearchArgs),

    /// List recent conversations
    List(ListArgs),

    /// Show conversation statistics
    Stats(StatsArgs),

    /// Delete old conversations
    Prune(PruneArgs),
}

/// Arguments for the search subcommand.
#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Search query (searches in titles and message content)
    pub query: String,

    /// Search only in messages (not conversation titles)
    #[arg(long, short = 'm')]
    pub messages_only: bool,

    /// Limit results to a specific conversation ID
    #[arg(long, short = 'c')]
    pub conversation: Option<String>,

    /// Maximum number of results
    #[arg(long, short = 'n', default_value = "20")]
    pub limit: usize,

    /// Sort order: relevance, newest, oldest, messages
    #[arg(long, short = 's', default_value = "relevance")]
    pub sort: String,

    /// Filter: only conversations after this date (YYYY-MM-DD)
    #[arg(long)]
    pub from: Option<String>,

    /// Filter: only conversations before this date (YYYY-MM-DD)
    #[arg(long)]
    pub to: Option<String>,

    /// Output format: text, json
    #[arg(long, short = 'o', default_value = "text")]
    pub format: String,
}

/// Arguments for the list subcommand.
#[derive(Debug, Args)]
pub struct ListArgs {
    /// Maximum number of conversations to list
    #[arg(long, short = 'n', default_value = "10")]
    pub limit: usize,

    /// Number of conversations to skip
    #[arg(long, default_value = "0")]
    pub offset: usize,

    /// Output format: text, json
    #[arg(long, short = 'o', default_value = "text")]
    pub format: String,
}

/// Arguments for the stats subcommand.
#[derive(Debug, Args)]
pub struct StatsArgs {
    /// Filter: only count conversations after this date (YYYY-MM-DD)
    #[arg(long)]
    pub from: Option<String>,

    /// Filter: only count conversations before this date (YYYY-MM-DD)
    #[arg(long)]
    pub to: Option<String>,

    /// Output format: text, json
    #[arg(long, short = 'o', default_value = "text")]
    pub format: String,
}

/// Arguments for the prune subcommand.
#[derive(Debug, Args)]
pub struct PruneArgs {
    /// Delete conversations older than this date (YYYY-MM-DD)
    #[arg(long, required = true)]
    pub older_than: String,

    /// Skip confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
}

/// Execute the history command.
pub async fn execute(args: HistoryArgs) -> Result<()> {
    let config = SqliteConfig {
        path: args.database.to_string_lossy().to_string(),
        ..Default::default()
    };

    let storage = SqliteStorage::new(&config).context("Failed to open database")?;
    storage
        .migrate()
        .await
        .context("Failed to run migrations")?;

    match args.command {
        HistoryCommand::Search(search_args) => execute_search(&storage, search_args).await,
        HistoryCommand::List(list_args) => execute_list(&storage, list_args).await,
        HistoryCommand::Stats(stats_args) => execute_stats(&storage, stats_args).await,
        HistoryCommand::Prune(prune_args) => execute_prune(&storage, prune_args).await,
    }
}

/// Execute the search subcommand.
async fn execute_search(storage: &SqliteStorage, args: SearchArgs) -> Result<()> {
    debug!(query = %args.query, "Searching history");

    if args.messages_only {
        // Search only in messages
        let results = storage
            .search_messages(&args.query, args.conversation.as_deref(), args.limit)
            .await
            .context("Search failed")?;

        if args.format == "json" {
            print_message_results_json(&results)?;
        } else {
            print_message_results_text(&results, &args.query);
        }
    } else {
        // Search in conversations and messages
        let sort_by = match args.sort.to_lowercase().as_str() {
            "newest" | "date" => SearchSortBy::DateNewest,
            "oldest" => SearchSortBy::DateOldest,
            "messages" | "count" => SearchSortBy::MessageCount,
            _ => SearchSortBy::Relevance,
        };

        let options = SearchOptions {
            sort_by,
            from_date: args.from,
            to_date: args.to,
            limit: args.limit,
            offset: 0,
        };

        let results = storage
            .search_conversations(&args.query, options)
            .await
            .context("Search failed")?;

        if args.format == "json" {
            print_conversation_results_json(&results)?;
        } else {
            print_conversation_results_text(&results, &args.query);
        }
    }

    Ok(())
}

/// Execute the list subcommand.
async fn execute_list(storage: &SqliteStorage, args: ListArgs) -> Result<()> {
    let conversations = storage
        .list_conversations(args.limit, args.offset)
        .await
        .context("Failed to list conversations")?;

    if args.format == "json" {
        let json = serde_json::json!({
            "conversations": conversations.iter().map(|c| {
                serde_json::json!({
                    "id": c.id,
                    "title": c.title,
                    "created_at": c.created_at,
                    "updated_at": c.updated_at,
                })
            }).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        if conversations.is_empty() {
            println!("{}", "No conversations found.".dimmed());
            return Ok(());
        }

        println!("{}", "Recent Conversations".bold().underline());
        println!();

        for conv in &conversations {
            let title = conv.title.as_deref().unwrap_or("(untitled)").to_string();
            let date = format_date(&conv.updated_at);

            println!("  {} {}", conv.id[..8].cyan(), title.bold());
            println!("    {} {}", "Last updated:".dimmed(), date.dimmed());
            println!();
        }

        println!(
            "{}",
            format!("Showing {} conversation(s)", conversations.len()).dimmed()
        );
    }

    Ok(())
}

/// Execute the stats subcommand.
async fn execute_stats(storage: &SqliteStorage, args: StatsArgs) -> Result<()> {
    let stats = storage
        .get_conversation_stats(args.from.as_deref(), args.to.as_deref())
        .await
        .context("Failed to get statistics")?;

    if args.format == "json" {
        let json = serde_json::json!({
            "total_conversations": stats.total_conversations,
            "total_messages": stats.total_messages,
            "oldest_conversation": stats.oldest_conversation,
            "newest_conversation": stats.newest_conversation,
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        print_stats_text(&stats);
    }

    Ok(())
}

/// Execute the prune subcommand.
async fn execute_prune(storage: &SqliteStorage, args: PruneArgs) -> Result<()> {
    // Get count of conversations that will be deleted
    let stats_before = storage
        .get_conversation_stats(None, Some(&args.older_than))
        .await
        .context("Failed to get statistics")?;

    let to_delete = stats_before.total_conversations;

    if to_delete == 0 {
        println!(
            "{}",
            format!("No conversations found older than {}", args.older_than).yellow()
        );
        return Ok(());
    }

    // Confirm deletion
    if !args.yes {
        println!(
            "{}",
            format!(
                "This will delete {} conversation(s) older than {}",
                to_delete, args.older_than
            )
            .yellow()
        );
        print!("Continue? [y/N] ");
        std::io::Write::flush(&mut std::io::stdout())?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("{}", "Aborted.".dimmed());
            return Ok(());
        }
    }

    let deleted = storage
        .prune_conversations(&args.older_than)
        .await
        .context("Failed to prune conversations")?;

    println!("{}", format!("Deleted {} conversation(s)", deleted).green());

    Ok(())
}

/// Print conversation search results as text.
fn print_conversation_results_text(results: &[SqliteSearchResult], query: &str) {
    if results.is_empty() {
        println!("{}", format!("No results found for '{}'", query).dimmed());
        return;
    }

    println!(
        "{}",
        format!("Found {} result(s) for '{}'", results.len(), query)
            .bold()
            .underline()
    );
    println!();

    for result in results {
        let title = result.title.as_deref().unwrap_or("(untitled)").to_string();
        let date = format_date(&result.updated_at);

        println!(
            "  {} {} {}",
            result.conversation_id[..8].cyan(),
            title.bold(),
            format!("({} messages)", result.message_count).dimmed()
        );
        println!("    {} {}", "Updated:".dimmed(), date.dimmed());

        if let Some(ref snippet) = result.snippet {
            let highlighted = highlight_text(snippet, query);
            println!("    {}", highlighted);
        }
        println!();
    }
}

/// Print message search results as text.
fn print_message_results_text(results: &[MessageSearchResult], query: &str) {
    if results.is_empty() {
        println!("{}", format!("No messages found for '{}'", query).dimmed());
        return;
    }

    println!(
        "{}",
        format!("Found {} message(s) for '{}'", results.len(), query)
            .bold()
            .underline()
    );
    println!();

    for result in results {
        let conv_title = result.conversation_title.as_deref().unwrap_or("(untitled)");
        let date = format_date(&result.created_at);
        let role_colored = match result.role.as_str() {
            "user" => result.role.blue(),
            "assistant" => result.role.green(),
            _ => result.role.dimmed(),
        };

        println!(
            "  {} [{}] {}",
            result.message_id[..8].cyan(),
            role_colored,
            conv_title.bold()
        );
        println!("    {} {}", "Date:".dimmed(), date.dimmed());

        if let Some(ref snippet) = result.snippet {
            let highlighted = highlight_text(snippet, query);
            println!("    {}", highlighted);
        }
        println!();
    }
}

/// Print conversation search results as JSON.
fn print_conversation_results_json(results: &[SqliteSearchResult]) -> Result<()> {
    let json = serde_json::json!({
        "results": results.iter().map(|r| {
            serde_json::json!({
                "conversation_id": r.conversation_id,
                "title": r.title,
                "created_at": r.created_at,
                "updated_at": r.updated_at,
                "message_count": r.message_count,
                "snippet": r.snippet,
                "highlights": r.highlights.iter().map(|h| {
                    serde_json::json!({ "start": h.start, "end": h.end })
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>()
    });
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

/// Print message search results as JSON.
fn print_message_results_json(results: &[MessageSearchResult]) -> Result<()> {
    let json = serde_json::json!({
        "results": results.iter().map(|r| {
            serde_json::json!({
                "message_id": r.message_id,
                "conversation_id": r.conversation_id,
                "conversation_title": r.conversation_title,
                "role": r.role,
                "content": r.content,
                "created_at": r.created_at,
                "snippet": r.snippet,
                "highlights": r.highlights.iter().map(|h| {
                    serde_json::json!({ "start": h.start, "end": h.end })
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>()
    });
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

/// Print statistics as text.
fn print_stats_text(stats: &ConversationStats) {
    println!("{}", "Conversation Statistics".bold().underline());
    println!();
    println!(
        "  {} {}",
        "Total conversations:".dimmed(),
        stats.total_conversations.to_string().bold()
    );
    println!(
        "  {} {}",
        "Total messages:".dimmed(),
        stats.total_messages.to_string().bold()
    );

    if let Some(ref oldest) = stats.oldest_conversation {
        println!(
            "  {} {}",
            "Oldest conversation:".dimmed(),
            format_date(oldest)
        );
    }

    if let Some(ref newest) = stats.newest_conversation {
        println!(
            "  {} {}",
            "Newest conversation:".dimmed(),
            format_date(newest)
        );
    }
}

/// Format an ISO date string to a more readable format.
fn format_date(iso_date: &str) -> String {
    // Try to parse and format, fall back to original if parsing fails
    chrono::DateTime::parse_from_rfc3339(iso_date)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| iso_date.to_string())
}

/// Highlight search query matches in text.
fn highlight_text(text: &str, query: &str) -> String {
    let query_lower = query.to_lowercase();
    let text_lower = text.to_lowercase();

    let mut result = String::new();
    let mut last_end = 0;

    for (start, _) in text_lower.match_indices(&query_lower) {
        // Add text before match
        result.push_str(&text[last_end..start]);
        // Add highlighted match
        let end = start + query.len();
        result.push_str(&text[start..end].yellow().bold().to_string());
        last_end = end;
    }

    // Add remaining text
    result.push_str(&text[last_end..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_date() {
        let iso = "2024-01-15T14:30:00+00:00";
        let formatted = format_date(iso);
        assert!(formatted.contains("2024-01-15"));
    }

    #[test]
    fn test_format_date_invalid() {
        let invalid = "not-a-date";
        let result = format_date(invalid);
        assert_eq!(result, "not-a-date");
    }

    #[test]
    fn test_highlight_text() {
        // Force colored output even in CI environments without a TTY
        colored::control::set_override(true);
        
        let text = "Hello World, hello again";
        let result = highlight_text(text, "hello");
        // Should contain ANSI codes for yellow/bold
        assert!(result.contains("\x1b["));
        
        // Reset to default behavior
        colored::control::unset_override();
    }

    #[test]
    fn test_highlight_text_no_match() {
        let text = "Hello World";
        let result = highlight_text(text, "xyz");
        assert_eq!(result, text);
    }
}
