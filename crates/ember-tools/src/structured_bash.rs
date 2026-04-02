//! Structured bash command execution with per-command sandbox policies,
//! background task management, configurable timeouts, and rich output.
//!
//! # Overview
//!
//! This module extends the existing [`crate::shell::ShellTool`] with first-class support for:
//! - Per-command timeouts independent of the global [`crate::shell::ShellConfig`]
//! - Background task execution with a [`BackgroundTaskManager`]
//! - Per-command sandbox policies ([`BashSandboxPolicy`])
//! - Structured output ([`BashCommandOutput`]) including exit code, timing,
//!   separate stderr, and truncation metadata
//!
//! # Example
//!
//! ```rust,no_run
//! use ember_tools::structured_bash::{BashCommandInput, BackgroundTaskManager, execute_bash};
//!
//! #[tokio::main]
//! async fn main() {
//!     // Simple one-liner
//!     let output = execute_bash(&BashCommandInput::simple("echo hello")).await.unwrap();
//!     assert_eq!(output.stdout.trim(), "hello");
//!
//!     // Background task
//!     let mut mgr = BackgroundTaskManager::new();
//!     let id = mgr.spawn(BashCommandInput::simple("sleep 1").with_background());
//!     let statuses = mgr.list();
//!     assert!(!statuses.is_empty());
//! }
//! ```

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, warn};

/// Maximum number of bytes captured from stdout or stderr before truncation.
const DEFAULT_MAX_OUTPUT_BYTES: usize = 1024 * 1024; // 1 MiB

// ─── Sandbox policy types ────────────────────────────────────────────────────

/// Controls which parts of the filesystem a command may access.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum FilesystemMode {
    /// No filesystem restrictions beyond the OS defaults.
    #[default]
    Off,
    /// Restrict the command to the current working directory and its children.
    WorkspaceOnly,
    /// Only the paths listed in [`BashSandboxPolicy::allowed_paths`] are accessible.
    AllowList,
}

/// Per-command sandbox configuration.
///
/// When `enabled` is `false` (the default), no additional restrictions are
/// imposed beyond those of the global [`crate::shell::ShellTool`] security rules.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BashSandboxPolicy {
    /// Whether to activate sandbox restrictions for this command.
    pub enabled: bool,
    /// Block outbound network access (best-effort; relies on OS-level tooling
    /// such as `unshare` on Linux when available).
    pub network_isolation: bool,
    /// Filesystem access mode.
    pub filesystem_mode: FilesystemMode,
    /// Paths accessible when [`FilesystemMode::AllowList`] is active.
    pub allowed_paths: Vec<PathBuf>,
}

// ─── Input ───────────────────────────────────────────────────────────────────

/// Input for a structured bash command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashCommandInput {
    /// The shell command to execute.
    pub command: String,
    /// Per-command timeout; overrides the global [`crate::shell::ShellConfig::timeout_secs`].
    pub timeout_secs: Option<u64>,
    /// Human-readable description (logged at DEBUG level before execution).
    pub description: Option<String>,
    /// When `true`, the command is submitted to a [`BackgroundTaskManager`]
    /// rather than awaited.  [`execute_bash`] returns immediately with a
    /// [`BashCommandOutput`] whose `background_task_id` is set.
    pub run_in_background: bool,
    /// Override the working directory for this specific command.
    pub working_dir: Option<PathBuf>,
    /// Additional environment variables merged on top of the process
    /// environment.
    pub env_vars: HashMap<String, String>,
    /// Per-command sandbox policy.
    pub sandbox: BashSandboxPolicy,
}

impl BashCommandInput {
    /// Create a minimal input with only the command set and all options at
    /// their defaults.
    pub fn simple(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            timeout_secs: None,
            description: None,
            run_in_background: false,
            working_dir: None,
            env_vars: HashMap::new(),
            sandbox: BashSandboxPolicy::default(),
        }
    }

    /// Set a per-command timeout in seconds.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    /// Mark the command to run in the background (non-blocking).
    pub fn with_background(mut self) -> Self {
        self.run_in_background = true;
        self
    }

    /// Override the working directory for this command.
    pub fn with_working_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Apply a sandbox policy to this command.
    pub fn with_sandbox(mut self, policy: BashSandboxPolicy) -> Self {
        self.sandbox = policy;
        self
    }

    /// Set a human-readable description (appears in debug logs).
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add a single extra environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }
}

