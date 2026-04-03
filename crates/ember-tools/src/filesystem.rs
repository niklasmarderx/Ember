//! Filesystem operations tool.

use crate::patch::{write_file_tracked, FileOpHistory};
use crate::{registry::ToolOutput, Error, Result, ToolDefinition, ToolHandler};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio::fs;
use tracing::debug;

// ──────────────────────────────────────────────────────────────────────────────
// Global undo history
// ──────────────────────────────────────────────────────────────────────────────

/// Process-wide history of tracked filesystem writes, used by `/undo`.
///
/// Guarded by a `Mutex` so it is safe to share across async tasks.
static FILE_OP_HISTORY: Mutex<Option<FileOpHistory>> = Mutex::new(None);

/// Access the process-wide [`FileOpHistory`], initialising it on first use.
fn with_history<T>(f: impl FnOnce(&mut FileOpHistory) -> T) -> T {
    let mut guard = FILE_OP_HISTORY.lock().expect("FILE_OP_HISTORY poisoned");
    let history = guard.get_or_insert_with(|| FileOpHistory::new(50));
    f(history)
}

/// Undo the last file write recorded in the process-wide history.
///
/// Returns:
/// - `Ok(Some(path))` — the file at `path` was successfully restored.
/// - `Ok(None)`       — there is nothing to undo.
/// - `Err(_)`         — the restoration failed.
pub fn undo_last() -> anyhow::Result<Option<String>> {
    // Pull the last entry out of history and grab its path before restoring.
    let entry = with_history(|h| h.pop_last());
    match entry {
        None => Ok(None),
        Some(result) => {
            let path = result.file_path.to_string_lossy().to_string();
            crate::patch::undo_write(&result).map_err(anyhow::Error::from)?;
            Ok(Some(path))
        }
    }
}

/// Configuration for the filesystem tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemConfig {
    /// Allowed directories (empty = cwd only)
    pub allowed_paths: Vec<PathBuf>,

    /// Maximum file size to read in bytes
    pub max_read_bytes: usize,

    /// Maximum file size to write in bytes
    pub max_write_bytes: usize,

    /// Whether to allow file deletion
    pub allow_delete: bool,

    /// Whether to allow creating directories
    pub allow_mkdir: bool,
}

impl Default for FilesystemConfig {
    fn default() -> Self {
        Self {
            allowed_paths: Vec::new(),
            max_read_bytes: 10 * 1024 * 1024,  // 10MB
            max_write_bytes: 10 * 1024 * 1024, // 10MB
            allow_delete: false,
            allow_mkdir: true,
        }
    }
}

/// Filesystem operations tool.
pub struct FilesystemTool {
    config: FilesystemConfig,
    enabled: bool,
}

impl FilesystemTool {
    /// Create a new filesystem tool with default configuration.
    pub fn new() -> Self {
        Self {
            config: FilesystemConfig::default(),
            enabled: true,
        }
    }

    /// Create a filesystem tool with custom configuration.
    pub fn with_config(config: FilesystemConfig) -> Self {
        Self {
            config,
            enabled: true,
        }
    }

