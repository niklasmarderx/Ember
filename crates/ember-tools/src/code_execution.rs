//! Code Execution Tool for running Python and JavaScript code.
//!
//! This module provides sandboxed code execution capabilities for:
//! - Python code (via subprocess)
//! - JavaScript code (via Node.js subprocess)
//!
//! # Security
//!
//! Code execution is inherently dangerous. This tool provides several
//! safety mechanisms:
//! - Configurable execution timeout
//! - Working directory restrictions
//! - Environment variable filtering
//! - Output size limits
//! - Optional sandbox mode (future: WASM isolation)

use crate::{Error, Result, ToolDefinition, ToolHandler, ToolOutput};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{debug, error, info, warn};

/// Supported programming languages for code execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Python (python3)
    Python,
    /// JavaScript (Node.js)
    JavaScript,
    /// Shell/Bash
    Shell,
}

impl Language {
    /// Get the command to execute this language.
    fn command(&self) -> &'static str {
        match self {
            Self::Python => "python3",
            Self::JavaScript => "node",
            Self::Shell => "bash",
        }
    }

    /// Get the file extension for this language.
    #[allow(dead_code)]
    fn extension(&self) -> &'static str {
        match self {
            Self::Python => "py",
            Self::JavaScript => "js",
            Self::Shell => "sh",
        }
    }

    /// Get additional command arguments.
    fn args(&self) -> Vec<&'static str> {
        match self {
            Self::Python => vec!["-u"], // Unbuffered output
            Self::JavaScript => vec![],
            Self::Shell => vec![],
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Python => write!(f, "python"),
            Self::JavaScript => write!(f, "javascript"),
            Self::Shell => write!(f, "shell"),
        }
    }
}

impl std::str::FromStr for Language {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "python" | "python3" | "py" => Ok(Self::Python),
            "javascript" | "js" | "node" => Ok(Self::JavaScript),
            "shell" | "bash" | "sh" => Ok(Self::Shell),
            _ => Err(format!(
                "Unknown language: {}. Supported: python, javascript, shell",
                s
            )),
        }
    }
}

/// Configuration for code execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExecutionConfig {
    /// Maximum execution time
    pub timeout: Duration,

    /// Maximum output size in bytes
    pub max_output_size: usize,

    /// Working directory for execution
    pub working_dir: Option<PathBuf>,

    /// Allowed environment variables (whitelist)
    pub allowed_env_vars: Vec<String>,

    /// Whether to allow network access (if sandbox supports it)
    pub allow_network: bool,

    /// Whether to allow file system access
    pub allow_filesystem: bool,

    /// Languages that are enabled
    pub enabled_languages: Vec<Language>,

    /// Custom Python path
    pub python_path: Option<PathBuf>,

    /// Custom Node.js path
    pub node_path: Option<PathBuf>,
}

impl Default for CodeExecutionConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_output_size: 1024 * 1024, // 1 MB
            working_dir: None,
            allowed_env_vars: vec![
                "PATH".to_string(),
                "HOME".to_string(),
                "USER".to_string(),
                "LANG".to_string(),
                "LC_ALL".to_string(),
            ],
            allow_network: false,
            allow_filesystem: true,
            enabled_languages: vec![Language::Python, Language::JavaScript],
            python_path: None,
            node_path: None,
        }
    }
}

/// Result of code execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Standard output
    pub stdout: String,

    /// Standard error
    pub stderr: String,

    /// Exit code (if process completed)
    pub exit_code: Option<i32>,

    /// Whether execution timed out
    pub timed_out: bool,

    /// Execution time in milliseconds
    pub execution_time_ms: u64,

    /// Whether output was truncated
    pub output_truncated: bool,
}

impl ExecutionResult {
    /// Check if execution was successful.
    pub fn is_success(&self) -> bool {
        !self.timed_out && self.exit_code == Some(0)
    }

    /// Get combined output.
    pub fn output(&self) -> String {
        if self.stderr.is_empty() {
            self.stdout.clone()
        } else if self.stdout.is_empty() {
            self.stderr.clone()
        } else {
            format!("{}\n---stderr---\n{}", self.stdout, self.stderr)
        }
    }
}

