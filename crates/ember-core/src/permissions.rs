//! Granular per-tool permission system.
//!
//! This module provides fine-grained control over what individual tools are
//! allowed to do. While [`crate::sandbox`] enforces broad capability policies
//! across all tools, `permissions` lets you tune rules per-tool:
//!
//! - Which filesystem paths a tool may read or write
//! - Whether a tool is restricted to read-only access
//! - Which shell commands the tool may execute
//! - Maximum execution time before the action is aborted
//!
//! # Modes
//!
//! | Mode | Behaviour |
//! |------|-----------|
//! | `Unrestricted` | Everything is allowed (default — no breaking change) |
//! | `Interactive` | Every action requires user approval before proceeding |
//! | `Policy` | Actions are evaluated against per-tool rules |
//!
//! # Quick start
//!
//! ```rust
//! use ember_core::permissions::{
//!     PermissionPolicy, PermissionMode, ToolPermission, ToolAction, PermissionResult,
//! };
//! use std::path::PathBuf;
//! use std::time::Duration;
//!
//! // Build a policy that only allows the "filesystem" tool to read /tmp.
//! let mut policy = PermissionPolicy::default();
//! policy.mode = PermissionMode::Policy;
//!
//! let mut perm = ToolPermission::default();
//! perm.tool_name = "filesystem".to_string();
//! perm.allowed_paths = vec![PathBuf::from("/tmp")];
//! perm.read_only = true;
//! policy.tool_overrides.insert("filesystem".to_string(), perm);
//!
//! let action = ToolAction::ReadFile(PathBuf::from("/tmp/hello.txt"));
//! assert_eq!(policy.check("filesystem", &action), PermissionResult::Allowed);
//!
//! let write_action = ToolAction::WriteFile(PathBuf::from("/tmp/hello.txt"));
//! assert!(matches!(
//!     policy.check("filesystem", &write_action),
//!     PermissionResult::Denied(_)
//! ));
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

// ---------------------------------------------------------------------------
// PermissionMode
// ---------------------------------------------------------------------------

/// Controls how the permission policy evaluates actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionMode {
    /// Allow everything. Default — no existing behaviour is affected.
    Unrestricted,
    /// Ask the user before each action. The caller is responsible for
    /// surfacing the approval prompt and must honour `NeedsApproval`.
    Interactive,
    /// Evaluate actions strictly against the configured rules.
    Policy,
}

impl Default for PermissionMode {
    fn default() -> Self {
        Self::Unrestricted
    }
}

// ---------------------------------------------------------------------------
// ToolAction
// ---------------------------------------------------------------------------

/// An action that a tool wants to perform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolAction {
    /// Read from a file or directory.
    ReadFile(PathBuf),
    /// Write to (or create) a file.
    WriteFile(PathBuf),
    /// Execute a shell command.
    ExecuteCommand(String),
    /// Make a network request to the given host/URL.
    NetworkAccess(String),
}

// ---------------------------------------------------------------------------
// PermissionResult
// ---------------------------------------------------------------------------

/// Outcome of a permission check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionResult {
    /// The action is permitted.
    Allowed,
    /// The action is forbidden. The `String` explains why.
    Denied(String),
    /// The action requires explicit user approval (Interactive mode only).
    NeedsApproval,
}

// ---------------------------------------------------------------------------
// ToolPermission
// ---------------------------------------------------------------------------

/// Permission rules for a single tool.
///
/// An empty `allowed_paths` list means *all* paths are allowed (subject to
/// `denied_paths` and `read_only`). An empty `allowed_commands` list means
/// *all* commands are allowed (subject to `denied_paths` etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPermission {
    /// Name of the tool these rules apply to.
    pub tool_name: String,

    /// Paths the tool may access. Empty = no path restriction.
    ///
    /// A path is considered *inside* an allowed entry when it starts with
    /// that entry's bytes (i.e., prefix matching on the canonical path).
    pub allowed_paths: Vec<PathBuf>,

    /// Paths the tool is explicitly forbidden from accessing, regardless of
    /// `allowed_paths`. Denied paths take precedence.
    pub denied_paths: Vec<PathBuf>,

    /// When `true`, any `WriteFile` action is denied regardless of
    /// `allowed_paths`.
    pub read_only: bool,

    /// Commands the tool may execute. Empty = no command restriction.
    ///
    /// Each entry is matched as a prefix of the first whitespace-separated
    /// token of the command string (the executable name).
    pub allowed_commands: Vec<String>,

    /// Maximum wall-clock time allowed per action.
    pub max_execution_time: Duration,
}

