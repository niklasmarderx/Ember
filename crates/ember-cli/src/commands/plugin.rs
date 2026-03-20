//! Plugin management CLI commands.
//!
//! Commands for searching, installing, updating, and managing Ember plugins.
//!
//! Note: These commands are prepared for future integration into the main CLI.
//! They are currently not wired up to the command router.

#![allow(dead_code)]

use clap::{Args, Subcommand};
use colored::Colorize;
use ember_plugins::marketplace::{
    PluginCategory, RegistryClient, RegistryConfig, SearchQuery, SearchSort,
};
use std::path::PathBuf;
use tabled::{Table, Tabled};
use tokio::fs;

/// Plugin management commands.
#[derive(Debug, Args)]
pub struct PluginArgs {
    #[command(subcommand)]
    pub command: PluginCommand,
}

/// Available plugin commands.
#[derive(Debug, Subcommand)]
pub enum PluginCommand {
    /// Search for plugins in the marketplace.
    Search {
        /// Search query.
        query: String,
        /// Filter by category.
        #[arg(short, long)]
        category: Option<String>,
        /// Sort by field (downloads, rating, name, updated, trending).
        #[arg(short, long, default_value = "downloads")]
        sort: String,
        /// Number of results to show.
        #[arg(short, long, default_value = "10")]
        limit: u32,
    },
    /// Install a plugin from the marketplace.
    Install {
        /// Plugin name.
        name: String,
        /// Specific version to install.
        #[arg(short, long)]
        version: Option<String>,
    },
    /// Uninstall a plugin.
    Uninstall {
        /// Plugin name.
        name: String,
    },
    /// Update a plugin to the latest version.
    Update {
        /// Plugin name (or --all for all plugins).
        name: Option<String>,
        /// Update all plugins.
        #[arg(short, long)]
        all: bool,
    },
    /// List installed plugins.
    List,
    /// Show plugin details.
    Info {
        /// Plugin name.
        name: String,
    },
    /// Check for available updates.
    CheckUpdates,
    /// Show featured plugins from the marketplace.
    Featured,
    /// Show trending plugins from the marketplace.
    Trending,
    /// Clear plugin cache.
    CacheClear,
}

/// Table row for plugin list.
#[derive(Tabled)]
struct PluginRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Version")]
    version: String,
    #[tabled(rename = "Description")]
    description: String,
    #[tabled(rename = "Downloads")]
    downloads: String,
    #[tabled(rename = "Rating")]
    rating: String,
}

/// Table row for installed plugins.
#[derive(Tabled)]
struct InstalledPluginRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Version")]
    version: String,
    #[tabled(rename = "Status")]
    status: String,
}

/// Get the plugin cache directory.
fn get_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ember")
        .join("plugins")
}

/// Create a registry client with custom cache directory.
fn create_client() -> Result<RegistryClient, anyhow::Error> {
    let config = RegistryConfig {
        cache_dir: get_cache_dir(),
        ..Default::default()
    };
    Ok(RegistryClient::with_config(config)?)
}

/// Execute plugin command.
pub async fn execute(args: PluginArgs) -> anyhow::Result<()> {
    match args.command {
        PluginCommand::Search {
            query,
            category,
            sort,
            limit,
        } => search_plugins(&query, category, &sort, limit).await,
        PluginCommand::Install { name, version } => install_plugin(&name, version).await,
        PluginCommand::Uninstall { name } => uninstall_plugin(&name).await,
        PluginCommand::Update { name, all } => update_plugins(name, all).await,
        PluginCommand::List => list_plugins().await,
        PluginCommand::Info { name } => show_plugin_info(&name).await,
        PluginCommand::CheckUpdates => check_updates().await,
        PluginCommand::Featured => show_featured().await,
        PluginCommand::Trending => show_trending().await,
        PluginCommand::CacheClear => clear_cache().await,
    }
}