/// Code Execution Tool.
pub struct CodeExecutionTool {
    config: CodeExecutionConfig,
}

impl CodeExecutionTool {
    /// Create a new code execution tool with default configuration.
    pub fn new() -> Self {
        Self {
            config: CodeExecutionConfig::default(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: CodeExecutionConfig) -> Self {
        Self { config }
    }

    /// Execute code in the specified language.
    pub async fn execute(&self, language: Language, code: &str) -> Result<ExecutionResult> {
        // Check if language is enabled
        if !self.config.enabled_languages.contains(&language) {
            return Err(Error::invalid_arguments(
                "execute_code",
                format!("Language {} is not enabled", language),
            ));
        }

        info!(language = %language, code_len = code.len(), "Executing code");

        let start = std::time::Instant::now();

        // Get the command path
        let cmd_path = match language {
            Language::Python => self.config.python_path.clone(),
            Language::JavaScript => self.config.node_path.clone(),
            Language::Shell => None,
        };

        let cmd_name = cmd_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| language.command().to_string());

        // Build command
        let mut cmd = Command::new(&cmd_name);

        // Add language-specific args
        for arg in language.args() {
            cmd.arg(arg);
        }

        // For Python and JavaScript, we pass code via stdin
        // For Shell, we also use stdin
        cmd.arg("-");

        // Set working directory
        if let Some(ref dir) = self.config.working_dir {
            cmd.current_dir(dir);
        }

        // Filter environment variables
        cmd.env_clear();
        for key in &self.config.allowed_env_vars {
            if let Ok(value) = std::env::var(key) {
                cmd.env(key, value);
            }
        }

        // Set up stdio
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Spawn process
        let mut child = cmd.spawn().map_err(|e| {
            Error::execution_failed(
                "execute_code",
                format!("Failed to spawn {}: {}", cmd_name, e),
            )
        })?;

        // Write code to stdin
        if let Some(mut stdin) = child.stdin.take() {
            let code_bytes = code.as_bytes().to_vec();
            tokio::spawn(async move {
                let _ = stdin.write_all(&code_bytes).await;
            });
        }

        // Wait for completion with timeout
        let timeout_duration = self.config.timeout;
        let max_output = self.config.max_output_size;

        let output_result = tokio::select! {
            result = child.wait_with_output() => {
                match result {
                    Ok(output) => Ok(output),
                    Err(e) => Err(e),
                }
            }
            _ = tokio::time::sleep(timeout_duration) => {
                // Timeout occurred - we cannot kill the child here since wait_with_output took ownership
                // Return a timeout indicator
                Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Execution timed out"))
            }
        };

        let execution_time_ms = start.elapsed().as_millis() as u64;

        match output_result {
            Ok(output) => {
                let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let mut output_truncated = false;

                // Truncate if needed
                if stdout.len() > max_output {
                    stdout.truncate(max_output);
                    stdout.push_str("\n... [output truncated]");
                    output_truncated = true;
                }
                if stderr.len() > max_output {
                    stderr.truncate(max_output);
                    stderr.push_str("\n... [output truncated]");
                    output_truncated = true;
                }

                debug!(
                    exit_code = output.status.code(),
                    stdout_len = stdout.len(),
                    stderr_len = stderr.len(),
                    "Code execution completed"
                );

                Ok(ExecutionResult {
                    stdout,
                    stderr,
                    exit_code: output.status.code(),
                    timed_out: false,
                    execution_time_ms,
                    output_truncated,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                warn!(
                    timeout_secs = timeout_duration.as_secs(),
                    "Code execution timed out"
                );

                Ok(ExecutionResult {
                    stdout: String::new(),
                    stderr: format!(
                        "Execution timed out after {} seconds",
                        timeout_duration.as_secs()
                    ),
                    exit_code: None,
                    timed_out: true,
                    execution_time_ms,
                    output_truncated: false,
                })
            }
            Err(e) => {
                error!(error = %e, "Process execution failed");
                Err(Error::execution_failed(
                    "execute_code",
                    format!("Process execution failed: {}", e),
                ))
            }
        }
    }

    /// Execute Python code.
    pub async fn execute_python(&self, code: &str) -> Result<ExecutionResult> {
        self.execute(Language::Python, code).await
    }

    /// Execute JavaScript code.
    pub async fn execute_javascript(&self, code: &str) -> Result<ExecutionResult> {
        self.execute(Language::JavaScript, code).await
    }

    /// Check if a language runtime is available.
    pub async fn is_available(&self, language: Language) -> bool {
        let cmd = match language {
            Language::Python => self
                .config
                .python_path
                .clone()
                .unwrap_or_else(|| PathBuf::from("python3")),
            Language::JavaScript => self
                .config
                .node_path
                .clone()
                .unwrap_or_else(|| PathBuf::from("node")),
            Language::Shell => PathBuf::from("bash"),
        };

        Command::new(&cmd)
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Get version information for a language runtime.
    pub async fn get_version(&self, language: Language) -> Result<String> {
        let cmd = match language {
            Language::Python => self
                .config
                .python_path
                .clone()
                .unwrap_or_else(|| PathBuf::from("python3")),
            Language::JavaScript => self
                .config
                .node_path
                .clone()
                .unwrap_or_else(|| PathBuf::from("node")),
            Language::Shell => PathBuf::from("bash"),
        };

        let output = Command::new(&cmd)
            .arg("--version")
            .output()
            .await
            .map_err(|e| {
                Error::execution_failed("execute_code", format!("Failed to get version: {}", e))
            })?;

        let version = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Some tools output version to stderr
        let version_str = if version.trim().is_empty() {
            stderr.trim()
        } else {
            version.trim()
        };

        Ok(version_str.to_string())
    }
}

impl Default for CodeExecutionTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolHandler for CodeExecutionTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "execute_code",
            "Execute code in Python, JavaScript, or shell",
        )
        .add_string_param(
            "language",
            "Programming language (python, javascript, shell)",
            true,
        )
        .add_string_param("code", "The code to execute", true)
    }

    async fn execute(&self, arguments: Value) -> Result<ToolOutput> {
        let language_str = arguments
            .get("language")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Error::invalid_arguments("execute_code", "Missing 'language' parameter")
            })?;

