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
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::useless_format)]
#![allow(clippy::unused_self)]
#![allow(clippy::manual_strip)]
#![allow(clippy::similar_names)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::format_push_string)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unused_async)]

mod error;
mod registry;

/// Custom Tool SDK for creating tools.
pub mod sdk;

#[cfg(feature = "shell")]
pub mod shell;

#[cfg(feature = "shell")]
pub mod structured_bash;

#[cfg(feature = "filesystem")]
pub mod filesystem;

pub mod patch;

#[cfg(feature = "web")]
pub mod web;

#[cfg(feature = "git")]
pub mod git;

#[cfg(feature = "code-execution")]
pub mod code_execution;

#[cfg(feature = "database")]
pub mod database;

#[cfg(feature = "image")]
pub mod image;

#[cfg(feature = "api")]
pub mod api;

pub use error::{Error, Result};
pub use registry::{ToolDefinition, ToolHandler, ToolOutput, ToolRegistry};

// SDK exports
pub use sdk::{AsyncTool, ParamDef, ParamExtractor, ParamType, SimpleTool, SimpleToolBuilder};

#[cfg(feature = "shell")]
pub use shell::ShellTool;

#[cfg(feature = "shell")]
pub use structured_bash::{
    execute_bash, BackgroundTask, BackgroundTaskManager, BashCommandInput, BashCommandOutput,
    BashSandboxPolicy, FilesystemMode, ShellError, TaskStatus,
};

#[cfg(feature = "filesystem")]
pub use filesystem::FilesystemTool;

pub use patch::{
    apply_patch, compute_diff, format_unified_diff, reverse_hunks, undo_write, write_file_tracked,
    DiffLine, FileOpHistory, FileWriteResult, PatchError, PatchHunk,
};

#[cfg(feature = "web")]
pub use web::WebTool;

#[cfg(feature = "git")]
pub use git::GitTool;

#[cfg(feature = "code-execution")]
pub use code_execution::{
    CodeExecutionConfig, CodeExecutionTool, ExecutionResult, Language, ReplSession,
};

#[cfg(feature = "database")]
pub use database::{DatabaseConfig, DatabaseTool, DatabaseType, QueryResult};

#[cfg(feature = "image")]
pub use image::{
    FlipDirection, ImageConfig, ImageFormat, ImageMetadata, ImageOperation, ImageTool,
};

#[cfg(feature = "api")]
pub use api::{
    ApiConfig, ApiRequest, ApiRequestBuilder, ApiResponse, ApiTool, AuthScheme, HttpMethod,
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

    #[cfg(feature = "database")]
    pub use crate::{DatabaseConfig, DatabaseTool, DatabaseType, QueryResult};

    #[cfg(feature = "image")]
    pub use crate::{
        FlipDirection, ImageConfig, ImageFormat, ImageMetadata, ImageOperation, ImageTool,
    };

    #[cfg(feature = "api")]
    pub use crate::{
        ApiConfig, ApiRequest, ApiRequestBuilder, ApiResponse, ApiTool, AuthScheme, HttpMethod,
    };
}