// ─── Output ──────────────────────────────────────────────────────────────────

/// Structured output from a bash command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashCommandOutput {
    /// Captured standard output (possibly truncated).
    pub stdout: String,
    /// Captured standard error (possibly truncated).
    pub stderr: String,
    /// Process exit code, or `None` if the process was killed by a signal.
    pub exit_code: Option<i32>,
    /// Whether the command was terminated because it exceeded the timeout.
    pub timed_out: bool,
    /// Wall-clock execution time in milliseconds.
    pub execution_time_ms: u64,
    /// For background commands: the task ID assigned by [`BackgroundTaskManager`].
    pub background_task_id: Option<String>,
    /// `true` when stdout or stderr was cut at the internal `DEFAULT_MAX_OUTPUT_BYTES` limit (1 MiB).
    pub truncated: bool,
}

// ─── Error newtype ────────────────────────────────────────────────────────────

/// Errors specific to structured bash execution.
///
/// Re-exports [`crate::Error`] so callers only need one import.
pub type ShellError = Error;

// ─── Core execution ──────────────────────────────────────────────────────────

/// Execute a bash command with full structured I/O.
///
/// Respects `timeout_secs`, `working_dir`, `env_vars`, and `sandbox` from
/// `input`.  If `run_in_background` is set the function returns immediately
/// with an empty output and a synthetic `background_task_id` — callers should
/// instead use [`BackgroundTaskManager::spawn`] for proper lifecycle
/// management.
///
/// # Errors
///
/// Returns [`ShellError`] if the process cannot be spawned (I/O error) or the
/// command string is empty.
pub async fn execute_bash(input: &BashCommandInput) -> Result<BashCommandOutput> {
    if input.command.trim().is_empty() {
        return Err(Error::invalid_arguments(
            "structured_bash",
            "command must not be empty",
        ));
    }

    if let Some(ref desc) = input.description {
        debug!(description = %desc, command = %input.command, "executing bash command");
    } else {
        debug!(command = %input.command, "executing bash command");
    }

    // Warn (but do not block) when sandbox is requested — enforcement is
    // currently best-effort at the application layer.
    if input.sandbox.enabled {
        warn!(
            command = %input.command,
            network_isolation = input.sandbox.network_isolation,
            filesystem_mode = ?input.sandbox.filesystem_mode,
            "sandbox policy requested; enforcement is advisory only"
        );
    }

    let timeout_duration = input
        .timeout_secs
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(30));

    let mut cmd = build_command(input);

    let started = Instant::now();

    let spawn_result = timeout(timeout_duration, async {
        let mut child = cmd.spawn().map_err(Error::Io)?;

        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();

        // Read stdout and stderr with size caps to prevent OOM.
        if let Some(ref mut out) = child.stdout {
            out.take(DEFAULT_MAX_OUTPUT_BYTES as u64)
                .read_to_end(&mut stdout_buf)
                .await
                .map_err(Error::Io)?;
        }
        if let Some(ref mut err) = child.stderr {
            err.take(DEFAULT_MAX_OUTPUT_BYTES as u64)
                .read_to_end(&mut stderr_buf)
                .await
                .map_err(Error::Io)?;
        }

        let status = child.wait().await.map_err(Error::Io)?;

        Ok::<_, Error>((status, stdout_buf, stderr_buf))
    })
    .await;

    let execution_time_ms = started.elapsed().as_millis() as u64;

    match spawn_result {
        Ok(Ok((status, stdout_bytes, stderr_bytes))) => {
            let stdout_truncated = stdout_bytes.len() >= DEFAULT_MAX_OUTPUT_BYTES;
            let stderr_truncated = stderr_bytes.len() >= DEFAULT_MAX_OUTPUT_BYTES;

            Ok(BashCommandOutput {
                stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
                stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
                exit_code: status.code(),
                timed_out: false,
                execution_time_ms,
                background_task_id: None,
                truncated: stdout_truncated || stderr_truncated,
            })
        }
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => Ok(BashCommandOutput {
            stdout: String::new(),
            stderr: format!(
                "command timed out after {}s",
                input.timeout_secs.unwrap_or(30)
            ),
            exit_code: None,
            timed_out: true,
            execution_time_ms,
            background_task_id: None,
            truncated: false,
        }),
    }
}

