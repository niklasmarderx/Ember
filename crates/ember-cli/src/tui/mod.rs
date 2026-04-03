//! Terminal User Interface for Ember using ratatui.
//!
//! Provides a split-screen interface with:
//! - Chat history panel
//! - Input field
//! - Status bar with model info and token count

#[cfg(feature = "tui")]
mod app;
#[cfg(feature = "tui")]
mod ui;

#[allow(dead_code)]
pub mod renderer;

#[allow(unused_imports)]
pub use renderer::{ColorTheme, Spinner, TerminalRenderer, ToolOutputFormatter};

#[cfg(feature = "tui")]
pub async fn run(config: crate::config::AppConfig) -> anyhow::Result<()> {
    app::run(config).await
}
