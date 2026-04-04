//! Multi-Agent Collaboration System
//!
//! This module provides advanced features for multi-agent collaboration:
//! - Agent Communication Protocol (ACP) for structured messaging
//! - Shared memory and context management
//! - Task delegation and load balancing
//! - Consensus mechanisms for decision making
//! - Agent discovery and capability matching

use crate::error::Error as CoreError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

// ============================================================================
// Agent Communication Protocol (ACP)
// ============================================================================

/// Protocol version for compatibility checking.
pub const ACP_VERSION: &str = "1.0.0";

/// Unique identifier for a collaboration session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    /// Create a new session ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a unique session ID.
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Message envelope for the Agent Communication Protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ACPMessage {
    /// Unique message ID.
    pub id: String,
    /// Protocol version.
    pub version: String,
    /// Sender agent ID.
    pub sender: String,
    /// Recipient agent ID (None = broadcast).
    pub recipient: Option<String>,
    /// Session this message belongs to.
    pub session_id: SessionId,
    /// Message type.
    pub message_type: ACPMessageType,
    /// Message payload.
    pub payload: serde_json::Value,
    /// Message priority (higher = more urgent).
    pub priority: u8,
    /// Timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Time-to-live in seconds (None = no expiry).
    pub ttl: Option<u64>,
    /// Correlation ID for request-response patterns.
    pub correlation_id: Option<String>,
    /// Message metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ACPMessage {
    /// Create a new ACP message.
    pub fn new(
        sender: impl Into<String>,
        session_id: SessionId,
        message_type: ACPMessageType,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            version: ACP_VERSION.to_string(),
            sender: sender.into(),
            recipient: None,
            session_id,
            message_type,
            payload,
            priority: 5,
            timestamp: chrono::Utc::now(),
            ttl: None,
            correlation_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Set the recipient.
    pub fn to(mut self, recipient: impl Into<String>) -> Self {
        self.recipient = Some(recipient.into());
        self
    }

    /// Set the priority.
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    /// Set the TTL.
    pub fn with_ttl(mut self, ttl_seconds: u64) -> Self {
        self.ttl = Some(ttl_seconds);
        self
    }

    /// Set the correlation ID.
    pub fn with_correlation(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Check if the message has expired.
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            let expiry = self.timestamp + chrono::Duration::seconds(ttl as i64);
            chrono::Utc::now() > expiry
        } else {
            false
        }
    }

    /// Create a reply to this message.
    pub fn reply(&self, sender: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            version: ACP_VERSION.to_string(),
            sender: sender.into(),
            recipient: Some(self.sender.clone()),
            session_id: self.session_id.clone(),
            message_type: ACPMessageType::Response,
            payload,
            priority: self.priority,
            timestamp: chrono::Utc::now(),
            ttl: None,
            correlation_id: Some(self.id.clone()),
            metadata: HashMap::new(),
        }
    }
}

/// Types of ACP messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ACPMessageType {
    // Discovery
    /// Agent announcing its presence.
    Announce,
    /// Query for agents with specific capabilities.
    Discovery,
    /// Response to discovery query.
    DiscoveryResponse,

    // Task Management
    /// Request to delegate a task.
    TaskRequest,
    /// Accept a task delegation.
    TaskAccept,
    /// Reject a task delegation.
    TaskReject,
    /// Report task progress.
    TaskProgress,
    /// Report task completion.
    TaskComplete,
    /// Report task failure.
    TaskFailed,
    /// Cancel a task.
    TaskCancel,

    // Communication
    /// Request for information or action.
    Request,
    /// Response to a request.
    Response,
    /// Share information (no response expected).
    Inform,
    /// Request for assistance.
    HelpRequest,
    /// Offer to assist.
    HelpOffer,

    // Consensus
    /// Propose a decision.
    Propose,
    /// Vote on a proposal.
    Vote,
    /// Announce consensus reached.
    Consensus,
    /// Veto a proposal.
    Veto,

    // Control
    /// Heartbeat/keepalive.
    Ping,
    /// Response to ping.
    Pong,
    /// Agent leaving the session.
    Leave,
    /// Error notification.
    Error,

    // Custom
    /// Custom message type.
    Custom(String),
}

// ============================================================================
// Shared Memory System
// ============================================================================

