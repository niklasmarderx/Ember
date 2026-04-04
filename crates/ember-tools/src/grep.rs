//! Grep tool — search file contents by regex pattern.
//!
//! Provides a dedicated `grep` tool so the LLM can search for code patterns,
//! function definitions, imports, etc. across a directory tree.

use crate::{registry::ToolOutput, Error, Result, ToolDefinition, ToolHandler};
use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::debug;

/// Maximum number of matching lines to return (prevents huge outputs).
const MAX_MATCHES: usize = 200;

/// Maximum file size (in bytes) to search — skip binaries / huge files.
const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024; // 2 MB

/// Grep tool for searching file contents.
pub struct GrepTool {
    enabled: bool,
}

impl GrepTool {
    /// Create a new `GrepTool`.
    pub fn new() -> Self {
        Self { enabled: true }
    }
}

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Recursively collect files under `dir`, respecting depth limit.
fn walk_dir(dir: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut results = Vec::new();
    walk_dir_inner(dir, 0, max_depth, &mut results);
    results
}

fn walk_dir_inner(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<PathBuf>) {
    if depth > max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden dirs and common non-source dirs
        if name.starts_with('.')
            || name == "node_modules"
            || name == "target"
            || name == "__pycache__"
            || name == "venv"
        {
            continue;
        }

        if path.is_dir() {
            walk_dir_inner(&path, depth + 1, max_depth, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

/// Check if a file is likely text (not binary).
fn is_text_extension(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Common source code / text extensions
    matches!(
        ext.as_str(),
        "rs" | "py"
            | "js"
            | "ts"
            | "jsx"
            | "tsx"
            | "go"
            | "java"
            | "c"
            | "cpp"
            | "h"
            | "hpp"
            | "cs"
            | "rb"
            | "php"
            | "swift"
            | "kt"
            | "scala"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "ps1"
            | "bat"
            | "cmd"
            | "lua"
            | "r"
            | "sql"
            | "html"
            | "css"
            | "scss"
            | "sass"
            | "less"
            | "xml"
            | "json"
            | "yaml"
            | "yml"
            | "toml"
            | "ini"
            | "cfg"
            | "conf"
            | "md"
            | "txt"
            | "rst"
            | "tex"
            | "csv"
            | "tsv"
            | "log"
            | "env"
            | "gitignore"
            | "dockerfile"
            | "makefile"
            | "cmake"
            | "gradle"
            | "sbt"
            | "cabal"
            | "zig"
            | "nim"
            | "ex"
            | "exs"
            | "erl"
            | "hs"
            | "ml"
            | "mli"
            | "vue"
            | "svelte"
            | "astro"
            | "prisma"
            | "proto"
            | "graphql"
            | "tf"
            | "hcl"
    ) || ext.is_empty() // files without extension (Makefile, Dockerfile, etc.)
}

#[async_trait]
impl ToolHandler for GrepTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "grep",
            "Search file contents for a regex pattern. Returns matching lines with file paths and line numbers. Use this to find function definitions, imports, string literals, or any code pattern.",
        )
        .with_parameters(serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regular expression pattern to search for (e.g., 'fn main', 'import.*React', 'TODO')"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in (defaults to current directory)"
                },
                "include": {
                    "type": "string",
                    "description": "File extension filter (e.g., 'rs', 'py', 'ts')"
                }
            },
            "required": ["pattern"]
        }))
    }

    async fn execute(&self, arguments: Value) -> Result<ToolOutput> {
        let pattern_str = arguments
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::invalid_arguments("grep", "Missing 'pattern' parameter"))?;

        let path_str = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let include_ext = arguments.get("include").and_then(|v| v.as_str());

        let regex = Regex::new(pattern_str)
            .map_err(|e| Error::invalid_arguments("grep", format!("Invalid regex: {}", e)))?;

        let base_path = PathBuf::from(path_str);
        let base_path = if base_path.is_absolute() {
            base_path
        } else {
            std::env::current_dir()?.join(&base_path)
        };

        debug!(pattern = pattern_str, path = %base_path.display(), "Grep search");

        // Collect files to search (blocking I/O — run off the async runtime)
        let base_clone = base_path.clone();
        let include_clone = include_ext.map(|s| s.to_string());
        let files = tokio::task::spawn_blocking(move || {
            let all = if base_clone.is_file() {
                vec![base_clone.clone()]
            } else {
                walk_dir(&base_clone, 10)
            };
            // Pre-filter by extension and text-ness to avoid re-stat later
            all.into_iter()
                .filter(|f| {
                    if let Some(ref ext) = include_clone {
                        let file_ext = f.extension().and_then(|e| e.to_str()).unwrap_or("");
                        file_ext.eq_ignore_ascii_case(ext)
                    } else {
                        is_text_extension(f)
                    }
                })
                .filter(|f| {
                    std::fs::metadata(f)
                        .map(|m| m.len() <= MAX_FILE_SIZE)
                        .unwrap_or(false)
                })
                .collect::<Vec<PathBuf>>()
        })
        .await
        .map_err(|e| Error::execution_failed("grep", format!("File walk failed: {}", e)))?;

        let mut matches: Vec<String> = Vec::new();
        let mut files_with_matches = 0usize;

        for file_path in &files {
            let content = match fs::read_to_string(file_path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            let rel_path = file_path
                .strip_prefix(&base_path)
                .unwrap_or(file_path)
                .to_string_lossy();

            let mut file_had_match = false;
            for (line_num, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    matches.push(format!("{}:{}: {}", rel_path, line_num + 1, line));
                    file_had_match = true;
                    if matches.len() >= MAX_MATCHES {
                        break;
                    }
                }
            }
            if file_had_match {
                files_with_matches += 1;
            }
            if matches.len() >= MAX_MATCHES {
                break;
            }
        }

        let output = if matches.is_empty() {
            format!("No matches found for pattern '{}'", pattern_str)
        } else {
            let header = format!(
                "Found {} match(es) in {} file(s):\n\n",
                matches.len(),
                files_with_matches
            );
            let truncated = if matches.len() >= MAX_MATCHES {
                format!("\n\n[… truncated at {} matches]", MAX_MATCHES)
            } else {
                String::new()
            };
            format!("{}{}{}", header, matches.join("\n"), truncated)
        };

        Ok(ToolOutput::success_with_data(
            &output,
            serde_json::json!({
                "match_count": matches.len(),
                "files_with_matches": files_with_matches,
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
    fn test_is_text_extension() {
        assert!(is_text_extension(Path::new("main.rs")));
        assert!(is_text_extension(Path::new("index.ts")));
        assert!(is_text_extension(Path::new("Makefile")));
        assert!(!is_text_extension(Path::new("image.png")));
        assert!(!is_text_extension(Path::new("binary.exe")));
    }
}