        let language: Language = language_str
            .parse()
            .map_err(|e: String| Error::invalid_arguments("execute_code", e))?;

        let code = arguments
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::invalid_arguments("execute_code", "Missing 'code' parameter"))?;

        let result = CodeExecutionTool::execute(self, language, code).await?;

        let status = if result.is_success() {
            "success"
        } else if result.timed_out {
            "timeout"
        } else {
            "error"
        };

        let output = format!(
            "[{}] Exit code: {:?}\nExecution time: {}ms\n\n{}",
            status,
            result.exit_code,
            result.execution_time_ms,
            result.output()
        );

        if result.is_success() {
            Ok(ToolOutput::success(output))
        } else {
            Ok(ToolOutput::error(output))
        }
    }
}

/// REPL (Read-Eval-Print Loop) session for interactive code execution.
pub struct ReplSession {
    language: Language,
    config: CodeExecutionConfig,
    history: Vec<(String, ExecutionResult)>,
    context: HashMap<String, String>,
}

impl ReplSession {
    /// Create a new REPL session.
    pub fn new(language: Language) -> Self {
        Self {
            language,
            config: CodeExecutionConfig::default(),
            history: Vec::new(),
            context: HashMap::new(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(language: Language, config: CodeExecutionConfig) -> Self {
        Self {
            language,
            config,
            history: Vec::new(),
            context: HashMap::new(),
        }
    }

    /// Execute code and add to history.
    pub async fn execute(&mut self, code: &str) -> Result<ExecutionResult> {
        let tool = CodeExecutionTool::with_config(self.config.clone());

        // For Python, we might want to maintain state across executions
        // This is a simplified version - a full REPL would need process persistence
        let full_code = match self.language {
            Language::Python => {
                // Build context from previous successful definitions
                let mut context_code = String::new();
                for (key, value) in &self.context {
                    context_code.push_str(&format!("{} = {}\n", key, value));
                }
                format!("{}{}", context_code, code)
            }
            _ => code.to_string(),
        };

        let result = tool.execute(self.language, &full_code).await?;

        self.history.push((code.to_string(), result.clone()));

        Ok(result)
    }

    /// Get execution history.
    pub fn history(&self) -> &[(String, ExecutionResult)] {
        &self.history
    }

    /// Clear history.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Set a context variable (for Python state).
    pub fn set_context(&mut self, name: &str, value: &str) {
        self.context.insert(name.to_string(), value.to_string());
    }

    /// Clear context.
    pub fn clear_context(&mut self) {
        self.context.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_parsing() {
        assert_eq!("python".parse::<Language>().unwrap(), Language::Python);
        assert_eq!("py".parse::<Language>().unwrap(), Language::Python);
        assert_eq!(
            "javascript".parse::<Language>().unwrap(),
            Language::JavaScript
        );
        assert_eq!("js".parse::<Language>().unwrap(), Language::JavaScript);
        assert_eq!("shell".parse::<Language>().unwrap(), Language::Shell);
        assert!("unknown".parse::<Language>().is_err());
    }

    #[test]
    fn test_language_command() {
        assert_eq!(Language::Python.command(), "python3");
        assert_eq!(Language::JavaScript.command(), "node");
        assert_eq!(Language::Shell.command(), "bash");
    }

    #[test]
    fn test_execution_result_success() {
        let result = ExecutionResult {
            stdout: "Hello".to_string(),
            stderr: String::new(),
            exit_code: Some(0),
            timed_out: false,
            execution_time_ms: 100,
            output_truncated: false,
        };
        assert!(result.is_success());
    }

    #[test]
    fn test_execution_result_failure() {
        let result = ExecutionResult {
            stdout: String::new(),
            stderr: "Error".to_string(),
            exit_code: Some(1),
            timed_out: false,
            execution_time_ms: 100,
            output_truncated: false,
        };
        assert!(!result.is_success());
    }

    #[test]
    fn test_execution_result_timeout() {
        let result = ExecutionResult {
            stdout: String::new(),
            stderr: "Timeout".to_string(),
            exit_code: None,
            timed_out: true,
            execution_time_ms: 30000,
            output_truncated: false,
        };
        assert!(!result.is_success());
    }

    #[tokio::test]
    async fn test_code_execution_tool_definition() {
        let tool = CodeExecutionTool::new();
        let def = ToolHandler::definition(&tool);
        assert_eq!(def.name, "execute_code");
    }

    // Integration tests require actual runtimes
    #[tokio::test]
    #[ignore = "requires Python runtime - run with: cargo test -- --ignored"]
    async fn test_python_execution() {
        let tool = CodeExecutionTool::new();
        if !tool.is_available(Language::Python).await {
            return;
        }

        let result = tool
            .execute_python("print('Hello from Python')")
            .await
            .unwrap();
        assert!(result.is_success());
        assert!(result.stdout.contains("Hello from Python"));
    }

    #[tokio::test]
    #[ignore = "requires Node.js runtime - run with: cargo test -- --ignored"]
    async fn test_javascript_execution() {
        let tool = CodeExecutionTool::new();
        if !tool.is_available(Language::JavaScript).await {
            return;
        }

        let result = tool
            .execute_javascript("console.log('Hello from JavaScript')")
            .await
            .unwrap();
        assert!(result.is_success());
        assert!(result.stdout.contains("Hello from JavaScript"));
    }

    #[tokio::test]
    #[ignore = "requires Python runtime - run with: cargo test -- --ignored"]
    async fn test_timeout() {
        let mut config = CodeExecutionConfig::default();
        config.timeout = Duration::from_millis(100);

        let tool = CodeExecutionTool::with_config(config);
        if !tool.is_available(Language::Python).await {
            return;
        }

        let result = tool
            .execute_python("import time; time.sleep(10)")
            .await
            .unwrap();
        assert!(result.timed_out);
        assert!(!result.is_success());
    }
}