/// Shared memory space for agent collaboration.
pub struct SharedMemory {
    /// Key-value store.
    store: Arc<RwLock<HashMap<String, SharedValue>>>,
    /// Subscribers for change notifications.
    subscribers: Arc<RwLock<HashMap<String, Vec<mpsc::Sender<SharedMemoryEvent>>>>>,
    /// Access control list.
    acl: Arc<RwLock<HashMap<String, AccessControl>>>,
    /// Memory history for conflict resolution.
    history: Arc<RwLock<VecDeque<SharedMemoryOperation>>>,
    /// Maximum history size.
    max_history: usize,
}

/// A value in shared memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedValue {
    /// The actual value.
    pub value: serde_json::Value,
    /// Agent that last modified this value.
    pub modified_by: String,
    /// Modification timestamp.
    pub modified_at: chrono::DateTime<chrono::Utc>,
    /// Version number for optimistic locking.
    pub version: u64,
    /// Tags for categorization.
    pub tags: HashSet<String>,
}

/// Access control for shared memory keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControl {
    /// Agents with read access.
    pub readers: HashSet<String>,
    /// Agents with write access.
    pub writers: HashSet<String>,
    /// Owner of this key.
    pub owner: String,
    /// Whether the key is public (all agents can read).
    pub is_public: bool,
}

impl Default for AccessControl {
    fn default() -> Self {
        Self {
            readers: HashSet::new(),
            writers: HashSet::new(),
            owner: String::new(),
            is_public: true,
        }
    }
}

/// Events emitted by shared memory.
#[derive(Debug, Clone)]
pub enum SharedMemoryEvent {
    /// Value was created.
    Created {
        /// Key.
        key: String,
        /// Value.
        value: SharedValue,
    },
    /// Value was updated.
    Updated {
        /// Key.
        key: String,
        /// Old value.
        old: SharedValue,
        /// New value.
        new: SharedValue,
    },
    /// Value was deleted.
    Deleted {
        /// Key.
        key: String,
        /// Deleted value.
        value: SharedValue,
    },
}

/// Operation recorded in shared memory history.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SharedMemoryOperation {
    /// Operation type.
    op_type: SharedMemoryOpType,
    /// Key affected.
    key: String,
    /// Value (for create/update).
    value: Option<SharedValue>,
    /// Agent that performed the operation.
    agent: String,
    /// Timestamp.
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum SharedMemoryOpType {
    Create,
    Update,
    Delete,
}

impl SharedMemory {
    /// Create a new shared memory instance.
    pub fn new() -> Self {
        Self::with_history_size(1000)
    }