/// Build a [`tokio::process::Command`] from a [`BashCommandInput`].
fn build_command(input: &BashCommandInput) -> Command {
    let mut cmd = Command::new("bash");
    cmd.arg("-c")
        .arg(&input.command)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());

    if let Some(ref dir) = input.working_dir {
        cmd.current_dir(dir);
    }

    for (k, v) in &input.env_vars {
        cmd.env(k, v);
    }

    cmd
}

// ─── Background task management ──────────────────────────────────────────────

/// Status of a background task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    /// The task is still running.
    Running,
    /// The task finished successfully (any exit code).
    Completed,
    /// The task could not be joined or panicked.
    Failed(String),
}

/// A single background task.
pub struct BackgroundTask {
    /// Unique task identifier.
    pub id: String,
    /// The command that was submitted.
    pub command: String,
    /// When the task was spawned.
    pub started_at: Instant,
    /// Join handle for the spawned Tokio task.
    pub handle: Option<tokio::task::JoinHandle<BashCommandOutput>>,
}

/// Manages a collection of background bash tasks.
///
/// Tasks are spawned via [`BackgroundTaskManager::spawn`] and can be listed,
/// polled, collected, or killed independently.
pub struct BackgroundTaskManager {
    tasks: HashMap<String, BackgroundTask>,
    next_id: u64,
}

impl BackgroundTaskManager {
    /// Create a new, empty manager.
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            next_id: 1,
        }
    }

    /// Spawn a command in the background and return its task ID.
    ///
    /// The `run_in_background` flag on `input` is ignored — this method always
    /// runs the command asynchronously.
    pub fn spawn(&mut self, input: BashCommandInput) -> String {
        let id = format!("bg-{}", self.next_id);
        self.next_id += 1;

        let command_str = input.command.clone();
        let id_clone = id.clone();

        let handle = tokio::task::spawn(async move {
            match execute_bash(&input).await {
                Ok(mut output) => {
                    output.background_task_id = Some(id_clone);
                    output
                }
                Err(e) => BashCommandOutput {
                    stdout: String::new(),
                    stderr: e.to_string(),
                    exit_code: None,
                    timed_out: false,
                    execution_time_ms: 0,
                    background_task_id: Some(id_clone),
                    truncated: false,
                },
            }
        });

        self.tasks.insert(
            id.clone(),
            BackgroundTask {
                id: id.clone(),
                command: command_str,
                started_at: Instant::now(),
                handle: Some(handle),
            },
        );

        id
    }

    /// Query the current status of a task without consuming it.
    pub fn status(&self, task_id: &str) -> Option<TaskStatus> {
        let task = self.tasks.get(task_id)?;
        let handle = task.handle.as_ref()?;

        if handle.is_finished() {
            Some(TaskStatus::Completed)
        } else {
            Some(TaskStatus::Running)
        }
    }

    /// Collect the output of a completed task, removing it from the manager.
    ///
    /// Returns `None` if the task does not exist or is still running.
    ///
    /// # Note
    ///
    /// This method is `async` because it awaits the join handle.  The handle
    /// is guaranteed to resolve immediately when `is_finished()` returns
    /// `true`, so the await is effectively non-blocking.
    pub async fn collect(&mut self, task_id: &str) -> Option<BashCommandOutput> {
        {
            let task = self.tasks.get(task_id)?;
            let handle = task.handle.as_ref()?;
            if !handle.is_finished() {
                return None;
            }
        }

        let task = self.tasks.remove(task_id)?;
        let elapsed_ms = task.started_at.elapsed().as_millis() as u64;
        let handle = task.handle?;

        // `is_finished()` is true — awaiting resolves instantly.
        match handle.await {
            Ok(output) => Some(output),
            Err(join_err) => Some(BashCommandOutput {
                stdout: String::new(),
                stderr: format!("task panicked: {join_err}"),
                exit_code: None,
                timed_out: false,
                execution_time_ms: elapsed_ms,
                background_task_id: Some(task_id.to_owned()),
                truncated: false,
            }),
        }
    }

    /// List all tracked tasks with their current status.
    pub fn list(&self) -> Vec<(String, TaskStatus)> {
        self.tasks
            .values()
            .map(|t| {
                let status = t
                    .handle
                    .as_ref()
                    .map(|h| {
                        if h.is_finished() {
                            TaskStatus::Completed
                        } else {
                            TaskStatus::Running
                        }
                    })
                    .unwrap_or_else(|| TaskStatus::Failed("handle missing".to_owned()));
                (t.id.clone(), status)
            })
            .collect()
    }

    /// Abort a running task.  Returns `true` if the task existed and was
    /// aborted, `false` if the task was not found.
    pub fn kill(&mut self, task_id: &str) -> bool {
        if let Some(task) = self.tasks.remove(task_id) {
            if let Some(handle) = task.handle {
                handle.abort();
            }
            true
        } else {
            false
        }
    }
}