    /// Add an allowed path.
    pub fn allow_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.allowed_paths.push(path.into());
        self
    }

    /// Allow file deletion.
    pub fn allow_delete(mut self, allow: bool) -> Self {
        self.config.allow_delete = allow;
        self
    }

    /// Set the enabled state.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Validate that a path is allowed.
    fn validate_path(&self, path: &Path) -> Result<PathBuf> {
        // Expand ~ and resolve the path
        let path_str = path.to_string_lossy();
        let expanded = shellexpand::tilde(&path_str).to_string();
        let expanded_path = PathBuf::from(&expanded);

        // Get absolute path
        let absolute = if expanded_path.is_absolute() {
            expanded_path
        } else {
            std::env::current_dir()?.join(&expanded_path)
        };

        // Canonicalize if the path exists (resolve symlinks)
        let canonical = if absolute.exists() {
            absolute.canonicalize()?
        } else {
            // For new files, canonicalize the parent
            if let Some(parent) = absolute.parent() {
                if parent.exists() {
                    let parent_canonical = parent.canonicalize()?;
                    parent_canonical.join(absolute.file_name().unwrap_or_default())
                } else {
                    absolute
                }
            } else {
                absolute
            }
        };

        // If no allowed paths are set, only allow current directory
        if self.config.allowed_paths.is_empty() {
            let cwd = std::env::current_dir()?.canonicalize()?;
            if !canonical.starts_with(&cwd) {
                return Err(Error::path_not_allowed(canonical.display().to_string()));
            }
        } else {
            // Check if path is under an allowed directory
            let allowed = self.config.allowed_paths.iter().any(|allowed| {
                if let Ok(allowed_canonical) = allowed.canonicalize() {
                    canonical.starts_with(&allowed_canonical)
                } else {
                    false
                }
            });

            if !allowed {
                return Err(Error::path_not_allowed(canonical.display().to_string()));
            }
        }

        Ok(canonical)
    }

    /// Read a file's contents.
    async fn read_file(&self, path: &str) -> Result<String> {
        let path = self.validate_path(Path::new(path))?;
        debug!(path = %path.display(), "Reading file");

        // Check file size
        let metadata = fs::metadata(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::FileNotFound(path.display().to_string())
            } else {
                Error::Io(e)
            }
        })?;

        if metadata.len() > self.config.max_read_bytes as u64 {
            return Err(Error::filesystem(format!(
                "File too large: {} bytes (max: {})",
                metadata.len(),
                self.config.max_read_bytes
            )));
        }

        let content = fs::read_to_string(&path).await?;
        Ok(content)
    }

    /// Write content to a file.
    async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        let path = self.validate_path(Path::new(path))?;
        debug!(path = %path.display(), "Writing file");

        if content.len() > self.config.max_write_bytes {
            return Err(Error::filesystem(format!(
                "Content too large: {} bytes (max: {})",
                content.len(),
                self.config.max_write_bytes
            )));
        }

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() && self.config.allow_mkdir {
                fs::create_dir_all(parent).await?;
            }
        }

        // Use tracked write so the change can be undone via /undo.
        let write_result = write_file_tracked(&path, content)?;
        with_history(|h| h.record(write_result));
        Ok(())
    }

    /// List directory contents.
    async fn list_directory(&self, path: &str) -> Result<Vec<FileInfo>> {
        let path = self.validate_path(Path::new(path))?;
        debug!(path = %path.display(), "Listing directory");

        let mut entries = Vec::new();
        let mut read_dir = fs::read_dir(&path).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let metadata = entry.metadata().await?;
            let file_type = if metadata.is_dir() {
                FileType::Directory
            } else if metadata.is_file() {
                FileType::File
            } else if metadata.is_symlink() {
                FileType::Symlink
            } else {
                FileType::Other
            };

            entries.push(FileInfo {
                name: entry.file_name().to_string_lossy().to_string(),
                path: entry.path().to_string_lossy().to_string(),
                file_type,
                size: metadata.len(),
                modified: metadata.modified().ok().map(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                }),
            });
        }

        // Sort by name
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(entries)
    }

    /// Delete a file or directory.
    async fn delete(&self, path: &str) -> Result<()> {
        if !self.config.allow_delete {
            return Err(Error::PermissionDenied(
                "File deletion is not allowed".to_string(),
            ));
        }

        let path = self.validate_path(Path::new(path))?;
        debug!(path = %path.display(), "Deleting");

        let metadata = fs::metadata(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::FileNotFound(path.display().to_string())
            } else {
                Error::Io(e)
            }
        })?;

        if metadata.is_dir() {
            fs::remove_dir_all(&path).await?;
        } else {
            fs::remove_file(&path).await?;
        }

        Ok(())
    }

    /// Search for files matching a pattern.
    async fn search(&self, path: &str, pattern: &str) -> Result<Vec<String>> {
        let base_path = self.validate_path(Path::new(path))?;
        debug!(path = %base_path.display(), pattern = pattern, "Searching files");

        let full_pattern = base_path.join(pattern).to_string_lossy().to_string();

        let matches: Vec<String> = glob::glob(&full_pattern)
            .map_err(|e| Error::filesystem(format!("Invalid pattern: {}", e)))?
            .filter_map(|entry| entry.ok())
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        Ok(matches)
    }
}