    /// Create with custom history size.
    pub fn with_history_size(max_history: usize) -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            acl: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(VecDeque::new())),
            max_history,
        }
    }

    /// Get a value from shared memory.
    pub async fn get(&self, key: &str, agent_id: &str) -> Option<SharedValue> {
        // Check access
        if !self.can_read(key, agent_id).await {
            return None;
        }

        let store = self.store.read().await;
        store.get(key).cloned()
    }

    /// Set a value in shared memory.
    pub async fn set(
        &self,
        key: &str,
        value: serde_json::Value,
        agent_id: &str,
    ) -> Result<u64, CoreError> {
        self.set_with_tags(key, value, agent_id, HashSet::new())
            .await
    }

    /// Set a value with tags.
    pub async fn set_with_tags(
        &self,
        key: &str,
        value: serde_json::Value,
        agent_id: &str,
        tags: HashSet<String>,
    ) -> Result<u64, CoreError> {
        // Check write access
        if !self.can_write(key, agent_id).await {
            return Err(CoreError::Agent(format!(
                "Agent {} does not have write access to key {}",
                agent_id, key
            )));
        }

        let mut store = self.store.write().await;
        let now = chrono::Utc::now();

        let (event, version) = if let Some(old) = store.get(key) {
            let new_value = SharedValue {
                value,
                modified_by: agent_id.to_string(),
                modified_at: now,
                version: old.version + 1,
                tags,
            };
            let version = new_value.version;
            let event = SharedMemoryEvent::Updated {
                key: key.to_string(),
                old: old.clone(),
                new: new_value.clone(),
            };
            store.insert(key.to_string(), new_value.clone());

            // Record history
            self.record_operation(SharedMemoryOperation {
                op_type: SharedMemoryOpType::Update,
                key: key.to_string(),
                value: Some(new_value),
                agent: agent_id.to_string(),
                timestamp: now,
            })
            .await;

            (event, version)
        } else {
            let new_value = SharedValue {
                value,
                modified_by: agent_id.to_string(),
                modified_at: now,
                version: 1,
                tags,
            };
            let version = new_value.version;
            let event = SharedMemoryEvent::Created {
                key: key.to_string(),
                value: new_value.clone(),
            };
            store.insert(key.to_string(), new_value.clone());

            // Set default ACL
            drop(store);
            let mut acl = self.acl.write().await;
            acl.insert(
                key.to_string(),
                AccessControl {
                    owner: agent_id.to_string(),
                    is_public: true,
                    ..Default::default()
                },
            );

            // Record history
            self.record_operation(SharedMemoryOperation {
                op_type: SharedMemoryOpType::Create,
                key: key.to_string(),
                value: Some(new_value),
                agent: agent_id.to_string(),
                timestamp: now,
            })
            .await;

            (event, version)
        };

        // Notify subscribers
        self.notify_subscribers(key, event).await;

        Ok(version)
    }

    /// Delete a value from shared memory.
    pub async fn delete(&self, key: &str, agent_id: &str) -> Result<(), CoreError> {
        if !self.can_write(key, agent_id).await {
            return Err(CoreError::Agent(format!(
                "Agent {} does not have write access to key {}",
                agent_id, key
            )));
        }

        let mut store = self.store.write().await;

        if let Some(old) = store.remove(key) {
            let event = SharedMemoryEvent::Deleted {
                key: key.to_string(),
                value: old,
            };

            // Record history
            self.record_operation(SharedMemoryOperation {
                op_type: SharedMemoryOpType::Delete,
                key: key.to_string(),
                value: None,
                agent: agent_id.to_string(),
                timestamp: chrono::Utc::now(),
            })
            .await;

            drop(store);
            self.notify_subscribers(key, event).await;
        }

        Ok(())
    }

    /// Compare and swap (optimistic locking).
    pub async fn compare_and_swap(
        &self,
        key: &str,
        expected_version: u64,
        new_value: serde_json::Value,
        agent_id: &str,
    ) -> Result<u64, CoreError> {
        let store = self.store.read().await;

        if let Some(current) = store.get(key) {
            if current.version != expected_version {
                return Err(CoreError::Agent(format!(
                    "Version conflict: expected {}, found {}",
                    expected_version, current.version
                )));
            }
        }

        drop(store);
        self.set(key, new_value, agent_id).await
    }

    /// List all keys matching a pattern.
    pub async fn list_keys(&self, pattern: Option<&str>) -> Vec<String> {
        let store = self.store.read().await;

        if let Some(pattern) = pattern {
            store
                .keys()
                .filter(|k| k.contains(pattern))
                .cloned()
                .collect()
        } else {
            store.keys().cloned().collect()
        }
    }

    /// Find values by tag.
    pub async fn find_by_tag(&self, tag: &str) -> Vec<(String, SharedValue)> {
        let store = self.store.read().await;

        store
            .iter()
            .filter(|(_, v)| v.tags.contains(tag))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Subscribe to changes for a key pattern.
    pub async fn subscribe(&self, key_pattern: &str) -> mpsc::Receiver<SharedMemoryEvent> {
        let (tx, rx) = mpsc::channel(100);

        let mut subscribers = self.subscribers.write().await;
        subscribers
            .entry(key_pattern.to_string())
            .or_default()
            .push(tx);

        rx
    }

    /// Set access control for a key.
    pub async fn set_acl(
        &self,
        key: &str,
        acl: AccessControl,
        agent_id: &str,
    ) -> Result<(), CoreError> {
        let current_acl = self.acl.read().await;

        if let Some(existing) = current_acl.get(key) {
            if existing.owner != agent_id {
                return Err(CoreError::Agent(format!(
                    "Only the owner can modify ACL for key {}",
                    key
                )));
            }
        }

        drop(current_acl);

        let mut acl_write = self.acl.write().await;
        acl_write.insert(key.to_string(), acl);

        Ok(())
    }

    /// Check if an agent can read a key.
    async fn can_read(&self, key: &str, agent_id: &str) -> bool {
        let acl = self.acl.read().await;

        if let Some(access) = acl.get(key) {
            access.is_public
                || access.owner == agent_id
                || access.readers.contains(agent_id)
                || access.writers.contains(agent_id)
        } else {
            true // No ACL = public access
        }
    }

    /// Check if an agent can write to a key.
    async fn can_write(&self, key: &str, agent_id: &str) -> bool {
        let acl = self.acl.read().await;

        if let Some(access) = acl.get(key) {
            access.owner == agent_id || access.writers.contains(agent_id)
        } else {
            true // No ACL = anyone can write
        }
    }

    /// Notify subscribers of a change.
    async fn notify_subscribers(&self, key: &str, event: SharedMemoryEvent) {
        let subscribers = self.subscribers.read().await;

        for (pattern, subs) in subscribers.iter() {
            if key.contains(pattern) || pattern == "*" {
                for tx in subs {
                    let _ = tx.send(event.clone()).await;
                }
            }
        }
    }

    /// Record an operation in history.
    async fn record_operation(&self, op: SharedMemoryOperation) {
        let mut history = self.history.write().await;

        if history.len() >= self.max_history {
            history.pop_front();
        }

        history.push_back(op);
    }
}