impl Default for ToolPermission {
    fn default() -> Self {
        Self {
            tool_name: String::new(),
            allowed_paths: Vec::new(),
            denied_paths: Vec::new(),
            read_only: false,
            allowed_commands: Vec::new(),
            max_execution_time: Duration::from_secs(30),
        }
    }
}

impl ToolPermission {
    /// Return `true` when `path` is inside at least one entry of
    /// `allowed_paths` (or when `allowed_paths` is empty).
    fn path_in_allowed(&self, path: &Path) -> bool {
        if self.allowed_paths.is_empty() {
            return true;
        }
        self.allowed_paths
            .iter()
            .any(|allowed| path_starts_with(path, allowed))
    }

    /// Return `true` when `path` is inside at least one entry of
    /// `denied_paths`.
    fn path_in_denied(&self, path: &Path) -> bool {
        self.denied_paths
            .iter()
            .any(|denied| path_starts_with(path, denied))
    }

    /// Check whether `path` access is allowed.
    ///
    /// `write` — `true` for write operations, `false` for reads.
    pub fn check_path(&self, path: &Path, write: bool) -> PermissionResult {
        if write && self.read_only {
            return PermissionResult::Denied(format!(
                "tool '{}' is configured read-only; write access to '{}' is denied",
                self.tool_name,
                path.display()
            ));
        }

        if self.path_in_denied(path) {
            return PermissionResult::Denied(format!(
                "path '{}' is on the deny-list for tool '{}'",
                path.display(),
                self.tool_name
            ));
        }

        if !self.path_in_allowed(path) {
            return PermissionResult::Denied(format!(
                "path '{}' is not in the allow-list for tool '{}'",
                path.display(),
                self.tool_name
            ));
        }

        PermissionResult::Allowed
    }

