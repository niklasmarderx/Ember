//! Glob tool — find files by pattern.
//!
//! Provides a dedicated `glob` tool so the LLM can discover files by name
//! pattern (e.g. `**/*.rs`, `src/**/*.test.ts`).

use crate::{registry::ToolOutput, Error, Result, ToolDefinition, ToolHandler};
use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;
use tracing::debug;

/// Maximum number of file paths to return.
const MAX_RESULTS: usize = 500;

/// Glob tool for finding files by name pattern.
pub struct GlobTool {
    enabled: bool,
}

impl GlobTool {
    /// Create a new `GlobTool`.
    pub fn new() -> Self {
        Self { enabled: true }
    }
}

impl Default for GlobTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolHandler for GlobTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "glob",
            "Find files matching a glob pattern. Returns file paths sorted by modification time (newest first). Use this to discover project files, find tests, locate configs, etc.",
        )
        .with_parameters(serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern (e.g., '**/*.rs', 'src/**/*.test.ts', 'Cargo.toml')"
                },
                "path": {
                    "type": "string",
                    "description": "Base directory to search in (defaults to current directory)"
                }
            },
            "required": ["pattern"]
        }))
    }

    async fn execute(&self, arguments: Value) -> Result<ToolOutput> {
        let pattern = arguments
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::invalid_arguments("glob", "Missing 'pattern' parameter"))?;

        let path_str = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let base_path = PathBuf::from(path_str);
        let base_path = if base_path.is_absolute() {
            base_path
        } else {
            std::env::current_dir()?.join(&base_path)
        };

        let full_pattern = base_path.join(pattern).to_string_lossy().to_string();
        debug!(pattern = %full_pattern, "Glob search");

        let mut entries: Vec<(PathBuf, std::time::SystemTime)> = glob::glob(&full_pattern)
            .map_err(|e| Error::invalid_arguments("glob", format!("Invalid pattern: {}", e)))?
            .filter_map(|entry| entry.ok())
            .filter_map(|path| {
                // Skip hidden files/dirs
                let is_hidden = path
                    .components()
                    .any(|c| {
                        c.as_os_str()
                            .to_string_lossy()
                            .starts_with('.')
                    });
                if is_hidden {
                    return None;
                }
                // Skip common junk dirs
                let path_str = path.to_string_lossy();
                if path_str.contains("/node_modules/")
                    || path_str.contains("/target/")
                    || path_str.contains("/__pycache__/")
                {
                    return None;
                }
                let modified = std::fs::metadata(&path)
                    .ok()?
                    .modified()
                    .ok()?;
                Some((path, modified))
            })
            .collect();

        // Sort by modification time, newest first
        entries.sort_by(|a, b| b.1.cmp(&a.1));

        let truncated = entries.len() > MAX_RESULTS;
        entries.truncate(MAX_RESULTS);

        let paths: Vec<String> = entries
            .iter()
            .map(|(p, _)| {
                p.strip_prefix(&base_path)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .to_string()
            })
            .collect();

        let output = if paths.is_empty() {
            format!("No files found matching '{}'", pattern)
        } else {
            let mut out = format!("Found {} file(s):\n\n", paths.len());
            for p in &paths {
                out.push_str(p);
                out.push('\n');
            }
            if truncated {
                out.push_str(&format!("\n[… truncated at {} results]", MAX_RESULTS));
            }
            out
        };

        Ok(ToolOutput::success_with_data(
            &output,
            serde_json::json!({
                "count": paths.len(),
                "files": paths,
            }),
        ))
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_tool_definition() {
        let tool = GlobTool::new();
        let def = tool.definition();
        assert_eq!(def.name, "glob");
    }
}