impl Default for SharedMemory {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Task Delegation System
// ============================================================================

/// Represents a delegatable task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborativeTask {
    /// Unique task ID.
    pub id: String,
    /// Task description.
    pub description: String,
    /// Original requestor.
    pub requestor: String,
    /// Currently assigned agent.
    pub assignee: Option<String>,
    /// Task status.
    pub status: TaskStatus,
    /// Required capabilities.
    pub required_capabilities: Vec<String>,
    /// Task priority (0-10).
    pub priority: u8,
    /// Task deadline.
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    /// Task input data.
    pub input: serde_json::Value,
    /// Task output data.
    pub output: Option<serde_json::Value>,
    /// Progress percentage (0-100).
    pub progress: u8,
    /// Error message if failed.
    pub error: Option<String>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Subtasks.
    pub subtasks: Vec<String>,
    /// Parent task (if this is a subtask).
    pub parent_task: Option<String>,
    /// Task metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Task status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task is pending assignment.
    Pending,
    /// Task is assigned but not started.
    Assigned,
    /// Task is in progress.
    InProgress,
    /// Task is paused.
    Paused,
    /// Task is completed successfully.
    Completed,
    /// Task failed.
    Failed,
    /// Task was cancelled.
    Cancelled,
}

impl CollaborativeTask {
    /// Create a new task.
    pub fn new(description: impl Into<String>, requestor: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            requestor: requestor.into(),
            assignee: None,
            status: TaskStatus::Pending,
            required_capabilities: Vec::new(),
            priority: 5,
            deadline: None,
            input: serde_json::Value::Null,
            output: None,
            progress: 0,
            error: None,
            created_at: now,
            updated_at: now,
            subtasks: Vec::new(),
            parent_task: None,
            metadata: HashMap::new(),
        }
    }

    /// Set required capabilities.
    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.required_capabilities = capabilities;
        self
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.min(10);
        self
    }

    /// Set deadline.
    pub fn with_deadline(mut self, deadline: chrono::DateTime<chrono::Utc>) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Set input data.
    pub fn with_input(mut self, input: serde_json::Value) -> Self {
        self.input = input;
        self
    }

    /// Check if task is overdue.
    pub fn is_overdue(&self) -> bool {
        if let Some(deadline) = self.deadline {
            chrono::Utc::now() > deadline
        } else {
            false
        }
    }
}

/// Task delegation manager.
pub struct TaskDelegator {
    /// Active tasks.
    tasks: Arc<RwLock<HashMap<String, CollaborativeTask>>>,
    /// Agent capabilities registry.
    capabilities: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    /// Agent workload (number of assigned tasks).
    workload: Arc<RwLock<HashMap<String, usize>>>,
    /// Maximum tasks per agent.
    max_tasks_per_agent: usize,
    /// Task event channel.
    event_tx: broadcast::Sender<TaskEvent>,
}

/// Events emitted by the task delegator.
#[derive(Debug, Clone)]
pub enum TaskEvent {
    /// Task was created.
    Created(CollaborativeTask),
    /// Task was assigned.
    Assigned {
        /// Task ID.
        task_id: String,
        /// Agent ID.
        agent_id: String,
    },
    /// Task progress updated.
    Progress {
        /// Task ID.
        task_id: String,
        /// Progress percentage.
        progress: u8,
    },
    /// Task completed.
    Completed {
        /// Task ID.
        task_id: String,
        /// Task output.
        output: serde_json::Value,
    },
    /// Task failed.
    Failed {
        /// Task ID.
        task_id: String,
        /// Error message.
        error: String,
    },
    /// Task cancelled.
    Cancelled {
        /// Task ID.
        task_id: String,
    },
}