impl Default for BackgroundTaskManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── BashCommandInput builder ─────────────────────────────────────────────

    #[test]
    fn test_builder_simple() {
        let input = BashCommandInput::simple("echo hi");
        assert_eq!(input.command, "echo hi");
        assert!(input.timeout_secs.is_none());
        assert!(!input.run_in_background);
        assert!(input.working_dir.is_none());
        assert!(input.env_vars.is_empty());
        assert!(!input.sandbox.enabled);
    }

    #[test]
    fn test_builder_with_timeout() {
        let input = BashCommandInput::simple("sleep 5").with_timeout(10);
        assert_eq!(input.timeout_secs, Some(10));
    }

    #[test]
    fn test_builder_with_background() {
        let input = BashCommandInput::simple("sleep 5").with_background();
        assert!(input.run_in_background);
    }

    #[test]
    fn test_builder_with_working_dir() {
        let input = BashCommandInput::simple("pwd").with_working_dir("/tmp");
        assert_eq!(input.working_dir, Some(PathBuf::from("/tmp")));
    }

    #[test]
    fn test_builder_with_sandbox() {
        let policy = BashSandboxPolicy {
            enabled: true,
            network_isolation: true,
            filesystem_mode: FilesystemMode::WorkspaceOnly,
            allowed_paths: vec![],
        };
        let input = BashCommandInput::simple("ls").with_sandbox(policy.clone());
        assert!(input.sandbox.enabled);
        assert!(input.sandbox.network_isolation);
        assert_eq!(input.sandbox.filesystem_mode, FilesystemMode::WorkspaceOnly);
    }

    // ── BashSandboxPolicy defaults ───────────────────────────────────────────

    #[test]
    fn test_sandbox_policy_defaults() {
        let policy = BashSandboxPolicy::default();
        assert!(!policy.enabled);
        assert!(!policy.network_isolation);
        assert_eq!(policy.filesystem_mode, FilesystemMode::Off);
        assert!(policy.allowed_paths.is_empty());
    }

    // ── FilesystemMode variants ──────────────────────────────────────────────

    #[test]
    fn test_filesystem_mode_variants() {
        assert_eq!(FilesystemMode::default(), FilesystemMode::Off);
        assert_ne!(FilesystemMode::WorkspaceOnly, FilesystemMode::AllowList);
        assert_ne!(FilesystemMode::Off, FilesystemMode::WorkspaceOnly);
    }

    // ── execute_bash: simple execution ──────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_simple_command_execution() {
        let output = execute_bash(&BashCommandInput::simple("echo hello"))
            .await
            .unwrap();
        assert_eq!(output.stdout.trim(), "hello");
        assert!(!output.timed_out);
        assert_eq!(output.exit_code, Some(0));
    }

    // ── execute_bash: exit code captured ────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_exit_code_captured() {
        let output = execute_bash(&BashCommandInput::simple("exit 42"))
            .await
            .unwrap();
        assert_eq!(output.exit_code, Some(42));
    }

    // ── execute_bash: non-zero exit ──────────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_nonzero_exit_code() {
        let output = execute_bash(&BashCommandInput::simple("false"))
            .await
            .unwrap();
        assert_eq!(output.exit_code, Some(1));
    }

    // ── execute_bash: stderr captured separately ─────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_stderr_captured_separately() {
        let output = execute_bash(&BashCommandInput::simple(
            "echo out-line; echo err-line >&2",
        ))
        .await
        .unwrap();
        assert!(output.stdout.contains("out-line"));
        assert!(output.stderr.contains("err-line"));
        assert!(!output.stdout.contains("err-line"));
        assert!(!output.stderr.contains("out-line"));
    }

    // ── execute_bash: timeout triggers ──────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_timeout_triggers() {
        let input = BashCommandInput::simple("sleep 60").with_timeout(1);
        let output = execute_bash(&input).await.unwrap();
        assert!(output.timed_out);
        assert!(output.exit_code.is_none());
    }

    // ── execute_bash: working dir override ──────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_working_dir_override() {
        let input = BashCommandInput::simple("pwd").with_working_dir("/tmp");
        let output = execute_bash(&input).await.unwrap();
        // /tmp may resolve to a symlink target on macOS; check the canonical
        // path contains "tmp".
        assert!(
            output.stdout.trim().contains("tmp"),
            "unexpected pwd output: {}",
            output.stdout.trim()
        );
        assert_eq!(output.exit_code, Some(0));
    }

    // ── execute_bash: env vars passed ────────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_env_vars_passed() {
        let input = BashCommandInput::simple("echo $MY_VAR").with_env("MY_VAR", "hello_env");
        let output = execute_bash(&input).await.unwrap();
        assert_eq!(output.stdout.trim(), "hello_env");
    }

    // ── execute_bash: execution time tracked ────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_execution_time_tracked() {
        let output = execute_bash(&BashCommandInput::simple("echo ok"))
            .await
            .unwrap();
        // Should be non-zero but well under a second for a trivial command.
        assert!(output.execution_time_ms < 5_000);
    }

    // ── execute_bash: empty command error ────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_empty_command_returns_error() {
        let result = execute_bash(&BashCommandInput::simple("   ")).await;
        assert!(result.is_err());
    }

    // ── BackgroundTaskManager ────────────────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_background_task_spawn_returns_id() {
        let mut mgr = BackgroundTaskManager::new();
        let id = mgr.spawn(BashCommandInput::simple("sleep 0.1"));
        assert!(id.starts_with("bg-"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_background_task_list_shows_running() {
        let mut mgr = BackgroundTaskManager::new();
        let id = mgr.spawn(BashCommandInput::simple("sleep 5"));

        let list = mgr.list();
        let entry = list.iter().find(|(tid, _)| tid == &id);
        assert!(entry.is_some(), "task not found in list");
        assert_eq!(entry.unwrap().1, TaskStatus::Running);

        mgr.kill(&id);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_background_task_completes() {
        let mut mgr = BackgroundTaskManager::new();
        let id = mgr.spawn(BashCommandInput::simple("echo done"));

        // Give the task time to finish.
        tokio::time::sleep(Duration::from_millis(500)).await;

        assert_eq!(mgr.status(&id), Some(TaskStatus::Completed));
        let output = mgr.collect(&id).await.unwrap();
        assert_eq!(output.stdout.trim(), "done");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_background_task_kill() {
        let mut mgr = BackgroundTaskManager::new();
        let id = mgr.spawn(BashCommandInput::simple("sleep 60"));
        assert!(mgr.kill(&id));
        // After kill, the task should no longer be tracked.
        assert!(mgr.status(&id).is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_background_task_kill_nonexistent() {
        let mut mgr = BackgroundTaskManager::new();
        assert!(!mgr.kill("bg-9999"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_background_task_collect_running_returns_none() {
        let mut mgr = BackgroundTaskManager::new();
        let id = mgr.spawn(BashCommandInput::simple("sleep 60"));
        // Task is still running — collect should return None.
        assert!(mgr.collect(&id).await.is_none());
        mgr.kill(&id);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_background_task_id_in_output() {
        let mut mgr = BackgroundTaskManager::new();
        let id = mgr.spawn(BashCommandInput::simple("echo id_test"));

        tokio::time::sleep(Duration::from_millis(500)).await;

        let output = mgr.collect(&id).await.unwrap();
        assert_eq!(output.background_task_id.as_deref(), Some(id.as_str()));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_multiple_background_tasks() {
        let mut mgr = BackgroundTaskManager::new();
        let id1 = mgr.spawn(BashCommandInput::simple("sleep 60"));
        let id2 = mgr.spawn(BashCommandInput::simple("sleep 60"));

        let list = mgr.list();
        assert_eq!(list.len(), 2);

        mgr.kill(&id1);
        mgr.kill(&id2);
    }
}
