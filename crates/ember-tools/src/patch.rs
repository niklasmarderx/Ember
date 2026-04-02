//! Structured file operations with unified diff tracking.
//!
//! Every file write produces a [`FileWriteResult`] containing the computed patch
//! hunks, so callers always know exactly what changed. Hunks can be reversed for
//! undo and applied forward or backward with [`apply_patch`].
//!
//! The diff algorithm is a pure-Rust LCS-based line differ — no external crates.

use serde::{Deserialize, Serialize};
use std::fmt::Write as FmtWrite;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

// ──────────────────────────────────────────────────────────────────────────────
// Core types
// ──────────────────────────────────────────────────────────────────────────────

/// A single line in a unified diff hunk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DiffLine {
    /// Unchanged line (unified diff prefix: `" "`).
    Context(String),
    /// Line added in the new version (prefix: `"+"`).
    Added(String),
    /// Line removed from the original (prefix: `"-"`).
    Removed(String),
}

/// One contiguous block of changes in a unified diff.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchHunk {
    /// 1-based first line in the *original* file covered by this hunk.
    pub old_start: usize,
    /// Number of original lines spanned (context + removed).
    pub old_lines: usize,
    /// 1-based first line in the *result* file covered by this hunk.
    pub new_start: usize,
    /// Number of result lines spanned (context + added).
    pub new_lines: usize,
    /// Ordered diff lines for this hunk.
    pub lines: Vec<DiffLine>,
}

/// Everything produced by a tracked file write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWriteResult {
    /// Absolute path that was written.
    pub file_path: PathBuf,
    /// `true` when the file did not exist before this write.
    pub created: bool,
    /// Original content, or `None` when the file was newly created.
    pub original_content: Option<String>,
    /// Content that was written.
    pub new_content: String,
    /// Computed diff hunks.
    pub hunks: Vec<PatchHunk>,
    /// Total lines added across all hunks.
    pub lines_added: usize,
    /// Total lines removed across all hunks.
    pub lines_removed: usize,
}

// ──────────────────────────────────────────────────────────────────────────────
// Error type
// ──────────────────────────────────────────────────────────────────────────────

/// Errors that can occur while applying a patch.
#[derive(Debug, Error)]
pub enum PatchError {
    /// A hunk's context/removal lines do not match the current file content.
    #[error("hunk mismatch at original line {line}: expected {expected:?}, found {found:?}")]
    HunkMismatch {
        /// 1-based line number in the original where the mismatch occurred.
        line: usize,
        /// What the hunk expected to find.
        expected: String,
        /// What was actually in the file.
        found: String,
    },
    /// The patch refers to a line that does not exist.
    #[error("patch references line {line} but original only has {total} lines")]
    OutOfBounds {
        /// Referenced line (1-based).
        line: usize,
        /// Actual number of lines.
        total: usize,
    },
}

// ──────────────────────────────────────────────────────────────────────────────
// LCS-based diff
// ──────────────────────────────────────────────────────────────────────────────