impl TaskDelegator {
    /// Create a new task delegator.
    pub fn new() -> Self {
        Self::with_max_tasks(5)
    }

    /// Create with custom max tasks per agent.
    pub fn with_max_tasks(max_tasks: usize) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            capabilities: Arc::new(RwLock::new(HashMap::new())),
            workload: Arc::new(RwLock::new(HashMap::new())),
            max_tasks_per_agent: max_tasks,
            event_tx,
        }
    }

    /// Subscribe to task events.
    pub fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
        self.event_tx.subscribe()
    }

    /// Register agent capabilities.
    pub async fn register_capabilities(&self, agent_id: &str, caps: HashSet<String>) {
        let mut capabilities = self.capabilities.write().await;
        capabilities.insert(agent_id.to_string(), caps);

        let mut workload = self.workload.write().await;
        workload.entry(agent_id.to_string()).or_insert(0);
    }

    /// Submit a new task.
    pub async fn submit_task(&self, task: CollaborativeTask) -> String {
        let task_id = task.id.clone();

        let mut tasks = self.tasks.write().await;
        tasks.insert(task_id.clone(), task.clone());

        let _ = self.event_tx.send(TaskEvent::Created(task));

        task_id
    }

    /// Find the best agent for a task.
    pub async fn find_best_agent(&self, task: &CollaborativeTask) -> Option<String> {
        let capabilities = self.capabilities.read().await;
        let workload = self.workload.read().await;

        let mut candidates: Vec<_> = capabilities
            .iter()
            .filter(|(agent_id, caps)| {
                // Check if agent has required capabilities
                task.required_capabilities.iter().all(|c| caps.contains(c))
                    // Check if agent has capacity
                    && workload.get(*agent_id).copied().unwrap_or(0) < self.max_tasks_per_agent
            })
            .collect();

        // Sort by workload (least loaded first)
        candidates.sort_by_key(|(agent_id, _)| workload.get(*agent_id).copied().unwrap_or(0));

        candidates.first().map(|(id, _)| (*id).clone())
    }

    /// Assign a task to an agent.
    pub async fn assign_task(&self, task_id: &str, agent_id: &str) -> Result<(), CoreError> {
        let mut tasks = self.tasks.write().await;
        let mut workload = self.workload.write().await;

        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| CoreError::Agent(format!("Task {} not found", task_id)))?;

        // Check agent capacity
        let current_workload = workload.get(agent_id).copied().unwrap_or(0);
        if current_workload >= self.max_tasks_per_agent {
            return Err(CoreError::Agent(format!(
                "Agent {} has reached maximum task limit",
                agent_id
            )));
        }

        task.assignee = Some(agent_id.to_string());
        task.status = TaskStatus::Assigned;
        task.updated_at = chrono::Utc::now();

        *workload.entry(agent_id.to_string()).or_insert(0) += 1;

        let _ = self.event_tx.send(TaskEvent::Assigned {
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
        });

        Ok(())
    }

    /// Update task progress.
    pub async fn update_progress(&self, task_id: &str, progress: u8) -> Result<(), CoreError> {
        let mut tasks = self.tasks.write().await;

        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| CoreError::Agent(format!("Task {} not found", task_id)))?;

        task.progress = progress.min(100);
        task.status = TaskStatus::InProgress;
        task.updated_at = chrono::Utc::now();

        let _ = self.event_tx.send(TaskEvent::Progress {
            task_id: task_id.to_string(),
            progress,
        });

        Ok(())
    }

    /// Complete a task.
    pub async fn complete_task(
        &self,
        task_id: &str,
        output: serde_json::Value,
    ) -> Result<(), CoreError> {
        let mut tasks = self.tasks.write().await;
        let mut workload = self.workload.write().await;

        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| CoreError::Agent(format!("Task {} not found", task_id)))?;

        if let Some(ref agent_id) = task.assignee {
            if let Some(w) = workload.get_mut(agent_id) {
                *w = w.saturating_sub(1);
            }
        }

        task.status = TaskStatus::Completed;
        task.progress = 100;
        task.output = Some(output.clone());
        task.updated_at = chrono::Utc::now();

        let _ = self.event_tx.send(TaskEvent::Completed {
            task_id: task_id.to_string(),
            output,
        });

        Ok(())
    }

    /// Fail a task.
    pub async fn fail_task(
        &self,
        task_id: &str,
        error: impl Into<String>,
    ) -> Result<(), CoreError> {
        let error = error.into();
        let mut tasks = self.tasks.write().await;
        let mut workload = self.workload.write().await;

        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| CoreError::Agent(format!("Task {} not found", task_id)))?;

        if let Some(ref agent_id) = task.assignee {
            if let Some(w) = workload.get_mut(agent_id) {
                *w = w.saturating_sub(1);
            }
        }

        task.status = TaskStatus::Failed;
        task.error = Some(error.clone());
        task.updated_at = chrono::Utc::now();

        let _ = self.event_tx.send(TaskEvent::Failed {
            task_id: task_id.to_string(),
            error,
        });

        Ok(())
    }

    /// Get a task by ID.
    pub async fn get_task(&self, task_id: &str) -> Option<CollaborativeTask> {
        let tasks = self.tasks.read().await;
        tasks.get(task_id).cloned()
    }

    /// List all tasks.
    pub async fn list_tasks(&self, status_filter: Option<TaskStatus>) -> Vec<CollaborativeTask> {
        let tasks = self.tasks.read().await;

        tasks
            .values()
            .filter(|t| status_filter.is_none_or(|s| t.status == s))
            .cloned()
            .collect()
    }

    /// Get agent workload.
    pub async fn get_workload(&self, agent_id: &str) -> usize {
        let workload = self.workload.read().await;
        workload.get(agent_id).copied().unwrap_or(0)
    }
}

