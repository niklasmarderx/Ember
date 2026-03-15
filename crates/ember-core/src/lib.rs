//! # Ember Core
//!
//! Core agent runtime for the Ember AI framework.
//!
//! This crate provides the fundamental building blocks for creating AI agents:
//! - Agent runtime with ReAct pattern (Plan → Act → Observe → Loop)
//! - Conversation and memory management
//! - Context window optimization
//! - Tool execution coordination
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use ember_core::{Agent, AgentConfig};
//! use ember_llm::openai::OpenAIProvider;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let provider = OpenAIProvider::from_env()?;
//!     let agent = Agent::builder()
//!         .provider(provider)
//!         .system_prompt("You are a helpful assistant.")
//!         .build()?;
//!
//!     let response = agent.chat("Hello!").await?;
//!     println!("{}", response.content);
//!     Ok(())
//! }
//! ```

#![deny(missing_docs)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

mod error;
mod config;
mod context;
mod memory;
mod agent;
mod conversation;
mod planning;
mod checkpoint;
mod orchestrator;
mod collaboration;
mod self_healing;
mod privacy;
mod cache;
mod sandbox;
mod streaming;
mod task_planner;
mod knowledge_graph;
pub mod thinking;

pub use error::{Error, Result};
/// Alias for CoreError used by internal modules
pub type CoreError = Error;
pub use config::{AgentConfig, AgentConfigBuilder};
pub use context::{Context, ContextManager};
pub use memory::{Memory, MemoryEntry, MemoryStore};
pub use agent::{Agent, AgentBuilder, AgentState};
pub use conversation::{Conversation, ConversationId, Turn};
pub use planning::{AgentMode, Plan, PlanBuilder, PlanStep, PlannerConfig};
pub use checkpoint::{Checkpoint, CheckpointConfig, CheckpointId, CheckpointManager};
pub use orchestrator::{
    AgentId, AgentRole, AgentStatus, AgentConfig as OrchestratorAgentConfig,
    AgentConfigBuilder as OrchestratorAgentConfigBuilder, AgentMessage, AgentMessageType,
    Orchestrator, OrchestratorTask, TaskResult, WorkflowBuilder,
};
pub use self_healing::{
    CircuitBreaker, CircuitState, ErrorCategory, RecoveryRecord, RecoveryStats,
    RecoveryStrategy, SelfHealingSystem,
};
pub use privacy::{
    PrivacyShield, PrivacyConfig, PrivacyLevel, PrivacyStats,
    PiiType, PiiMatch, DataMinimizer, AuditEntry, AccessType,
};
pub use cache::{
    ResponseCache, CacheConfig, CacheStats, CachedResponse,
    ToolCache, EmbeddingCache,
};
pub use sandbox::{
    SecuritySandbox, SecurityConfig, SecurityLevel, SecurityCheckResult,
    Capability, ResourceLimits, PathRules, NetworkRules, CommandRules,
    SecurityEvent, SecurityEventType,
};
pub use streaming::{
    StreamingResponse, StreamController, StreamToken, StreamState, StreamStats,
    StreamConfig, StreamTransformer, FilterTransformer, MapTransformer,
    TokenAggregator, MultiStreamMerger, MergeStrategy, StreamBuilder,
};
pub use task_planner::{
    TaskPlanner, PlannerConfig as TaskPlannerConfig, PlannerConfigBuilder as TaskPlannerConfigBuilder, PlannerStats,
    ExecutionPlan, ExecutionProgress, ProgressCallback,
    Task, TaskId, TaskType, TaskStatus, TaskPriority, TaskComplexity,
    Goal, TaskExecutor, DefaultTaskExecutor, TaskPlanBuilder,
};
pub use collaboration::{
    ACPMessage, ACPMessageType, SessionId, ACP_VERSION,
    SharedMemory, SharedValue, SharedMemoryEvent, AccessControl,
    CollaborativeTask, TaskStatus as CollaborativeTaskStatus, TaskDelegator, TaskEvent,
    Proposal, ProposalStatus, ConsensusManager, ConsensusEvent,
};
pub use knowledge_graph::{
    KnowledgeGraph, GraphConfig, GraphStats, GraphExport,
    Entity, EntityId, Relationship, RelationshipId, RelationDirection,
    PropertyValue, PropertyFilter, FilterOperation,
    GraphQuery, QueryResult, TraversalOptions, TraversalResult,
};

/// Re-export commonly used types from ember-llm
pub mod llm {
    pub use ember_llm::{
        LLMProvider, Message, Role, CompletionRequest, CompletionResponse,
        ToolDefinition, ToolCall, ToolResult, StreamChunk,
    };
    
    /// Alias for backward compatibility
    pub type Tool = ToolDefinition;
}

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::{
        Agent, AgentBuilder, AgentConfig, AgentConfigBuilder, AgentState,
        Context, ContextManager, Memory, MemoryEntry,
        Conversation, ConversationId, Turn,
        AgentMode, Plan, PlanBuilder, PlanStep,
        Checkpoint, CheckpointConfig, CheckpointId, CheckpointManager,
        // Multi-Agent Orchestration
        AgentId, AgentRole, AgentStatus, Orchestrator, OrchestratorTask, WorkflowBuilder,
        // Self-Healing System
        SelfHealingSystem, ErrorCategory, CircuitState,
        // Privacy Shield
        PrivacyShield, PrivacyLevel, PiiType,
        // Smart Caching
        ResponseCache, CacheStats,
        // Security Sandbox
        SecuritySandbox, SecurityLevel, Capability,
        // Real-time Streaming
        StreamingResponse, StreamController, StreamToken, StreamState,
        // Intelligent Task Planner
        TaskPlanner, ExecutionPlan, Task, TaskId, Goal,
        // Knowledge Graph
        KnowledgeGraph, Entity, EntityId, Relationship, GraphQuery,
        Error, Result,
    };
    pub use crate::llm::*;
}