/// Search for plugins.
async fn search_plugins(
    query: &str,
    category: Option<String>,
    sort: &str,
    limit: u32,
) -> anyhow::Result<()> {
    println!("{}", "Searching plugins...".cyan());

    let client = create_client()?;

    let sort_by = match sort.to_lowercase().as_str() {
        "downloads" => Some(SearchSort::Downloads),
        "rating" => Some(SearchSort::Rating),
        "updated" => Some(SearchSort::RecentlyUpdated),
        "trending" => Some(SearchSort::Trending),
        _ => Some(SearchSort::Downloads),
    };

    let plugin_category = category.and_then(|c| match c.to_lowercase().as_str() {
        "ai" | "ml" => Some(PluginCategory::Ai),
        "productivity" => Some(PluginCategory::Productivity),
        "devtools" | "developer" => Some(PluginCategory::Developer),
        "integration" => Some(PluginCategory::Integration),
        "utility" | "utilities" => Some(PluginCategory::Utility),
        "data" => Some(PluginCategory::Data),
        "communication" => Some(PluginCategory::Communication),
        "security" => Some(PluginCategory::Security),
        _ => None,
    });

    let search_query = SearchQuery {
        query: Some(query.to_string()),
        category: plugin_category,
        sort_by,
        page_size: Some(limit),
        ..Default::default()
    };

    match client.search(&search_query).await {
        Ok(results) => {
            if results.plugins.is_empty() {
                println!("{}", "No plugins found matching your query.".yellow());
                return Ok(());
            }

            println!(
                "\n{} Found {} plugins (showing {})\n",
                "[OK]".green(),
                results.total,
                results.plugins.len()
            );

            let rows: Vec<PluginRow> = results
                .plugins
                .iter()
                .map(|p| PluginRow {
                    name: p.name.clone(),
                    version: p.latest_version().map(|v| v.version.to_string()).unwrap_or_else(|| "N/A".to_string()),
                    description: truncate(&p.description, 40),
                    downloads: format_number(p.stats.downloads),
                    rating: format!("{:.1}", p.stats.average_rating),
                })
                .collect();

            let table = Table::new(rows).to_string();
            println!("{}", table);

            println!(
                "\n{} Use {} to install a plugin",
                "[Tip]".blue(),
                "ember plugin install <name>".cyan()
            );
        }
        Err(e) => {
            println!(
                "{}",
                format!("Could not connect to marketplace: {}", e).yellow()
            );
            show_example_plugins();
        }
    }

    Ok(())
}

/// Install a plugin.
async fn install_plugin(name: &str, version: Option<String>) -> anyhow::Result<()> {
    println!(
        "{} Installing plugin: {}{}",
        "[Install]".cyan(),
        name.green(),
        version
            .as_ref()
            .map(|v| format!("@{}", v))
            .unwrap_or_default()
    );

    let client = create_client()?;

    // Get plugin info
    match client.get_plugin(name).await {
        Ok(metadata) => {
            let target_version = if let Some(v) = version {
                // Find specific version
                match client.get_plugin_version(name, &v).await {
                    Ok(ver) => ver,
                    Err(e) => {
                        println!("\n{} Version not found: {}", "[Error]".red(), e);
                        return Ok(());
                    }
                }
            } else {
                // Use latest version
                match metadata.latest_version() {
                    Some(v) => v.clone(),
                    None => {
                        println!("\n{} No versions available for this plugin", "[Error]".red());
                        return Ok(());
                    }
                }
            };

            // Download the plugin
            match client.download_plugin(name, &target_version).await {
                Ok(path) => {
                    println!(
                        "\n{} Plugin {} v{} installed successfully!",
                        "[OK]".green(),
                        name.green(),
                        target_version.version
                    );
                    println!("  Location: {}", path.display());
                }
                Err(e) => {
                    println!("\n{} Download failed: {}", "[Error]".red(), e);
                }
            }
        }
        Err(e) => {
            println!("\n{} Could not find plugin: {}", "[Error]".red(), e);
            println!("\n{} Tips:", "[Tip]".blue());
            println!("  - Check if the plugin name is correct");
            println!("  - Verify your internet connection");
            println!("  - Try: ember plugin search {}", name);
        }
    }

    Ok(())
}

