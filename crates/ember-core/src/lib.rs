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
// --- Pedantic style lints (acceptable globally for a 25k-line crate) ---
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::similar_names)]
#![allow(clippy::struct_field_names)]
// --- Casting lints (pervasive in token-counting / metrics code) ---
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_lossless)]
// --- Pattern / style preferences ---
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::single_match_else)]
#![allow(clippy::if_not_else)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::redundant_closure_for_method_calls)]

mod agent;
pub mod bootstrap;
mod cache;
mod checkpoint;
mod collaboration;
pub mod compaction;
mod config;
pub mod config_merge;
mod context;
mod context_manager;
mod conversation;
mod cost_predictor;
mod error;
mod knowledge_graph;
mod memory;
pub mod memory_optimization;
pub mod oauth;
mod orchestrator;
pub mod performance;
pub mod permissions;
mod planning;
mod privacy;
pub mod runtime;
mod sandbox;
pub mod security;
mod self_healing;
pub mod session_fork;
mod streaming;
pub mod system_prompt;
mod task_planner;
pub mod thinking;
pub mod tool_executor;
mod tool_selector;
pub mod usage_tracker;

pub use bootstrap::{BootstrapPhase, BootstrapPlan, BootstrapTimer};
pub use error::{Error, Result};
pub use runtime::{
    CompactionSummary, ConversationRuntime, LlmBackend, LlmResponse, ResponseBlock, RuntimeConfig,
    RuntimeError, RuntimeEvent, RuntimeStats, TokenUsageUpdate, ToolBackend,
};
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
pub use compaction::{
    compact_conversation, compact_message_history, estimate_string_tokens,
    estimate_tokens as estimate_conversation_tokens, should_compact, CompactMessageInfo,
    CompactionConfig, CompactionResult, ContextBudget, StrategyReflection, StrategyTracker,
};
pub use config::{AgentConfig, AgentConfigBuilder};
pub use config_merge::{
    deep_merge, discover_config_files, load_config_file, ConfigEntry, ConfigLoader, ConfigSource,
    MergedConfig,
};
pub use context::{Context, ContextManager};
pub use context_manager::{
    ContextManager as ContextManagerV2, ContextManagerBuilder as ContextManagerV2Builder,
    ContextMessage, MessageRole, PriorityWeights, PruningStrategy, TokenCount,
};
pub use conversation::{
    Conversation, ConversationExport, ConversationId, ExportFormat, ExportMessage, ExportMetadata,
    ExportToolCall, Turn,
};
pub use cost_predictor::{
    BudgetAlert, BudgetConfig, CostPredictor, CostRecommendation, PredictionResult, UsageRecord,
    UsageStats,
};
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
pub use performance::{
    BatchConfig,
    // Batch Processing
    BatchProcessor,
    BatchResult,
    BreakerConfig,
    BreakerState,
    CircuitBreakerError,
    CircuitBreakerStats,
    // Circuit Breaker
    CircuitBreakerV2,
    // Connection Pooling
    ConnectionPool,
    InternerStats,
    // Object Pooling
    ObjectPool,
    ObjectPoolStats,
    PoolConfig,
    PoolStats,
    PooledConnection,
    PooledObject,
    SchedulerConfig,
    SchedulerStats,
    // String Interner
    StringInterner,
    TaskPriority as PerformanceTaskPriority,
    // Task Scheduler
    TaskScheduler,
    // Throttler
    Throttler,
};
pub use permissions::{
    PermissionMode, PermissionPolicy, PermissionResult, ToolAction, ToolPermission,
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
pub use security::{
    AuditCategory,
    AuditConfig,
    AuditEvent,
    // Audit Logging
    AuditLogger,
    AuditOutcome,
    AuditQuery,
    AuditSeverity,
    ConditionOperator,
    // Input Validation
    InputValidator,
    PolicyContext,
    // Security Policy Engine
    PolicyEngine,
    PolicyResult,
    RateLimitConfig,
    RateLimitResult,
    // Rate Limiting
    RateLimiter,
    RuleAction,
    RuleCondition,
    SecurityPolicy,
    SecurityRule,
    ValidationConfig,
    ValidationError,
    ValidationMetadata,
    ValidationResult,
};
pub use self_healing::{
    CircuitBreaker, CircuitState, ErrorCategory, RecoveryRecord, RecoveryStats, RecoveryStrategy,
    SelfHealingSystem,
};
pub use session_fork::{ForkNode, SessionFork, SessionForkManager};
pub use streaming::{
    FilterTransformer, MapTransformer, MergeStrategy, MultiStreamMerger, StreamBuilder,
    StreamConfig, StreamController, StreamState, StreamStats, StreamToken, StreamTransformer,
    StreamingResponse, TokenAggregator,
};
pub use system_prompt::{
    classify_tool_risk, detect_project_kind, ProjectKind, RiskTier, SystemPromptBuilder,
};
pub use task_planner::{
    DefaultTaskExecutor, ExecutionPlan, ExecutionProgress, Goal,
    PlannerConfig as TaskPlannerConfig, PlannerConfigBuilder as TaskPlannerConfigBuilder,
    PlannerStats, ProgressCallback, Task, TaskComplexity, TaskExecutor, TaskId, TaskPlanBuilder,
    TaskPlanner, TaskPriority, TaskStatus, TaskType,
};
pub use tool_executor::{
    AsyncFunctionTool, AsyncTool, ExecutionMetrics, ExecutorConfig, FunctionTool, MetricsSummary,
    ToolContext, ToolExecutionResult, ToolExecutor, ToolRegistry,
};
pub use tool_selector::{
    SelectionContext, ToolCapability, ToolMetadata, ToolRecommendation, ToolSelector,
    ToolSelectorConfig,
};
pub use usage_tracker::{
    pricing_for_model, ModelPricing, SessionUsageTracker, TurnUsage, UsageCostEstimate,
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
        // Granular tool permissions
        PermissionMode,
        PermissionPolicy,
        PermissionResult,
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
        ToolAction,
        ToolPermission,
        Turn,
        WorkflowBuilder,
    };
}