/// Compute the Longest Common Subsequence of two line slices using bottom-up DP.
///
/// Returns a list of `(old_idx, new_idx)` pairs for lines that are shared.
fn lcs(old: &[&str], new: &[&str]) -> Vec<(usize, usize)> {
    let m = old.len();
    let n = new.len();

    // dp[i][j] = length of LCS of old[..i] and new[..j]
    // Use a flat Vec to avoid nested allocation.
    let mut dp = vec![0usize; (m + 1) * (n + 1)];

    for i in 1..=m {
        for j in 1..=n {
            dp[i * (n + 1) + j] = if old[i - 1] == new[j - 1] {
                dp[(i - 1) * (n + 1) + (j - 1)] + 1
            } else {
                dp[(i - 1) * (n + 1) + j].max(dp[i * (n + 1) + (j - 1)])
            };
        }
    }

    // Back-track to recover the actual pairs.
    let mut pairs = Vec::new();
    let (mut i, mut j) = (m, n);
    while i > 0 && j > 0 {
        if old[i - 1] == new[j - 1] {
            pairs.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if dp[(i - 1) * (n + 1) + j] >= dp[i * (n + 1) + (j - 1)] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    pairs.reverse();
    pairs
}

/// Compute unified diff hunks (3 lines of context) between two strings.
///
/// Returns an empty `Vec` when `original == modified`.
pub fn compute_diff(original: &str, modified: &str) -> Vec<PatchHunk> {
    // Fast path: identical content.
    if original == modified {
        return Vec::new();
    }

    let old_lines: Vec<&str> = original.lines().collect();
    let new_lines: Vec<&str> = modified.lines().collect();

    // Build a flat edit script from the LCS.
    // Each element is `(old_idx_opt, new_idx_opt)`.
    let common = lcs(&old_lines, &new_lines);

    // Reconstruct the full edit script.
    // We interleave removals, common lines and additions in order.
    #[allow(clippy::items_after_statements)]
    #[derive(Debug, Clone, Copy)]
    enum Edit {
        Keep(usize, usize), // (old_idx, new_idx)
        Remove(usize),      // old_idx
        Add(usize),         // new_idx
    }

    let mut edits: Vec<Edit> = Vec::new();
    let (mut oi, mut ni) = (0usize, 0usize);

    for &(ci, cj) in &common {
        // Everything before the common line on the old side is removed.
        while oi < ci {
            edits.push(Edit::Remove(oi));
            oi += 1;
        }
        // Everything before the common line on the new side is added.
        while ni < cj {
            edits.push(Edit::Add(ni));
            ni += 1;
        }
        edits.push(Edit::Keep(oi, ni));
        oi += 1;
        ni += 1;
    }
    // Trailing removals/additions after last common line.
    while oi < old_lines.len() {
        edits.push(Edit::Remove(oi));
        oi += 1;
    }
    while ni < new_lines.len() {
        edits.push(Edit::Add(ni));
        ni += 1;
    }

    // Group into hunks with 3 lines of context.
    const CONTEXT: usize = 3;

    // Find ranges of "changed" edits (Remove/Add) and expand them by CONTEXT lines.
    // A hunk covers [edit_lo, edit_hi) in the edits slice.
    let changed_positions: Vec<usize> = edits
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e, Edit::Remove(_) | Edit::Add(_)))
        .map(|(i, _)| i)
        .collect();

    if changed_positions.is_empty() {
        return Vec::new();
    }

    // Merge nearby change groups into single hunk ranges.
    let mut hunk_ranges: Vec<(usize, usize)> = Vec::new(); // (lo, hi) in edits index
    let first = changed_positions[0];
    let lo = first.saturating_sub(CONTEXT);
    let hi = (first + CONTEXT + 1).min(edits.len());
    hunk_ranges.push((lo, hi));

    for &pos in &changed_positions[1..] {
        let last = hunk_ranges.last_mut().unwrap();
        let extended_hi = (pos + CONTEXT + 1).min(edits.len());
        let expanded_lo = pos.saturating_sub(CONTEXT);
        if expanded_lo <= last.1 {
            // Overlapping or adjacent — extend.
            last.1 = extended_hi.max(last.1);
        } else {
            hunk_ranges.push((expanded_lo, extended_hi));
        }
    }

    // Convert each hunk range into a PatchHunk.
    let mut hunks = Vec::new();
    for (lo, hi) in hunk_ranges {
        let mut diff_lines: Vec<DiffLine> = Vec::new();
        let mut old_start: Option<usize> = None;
        let mut new_start: Option<usize> = None;
        let mut old_count = 0usize;
        let mut new_count = 0usize;

        for edit in &edits[lo..hi] {
            match *edit {
                Edit::Keep(oi, ni) => {
                    old_start.get_or_insert(oi);
                    new_start.get_or_insert(ni);
                    diff_lines.push(DiffLine::Context(old_lines[oi].to_owned()));
                    old_count += 1;
                    new_count += 1;
                }
                Edit::Remove(oi) => {
                    old_start.get_or_insert(oi);
                    // new_start stays at the last new index seen (or 0 if none yet)
                    if new_start.is_none() {
                        // Hunk starts with removals — figure out new_start from context.
                        // The new_start equals the first new-side line that will appear
                        // in context. Since we have no Keep before this, use the new
                        // offset of the first later Keep (or 0).
                        new_start = Some(
                            edits[lo..hi]
                                .iter()
                                .find_map(|e| {
                                    if let Edit::Keep(_, nj) = e {
                                        Some(*nj)
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or(0),
                        );
                    }
                    diff_lines.push(DiffLine::Removed(old_lines[oi].to_owned()));
                    old_count += 1;
                }
                Edit::Add(ni) => {
                    if old_start.is_none() {
                        old_start = Some(
                            edits[lo..hi]
                                .iter()
                                .find_map(|e| {
                                    if let Edit::Keep(oj, _) = e {
                                        Some(*oj)
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or(0),
                        );
                    }
                    new_start.get_or_insert(ni);
                    diff_lines.push(DiffLine::Added(new_lines[ni].to_owned()));
                    new_count += 1;
                }
            }
        }

        // old_start / new_start are 0-based indices; convert to 1-based.
        let old_start_1 = old_start.map(|x| x + 1).unwrap_or(1);
        let new_start_1 = new_start.map(|x| x + 1).unwrap_or(1);

        hunks.push(PatchHunk {
            old_start: old_start_1,
            old_lines: old_count,
            new_start: new_start_1,
            new_lines: new_count,
            lines: diff_lines,
        });
    }

    hunks
}

// ──────────────────────────────────────────────────────────────────────────────
// Apply & reverse
// ──────────────────────────────────────────────────────────────────────────────

/// Apply `hunks` to `original`, returning the patched string.
///
/// The hunks must be sorted by `old_start` (as produced by [`compute_diff`]).
pub fn apply_patch(original: &str, hunks: &[PatchHunk]) -> Result<String, PatchError> {
    let orig_lines: Vec<&str> = original.lines().collect();
    let mut result: Vec<&str> = Vec::with_capacity(orig_lines.len());
    // Track whether the original ended with a newline.
    let trailing_newline = original.ends_with('\n');

    // `cursor` is a 0-based index into `orig_lines` tracking how far we've consumed.
    let mut cursor = 0usize;

    for hunk in hunks {
        // old_start is 1-based.
        let hunk_old_start = hunk.old_start.saturating_sub(1); // 0-based

        // Validate we won't run past the end.
        if hunk_old_start > orig_lines.len() {
            return Err(PatchError::OutOfBounds {
                line: hunk.old_start,
                total: orig_lines.len(),
            });
        }

        // Copy unchanged lines before this hunk.
        for &line in &orig_lines[cursor..hunk_old_start] {
            result.push(line);
        }
        cursor = hunk_old_start;

        // Process the hunk lines.
        for dl in &hunk.lines {
            match dl {
                DiffLine::Context(expected) => {
                    if cursor >= orig_lines.len() {
                        return Err(PatchError::OutOfBounds {
                            line: cursor + 1,
                            total: orig_lines.len(),
                        });
                    }
                    let actual = orig_lines[cursor];
                    if actual != expected.as_str() {
                        return Err(PatchError::HunkMismatch {
                            line: cursor + 1,
                            expected: expected.clone(),
                            found: actual.to_owned(),
                        });
                    }
                    result.push(actual);
                    cursor += 1;
                }
                DiffLine::Removed(expected) => {
                    if cursor >= orig_lines.len() {
                        return Err(PatchError::OutOfBounds {
                            line: cursor + 1,
                            total: orig_lines.len(),
                        });
                    }
                    let actual = orig_lines[cursor];
                    if actual != expected.as_str() {
                        return Err(PatchError::HunkMismatch {
                            line: cursor + 1,
                            expected: expected.clone(),
                            found: actual.to_owned(),
                        });
                    }
                    // Skip — this line is being removed.
                    cursor += 1;
                }
                DiffLine::Added(line) => {
                    result.push(line.as_str());
                }
            }
        }
    }

    // Append any remaining original lines after the last hunk.
    for &line in &orig_lines[cursor..] {
        result.push(line);
    }

    let mut out = result.join("\n");
    if trailing_newline || (!original.is_empty() && out.len() < original.len()) {
        // Preserve trailing newline if original had one.
        if original.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

/// Reverse a set of hunks so they can undo the change (swap Added ↔ Removed,
/// swap old/new counts and start positions).
pub fn reverse_hunks(hunks: &[PatchHunk]) -> Vec<PatchHunk> {
    hunks
        .iter()
        .map(|h| {
            let lines: Vec<DiffLine> = h
                .lines
                .iter()
                .map(|dl| match dl {
                    DiffLine::Added(s) => DiffLine::Removed(s.clone()),
                    DiffLine::Removed(s) => DiffLine::Added(s.clone()),
                    DiffLine::Context(s) => DiffLine::Context(s.clone()),
                })
                .collect();

            PatchHunk {
                old_start: h.new_start,
                old_lines: h.new_lines,
                new_start: h.old_start,
                new_lines: h.old_lines,
                lines,
            }
        })
        .collect()
}

// ──────────────────────────────────────────────────────────────────────────────
// Format
// ──────────────────────────────────────────────────────────────────────────────

/// Format hunks as a unified diff string (similar to `diff -u` output).
pub fn format_unified_diff(file_path: &str, hunks: &[PatchHunk]) -> String {
    let mut out = String::new();
    if hunks.is_empty() {
        return out;
    }
    let _ = writeln!(out, "--- {file_path}");
    let _ = writeln!(out, "+++ {file_path}");
    for hunk in hunks {
        let _ = writeln!(
            out,
            "@@ -{},{} +{},{} @@",
            hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
        );
        for dl in &hunk.lines {
            let _ = match dl {
                DiffLine::Context(s) => writeln!(out, " {s}"),
                DiffLine::Added(s) => writeln!(out, "+{s}"),
                DiffLine::Removed(s) => writeln!(out, "-{s}"),
            };
        }
    }
    out
}

// ──────────────────────────────────────────────────────────────────────────────
// File I/O
// ──────────────────────────────────────────────────────────────────────────────

/// Write `new_content` to `path`, computing a diff against the previous content.
///
/// Returns a [`FileWriteResult`] with full change information.
pub fn write_file_tracked(path: &Path, new_content: &str) -> io::Result<FileWriteResult> {
    let (original_content, created) = if path.exists() {
        (Some(std::fs::read_to_string(path)?), false)
    } else {
        (None, true)
    };

    std::fs::write(path, new_content)?;

    let original_ref = original_content.as_deref().unwrap_or("");
    let hunks = compute_diff(original_ref, new_content);

    let lines_added = hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|dl| matches!(dl, DiffLine::Added(_)))
        .count();
    let lines_removed = hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|dl| matches!(dl, DiffLine::Removed(_)))
        .count();

    Ok(FileWriteResult {
        file_path: path.to_path_buf(),
        created,
        original_content,
        new_content: new_content.to_owned(),
        hunks,
        lines_added,
        lines_removed,
    })
}

/// Undo a previous write by restoring the original content (or deleting the
/// file if it was newly created).
pub fn undo_write(result: &FileWriteResult) -> io::Result<()> {
    if let Some(original) = &result.original_content {
        std::fs::write(&result.file_path, original)
    } else {
        // File was created by this operation — remove it.
        if result.file_path.exists() {
            std::fs::remove_file(&result.file_path)?;
        }
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// History
// ──────────────────────────────────────────────────────────────────────────────

/// Rolling history of tracked file operations.
///
/// Older entries are dropped when `max_history` is exceeded (FIFO).
pub struct FileOpHistory {
    operations: Vec<FileWriteResult>,
    max_history: usize,
}

impl FileOpHistory {
    /// Create a new history with a maximum depth.
    pub fn new(max_history: usize) -> Self {
        Self {
            operations: Vec::new(),
            max_history,
        }
    }

    /// Record a completed file operation.
    ///
    /// If the history is full the oldest entry is evicted.
    pub fn record(&mut self, result: FileWriteResult) {
        if self.max_history == 0 {
            return;
        }
        if self.operations.len() >= self.max_history {
            self.operations.remove(0);
        }
        self.operations.push(result);
    }

    /// Undo the most recent operation and remove it from history.
    ///
    /// Returns `None` if the history is empty.
    pub fn undo_last(&mut self) -> Option<io::Result<()>> {
        let last = self.operations.pop()?;
        Some(undo_write(&last))
    }

    /// Read-only view of all recorded operations (oldest first).
    pub fn history(&self) -> &[FileWriteResult] {
        &self.operations
    }

    /// Number of entries currently in history.
    pub fn len(&self) -> usize {
        self.operations.len()
    }

    /// `true` if no operations have been recorded yet.
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ── compute_diff ──────────────────────────────────────────────────────────

    #[test]
    fn test_diff_identical_files_no_hunks() {
        let text = "line one\nline two\nline three\n";
        let hunks = compute_diff(text, text);
        assert!(hunks.is_empty(), "identical input must produce no hunks");
    }

    #[test]
    fn test_diff_added_lines() {
        let original = "a\nb\nc\n";
        let modified = "a\nb\nnew\nc\n";
        let hunks = compute_diff(original, modified);
        assert!(!hunks.is_empty());
        let added: Vec<_> = hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|dl| matches!(dl, DiffLine::Added(_)))
            .collect();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0], &DiffLine::Added("new".to_owned()));
    }

    #[test]
    fn test_diff_removed_lines() {
        let original = "a\nb\nc\n";
        let modified = "a\nc\n";
        let hunks = compute_diff(original, modified);
        assert!(!hunks.is_empty());
        let removed: Vec<_> = hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|dl| matches!(dl, DiffLine::Removed(_)))
            .collect();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0], &DiffLine::Removed("b".to_owned()));
    }

    #[test]
    fn test_diff_modified_lines() {
        let original = "a\nold line\nc\n";
        let modified = "a\nnew line\nc\n";
        let hunks = compute_diff(original, modified);
        assert!(!hunks.is_empty());
        let all_lines: Vec<_> = hunks.iter().flat_map(|h| &h.lines).collect();
        assert!(all_lines
            .iter()
            .any(|dl| *dl == &DiffLine::Removed("old line".to_owned())));
        assert!(all_lines
            .iter()
            .any(|dl| *dl == &DiffLine::Added("new line".to_owned())));
    }

    #[test]
    fn test_diff_complete_replacement() {
        let original = "foo\nbar\n";
        let modified = "baz\nqux\n";
        let hunks = compute_diff(original, modified);
        assert!(!hunks.is_empty());
        let added: usize = hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|dl| matches!(dl, DiffLine::Added(_)))
            .count();
        let removed: usize = hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|dl| matches!(dl, DiffLine::Removed(_)))
            .count();
        assert_eq!(added, 2);
        assert_eq!(removed, 2);
    }

    // ── apply_patch ───────────────────────────────────────────────────────────

    #[test]
    fn test_apply_patch_reconstructs_modified() {
        let original = "line1\nline2\nline3\nline4\nline5\n";
        let modified = "line1\nline2\nchanged\nline4\nline5\n";
        let hunks = compute_diff(original, modified);
        let result = apply_patch(original, &hunks).expect("patch must apply cleanly");
        assert_eq!(result, modified);
    }

    #[test]
    fn test_apply_patch_empty_hunks_returns_original() {
        let original = "unchanged\n";
        let result = apply_patch(original, &[]).unwrap();
        assert_eq!(result, original);
    }

    // ── reverse_hunks ─────────────────────────────────────────────────────────

    #[test]
    fn test_reverse_hunks_undo_change() {
        let original = "alpha\nbeta\ngamma\n";
        let modified = "alpha\nnew beta\ngamma\n";
        let hunks = compute_diff(original, modified);

        // Apply forward: original → modified
        let patched = apply_patch(original, &hunks).unwrap();
        assert_eq!(patched, modified);

        // Reverse and apply: modified → original
        let reversed = reverse_hunks(&hunks);
        let undone = apply_patch(modified, &reversed).unwrap();
        assert_eq!(undone, original);
    }

    // ── format_unified_diff ───────────────────────────────────────────────────

    #[test]
    fn test_format_unified_diff_output() {
        let original = "a\nb\nc\n";
        let modified = "a\nx\nc\n";
        let hunks = compute_diff(original, modified);
        let diff = format_unified_diff("test.txt", &hunks);

        assert!(diff.contains("--- test.txt"), "missing --- header");
        assert!(diff.contains("+++ test.txt"), "missing +++ header");
        assert!(diff.contains("@@"), "missing hunk header");
        assert!(diff.contains("-b"), "missing removed line");
        assert!(diff.contains("+x"), "missing added line");
    }

    #[test]
    fn test_format_unified_diff_empty_for_no_hunks() {
        let diff = format_unified_diff("file.txt", &[]);
        assert!(diff.is_empty());
    }

    // ── write_file_tracked ────────────────────────────────────────────────────

    #[test]
    fn test_write_file_tracked_existing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("file.txt");
        fs::write(&path, "original content\n").unwrap();

        let result =
            write_file_tracked(&path, "new content\n").expect("write_file_tracked must succeed");

        assert!(!result.created);
        assert_eq!(
            result.original_content.as_deref(),
            Some("original content\n")
        );
        assert_eq!(result.new_content, "new content\n");
        assert!(!result.hunks.is_empty());
        // The file on disk should now hold the new content.
        assert_eq!(fs::read_to_string(&path).unwrap(), "new content\n");
    }

    #[test]
    fn test_write_file_tracked_new_file_created_flag() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("brand_new.txt");

        let result = write_file_tracked(&path, "hello\n").expect("write_file_tracked must succeed");

        assert!(result.created, "created flag must be true for new files");
        assert!(result.original_content.is_none());
        assert_eq!(result.new_content, "hello\n");
    }

    // ── FileOpHistory ─────────────────────────────────────────────────────────

    #[test]
    fn test_history_record_and_undo() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tracked.txt");

        fs::write(&path, "first\n").unwrap();
        let result = write_file_tracked(&path, "second\n").unwrap();

        let mut history = FileOpHistory::new(10);
        history.record(result);

        assert_eq!(history.len(), 1);
        let undo_result = history.undo_last();
        assert!(undo_result.is_some());
        undo_result.unwrap().expect("undo must succeed");

        // File should have been restored.
        assert_eq!(fs::read_to_string(&path).unwrap(), "first\n");
        assert!(history.is_empty());
    }

    #[test]
    fn test_history_max_history_limit() {
        let dir = TempDir::new().unwrap();
        let mut history = FileOpHistory::new(3);

        // Record 5 operations (only the last 3 should be kept).
        for i in 0u8..5 {
            let path = dir.path().join(format!("f{i}.txt"));
            fs::write(&path, "old\n").unwrap();
            let result = write_file_tracked(&path, "new\n").unwrap();
            history.record(result);
        }

        assert_eq!(
            history.len(),
            3,
            "history must not exceed max_history entries"
        );
    }

    // ── lines_added / lines_removed counts ───────────────────────────────────

    #[test]
    fn test_lines_added_removed_counts() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("counts.txt");
        fs::write(&path, "a\nb\nc\n").unwrap();

        // Remove "b", add "x" and "y".
        let result = write_file_tracked(&path, "a\nx\ny\nc\n").unwrap();

        assert_eq!(result.lines_added, 2, "expected 2 added lines");
        assert_eq!(result.lines_removed, 1, "expected 1 removed line");
    }
}