/// Uninstall a plugin.
async fn uninstall_plugin(name: &str) -> anyhow::Result<()> {
    println!("{} Uninstalling plugin: {}", "[Uninstall]".cyan(), name.yellow());

    let cache_dir = get_cache_dir();
    let plugin_dir = cache_dir.join(name);

    if plugin_dir.exists() {
        fs::remove_dir_all(&plugin_dir).await?;
        println!(
            "\n{} Plugin {} uninstalled successfully!",
            "[OK]".green(),
            name.green()
        );
    } else {
        println!("\n{} Plugin {} is not installed.", "[Info]".yellow(), name);
    }

    Ok(())
}

/// Update plugins.
async fn update_plugins(name: Option<String>, all: bool) -> anyhow::Result<()> {
    if all {
        println!("{} Checking for updates...", "[Update]".cyan());
        // TODO: Implement batch update when installed plugin tracking is available
        println!("{} Batch update not yet implemented.", "[Info]".yellow());
        println!("  Use: ember plugin update <plugin-name>");
    } else if let Some(plugin_name) = name {
        println!("{} Updating plugin: {}", "[Update]".cyan(), plugin_name.yellow());

        let client = create_client()?;

        match client.get_plugin(&plugin_name).await {
            Ok(metadata) => {
                if let Some(latest) = metadata.latest_version() {
                    match client.download_plugin(&plugin_name, latest).await {
                        Ok(path) => {
                            println!(
                                "\n{} Plugin {} updated to v{}!",
                                "[OK]".green(),
                                plugin_name.green(),
                                latest.version
                            );
                            println!("  Location: {}", path.display());
                        }
                        Err(e) => {
                            println!("\n{} Update failed: {}", "[Error]".red(), e);
                        }
                    }
                } else {
                    println!("\n{} No versions available", "[Error]".red());
                }
            }
            Err(e) => {
                println!("\n{} Could not find plugin: {}", "[Error]".red(), e);
            }
        }
    } else {
        println!(
            "{} Please specify a plugin name or use --all",
            "[Warning]".yellow()
        );
    }

    Ok(())
}

/// List installed plugins.
async fn list_plugins() -> anyhow::Result<()> {
    println!("{} Installed plugins:\n", "[Plugins]".cyan());

    let cache_dir = get_cache_dir();

    if !cache_dir.exists() {
        println!("{}", "No plugins installed.".yellow());
        println!(
            "\n{} Use {} to search for plugins",
            "[Tip]".blue(),
            "ember plugin search <query>".cyan()
        );
        return Ok(());
    }

    let mut entries = fs::read_dir(&cache_dir).await?;
    let mut plugins = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            plugins.push(InstalledPluginRow {
                name,
                version: "installed".to_string(),
                status: "active".green().to_string(),
            });
        }
    }

    if plugins.is_empty() {
        println!("{}", "No plugins installed.".yellow());
        return Ok(());
    }

    let table = Table::new(&plugins).to_string();
    println!("{}", table);

    println!("\n{} {} plugins installed", "[Stats]".blue(), plugins.len());

    Ok(())
}

/// Show plugin details.
async fn show_plugin_info(name: &str) -> anyhow::Result<()> {
    println!("{} Plugin info: {}\n", "[Info]".cyan(), name.green());

    let client = create_client()?;

    match client.get_plugin(name).await {
        Ok(metadata) => {
            let author_name = metadata.authors.first()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            println!("{}", "Marketplace Info:".blue().bold());
            println!("  Name:        {}", metadata.name);
            println!("  ID:          {}", metadata.id);
            println!("  Description: {}", metadata.description);
            println!("  Author:      {}", author_name);
            println!("  Downloads:   {}", format_number(metadata.stats.downloads));
            println!("  Rating:      {:.1} ({} ratings)", metadata.stats.average_rating, metadata.stats.review_count);
            println!("  Featured:    {}", if metadata.featured { "Yes" } else { "No" });
            println!("  Verified:    {}", if metadata.verified { "Yes" } else { "No" });

            if let Some(latest) = metadata.latest_version() {
                println!("\n{}", "Latest Version:".blue().bold());
                println!("  Version:     {}", latest.version);
                println!("  Released:    {}", latest.released_at.format("%Y-%m-%d"));
                println!("  Min Ember:   {}", latest.ember_version);
            }
        }
        Err(e) => {
            println!("{} Could not fetch plugin info: {}", "[Error]".red(), e);
        }
    }

    Ok(())
}

