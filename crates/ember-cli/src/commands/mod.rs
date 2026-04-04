//! CLI command implementations.

pub mod bench;
pub mod chat;
pub mod code;
pub mod completions;
pub mod config;
pub mod context_builder;
pub mod display;
pub mod export;
pub mod git;
pub mod history;
#[cfg(feature = "plugins")]
pub mod plugin;
pub mod provider_factory;
pub mod risk;
#[cfg(feature = "serve")]
pub mod serve;
pub mod session;
pub mod slash;
pub mod terminal;
