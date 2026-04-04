//! Memory management for agent conversations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Represents a single memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique identifier
    pub id: Uuid,

    /// Entry content
    pub content: String,

    /// Entry type/category
    pub entry_type: MemoryType,

    /// When this memory was created
    pub created_at: DateTime<Utc>,

    /// When this memory was last accessed
    pub last_accessed: DateTime<Utc>,

    /// Importance score (0.0 - 1.0)
    pub importance: f32,

    /// Associated metadata
    pub metadata: HashMap<String, String>,

    /// Embedding vector for semantic search (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}

impl MemoryEntry {
    /// Create a new memory entry.
    pub fn new(content: impl Into<String>, entry_type: MemoryType) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            content: content.into(),
            entry_type,
            created_at: now,
            last_accessed: now,
            importance: 0.5,
            metadata: HashMap::new(),
            embedding: None,
        }
    }

    /// Create a fact memory.
    pub fn fact(content: impl Into<String>) -> Self {
        Self::new(content, MemoryType::Fact)
    }

    /// Create a summary memory.
    pub fn summary(content: impl Into<String>) -> Self {
        Self::new(content, MemoryType::Summary)
    }

    /// Create a preference memory.
    pub fn preference(content: impl Into<String>) -> Self {
        Self::new(content, MemoryType::Preference)
    }

    /// Set importance score.
    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set embedding vector.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Mark as accessed (updates last_accessed timestamp).
    pub fn touch(&mut self) {
        self.last_accessed = Utc::now();
    }
}

/// Type of memory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    /// A factual piece of information
    Fact,
    /// A conversation summary
    Summary,
    /// User preference or setting
    Preference,
    /// An event or action that occurred
    Event,
    /// A learned pattern or insight
    Insight,
    /// Custom type
    Custom,
}

/// In-memory storage for agent memories.
#[derive(Debug, Default)]
pub struct Memory {
    /// All memory entries
    entries: HashMap<Uuid, MemoryEntry>,

    /// Index by type
    by_type: HashMap<MemoryType, Vec<Uuid>>,

    /// Maximum entries to keep
    max_entries: Option<usize>,
}

impl Memory {
    /// Create a new memory store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a memory store with a maximum size.
    pub fn with_max_entries(max: usize) -> Self {
        Self {
            entries: HashMap::new(),
            by_type: HashMap::new(),
            max_entries: Some(max),
        }
    }

    /// Add a memory entry.
    pub fn add(&mut self, entry: MemoryEntry) -> Uuid {
        let id = entry.id;
        let entry_type = entry.entry_type;

        self.entries.insert(id, entry);
        self.by_type.entry(entry_type).or_default().push(id);

        // Enforce max entries if set
        if let Some(max) = self.max_entries {
            while self.entries.len() > max {
                self.remove_least_important();
            }
        }

        id
    }

    /// Get a memory entry by ID.
    pub fn get(&self, id: &Uuid) -> Option<&MemoryEntry> {
        self.entries.get(id)
    }

    /// Get a mutable memory entry by ID.
    pub fn get_mut(&mut self, id: &Uuid) -> Option<&mut MemoryEntry> {
        self.entries.get_mut(id)
    }

    /// Remove a memory entry.
    pub fn remove(&mut self, id: &Uuid) -> Option<MemoryEntry> {
        if let Some(entry) = self.entries.remove(id) {
            if let Some(ids) = self.by_type.get_mut(&entry.entry_type) {
                ids.retain(|i| i != id);
            }
            Some(entry)
        } else {
            None
        }
    }