/// Check for updates.
async fn check_updates() -> anyhow::Result<()> {
    println!("{} Checking for updates...\n", "[Check]".cyan());

    // TODO: Implement proper installed plugin tracking
    println!("{} Update checking requires installed plugin tracking.", "[Info]".yellow());
    println!("  This feature will be available in a future version.");

    Ok(())
}

/// Show featured plugins.
async fn show_featured() -> anyhow::Result<()> {
    println!("{} Featured Plugins\n", "[Featured]".cyan());

    let client = create_client()?;

    match client.get_featured().await {
        Ok(plugins) => {
            if plugins.is_empty() {
                println!("{}", "No featured plugins available.".yellow());
                return Ok(());
            }

            for p in &plugins {
                println!(
                    "  - {} v{} - {}",
                    p.name.green(),
                    p.latest_version().map(|v| v.version.to_string()).unwrap_or_else(|| "?".to_string()),
                    truncate(&p.description, 50)
                );
                println!("    {} downloads, {:.1} rating", format_number(p.stats.downloads), p.stats.average_rating);
            }
        }
        Err(e) => {
            println!("{} Could not fetch featured plugins: {}", "[Error]".yellow(), e);
            show_example_plugins();
        }
    }

    Ok(())
}

/// Show trending plugins.
async fn show_trending() -> anyhow::Result<()> {
    println!("{} Trending Plugins\n", "[Trending]".cyan());

    let client = create_client()?;

    match client.get_trending().await {
        Ok(plugins) => {
            if plugins.is_empty() {
                println!("{}", "No trending plugins available.".yellow());
                return Ok(());
            }

            for p in &plugins {
                println!(
                    "  - {} v{} - {}",
                    p.name.green(),
                    p.latest_version().map(|v| v.version.to_string()).unwrap_or_else(|| "?".to_string()),
                    truncate(&p.description, 50)
                );
            }
        }
        Err(e) => {
            println!("{} Could not fetch trending plugins: {}", "[Error]".yellow(), e);
            show_example_plugins();
        }
    }

    Ok(())
}

/// Clear plugin cache.
async fn clear_cache() -> anyhow::Result<()> {
    println!("{} Clearing plugin cache...", "[Cache]".cyan());

    let cache_dir = get_cache_dir();
    if cache_dir.exists() {
        fs::remove_dir_all(&cache_dir).await?;
        fs::create_dir_all(&cache_dir).await?;
    }

    println!("{} Plugin cache cleared!", "[OK]".green());

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Show example plugins (for demo when offline).
fn show_example_plugins() {
    println!("\n{}", "Example Plugins:".blue().bold());

    let examples = vec![
        ("weather", "1.2.0", "Get weather forecasts for any location", "12.5k", "4.8"),
        ("slack", "2.0.1", "Send messages and notifications to Slack", "8.3k", "4.6"),
        ("github", "1.5.0", "Interact with GitHub repositories and issues", "15.2k", "4.9"),
        ("jira", "1.1.0", "Create and manage Jira tickets", "5.7k", "4.3"),
        ("calendar", "1.0.0", "Manage Google Calendar events", "3.2k", "4.5"),
    ];

    for (name, version, desc, downloads, rating) in examples {
        println!(
            "  - {} v{} ({} downloads, rating {}) - {}",
            name.green(),
            version,
            downloads,
            rating,
            desc
        );
    }

    println!(
        "\n{} These are example plugins for demonstration.",
        "[Info]".blue()
    );
}

/// Truncate string to max length.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Format large numbers with K/M suffixes.
fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}