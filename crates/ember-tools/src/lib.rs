//! # Ember Tools
//!
//! Built-in tools for Ember AI agents.
//!
//! This crate provides a collection of tools that agents can use to interact
//! with the system:
//! - Shell command execution
//! - Filesystem operations (read, write, list, search)
//! - Web requests (HTTP GET, POST, etc.)
//!
//! ## Example
//!
//! ```rust,no_run
//! use ember_tools::{ShellTool, FilesystemTool, ToolRegistry};
//!
//! let mut registry = ToolRegistry::new();
//! registry.register(ShellTool::new());
//! registry.register(FilesystemTool::new());
//!
//! // Get tool definitions for the LLM
//! let tools = registry.tool_definitions();
//! ```

#![deny(missing_docs)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

mod error;
mod registry;

/// Custom Tool SDK for creating tools.
pub mod sdk;

#[cfg(feature = "shell")]
pub mod shell;

#[cfg(feature = "filesystem")]
pub mod filesystem;

#[cfg(feature = "web")]
pub mod web;

#[cfg(feature = "git")]
pub mod git;

#[cfg(feature = "code-execution")]
pub mod code_execution;

pub use error::{Error, Result};
pub use registry::{ToolDefinition, ToolHandler, ToolOutput, ToolRegistry};

// SDK exports
pub use sdk::{AsyncTool, ParamDef, ParamExtractor, ParamType, SimpleTool, SimpleToolBuilder};

#[cfg(feature = "shell")]
pub use shell::ShellTool;

#[cfg(feature = "filesystem")]
pub use filesystem::FilesystemTool;

#[cfg(feature = "web")]
pub use web::WebTool;

#[cfg(feature = "git")]
pub use git::GitTool;

#[cfg(feature = "code-execution")]
pub use code_execution::{
    CodeExecutionConfig, CodeExecutionTool, ExecutionResult, Language, ReplSession,
};

/// Prelude for convenient imports.
pub mod prelude {
    pub use crate::sdk::validation;
    pub use crate::sdk::{
        AsyncTool, ParamDef, ParamExtractor, ParamType, SimpleTool, SimpleToolBuilder,
    };
    pub use crate::{Error, Result, ToolDefinition, ToolHandler, ToolOutput, ToolRegistry};

    #[cfg(feature = "shell")]
    pub use crate::ShellTool;

    #[cfg(feature = "filesystem")]
    pub use crate::FilesystemTool;

    #[cfg(feature = "web")]
    pub use crate::WebTool;

    #[cfg(feature = "git")]
    pub use crate::GitTool;

    #[cfg(feature = "code-execution")]
    pub use crate::{
        CodeExecutionConfig, CodeExecutionTool, ExecutionResult, Language, ReplSession,
    };
}