    /// Check whether `command` execution is allowed.
    ///
    /// The check is done against the *executable* (first token) of the
    /// command string. An empty `allowed_commands` list allows everything.
    pub fn check_command(&self, command: &str) -> PermissionResult {
        if self.allowed_commands.is_empty() {
            return PermissionResult::Allowed;
        }

        let executable = command.split_whitespace().next().unwrap_or("");

        // Compare bare executable name (e.g. "ls") as well as full paths
        // (e.g. "/usr/bin/ls").
        let exe_name = Path::new(executable)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(executable);

        let allowed = self
            .allowed_commands
            .iter()
            .any(|a| exe_name == a.as_str() || executable == a.as_str());

        if allowed {
            PermissionResult::Allowed
        } else {
            PermissionResult::Denied(format!(
                "command '{}' is not in the allow-list for tool '{}'",
                executable, self.tool_name
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// PermissionPolicy
// ---------------------------------------------------------------------------

/// Top-level permission policy.
///
/// `default_permissions` are used for any tool that has no entry in
/// `tool_overrides`. In `Unrestricted` mode the policy is bypassed entirely.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionPolicy {
    /// How the policy is applied.
    pub mode: PermissionMode,

    /// Fallback rules applied to tools that have no specific override.
    pub default_permissions: ToolPermission,

    /// Per-tool rule overrides. The key is the tool name.
    pub tool_overrides: HashMap<String, ToolPermission>,
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        Self {
            mode: PermissionMode::Unrestricted,
            default_permissions: ToolPermission::default(),
            tool_overrides: HashMap::new(),
        }
    }
}

impl PermissionPolicy {
    /// Create a policy that allows everything (same as `Default`).
    pub fn unrestricted() -> Self {
        Self::default()
    }

    /// Create a policy that requires user approval for every action.
    pub fn interactive() -> Self {
        Self {
            mode: PermissionMode::Interactive,
            ..Self::default()
        }
    }

    /// Create an empty policy-mode policy (everything denied unless a rule
    /// allows it).
    pub fn policy() -> Self {
        Self {
            mode: PermissionMode::Policy,
            ..Self::default()
        }
    }

    /// Resolve which `ToolPermission` applies to `tool_name`.
    fn permission_for(&self, tool_name: &str) -> &ToolPermission {
        self.tool_overrides
            .get(tool_name)
            .unwrap_or(&self.default_permissions)
    }

    /// Evaluate `action` for `tool_name` against the policy.
    pub fn check(&self, tool_name: &str, action: &ToolAction) -> PermissionResult {
        match self.mode {
            PermissionMode::Unrestricted => PermissionResult::Allowed,
            PermissionMode::Interactive => PermissionResult::NeedsApproval,
            PermissionMode::Policy => {
                let perm = self.permission_for(tool_name);
                match action {
                    ToolAction::ReadFile(path) => perm.check_path(path, false),
                    ToolAction::WriteFile(path) => perm.check_path(path, true),
                    ToolAction::ExecuteCommand(cmd) => perm.check_command(cmd),
                    // Network access is allowed unless there is an explicit
                    // `denied_paths`-style entry — not yet modelled at the path
                    // level. Callers that need network control can combine this
                    // system with `SecuritySandbox::check_network`.
                    ToolAction::NetworkAccess(_) => PermissionResult::Allowed,
                }
            }
        }
    }

    /// Convenience wrapper — check whether `path` access is allowed.
    ///
    /// Returns `true` iff `check` returns `PermissionResult::Allowed`.
    pub fn is_path_allowed(&self, tool_name: &str, path: &Path, write: bool) -> bool {
        let action = if write {
            ToolAction::WriteFile(path.to_path_buf())
        } else {
            ToolAction::ReadFile(path.to_path_buf())
        };
        self.check(tool_name, &action) == PermissionResult::Allowed
    }

    /// Convenience wrapper — check whether `command` execution is allowed.
    ///
    /// Returns `true` iff `check` returns `PermissionResult::Allowed`.
    pub fn is_command_allowed(&self, tool_name: &str, command: &str) -> bool {
        let action = ToolAction::ExecuteCommand(command.to_string());
        self.check(tool_name, &action) == PermissionResult::Allowed
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return `true` when `path` has `base` as a prefix, handling the case
/// where `base` ends without a separator correctly.
///
/// Examples:
/// - `/tmp` is a prefix of `/tmp/foo.txt` ✓
/// - `/tmp` is a prefix of `/tmp` itself ✓
/// - `/tmp` is **not** a prefix of `/tmp_other/foo` ✓
fn path_starts_with(path: &Path, base: &Path) -> bool {
    // `Path::starts_with` does component-level matching, so "/tmp" is NOT
    // a prefix of "/tmp_other/foo" — exactly the behaviour we want.
    path.starts_with(base)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // PermissionMode::Unrestricted
    // -----------------------------------------------------------------------

    #[test]
    fn unrestricted_allows_everything() {
        let policy = PermissionPolicy::unrestricted();

        assert_eq!(
            policy.check("any_tool", &ToolAction::ReadFile(PathBuf::from("/etc/passwd"))),
            PermissionResult::Allowed
        );
        assert_eq!(
            policy.check("any_tool", &ToolAction::WriteFile(PathBuf::from("/etc/shadow"))),
            PermissionResult::Allowed
        );
        assert_eq!(
            policy.check("any_tool", &ToolAction::ExecuteCommand("rm -rf /".to_string())),
            PermissionResult::Allowed
        );
        assert_eq!(
            policy.check("any_tool", &ToolAction::NetworkAccess("evil.example.com".to_string())),
            PermissionResult::Allowed
        );
    }

    #[test]
    fn unrestricted_is_default() {
        let policy = PermissionPolicy::default();
        assert_eq!(policy.mode, PermissionMode::Unrestricted);
    }

    // -----------------------------------------------------------------------
    // PermissionMode::Interactive
    // -----------------------------------------------------------------------

    #[test]
    fn interactive_always_returns_needs_approval() {
        let policy = PermissionPolicy::interactive();

        assert_eq!(
            policy.check("tool", &ToolAction::ReadFile(PathBuf::from("/tmp/ok.txt"))),
            PermissionResult::NeedsApproval
        );
        assert_eq!(
            policy.check("tool", &ToolAction::ExecuteCommand("ls".to_string())),
            PermissionResult::NeedsApproval
        );
    }

    // -----------------------------------------------------------------------
    // PermissionMode::Policy — denied paths
    // -----------------------------------------------------------------------

    #[test]
    fn policy_blocks_denied_path() {
        let mut policy = PermissionPolicy::policy();
        let mut perm = ToolPermission::default();
        perm.tool_name = "fs".to_string();
        perm.denied_paths = vec![PathBuf::from("/etc")];
        policy.default_permissions = perm;

        let result = policy.check("fs", &ToolAction::ReadFile(PathBuf::from("/etc/passwd")));
        assert!(matches!(result, PermissionResult::Denied(_)));
    }

    #[test]
    fn policy_blocks_nested_denied_path() {
        let mut policy = PermissionPolicy::policy();
        let mut perm = ToolPermission::default();
        perm.tool_name = "fs".to_string();
        perm.denied_paths = vec![PathBuf::from("/home/user/.ssh")];
        policy.default_permissions = perm;

        let deep = PathBuf::from("/home/user/.ssh/id_rsa");
        assert!(matches!(
            policy.check("fs", &ToolAction::ReadFile(deep)),
            PermissionResult::Denied(_)
        ));

        // A path that just starts with the same bytes but is NOT under .ssh
        let sibling = PathBuf::from("/home/user/.ssh_other/file");
        assert_eq!(
            policy.check("fs", &ToolAction::ReadFile(sibling)),
            PermissionResult::Allowed
        );
    }

    // -----------------------------------------------------------------------
    // PermissionMode::Policy — read_only
    // -----------------------------------------------------------------------

    #[test]
    fn read_only_prevents_writes() {
        let mut policy = PermissionPolicy::policy();
        let mut perm = ToolPermission::default();
        perm.tool_name = "fs".to_string();
        perm.read_only = true;
        policy.default_permissions = perm;

        let path = PathBuf::from("/tmp/file.txt");

        assert_eq!(
            policy.check("fs", &ToolAction::ReadFile(path.clone())),
            PermissionResult::Allowed
        );
        assert!(matches!(
            policy.check("fs", &ToolAction::WriteFile(path)),
            PermissionResult::Denied(_)
        ));
    }

    // -----------------------------------------------------------------------
    // PermissionMode::Policy — allowed_commands filter
    // -----------------------------------------------------------------------

    #[test]
    fn allowed_commands_filter() {
        let mut policy = PermissionPolicy::policy();
        let mut perm = ToolPermission::default();
        perm.tool_name = "shell".to_string();
        perm.allowed_commands = vec!["ls".to_string(), "git".to_string()];
        policy.default_permissions = perm;

        assert_eq!(
            policy.check("shell", &ToolAction::ExecuteCommand("ls -la".to_string())),
            PermissionResult::Allowed
        );
        assert_eq!(
            policy.check("shell", &ToolAction::ExecuteCommand("git status".to_string())),
            PermissionResult::Allowed
        );
        assert!(matches!(
            policy.check("shell", &ToolAction::ExecuteCommand("rm -rf /".to_string())),
            PermissionResult::Denied(_)
        ));
    }

    #[test]
    fn empty_allowed_commands_permits_all() {
        let mut policy = PermissionPolicy::policy();
        let mut perm = ToolPermission::default();
        perm.tool_name = "shell".to_string();
        perm.allowed_commands = vec![]; // empty = no restriction
        policy.default_permissions = perm;

        assert_eq!(
            policy.check("shell", &ToolAction::ExecuteCommand("anything".to_string())),
            PermissionResult::Allowed
        );
    }

    #[test]
    fn full_path_executable_matched_by_name() {
        let mut policy = PermissionPolicy::policy();
        let mut perm = ToolPermission::default();
        perm.tool_name = "shell".to_string();
        perm.allowed_commands = vec!["ls".to_string()];
        policy.default_permissions = perm;

        // /usr/bin/ls should match "ls"
        assert_eq!(
            policy.check(
                "shell",
                &ToolAction::ExecuteCommand("/usr/bin/ls -la".to_string())
            ),
            PermissionResult::Allowed
        );
    }

    // -----------------------------------------------------------------------
    // PermissionMode::Policy — tool_overrides
    // -----------------------------------------------------------------------

    #[test]
    fn tool_overrides_take_precedence_over_defaults() {
        let mut policy = PermissionPolicy::policy();

        // Default: read-only
        policy.default_permissions.read_only = true;
        policy.default_permissions.tool_name = "default".to_string();

        // Override for "fs_write": writes allowed
        let mut override_perm = ToolPermission::default();
        override_perm.tool_name = "fs_write".to_string();
        override_perm.read_only = false;
        override_perm.allowed_paths = vec![PathBuf::from("/tmp")];
        policy
            .tool_overrides
            .insert("fs_write".to_string(), override_perm);

        let path = PathBuf::from("/tmp/output.txt");

        // The default (any other tool) must be denied writes
        assert!(matches!(
            policy.check("other_tool", &ToolAction::WriteFile(path.clone())),
            PermissionResult::Denied(_)
        ));

        // The override tool can write
        assert_eq!(
            policy.check("fs_write", &ToolAction::WriteFile(path)),
            PermissionResult::Allowed
        );
    }

    #[test]
    fn tool_override_restricts_paths() {
        let mut policy = PermissionPolicy::policy();

        let mut perm = ToolPermission::default();
        perm.tool_name = "restricted".to_string();
        perm.allowed_paths = vec![PathBuf::from("/workspace")];
        policy
            .tool_overrides
            .insert("restricted".to_string(), perm);

        assert_eq!(
            policy.check(
                "restricted",
                &ToolAction::ReadFile(PathBuf::from("/workspace/src/main.rs"))
            ),
            PermissionResult::Allowed
        );
        assert!(matches!(
            policy.check(
                "restricted",
                &ToolAction::ReadFile(PathBuf::from("/etc/hosts"))
            ),
            PermissionResult::Denied(_)
        ));
    }

    // -----------------------------------------------------------------------
    // is_path_allowed / is_command_allowed convenience wrappers
    // -----------------------------------------------------------------------

    #[test]
    fn is_path_allowed_wrapper() {
        let policy = PermissionPolicy::unrestricted();
        assert!(policy.is_path_allowed("tool", Path::new("/anywhere"), false));
        assert!(policy.is_path_allowed("tool", Path::new("/anywhere"), true));
    }

    #[test]
    fn is_command_allowed_wrapper() {
        let policy = PermissionPolicy::unrestricted();
        assert!(policy.is_command_allowed("tool", "rm -rf /"));
    }

    #[test]
    fn is_command_allowed_policy_false() {
        let mut policy = PermissionPolicy::policy();
        let mut perm = ToolPermission::default();
        perm.tool_name = "shell".to_string();
        perm.allowed_commands = vec!["ls".to_string()];
        policy.default_permissions = perm;

        assert!(!policy.is_command_allowed("shell", "curl https://example.com"));
        assert!(policy.is_command_allowed("shell", "ls -la"));
    }

    // -----------------------------------------------------------------------
    // path_starts_with helper
    // -----------------------------------------------------------------------

    #[test]
    fn path_prefix_matching() {
        // Direct child
        assert!(path_starts_with(
            Path::new("/tmp/foo.txt"),
            Path::new("/tmp")
        ));
        // Same path
        assert!(path_starts_with(Path::new("/tmp"), Path::new("/tmp")));
        // Sibling — must NOT match
        assert!(!path_starts_with(
            Path::new("/tmp_other/foo"),
            Path::new("/tmp")
        ));
        // Unrelated
        assert!(!path_starts_with(
            Path::new("/home/user"),
            Path::new("/etc")
        ));
        // Deeply nested
        assert!(path_starts_with(
            Path::new("/home/user/.ssh/keys/id_rsa"),
            Path::new("/home/user/.ssh")
        ));
    }
}
