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

mod agent;
mod cache;
mod checkpoint;
mod collaboration;
mod config;
mod context;
mod conversation;
mod error;
mod knowledge_graph;
mod memory;
mod orchestrator;
mod planning;
mod privacy;
mod sandbox;
mod self_healing;
mod streaming;
mod task_planner;
pub mod thinking;

pub use error::{Error, Result};
/// Alias for CoreError used by internal modules
pub type CoreError = Error;
pub use agent::{Agent, AgentBuilder, AgentState};
pub use cache::{
    CacheConfig, CacheStats, CachedResponse, EmbeddingCache, ResponseCache, ToolCache,
};
pub use checkpoint::{Checkpoint, CheckpointConfig, CheckpointId, CheckpointManager};
pub use collaboration::{
    ACPMessage, ACPMessageType, AccessControl, CollaborativeTask, ConsensusEvent, ConsensusManager,
    Proposal, ProposalStatus, SessionId, SharedMemory, SharedMemoryEvent, SharedValue,
    TaskDelegator, TaskEvent, TaskStatus as CollaborativeTaskStatus, ACP_VERSION,
};
pub use config::{AgentConfig, AgentConfigBuilder};
pub use context::{Context, ContextManager};
pub use conversation::{Conversation, ConversationId, Turn};
pub use knowledge_graph::{
    Entity, EntityId, FilterOperation, GraphConfig, GraphExport, GraphQuery, GraphStats,
    KnowledgeGraph, PropertyFilter, PropertyValue, QueryResult, RelationDirection, Relationship,
    RelationshipId, TraversalOptions, TraversalResult,
};
pub use memory::{Memory, MemoryEntry, MemoryStore};
pub use orchestrator::{
    AgentConfig as OrchestratorAgentConfig, AgentConfigBuilder as OrchestratorAgentConfigBuilder,
    AgentId, AgentMessage, AgentMessageType, AgentRole, AgentStatus, Orchestrator,
    OrchestratorTask, TaskResult, WorkflowBuilder,
};
pub use planning::{AgentMode, Plan, PlanBuilder, PlanStep, PlannerConfig};
pub use privacy::{
    AccessType, AuditEntry, DataMinimizer, PiiMatch, PiiType, PrivacyConfig, PrivacyLevel,
    PrivacyShield, PrivacyStats,
};
pub use sandbox::{
    Capability, CommandRules, NetworkRules, PathRules, ResourceLimits, SecurityCheckResult,
    SecurityConfig, SecurityEvent, SecurityEventType, SecurityLevel, SecuritySandbox,
};
pub use self_healing::{
    CircuitBreaker, CircuitState, ErrorCategory, RecoveryRecord, RecoveryStats, RecoveryStrategy,
    SelfHealingSystem,
};
pub use streaming::{
    FilterTransformer, MapTransformer, MergeStrategy, MultiStreamMerger, StreamBuilder,
    StreamConfig, StreamController, StreamState, StreamStats, StreamToken, StreamTransformer,
    StreamingResponse, TokenAggregator,
};
pub use task_planner::{
    DefaultTaskExecutor, ExecutionPlan, ExecutionProgress, Goal,
    PlannerConfig as TaskPlannerConfig, PlannerConfigBuilder as TaskPlannerConfigBuilder,
    PlannerStats, ProgressCallback, Task, TaskComplexity, TaskExecutor, TaskId, TaskPlanBuilder,
    TaskPlanner, TaskPriority, TaskStatus, TaskType,
};

/// Re-export commonly used types from ember-llm
pub mod llm {
    pub use ember_llm::{
        CompletionRequest, CompletionResponse, LLMProvider, Message, Role, StreamChunk, ToolCall,
        ToolDefinition, ToolResult,
    };

    /// Alias for backward compatibility
    pub type Tool = ToolDefinition;
}

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::llm::*;
    pub use crate::{
        Agent,
        AgentBuilder,
        AgentConfig,
        AgentConfigBuilder,
        // Multi-Agent Orchestration
        AgentId,
        AgentMode,
        AgentRole,
        AgentState,
        AgentStatus,
        CacheStats,
        Capability,
        Checkpoint,
        CheckpointConfig,
        CheckpointId,
        CheckpointManager,
        CircuitState,
        Context,
        ContextManager,
        Conversation,
        ConversationId,
        Entity,
        EntityId,
        Error,
        ErrorCategory,
        ExecutionPlan,
        Goal,
        GraphQuery,
        // Knowledge Graph
        KnowledgeGraph,
        Memory,
        MemoryEntry,
        Orchestrator,
        OrchestratorTask,
        PiiType,
        Plan,
        PlanBuilder,
        PlanStep,
        PrivacyLevel,
        // Privacy Shield
        PrivacyShield,
        Relationship,
        // Smart Caching
        ResponseCache,
        Result,
        SecurityLevel,
        // Security Sandbox
        SecuritySandbox,
        // Self-Healing System
        SelfHealingSystem,
        StreamController,
        StreamState,
        StreamToken,
        // Real-time Streaming
        StreamingResponse,
        Task,
        TaskId,
        // Intelligent Task Planner
        TaskPlanner,
        Turn,
        WorkflowBuilder,
    };
}