    /// Get all entries of a specific type.
    pub fn get_by_type(&self, entry_type: MemoryType) -> Vec<&MemoryEntry> {
        self.by_type
            .get(&entry_type)
            .map(|ids| ids.iter().filter_map(|id| self.entries.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all entries.
    pub fn all(&self) -> impl Iterator<Item = &MemoryEntry> {
        self.entries.values()
    }

    /// Get the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Search entries by content (simple substring match).
    pub fn search(&self, query: &str) -> Vec<&MemoryEntry> {
        let query_lower = query.to_lowercase();
        self.entries
            .values()
            .filter(|e| e.content.to_lowercase().contains(&query_lower))
            .collect()
    }

    /// Get the most important entries.
    pub fn most_important(&self, n: usize) -> Vec<&MemoryEntry> {
        let mut entries: Vec<_> = self.entries.values().collect();
        entries.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap_or(std::cmp::Ordering::Equal));
        entries.into_iter().take(n).collect()
    }

    /// Get the most recently accessed entries.
    pub fn most_recent(&self, n: usize) -> Vec<&MemoryEntry> {
        let mut entries: Vec<_> = self.entries.values().collect();
        entries.sort_by(|a, b| b.last_accessed.cmp(&a.last_accessed));
        entries.into_iter().take(n).collect()
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.by_type.clear();
    }

    /// Remove the least important entry.
    fn remove_least_important(&mut self) {
        if let Some((id, _)) = self
            .entries
            .iter()
            .min_by(|(_, a), (_, b)| a.importance.partial_cmp(&b.importance).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, e)| (*id, e.entry_type))
        {
            self.remove(&id);
        }
    }
}

/// Trait for persistent memory storage.
pub trait MemoryStore: Send + Sync {
    /// Save a memory entry.
    fn save(&self, entry: &MemoryEntry) -> crate::Result<()>;

    /// Load a memory entry by ID.
    fn load(&self, id: &Uuid) -> crate::Result<Option<MemoryEntry>>;

    /// Delete a memory entry.
    fn delete(&self, id: &Uuid) -> crate::Result<()>;

    /// List all memory IDs.
    fn list_ids(&self) -> crate::Result<Vec<Uuid>>;

    /// Search memories by content.
    fn search(&self, query: &str, limit: usize) -> crate::Result<Vec<MemoryEntry>>;

    /// Search memories by embedding (semantic search).
    fn search_by_embedding(
        &self,
        embedding: &[f32],
        limit: usize,
    ) -> crate::Result<Vec<MemoryEntry>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_entry_creation() {
        let entry = MemoryEntry::fact("The user prefers dark mode")
            .with_importance(0.8)
            .with_metadata("source", "user_settings");

        assert_eq!(entry.entry_type, MemoryType::Fact);
        assert_eq!(entry.importance, 0.8);
        assert!(entry.metadata.contains_key("source"));
    }

    #[test]
    fn test_memory_storage() {
        let mut memory = Memory::new();

        let entry1 = MemoryEntry::fact("Fact 1").with_importance(0.5);
        let entry2 = MemoryEntry::preference("Preference 1").with_importance(0.9);

        let id1 = memory.add(entry1);
        let id2 = memory.add(entry2);

        assert_eq!(memory.len(), 2);
        assert!(memory.get(&id1).is_some());
        assert!(memory.get(&id2).is_some());
    }

    #[test]
    fn test_memory_search() {
        let mut memory = Memory::new();

        memory.add(MemoryEntry::fact("The user likes coffee"));
        memory.add(MemoryEntry::fact("The user dislikes tea"));
        memory.add(MemoryEntry::preference("Dark mode enabled"));

        let results = memory.search("user");
        assert_eq!(results.len(), 2);

        let results = memory.search("dark");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_memory_by_type() {
        let mut memory = Memory::new();

        memory.add(MemoryEntry::fact("Fact 1"));
        memory.add(MemoryEntry::fact("Fact 2"));
        memory.add(MemoryEntry::preference("Pref 1"));

        let facts = memory.get_by_type(MemoryType::Fact);
        assert_eq!(facts.len(), 2);

        let prefs = memory.get_by_type(MemoryType::Preference);
        assert_eq!(prefs.len(), 1);
    }

    #[test]
    fn test_memory_max_entries() {
        let mut memory = Memory::with_max_entries(2);

        memory.add(MemoryEntry::fact("Low importance").with_importance(0.1));
        memory.add(MemoryEntry::fact("High importance").with_importance(0.9));
        memory.add(MemoryEntry::fact("Medium importance").with_importance(0.5));

        // Should have removed the least important
        assert_eq!(memory.len(), 2);

        // Low importance should be gone
        let results = memory.search("Low importance");
        assert!(results.is_empty());
    }

    #[test]
    fn test_most_important() {
        let mut memory = Memory::new();

        memory.add(MemoryEntry::fact("Low").with_importance(0.1));
        memory.add(MemoryEntry::fact("High").with_importance(0.9));
        memory.add(MemoryEntry::fact("Medium").with_importance(0.5));

        let top = memory.most_important(2);
        assert_eq!(top.len(), 2);
        assert!(top[0].content.contains("High"));
        assert!(top[1].content.contains("Medium"));
    }
}
