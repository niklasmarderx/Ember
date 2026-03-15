//! SQLite storage backend for Ember.
//!
//! This module provides persistent storage using SQLite for conversations,
//! memories, and agent state.

use crate::error::{Result, StorageError};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};
use uuid::Uuid;

/// SQLite storage configuration.
#[derive(Debug, Clone)]
pub struct SqliteConfig {
    /// Path to the database file.
    pub path: String,
    /// Enable WAL mode for better concurrency.
    pub wal_mode: bool,
    /// Busy timeout in milliseconds.
    pub busy_timeout_ms: u32,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            path: "ember.db".to_string(),
            wal_mode: true,
            busy_timeout_ms: 5000,
        }
    }
}

impl SqliteConfig {
    /// Create a new in-memory database configuration.
    pub fn in_memory() -> Self {
        Self {
            path: ":memory:".to_string(),
            wal_mode: false,
            busy_timeout_ms: 5000,
        }
    }
}

/// SQLite storage backend.
pub struct SqliteStorage {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStorage {
    /// Create a new SQLite storage instance.
    ///
    /// # Arguments
    ///
    /// * `config` - SQLite configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or initialized.
    pub fn new(config: &SqliteConfig) -> Result<Self> {
        let conn = if config.path == ":memory:" {
            Connection::open_in_memory()?
        } else {
            let path = Path::new(&config.path);
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            Connection::open(path)?
        };

        // Configure connection
        conn.busy_timeout(std::time::Duration::from_millis(
            config.busy_timeout_ms as u64,
        ))?;

        if config.wal_mode && config.path != ":memory:" {
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        }

        // Enable foreign keys
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;

        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        info!("SQLite storage initialized at: {}", config.path);
        Ok(storage)
    }

    /// Run database migrations.
    ///
    /// # Errors
    ///
    /// Returns an error if migrations fail.
    pub async fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().await;

        conn.execute_batch(
            r#"
            -- Conversations table
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                metadata TEXT
            );

            -- Messages table
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                token_count INTEGER,
                metadata TEXT,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            -- Memories table
            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                importance REAL NOT NULL DEFAULT 0.5,
                created_at TEXT NOT NULL,
                last_accessed TEXT NOT NULL,
                access_count INTEGER NOT NULL DEFAULT 0,
                tags TEXT,
                metadata TEXT
            );

            -- Agent state table
            CREATE TABLE IF NOT EXISTS agent_state (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                state_key TEXT NOT NULL,
                state_value TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(agent_id, state_key)
            );