impl Default for TaskDelegator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Consensus Mechanism
// ============================================================================

/// Proposal for consensus voting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    /// Unique proposal ID.
    pub id: String,
    /// Proposal description.
    pub description: String,
    /// Proposer agent ID.
    pub proposer: String,
    /// Available options.
    pub options: Vec<String>,
    /// Votes received.
    pub votes: HashMap<String, usize>,
    /// Agents who have voted.
    pub voters: HashSet<String>,
    /// Required quorum (percentage).
    pub quorum: u8,
    /// Voting deadline.
    pub deadline: chrono::DateTime<chrono::Utc>,
    /// Proposal status.
    pub status: ProposalStatus,
    /// Winning option (if decided).
    pub winner: Option<usize>,
}

/// Proposal status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalStatus {
    /// Voting in progress.
    Open,
    /// Consensus reached.
    Decided,
    /// No consensus (tie or no quorum).
    Failed,
    /// Vetoed.
    Vetoed,
}

impl Proposal {
    /// Create a new proposal.
    pub fn new(
        description: impl Into<String>,
        proposer: impl Into<String>,
        options: Vec<String>,
        deadline: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        let mut votes = HashMap::new();
        for i in 0..options.len() {
            votes.insert(i.to_string(), 0);
        }

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            proposer: proposer.into(),
            options,
            votes,
            voters: HashSet::new(),
            quorum: 50,
            deadline,
            status: ProposalStatus::Open,
            winner: None,
        }
    }

    /// Set the quorum requirement.
    pub fn with_quorum(mut self, quorum: u8) -> Self {
        self.quorum = quorum.min(100);
        self
    }

    /// Cast a vote.
    pub fn vote(&mut self, agent_id: &str, option_index: usize) -> Result<(), String> {
        if self.status != ProposalStatus::Open {
            return Err("Proposal is not open for voting".to_string());
        }

        if chrono::Utc::now() > self.deadline {
            return Err("Voting deadline has passed".to_string());
        }

        if self.voters.contains(agent_id) {
            return Err("Agent has already voted".to_string());
        }

        if option_index >= self.options.len() {
            return Err("Invalid option index".to_string());
        }

        self.voters.insert(agent_id.to_string());
        *self.votes.entry(option_index.to_string()).or_insert(0) += 1;

        Ok(())
    }

    /// Check if quorum is reached.
    pub fn has_quorum(&self, total_agents: usize) -> bool {
        let participation = (self.voters.len() as f32 / total_agents as f32) * 100.0;
        participation >= self.quorum as f32
    }

    /// Tally votes and determine winner.
    pub fn tally(&mut self, total_agents: usize) -> Option<usize> {
        if !self.has_quorum(total_agents) {
            self.status = ProposalStatus::Failed;
            return None;
        }

        let mut max_votes = 0;
        let mut winner = None;
        let mut is_tie = false;

        for (idx, count) in &self.votes {
            let idx: usize = idx.parse().unwrap_or(0);
            if *count > max_votes {
                max_votes = *count;
                winner = Some(idx);
                is_tie = false;
            } else if *count == max_votes && max_votes > 0 {
                is_tie = true;
            }
        }

        if is_tie {
            self.status = ProposalStatus::Failed;
            None
        } else {
            self.status = ProposalStatus::Decided;
            self.winner = winner;
            winner
        }
    }

    /// Veto the proposal.
    pub fn veto(&mut self, _agent_id: &str) {
        self.status = ProposalStatus::Vetoed;
    }
}

