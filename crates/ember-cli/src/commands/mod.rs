//! CLI command implementations.

pub mod chat;
pub mod code;
pub mod completions;
pub mod config;
pub mod export;
pub mod git;
pub mod history;
#[cfg(feature = "plugins")]
pub mod plugin;
#[cfg(feature = "serve")]
pub mod serve;
pub mod slash;