impl Default for FilesystemTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolHandler for FilesystemTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "filesystem",
            "Perform filesystem operations: read, write, list, delete, and search files.",
        )
        .with_parameters(serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "description": "The operation to perform",
                    "enum": ["read", "write", "list", "delete", "search", "exists"]
                },
                "path": {
                    "type": "string",
                    "description": "File or directory path"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write (for write operation)"
                },
                "pattern": {
                    "type": "string",
                    "description": "Search pattern (for search operation, e.g., '**/*.rs')"
                }
            },
            "required": ["operation", "path"]
        }))
    }

    async fn execute(&self, arguments: Value) -> Result<ToolOutput> {
        let operation = arguments
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Error::invalid_arguments("filesystem", "Missing 'operation' parameter")
            })?;

        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::invalid_arguments("filesystem", "Missing 'path' parameter"))?;

        match operation {
            "read" => {
                let content = self.read_file(path).await?;
                Ok(ToolOutput::success(content))
            }
            "write" => {
                let content = arguments
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        Error::invalid_arguments(
                            "filesystem",
                            "Missing 'content' for write operation",
                        )
                    })?;
                self.write_file(path, content).await?;
                Ok(ToolOutput::success(format!(
                    "Successfully wrote to {}",
                    path
                )))
            }
            "list" => {
                let entries = self.list_directory(path).await?;
                let output = entries
                    .iter()
                    .map(|e| {
                        let type_indicator = match e.file_type {
                            FileType::Directory => "[DIR]",
                            FileType::File => "[FILE]",
                            FileType::Symlink => "[LINK]",
                            FileType::Other => "[OTHER]",
                        };
                        format!("{} {} ({} bytes)", type_indicator, e.name, e.size)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(ToolOutput::success_with_data(
                    output,
                    serde_json::to_value(&entries).unwrap_or_default(),
                ))
            }
            "delete" => {
                self.delete(path).await?;
                Ok(ToolOutput::success(format!(
                    "Successfully deleted {}",
                    path
                )))
            }
            "search" => {
                let pattern = arguments
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .unwrap_or("*");
                let matches = self.search(path, pattern).await?;
                let output = if matches.is_empty() {
                    "No files found".to_string()
                } else {
                    matches.join("\n")
                };
                Ok(ToolOutput::success_with_data(
                    output,
                    serde_json::json!({ "matches": matches }),
                ))
            }
            "exists" => {
                let validated = self.validate_path(Path::new(path));
                let exists = validated.map(|p| p.exists()).unwrap_or(false);
                Ok(ToolOutput::success_with_data(
                    if exists {
                        "Path exists"
                    } else {
                        "Path does not exist"
                    },
                    serde_json::json!({ "exists": exists }),
                ))
            }
            _ => Err(Error::invalid_arguments(
                "filesystem",
                format!("Unknown operation: {}", operation),
            )),
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Information about a file or directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// File name
    pub name: String,

    /// Full path
    pub path: String,

    /// Type of file
    pub file_type: FileType,

    /// Size in bytes
    pub size: u64,

    /// Last modified timestamp (Unix seconds)
    pub modified: Option<u64>,
}

/// Type of filesystem entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    /// Regular file
    File,
    /// Directory
    Directory,
    /// Symbolic link
    Symlink,
    /// Other (device, socket, etc.)
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_read_write_file() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FilesystemTool::new().allow_path(temp_dir.path());

        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_string_lossy().to_string();

        // Write
        let args = serde_json::json!({
            "operation": "write",
            "path": &file_path_str,
            "content": "Hello, World!"
        });
        let result = tool.execute(args).await.unwrap();
        assert!(result.success);

        // Read
        let args = serde_json::json!({
            "operation": "read",
            "path": &file_path_str
        });
        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Hello, World!"));
    }

    #[tokio::test]
    async fn test_list_directory() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FilesystemTool::new().allow_path(temp_dir.path());

        // Create some files
        fs::write(temp_dir.path().join("file1.txt"), "content1")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "content2")
            .await
            .unwrap();
        fs::create_dir(temp_dir.path().join("subdir"))
            .await
            .unwrap();

        let args = serde_json::json!({
            "operation": "list",
            "path": temp_dir.path().to_string_lossy()
        });
        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("file1.txt"));
        assert!(result.output.contains("file2.txt"));
        assert!(result.output.contains("[DIR]"));
    }

    #[tokio::test]
    async fn test_path_validation() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FilesystemTool::new().allow_path(temp_dir.path());

        // Valid path
        assert!(tool.validate_path(temp_dir.path()).is_ok());

        // Invalid path (outside allowed)
        assert!(tool.validate_path(Path::new("/etc/passwd")).is_err());
    }

    #[tokio::test]
    async fn test_file_exists() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FilesystemTool::new().allow_path(temp_dir.path());

        let file_path = temp_dir.path().join("exists.txt");
        fs::write(&file_path, "content").await.unwrap();

        let args = serde_json::json!({
            "operation": "exists",
            "path": file_path.to_string_lossy()
        });
        let result = tool.execute(args).await.unwrap();
        assert!(result.data.unwrap()["exists"].as_bool().unwrap());
    }
}