/// Consensus manager for coordinating votes.
pub struct ConsensusManager {
    /// Active proposals.
    proposals: Arc<RwLock<HashMap<String, Proposal>>>,
    /// Number of registered agents.
    agent_count: Arc<RwLock<usize>>,
    /// Event channel.
    event_tx: broadcast::Sender<ConsensusEvent>,
}

/// Events emitted by consensus manager.
#[derive(Debug, Clone)]
pub enum ConsensusEvent {
    /// New proposal created.
    ProposalCreated(Proposal),
    /// Vote cast.
    VoteCast {
        /// Proposal ID.
        proposal_id: String,
        /// Agent ID.
        agent_id: String,
        /// Option index.
        option: usize,
    },
    /// Consensus reached.
    ConsensusReached {
        /// Proposal ID.
        proposal_id: String,
        /// Winning option index.
        winner: usize,
        /// Winning option text.
        option: String,
    },
    /// No consensus.
    NoConsensus {
        /// Proposal ID.
        proposal_id: String,
    },
    /// Proposal vetoed.
    Vetoed {
        /// Proposal ID.
        proposal_id: String,
        /// Agent ID.
        agent_id: String,
    },
}

impl ConsensusManager {
    /// Create a new consensus manager.
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(100);
        Self {
            proposals: Arc::new(RwLock::new(HashMap::new())),
            agent_count: Arc::new(RwLock::new(0)),
            event_tx,
        }
    }

    /// Subscribe to consensus events.
    pub fn subscribe(&self) -> broadcast::Receiver<ConsensusEvent> {
        self.event_tx.subscribe()
    }

    /// Set the number of agents.
    pub async fn set_agent_count(&self, count: usize) {
        let mut agent_count = self.agent_count.write().await;
        *agent_count = count;
    }

    /// Create a new proposal.
    pub async fn create_proposal(&self, proposal: Proposal) -> String {
        let proposal_id = proposal.id.clone();

        let mut proposals = self.proposals.write().await;
        proposals.insert(proposal_id.clone(), proposal.clone());

        let _ = self
            .event_tx
            .send(ConsensusEvent::ProposalCreated(proposal));

        proposal_id
    }

    /// Cast a vote.
    pub async fn vote(
        &self,
        proposal_id: &str,
        agent_id: &str,
        option_index: usize,
    ) -> Result<(), CoreError> {
        let mut proposals = self.proposals.write().await;

        let proposal = proposals
            .get_mut(proposal_id)
            .ok_or_else(|| CoreError::Agent(format!("Proposal {} not found", proposal_id)))?;

        proposal
            .vote(agent_id, option_index)
            .map_err(CoreError::Agent)?;

        let _ = self.event_tx.send(ConsensusEvent::VoteCast {
            proposal_id: proposal_id.to_string(),
            agent_id: agent_id.to_string(),
            option: option_index,
        });

        // Check if we should tally
        let agent_count = *self.agent_count.read().await;
        if proposal.voters.len() >= agent_count {
            self.tally_proposal(proposal_id).await?;
        }

        Ok(())
    }

    /// Tally a proposal.
    pub async fn tally_proposal(&self, proposal_id: &str) -> Result<Option<usize>, CoreError> {
        let mut proposals = self.proposals.write().await;
        let agent_count = *self.agent_count.read().await;

        let proposal = proposals
            .get_mut(proposal_id)
            .ok_or_else(|| CoreError::Agent(format!("Proposal {} not found", proposal_id)))?;

        let winner = proposal.tally(agent_count);

        if let Some(winner_idx) = winner {
            let option = proposal
                .options
                .get(winner_idx)
                .cloned()
                .unwrap_or_default();

            let _ = self.event_tx.send(ConsensusEvent::ConsensusReached {
                proposal_id: proposal_id.to_string(),
                winner: winner_idx,
                option,
            });
        } else {
            let _ = self.event_tx.send(ConsensusEvent::NoConsensus {
                proposal_id: proposal_id.to_string(),
            });
        }

        Ok(winner)
    }

    /// Get a proposal.
    pub async fn get_proposal(&self, proposal_id: &str) -> Option<Proposal> {
        let proposals = self.proposals.read().await;
        proposals.get(proposal_id).cloned()
    }

    /// List all proposals.
    pub async fn list_proposals(&self, status_filter: Option<ProposalStatus>) -> Vec<Proposal> {
        let proposals = self.proposals.read().await;

        proposals
            .values()
            .filter(|p| status_filter.is_none_or(|s| p.status == s))
            .cloned()
            .collect()
    }
}