            -- Indexes for performance
            CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id);
            CREATE INDEX IF NOT EXISTS idx_messages_created ON messages(created_at);
            CREATE INDEX IF NOT EXISTS idx_memories_importance ON memories(importance DESC);
            CREATE INDEX IF NOT EXISTS idx_memories_last_accessed ON memories(last_accessed DESC);
            CREATE INDEX IF NOT EXISTS idx_agent_state_agent ON agent_state(agent_id);
            "#,
        )
        .map_err(|e| StorageError::MigrationFailed(e.to_string()))?;

        info!("Database migrations completed");
        Ok(())
    }

    // =========================================================================
    // Conversation Operations
    // =========================================================================

    /// Create a new conversation.
    pub async fn create_conversation(&self, title: Option<&str>) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, title, now, now],
        )?;

        debug!(conversation_id = %id, "Created conversation");
        Ok(id)
    }

    /// Get a conversation by ID.
    pub async fn get_conversation(&self, id: &str) -> Result<Option<ConversationRecord>> {
        let conn = self.conn.lock().await;
        let result = conn
            .query_row(
                "SELECT id, title, created_at, updated_at, metadata FROM conversations WHERE id = ?1",
                params![id],
                |row| {
                    Ok(ConversationRecord {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        created_at: row.get(2)?,
                        updated_at: row.get(3)?,
                        metadata: row.get(4)?,
                    })
                },
            )
            .optional()?;

        Ok(result)
    }

    /// List all conversations.
    pub async fn list_conversations(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ConversationRecord>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, title, created_at, updated_at, metadata FROM conversations ORDER BY updated_at DESC LIMIT ?1 OFFSET ?2",
        )?;

        let rows = stmt.query_map(params![limit as i64, offset as i64], |row| {
            Ok(ConversationRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                metadata: row.get(4)?,
            })
        })?;

        let mut conversations = Vec::new();
        for row in rows {
            conversations.push(row?);
        }

        Ok(conversations)
    }

    /// Delete a conversation and all its messages.
    pub async fn delete_conversation(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().await;
        let affected = conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    // =========================================================================
    // Message Operations
    // =========================================================================

    /// Add a message to a conversation.
    pub async fn add_message(
        &self,
        conversation_id: &str,
        message: &MessageRecord,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        let conn = self.conn.lock().await;

        // Insert message
        conn.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at, token_count, metadata) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                conversation_id,
                message.role,
                message.content,
                now,
                message.token_count,
                message.metadata,
            ],
        )?;

        // Update conversation updated_at
        conn.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now, conversation_id],
        )?;

        debug!(message_id = %id, conversation_id = %conversation_id, "Added message");
        Ok(id)
    }

    /// Get messages for a conversation.
    pub async fn get_messages(
        &self,
        conversation_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<MessageRecord>> {
        let conn = self.conn.lock().await;

        let query = match limit {
            Some(l) => format!(
                "SELECT id, conversation_id, role, content, created_at, token_count, metadata FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC LIMIT {}",
                l
            ),
            None => "SELECT id, conversation_id, role, content, created_at, token_count, metadata FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC".to_string(),
        };

        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(params![conversation_id], |row| {
            Ok(MessageRecord {
                id: Some(row.get(0)?),
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
                token_count: row.get(5)?,
                metadata: row.get(6)?,
            })
        })?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }

        Ok(messages)
    }

    // =========================================================================
    // Memory Operations
    // =========================================================================

    /// Store a memory.
    pub async fn store_memory(&self, memory: &MemoryRecord) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let tags_json = memory
            .tags
            .as_ref()
            .and_then(|t| serde_json::to_string(t).ok());

        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO memories (id, content, importance, created_at, last_accessed, access_count, tags, metadata) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                memory.content,
                memory.importance,
                now,
                now,
                0,
                tags_json,
                memory.metadata,
            ],
        )?;

        debug!(memory_id = %id, "Stored memory");
        Ok(id)
    }

    /// Get memories by importance.
    pub async fn get_memories_by_importance(
        &self,
        min_importance: f32,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, content, importance, created_at, last_accessed, access_count, tags, metadata FROM memories WHERE importance >= ?1 ORDER BY importance DESC LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![min_importance, limit as i64], |row| {
            let tags_json: Option<String> = row.get(6)?;
            let tags = tags_json.and_then(|t| serde_json::from_str(&t).ok());

            Ok(MemoryRecord {
                id: Some(row.get(0)?),
                content: row.get(1)?,
                importance: row.get(2)?,
                created_at: row.get(3)?,
                last_accessed: row.get(4)?,
                access_count: row.get(5)?,
                tags,
                metadata: row.get(7)?,
            })
        })?;

        let mut memories = Vec::new();
        for row in rows {
            memories.push(row?);
        }

        Ok(memories)
    }

    /// Get recent memories.
    pub async fn get_recent_memories(&self, limit: usize) -> Result<Vec<MemoryRecord>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, content, importance, created_at, last_accessed, access_count, tags, metadata FROM memories ORDER BY last_accessed DESC LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            let tags_json: Option<String> = row.get(6)?;
            let tags = tags_json.and_then(|t| serde_json::from_str(&t).ok());

            Ok(MemoryRecord {
                id: Some(row.get(0)?),
                content: row.get(1)?,
                importance: row.get(2)?,
                created_at: row.get(3)?,
                last_accessed: row.get(4)?,
                access_count: row.get(5)?,
                tags,
                metadata: row.get(7)?,
            })
        })?;

        let mut memories = Vec::new();
        for row in rows {
            memories.push(row?);
        }

        Ok(memories)
    }

    /// Update memory access (last_accessed and access_count).
    pub async fn touch_memory(&self, id: &str) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().await;
        let affected = conn.execute(
            "UPDATE memories SET last_accessed = ?1, access_count = access_count + 1 WHERE id = ?2",
            params![now, id],
        )?;
        Ok(affected > 0)
    }

    /// Delete a memory.
    pub async fn delete_memory(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().await;
        let affected = conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    // =========================================================================
    // Agent State Operations
    // =========================================================================

    /// Store agent state.
    pub async fn store_state<T: Serialize>(
        &self,
        agent_id: &str,
        key: &str,
        value: &T,
    ) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let value_json = serde_json::to_string(value)?;

        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO agent_state (id, agent_id, state_key, state_value, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, agent_id, key, value_json, now],
        )?;

        debug!(agent_id = %agent_id, key = %key, "Stored agent state");
        Ok(())
    }

    /// Get agent state.
    pub async fn get_state<T: DeserializeOwned>(
        &self,
        agent_id: &str,
        key: &str,
    ) -> Result<Option<T>> {
        let conn = self.conn.lock().await;
        let result: Option<String> = conn
            .query_row(
                "SELECT state_value FROM agent_state WHERE agent_id = ?1 AND state_key = ?2",
                params![agent_id, key],
                |row| row.get(0),
            )
            .optional()?;

        match result {
            Some(json) => {
                let value: T = serde_json::from_str(&json)?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// Delete agent state.
    pub async fn delete_state(&self, agent_id: &str, key: &str) -> Result<bool> {
        let conn = self.conn.lock().await;
        let affected = conn.execute(
            "DELETE FROM agent_state WHERE agent_id = ?1 AND state_key = ?2",
            params![agent_id, key],
        )?;
        Ok(affected > 0)
    }

    /// Clear all state for an agent.
    pub async fn clear_agent_state(&self, agent_id: &str) -> Result<usize> {
        let conn = self.conn.lock().await;
        let affected = conn.execute(
            "DELETE FROM agent_state WHERE agent_id = ?1",
            params![agent_id],
        )?;
        Ok(affected)
    }
}

/// Conversation database record.
#[derive(Debug, Clone)]
pub struct ConversationRecord {
    /// Unique conversation ID.
    pub id: String,
    /// Optional conversation title.
    pub title: Option<String>,
    /// Creation timestamp.
    pub created_at: String,
    /// Last update timestamp.
    pub updated_at: String,
    /// Optional JSON metadata.
    pub metadata: Option<String>,
}

/// Message database record.
#[derive(Debug, Clone)]
pub struct MessageRecord {
    /// Unique message ID (None for new messages).
    pub id: Option<String>,
    /// Parent conversation ID.
    pub conversation_id: String,
    /// Message role (user, assistant, system).
    pub role: String,
    /// Message content.
    pub content: String,
    /// Creation timestamp.
    pub created_at: Option<String>,
    /// Token count for this message.
    pub token_count: Option<i32>,
    /// Optional JSON metadata.
    pub metadata: Option<String>,
}

/// Memory database record.
#[derive(Debug, Clone)]
pub struct MemoryRecord {
    /// Unique memory ID (None for new memories).
    pub id: Option<String>,
    /// Memory content.
    pub content: String,
    /// Importance score (0.0 to 1.0).
    pub importance: f32,
    /// Creation timestamp.
    pub created_at: Option<String>,
    /// Last access timestamp.
    pub last_accessed: Option<String>,
    /// Access count.
    pub access_count: Option<i32>,
    /// Optional tags.
    pub tags: Option<Vec<String>>,
    /// Optional JSON metadata.
    pub metadata: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sqlite_storage_creation() {
        let config = SqliteConfig::in_memory();
        let storage = SqliteStorage::new(&config).unwrap();
        storage.migrate().await.unwrap();
    }

    #[tokio::test]
    async fn test_conversation_crud() {
        let config = SqliteConfig::in_memory();
        let storage = SqliteStorage::new(&config).unwrap();
        storage.migrate().await.unwrap();

        // Create
        let id = storage
            .create_conversation(Some("Test Chat"))
            .await
            .unwrap();
        assert!(!id.is_empty());

        // Read
        let conv = storage.get_conversation(&id).await.unwrap();
        assert!(conv.is_some());
        assert_eq!(conv.unwrap().title, Some("Test Chat".to_string()));

        // List
        let convs = storage.list_conversations(10, 0).await.unwrap();
        assert_eq!(convs.len(), 1);

        // Delete
        let deleted = storage.delete_conversation(&id).await.unwrap();
        assert!(deleted);

        let conv = storage.get_conversation(&id).await.unwrap();
        assert!(conv.is_none());
    }

    #[tokio::test]
    async fn test_message_operations() {
        let config = SqliteConfig::in_memory();
        let storage = SqliteStorage::new(&config).unwrap();
        storage.migrate().await.unwrap();

        let conv_id = storage.create_conversation(None).await.unwrap();

        let message = MessageRecord {
            id: None,
            conversation_id: conv_id.clone(),
            role: "user".to_string(),
            content: "Hello!".to_string(),
            created_at: None,
            token_count: Some(2),
            metadata: None,
        };

        let msg_id = storage.add_message(&conv_id, &message).await.unwrap();
        assert!(!msg_id.is_empty());

        let messages = storage.get_messages(&conv_id, None).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello!");
    }

    #[tokio::test]
    async fn test_memory_operations() {
        let config = SqliteConfig::in_memory();
        let storage = SqliteStorage::new(&config).unwrap();
        storage.migrate().await.unwrap();

        let memory = MemoryRecord {
            id: None,
            content: "User prefers dark mode".to_string(),
            importance: 0.8,
            created_at: None,
            last_accessed: None,
            access_count: None,
            tags: Some(vec!["preference".to_string()]),
            metadata: None,
        };

        let mem_id = storage.store_memory(&memory).await.unwrap();
        assert!(!mem_id.is_empty());

        let memories = storage.get_memories_by_importance(0.5, 10).await.unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].content, "User prefers dark mode");

        storage.touch_memory(&mem_id).await.unwrap();
        let memories = storage.get_recent_memories(10).await.unwrap();
        assert_eq!(memories[0].access_count, Some(1));
    }

    #[tokio::test]
    async fn test_agent_state() {
        let config = SqliteConfig::in_memory();
        let storage = SqliteStorage::new(&config).unwrap();
        storage.migrate().await.unwrap();

        let agent_id = "agent-1";
        let value = serde_json::json!({"counter": 42});

        storage
            .store_state(agent_id, "my_state", &value)
            .await
            .unwrap();

        let retrieved: Option<serde_json::Value> =
            storage.get_state(agent_id, "my_state").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap()["counter"], 42);

        storage.delete_state(agent_id, "my_state").await.unwrap();
        let retrieved: Option<serde_json::Value> =
            storage.get_state(agent_id, "my_state").await.unwrap();
        assert!(retrieved.is_none());
    }
}
