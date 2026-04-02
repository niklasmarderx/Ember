//! Session forking — branch a conversation at any point, explore alternatives,
//! and restore to a previous fork point.
//!
//! Like git branches for conversations: create a named fork, try a different
//! approach, then restore to the original state if it doesn't work out.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::conversation::{Conversation, Turn};

/// Metadata about a session fork.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionFork {
    /// Unique ID for this fork.
    pub fork_id: String,
    /// Which session was forked.
    pub parent_session_id: String,
    /// Turn index (exclusive) where the fork happened — i.e. the snapshot
    /// contains exactly `fork_point` turns.
    pub fork_point: usize,
    /// Optional human-readable branch name.
    pub branch_name: Option<String>,
    /// When this fork was created.
    pub created_at: DateTime<Utc>,
}

/// Node in the fork tree for display purposes.
#[derive(Debug, Clone)]
pub struct ForkNode {
    /// The fork metadata.
    pub fork: SessionFork,
    /// Number of turns captured in the snapshot.
    pub turn_count: usize,
    /// Whether this is the most recently created fork for its session.
    pub is_active: bool,
}

/// Manages session forks — branching and switching.
pub struct SessionForkManager {
    forks: Vec<SessionFork>,
    /// Maps fork_id → snapshot of turns at the fork point.
    conversation_snapshots: HashMap<String, Vec<Turn>>,
}

impl SessionForkManager {
    /// Create a new, empty fork manager.
    pub fn new() -> Self {
        Self {
            forks: Vec::new(),
            conversation_snapshots: HashMap::new(),
        }
    }

    /// Create a fork of the current conversation at the current point.
    ///
    /// Clones all turns that exist in `conversation` right now into a snapshot.
    /// Returns the new `SessionFork` metadata.
    pub fn fork(
        &mut self,
        conversation: &Conversation,
        session_id: &str,
        branch_name: Option<String>,
    ) -> SessionFork {
        let fork_id = Uuid::new_v4().to_string();
        let fork_point = conversation.turns.len();

        let fork = SessionFork {
            fork_id: fork_id.clone(),
            parent_session_id: session_id.to_string(),
            fork_point,
            branch_name,
            created_at: Utc::now(),
        };

        // Snapshot the turns at this exact moment.
        self.conversation_snapshots
            .insert(fork_id.clone(), conversation.turns.clone());

        self.forks.push(fork.clone());
        fork
    }

    /// List all forks for the given session, ordered by creation time (oldest
    /// first).
    pub fn list_forks(&self, session_id: &str) -> Vec<&SessionFork> {
        self.forks
            .iter()
            .filter(|f| f.parent_session_id == session_id)
            .collect()
    }

    /// Restore a conversation to a fork point.
    ///
    /// Returns `Some(turns)` with the snapshotted turns if the fork exists, or
    /// `None` if the `fork_id` is unknown.
    pub fn restore_fork(&self, fork_id: &str) -> Option<Vec<Turn>> {
        self.conversation_snapshots.get(fork_id).cloned()
    }

    /// Delete a fork and its snapshot.
    ///
    /// Returns `true` if the fork was found and removed, `false` otherwise.
    pub fn delete_fork(&mut self, fork_id: &str) -> bool {
        let before = self.forks.len();
        self.forks.retain(|f| f.fork_id != fork_id);
        self.conversation_snapshots.remove(fork_id);
        self.forks.len() < before
    }

    /// Get the metadata for a specific fork.
    pub fn get_fork(&self, fork_id: &str) -> Option<&SessionFork> {
        self.forks.iter().find(|f| f.fork_id == fork_id)
    }

    /// Total number of forks across all sessions.
    pub fn fork_count(&self) -> usize {
        self.forks.len()
    }