impl Default for ConsensusManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acp_message_creation() {
        let session = SessionId::generate();
        let msg = ACPMessage::new(
            "agent-1",
            session.clone(),
            ACPMessageType::Request,
            serde_json::json!({"query": "test"}),
        )
        .to("agent-2")
        .with_priority(8);

        assert_eq!(msg.sender, "agent-1");
        assert_eq!(msg.recipient, Some("agent-2".to_string()));
        assert_eq!(msg.priority, 8);
    }

    #[test]
    fn test_acp_message_reply() {
        let session = SessionId::generate();
        let msg = ACPMessage::new(
            "agent-1",
            session,
            ACPMessageType::Request,
            serde_json::json!({}),
        );

        let reply = msg.reply("agent-2", serde_json::json!({"response": "ok"}));

        assert_eq!(reply.sender, "agent-2");
        assert_eq!(reply.recipient, Some("agent-1".to_string()));
        assert_eq!(reply.correlation_id, Some(msg.id));
        assert_eq!(reply.message_type, ACPMessageType::Response);
    }

    #[tokio::test]
    async fn test_shared_memory_basic() {
        let memory = SharedMemory::new();

        memory
            .set("key1", serde_json::json!("value1"), "agent-1")
            .await
            .unwrap();

        let value = memory.get("key1", "agent-1").await;
        assert!(value.is_some());
        assert_eq!(value.unwrap().value, serde_json::json!("value1"));
    }

    #[tokio::test]
    async fn test_shared_memory_versioning() {
        let memory = SharedMemory::new();

        let v1 = memory
            .set("key", serde_json::json!(1), "agent-1")
            .await
            .unwrap();
        let v2 = memory
            .set("key", serde_json::json!(2), "agent-1")
            .await
            .unwrap();

        assert_eq!(v1, 1);
        assert_eq!(v2, 2);
    }

    #[tokio::test]
    async fn test_task_delegation() {
        let delegator = TaskDelegator::new();

        // Register agents
        delegator
            .register_capabilities("agent-1", vec!["coding".to_string()].into_iter().collect())
            .await;

        // Create task
        let task = CollaborativeTask::new("Write code", "user")
            .with_capabilities(vec!["coding".to_string()]);

        let task_id = delegator.submit_task(task).await;

        // Find best agent
        let task = delegator.get_task(&task_id).await.unwrap();
        let agent = delegator.find_best_agent(&task).await;

        assert_eq!(agent, Some("agent-1".to_string()));
    }

    #[test]
    fn test_proposal_voting() {
        let mut proposal = Proposal::new(
            "Choose color",
            "agent-1",
            vec!["red".to_string(), "blue".to_string()],
            chrono::Utc::now() + chrono::Duration::hours(1),
        );

        proposal.vote("agent-2", 0).unwrap();
        proposal.vote("agent-3", 0).unwrap();
        proposal.vote("agent-4", 1).unwrap();

        let winner = proposal.tally(4);

        assert_eq!(winner, Some(0)); // Red wins
    }

    #[test]
    fn test_proposal_tie() {
        let mut proposal = Proposal::new(
            "Choose color",
            "agent-1",
            vec!["red".to_string(), "blue".to_string()],
            chrono::Utc::now() + chrono::Duration::hours(1),
        );

        proposal.vote("agent-2", 0).unwrap();
        proposal.vote("agent-3", 1).unwrap();

        let winner = proposal.tally(2);

        assert_eq!(winner, None);
        assert_eq!(proposal.status, ProposalStatus::Failed);
    }
}