    /// Build the fork tree for a session.
    ///
    /// The "active" fork is the one created most recently among all forks for
    /// this session (i.e. the last element when sorted by `created_at`).
    pub fn fork_tree(&self, session_id: &str) -> Vec<ForkNode> {
        let session_forks: Vec<&SessionFork> = self.list_forks(session_id);

        if session_forks.is_empty() {
            return Vec::new();
        }

        // Determine which fork is "active" = most recently created.
        let newest_id = session_forks
            .iter()
            .max_by_key(|f| f.created_at)
            .map(|f| f.fork_id.as_str());

        session_forks
            .into_iter()
            .map(|fork| {
                let turn_count = self
                    .conversation_snapshots
                    .get(&fork.fork_id)
                    .map(|t| t.len())
                    .unwrap_or(0);

                let is_active = newest_id == Some(fork.fork_id.as_str());

                ForkNode {
                    fork: fork.clone(),
                    turn_count,
                    is_active,
                }
            })
            .collect()
    }
}

impl Default for SessionForkManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_conversation(turns: usize) -> Conversation {
        let mut conv = Conversation::new("You are helpful.");
        for i in 0..turns {
            let turn = conv.start_turn(format!("message {}", i));
            turn.assistant_response = format!("response {}", i);
            turn.complete();
        }
        conv
    }

    // -------------------------------------------------------------------------
    // 1. Fork creates a snapshot
    // -------------------------------------------------------------------------
    #[test]
    fn test_fork_creates_snapshot() {
        let mut mgr = SessionForkManager::new();
        let conv = make_conversation(3);

        let fork = mgr.fork(&conv, "session-1", None);

        assert!(mgr.conversation_snapshots.contains_key(&fork.fork_id));
        assert_eq!(mgr.conversation_snapshots[&fork.fork_id].len(), 3);
    }

    // -------------------------------------------------------------------------
    // 2. Fork has correct parent_session_id
    // -------------------------------------------------------------------------
    #[test]
    fn test_fork_parent_session_id() {
        let mut mgr = SessionForkManager::new();
        let conv = make_conversation(2);

        let fork = mgr.fork(&conv, "my-session", None);

        assert_eq!(fork.parent_session_id, "my-session");
    }

    // -------------------------------------------------------------------------
    // 3. Fork point matches conversation length
    // -------------------------------------------------------------------------
    #[test]
    fn test_fork_point_matches_conversation_length() {
        let mut mgr = SessionForkManager::new();
        let conv = make_conversation(5);

        let fork = mgr.fork(&conv, "session-x", None);

        assert_eq!(fork.fork_point, 5);
    }

    // -------------------------------------------------------------------------
    // 4. list_forks returns all forks for a session
    // -------------------------------------------------------------------------
    #[test]
    fn test_list_forks_returns_all_for_session() {
        let mut mgr = SessionForkManager::new();
        let conv = make_conversation(2);

        mgr.fork(&conv, "s1", None);
        mgr.fork(&conv, "s1", Some("branch-b".to_string()));
        mgr.fork(&conv, "other-session", None); // different session

        let forks = mgr.list_forks("s1");
        assert_eq!(forks.len(), 2);
        assert!(forks.iter().all(|f| f.parent_session_id == "s1"));
    }

    // -------------------------------------------------------------------------
    // 5. Restore fork returns correct turns
    // -------------------------------------------------------------------------
    #[test]
    fn test_restore_fork_returns_correct_turns() {
        let mut mgr = SessionForkManager::new();
        let conv = make_conversation(4);

        let fork = mgr.fork(&conv, "sess", None);
        let restored = mgr.restore_fork(&fork.fork_id).expect("fork must exist");

        assert_eq!(restored.len(), 4);
        // Verify content matches the original turns
        for (original, restored_turn) in conv.turns.iter().zip(restored.iter()) {
            assert_eq!(original.user_message, restored_turn.user_message);
            assert_eq!(original.assistant_response, restored_turn.assistant_response);
        }
    }

    // -------------------------------------------------------------------------
    // 6. Delete fork removes it
    // -------------------------------------------------------------------------
    #[test]
    fn test_delete_fork_removes_it() {
        let mut mgr = SessionForkManager::new();
        let conv = make_conversation(2);

        let fork = mgr.fork(&conv, "session", None);
        let fork_id = fork.fork_id.clone();

        assert!(mgr.delete_fork(&fork_id));
        assert!(mgr.get_fork(&fork_id).is_none());
        assert!(!mgr.conversation_snapshots.contains_key(&fork_id));
        assert_eq!(mgr.fork_count(), 0);
    }

    // -------------------------------------------------------------------------
    // 7. Multiple forks on the same session
    // -------------------------------------------------------------------------
    #[test]
    fn test_multiple_forks_same_session() {
        let mut mgr = SessionForkManager::new();
        let conv_a = make_conversation(2);
        let conv_b = make_conversation(4);

        let fork_a = mgr.fork(&conv_a, "session-multi", None);
        let fork_b = mgr.fork(&conv_b, "session-multi", None);

        assert_eq!(mgr.fork_count(), 2);
        assert_ne!(fork_a.fork_id, fork_b.fork_id);
        assert_eq!(
            mgr.restore_fork(&fork_a.fork_id).unwrap().len(),
            2
        );
        assert_eq!(
            mgr.restore_fork(&fork_b.fork_id).unwrap().len(),
            4
        );
    }

    // -------------------------------------------------------------------------
    // 8. Fork with branch name
    // -------------------------------------------------------------------------
    #[test]
    fn test_fork_with_branch_name() {
        let mut mgr = SessionForkManager::new();
        let conv = make_conversation(1);

        let fork = mgr.fork(&conv, "sess", Some("try-different-approach".to_string()));

        assert_eq!(
            fork.branch_name.as_deref(),
            Some("try-different-approach")
        );

        let stored = mgr.get_fork(&fork.fork_id).unwrap();
        assert_eq!(stored.branch_name.as_deref(), Some("try-different-approach"));
    }

    // -------------------------------------------------------------------------
    // 9. Fork tree structure
    // -------------------------------------------------------------------------
    #[test]
    fn test_fork_tree_structure() {
        let mut mgr = SessionForkManager::new();
        let conv2 = make_conversation(2);
        let conv5 = make_conversation(5);

        mgr.fork(&conv2, "tree-session", Some("approach-a".to_string()));
        // Second fork is newer → should be active
        let fork_b = mgr.fork(&conv5, "tree-session", Some("approach-b".to_string()));

        let tree = mgr.fork_tree("tree-session");
        assert_eq!(tree.len(), 2);

        // Exactly one node is active and it is the newest fork.
        let active_nodes: Vec<&ForkNode> = tree.iter().filter(|n| n.is_active).collect();
        assert_eq!(active_nodes.len(), 1);
        assert_eq!(active_nodes[0].fork.fork_id, fork_b.fork_id);

        // turn_counts reflect the snapshot sizes.
        let node_a = tree.iter().find(|n| !n.is_active).unwrap();
        let node_b = tree.iter().find(|n| n.is_active).unwrap();
        assert_eq!(node_a.turn_count, 2);
        assert_eq!(node_b.turn_count, 5);
    }

    // -------------------------------------------------------------------------
    // 10. Restore non-existent fork returns None
    // -------------------------------------------------------------------------
    #[test]
    fn test_restore_nonexistent_fork_returns_none() {
        let mgr = SessionForkManager::new();
        assert!(mgr.restore_fork("does-not-exist").is_none());
    }

    // -------------------------------------------------------------------------
    // 11. Empty manager returns empty lists
    // -------------------------------------------------------------------------
    #[test]
    fn test_empty_manager_returns_empty_lists() {
        let mgr = SessionForkManager::new();

        assert_eq!(mgr.list_forks("any-session").len(), 0);
        assert_eq!(mgr.fork_tree("any-session").len(), 0);
        assert_eq!(mgr.fork_count(), 0);
    }

    // -------------------------------------------------------------------------
    // 12. Delete non-existent fork returns false
    // -------------------------------------------------------------------------
    #[test]
    fn test_delete_nonexistent_fork_returns_false() {
        let mut mgr = SessionForkManager::new();
        assert!(!mgr.delete_fork("ghost-id"));
    }

    // -------------------------------------------------------------------------
    // 13. Fork of empty conversation
    // -------------------------------------------------------------------------
    #[test]
    fn test_fork_of_empty_conversation() {
        let mut mgr = SessionForkManager::new();
        let conv = make_conversation(0);

        let fork = mgr.fork(&conv, "session-empty", None);

        assert_eq!(fork.fork_point, 0);
        assert_eq!(
            mgr.restore_fork(&fork.fork_id).unwrap().len(),
            0
        );
    }
}
