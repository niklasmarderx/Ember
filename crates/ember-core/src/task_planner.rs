//! # Intelligent Task Planner
//!
//! Advanced AI-powered task planning and execution engine that breaks down
//! complex goals into manageable steps, manages dependencies, and adapts
//! plans dynamically based on execution feedback.
//!
//! ## Features
//!
//! - **Automatic Task Decomposition**: Breaks complex goals into atomic tasks
//! - **Dependency Management**: Tracks task dependencies and execution order
//! - **Dynamic Replanning**: Adapts plans based on task outcomes
//! - **Parallel Execution**: Identifies independent tasks for concurrent execution
//! - **Progress Tracking**: Real-time progress monitoring and estimation
//! - **Rollback Support**: Undo completed tasks if needed
//!
//! ## Example
//!
//! ```rust,ignore
//! use ember_core::task_planner::{TaskPlanner, Goal, PlannerConfig};
//!
//! let planner = TaskPlanner::new(PlannerConfig::default());
//!
//! // Create a complex goal
//! let goal = Goal::new("Build a REST API")
//!     .with_context("Using Rust and Axum framework")
//!     .with_constraint("Must be production-ready");
//!
//! // Generate an execution plan
//! let plan = planner.create_plan(goal).await?;
//!
//! // Execute with progress tracking
//! let result = planner.execute(plan, |progress| {
//!     println!("Progress: {}%", progress.percentage);
//! }).await?;
//! ```

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{Error, Result};

// ============================================================================
// Task Types
// ============================================================================

/// Unique identifier for a task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(Uuid);

impl TaskId {
    /// Creates a new unique task ID
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the inner UUID
    #[must_use]
    pub fn inner(&self) -> Uuid {
        self.0
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Priority level for tasks
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub enum TaskPriority {
    /// Lowest priority
    Low = 0,
    /// Normal priority
    #[default]
    Normal = 1,
    /// High priority
    High = 2,
    /// Critical priority - must be completed first
    Critical = 3,
}

/// Current status of a task
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task is waiting to be started
    Pending,
    /// Task is ready to execute (dependencies met)
    Ready,
    /// Task is currently executing
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed with error message
    Failed(String),
    /// Task was cancelled
    Cancelled,
    /// Task was skipped (dependency failed)
    Skipped,
    /// Task is blocked by dependencies
    Blocked,
}

impl TaskStatus {
    /// Check if task is in a terminal state
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed(_) | Self::Cancelled | Self::Skipped
        )
    }

    /// Check if task can be executed
    #[must_use]
    pub fn is_runnable(&self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// Type of task based on what action it performs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskType {
    /// Analysis task - gather information
    Analysis,
    /// Planning task - create a sub-plan
    Planning,
    /// Generation task - create content/code
    Generation,
    /// Validation task - verify output
    Validation,
    /// Execution task - run a command/tool
    Execution,
    /// Review task - human or AI review
    Review,
    /// Integration task - combine outputs
    Integration,
    /// Custom task type
    Custom(String),
}

/// Estimated complexity of a task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskComplexity {
    /// Simple task - few minutes
    Trivial,
    /// Easy task - under an hour
    Simple,
    /// Medium complexity - few hours
    Medium,
    /// Complex task - day of work
    Complex,
    /// Very complex - multiple days
    VeryComplex,
}

impl TaskComplexity {
    /// Get estimated duration for this complexity level
    #[must_use]
    pub fn estimated_duration(&self) -> Duration {
        match self {
            Self::Trivial => Duration::from_secs(60),
            Self::Simple => Duration::from_secs(300),
            Self::Medium => Duration::from_secs(1800),
            Self::Complex => Duration::from_secs(7200),
            Self::VeryComplex => Duration::from_secs(28800),
        }
    }
}

// ============================================================================
// Task Definition
// ============================================================================

/// A single task in the execution plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier
    pub id: TaskId,
    /// Human-readable name
    pub name: String,
    /// Detailed description
    pub description: String,
    /// Type of task
    pub task_type: TaskType,
    /// Priority level
    pub priority: TaskPriority,
    /// Estimated complexity
    pub complexity: TaskComplexity,
    /// Current status
    pub status: TaskStatus,
    /// IDs of tasks this depends on
    pub dependencies: Vec<TaskId>,
    /// IDs of tasks that depend on this
    pub dependents: Vec<TaskId>,
    /// Input data/context for this task
    pub inputs: HashMap<String, serde_json::Value>,
    /// Output data from this task
    pub outputs: HashMap<String, serde_json::Value>,
    /// Tags for categorization
    pub tags: HashSet<String>,
    /// When the task was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the task started executing
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the task completed
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Number of retry attempts
    pub retry_count: u32,
    /// Maximum retries allowed
    pub max_retries: u32,
    /// Whether this task can be rolled back
    pub rollbackable: bool,
    /// Metadata for the task
    pub metadata: HashMap<String, String>,
}

impl Task {
    /// Create a new task with the given name
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: TaskId::new(),
            name: name.into(),
            description: String::new(),
            task_type: TaskType::Execution,
            priority: TaskPriority::Normal,
            complexity: TaskComplexity::Medium,
            status: TaskStatus::Pending,
            dependencies: Vec::new(),
            dependents: Vec::new(),
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            tags: HashSet::new(),
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            retry_count: 0,
            max_retries: 3,
            rollbackable: false,
            metadata: HashMap::new(),
        }
    }

    /// Set the task description
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set the task type
    #[must_use]
    pub fn with_type(mut self, task_type: TaskType) -> Self {
        self.task_type = task_type;
        self
    }

    /// Set the priority
    #[must_use]
    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set the complexity
    #[must_use]
    pub fn with_complexity(mut self, complexity: TaskComplexity) -> Self {
        self.complexity = complexity;
        self
    }

    /// Add a dependency
    #[must_use]
    pub fn depends_on(mut self, task_id: TaskId) -> Self {
        if !self.dependencies.contains(&task_id) {
            self.dependencies.push(task_id);
        }
        self
    }

    /// Add an input
    #[must_use]
    pub fn with_input(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.inputs.insert(key.into(), value);
        self
    }

    /// Add a tag
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.insert(tag.into());
        self
    }

    /// Enable rollback support
    #[must_use]
    pub fn with_rollback(mut self) -> Self {
        self.rollbackable = true;
        self
    }

    /// Set max retries
    #[must_use]
    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }

    /// Get estimated duration
    #[must_use]
    pub fn estimated_duration(&self) -> Duration {
        self.complexity.estimated_duration()
    }

    /// Check if all dependencies are completed
    pub fn dependencies_met(&self, completed: &HashSet<TaskId>) -> bool {
        self.dependencies.iter().all(|dep| completed.contains(dep))
    }
}

// ============================================================================
// Goal Definition
// ============================================================================

/// A high-level goal to be achieved through task planning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    /// Unique identifier
    pub id: Uuid,
    /// The goal description
    pub description: String,
    /// Additional context
    pub context: String,
    /// Constraints to satisfy
    pub constraints: Vec<String>,
    /// Success criteria
    pub success_criteria: Vec<String>,
    /// Priority of this goal
    pub priority: TaskPriority,
    /// Maximum time allowed
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    /// Tags for categorization
    pub tags: HashSet<String>,
}

impl Goal {
    /// Create a new goal
    #[must_use]
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            description: description.into(),
            context: String::new(),
            constraints: Vec::new(),
            success_criteria: Vec::new(),
            priority: TaskPriority::Normal,
            deadline: None,
            tags: HashSet::new(),
        }
    }

    /// Add context
    #[must_use]
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = context.into();
        self
    }

    /// Add a constraint
    #[must_use]
    pub fn with_constraint(mut self, constraint: impl Into<String>) -> Self {
        self.constraints.push(constraint.into());
        self
    }

    /// Add a success criterion
    #[must_use]
    pub fn with_success_criterion(mut self, criterion: impl Into<String>) -> Self {
        self.success_criteria.push(criterion.into());
        self
    }

    /// Set priority
    #[must_use]
    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set deadline
    #[must_use]
    pub fn with_deadline(mut self, deadline: chrono::DateTime<chrono::Utc>) -> Self {
        self.deadline = Some(deadline);
        self
    }
}

// ============================================================================
// Execution Plan
// ============================================================================

/// A complete execution plan for achieving a goal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    /// Unique identifier
    pub id: Uuid,
    /// The goal this plan achieves
    pub goal: Goal,
    /// All tasks in the plan
    pub tasks: HashMap<TaskId, Task>,
    /// Execution order (tasks that can run in parallel are grouped)
    pub execution_stages: Vec<Vec<TaskId>>,
    /// Plan version (incremented on replanning)
    pub version: u32,
    /// When the plan was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the plan was last modified
    pub modified_at: chrono::DateTime<chrono::Utc>,
    /// Total estimated duration
    pub estimated_duration: Duration,
    /// Plan metadata
    pub metadata: HashMap<String, String>,
}

impl ExecutionPlan {
    /// Create a new execution plan for a goal
    #[must_use]
    pub fn new(goal: Goal) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            goal,
            tasks: HashMap::new(),
            execution_stages: Vec::new(),
            version: 1,
            created_at: now,
            modified_at: now,
            estimated_duration: Duration::ZERO,
            metadata: HashMap::new(),
        }
    }

    /// Add a task to the plan
    pub fn add_task(&mut self, task: Task) {
        let id = task.id;
        self.estimated_duration += task.estimated_duration();
        self.tasks.insert(id, task);
        self.modified_at = chrono::Utc::now();
    }

    /// Get a task by ID
    #[must_use]
    pub fn get_task(&self, id: TaskId) -> Option<&Task> {
        self.tasks.get(&id)
    }

    /// Get a mutable task by ID
    pub fn get_task_mut(&mut self, id: TaskId) -> Option<&mut Task> {
        self.tasks.get_mut(&id)
    }

    /// Calculate execution stages based on dependencies
    pub fn calculate_stages(&mut self) {
        self.execution_stages.clear();

        let mut completed: HashSet<TaskId> = HashSet::new();
        let mut remaining: HashSet<TaskId> = self.tasks.keys().copied().collect();

        while !remaining.is_empty() {
            // Find all tasks whose dependencies are met
            let ready: Vec<TaskId> = remaining
                .iter()
                .filter(|id| {
                    self.tasks
                        .get(id)
                        .is_some_and(|t| t.dependencies_met(&completed))
                })
                .copied()
                .collect();

            if ready.is_empty() {
                // Circular dependency detected
                warn!("Circular dependency detected in plan");
                break;
            }

            // Sort by priority within each stage
            let mut stage = ready;
            stage.sort_by(|a, b| {
                let task_a = self.tasks.get(a);
                let task_b = self.tasks.get(b);
                match (task_a, task_b) {
                    (Some(a), Some(b)) => b.priority.cmp(&a.priority),
                    _ => std::cmp::Ordering::Equal,
                }
            });

            for id in &stage {
                completed.insert(*id);
                remaining.remove(id);
            }

            self.execution_stages.push(stage);
        }
    }

    /// Get all tasks that are ready to execute
    #[must_use]
    pub fn get_ready_tasks(&self) -> Vec<TaskId> {
        let completed: HashSet<TaskId> = self
            .tasks
            .iter()
            .filter(|(_, t)| matches!(t.status, TaskStatus::Completed))
            .map(|(id, _)| *id)
            .collect();

        self.tasks
            .iter()
            .filter(|(_, t)| {
                matches!(t.status, TaskStatus::Pending | TaskStatus::Ready)
                    && t.dependencies_met(&completed)
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Get completion percentage
    #[must_use]
    pub fn completion_percentage(&self) -> f64 {
        if self.tasks.is_empty() {
            return 100.0;
        }

        let completed = self
            .tasks
            .values()
            .filter(|t| matches!(t.status, TaskStatus::Completed))
            .count();

        (completed as f64 / self.tasks.len() as f64) * 100.0
    }

    /// Check if plan is complete
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.tasks.values().all(|t| t.status.is_terminal())
    }

    /// Check if plan failed
    #[must_use]
    pub fn has_failed(&self) -> bool {
        self.tasks
            .values()
            .any(|t| matches!(t.status, TaskStatus::Failed(_)))
    }
}

// ============================================================================
// Progress Tracking
// ============================================================================

/// Progress information for plan execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionProgress {
    /// Current completion percentage
    pub percentage: f64,
    /// Number of completed tasks
    pub completed_tasks: usize,
    /// Total number of tasks
    pub total_tasks: usize,
    /// Currently running tasks
    pub running_tasks: Vec<TaskId>,
    /// Estimated time remaining
    pub estimated_remaining: Duration,
    /// Time elapsed
    pub elapsed: Duration,
    /// Current stage index
    pub current_stage: usize,
    /// Total stages
    pub total_stages: usize,
    /// Recent messages
    pub messages: Vec<String>,
}

/// Progress callback type
pub type ProgressCallback = Arc<dyn Fn(ExecutionProgress) + Send + Sync>;

// ============================================================================
// Task Executor
// ============================================================================

/// Trait for executing tasks
#[async_trait::async_trait]
pub trait TaskExecutor: Send + Sync {
    /// Execute a task and return the result
    async fn execute(&self, task: &Task) -> Result<HashMap<String, serde_json::Value>>;

    /// Check if this executor can handle the task type
    fn can_execute(&self, task_type: &TaskType) -> bool;

    /// Rollback a completed task
    async fn rollback(&self, task: &Task) -> Result<()> {
        let _ = task;
        Err(Error::NotImplemented("Rollback not supported".into()))
    }
}

/// Default task executor that handles basic task types
pub struct DefaultTaskExecutor;

#[async_trait::async_trait]
impl TaskExecutor for DefaultTaskExecutor {
    async fn execute(&self, task: &Task) -> Result<HashMap<String, serde_json::Value>> {
        debug!(task_id = %task.id, task_name = %task.name, "Executing task");

        // Simulate task execution based on complexity
        let duration = match task.complexity {
            TaskComplexity::Trivial => Duration::from_millis(100),
            TaskComplexity::Simple => Duration::from_millis(250),
            TaskComplexity::Medium => Duration::from_millis(500),
            TaskComplexity::Complex => Duration::from_secs(1),
            TaskComplexity::VeryComplex => Duration::from_secs(2),
        };

        tokio::time::sleep(duration).await;

        let mut outputs = HashMap::new();
        outputs.insert(
            "result".to_string(),
            serde_json::json!({
                "status": "completed",
                "task_name": task.name,
                "task_type": format!("{:?}", task.task_type),
            }),
        );

        Ok(outputs)
    }

    fn can_execute(&self, _task_type: &TaskType) -> bool {
        true
    }
}

// ============================================================================
// Planner Configuration
// ============================================================================

/// Configuration for the task planner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerConfig {
    /// Maximum concurrent tasks
    pub max_concurrent_tasks: usize,
    /// Enable automatic replanning on failure
    pub auto_replan: bool,
    /// Maximum replan attempts
    pub max_replan_attempts: u32,
    /// Enable parallel execution
    pub parallel_execution: bool,
    /// Task timeout
    pub task_timeout: Duration,
    /// Whether to continue on task failure
    pub continue_on_failure: bool,
    /// Maximum tasks in a plan
    pub max_tasks: usize,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 4,
            auto_replan: true,
            max_replan_attempts: 3,
            parallel_execution: true,
            task_timeout: Duration::from_secs(300),
            continue_on_failure: false,
            max_tasks: 100,
        }
    }
}

impl PlannerConfig {
    /// Create a new builder
    #[must_use]
    pub fn builder() -> PlannerConfigBuilder {
        PlannerConfigBuilder::default()
    }
}

/// Builder for planner configuration
#[derive(Debug, Default)]
pub struct PlannerConfigBuilder {
    config: PlannerConfig,
}

impl PlannerConfigBuilder {
    /// Set max concurrent tasks
    #[must_use]
    pub fn max_concurrent_tasks(mut self, max: usize) -> Self {
        self.config.max_concurrent_tasks = max;
        self
    }

    /// Enable/disable auto replanning
    #[must_use]
    pub fn auto_replan(mut self, enabled: bool) -> Self {
        self.config.auto_replan = enabled;
        self
    }

    /// Set max replan attempts
    #[must_use]
    pub fn max_replan_attempts(mut self, max: u32) -> Self {
        self.config.max_replan_attempts = max;
        self
    }

    /// Enable/disable parallel execution
    #[must_use]
    pub fn parallel_execution(mut self, enabled: bool) -> Self {
        self.config.parallel_execution = enabled;
        self
    }

    /// Set task timeout
    #[must_use]
    pub fn task_timeout(mut self, timeout: Duration) -> Self {
        self.config.task_timeout = timeout;
        self
    }

    /// Continue on failure
    #[must_use]
    pub fn continue_on_failure(mut self, enabled: bool) -> Self {
        self.config.continue_on_failure = enabled;
        self
    }

    /// Build the configuration
    #[must_use]
    pub fn build(self) -> PlannerConfig {
        self.config
    }
}

// ============================================================================
// Task Planner
// ============================================================================

/// Intelligent task planner for complex goal decomposition
pub struct TaskPlanner {
    config: PlannerConfig,
    executors: RwLock<Vec<Arc<dyn TaskExecutor>>>,
    active_plans: RwLock<HashMap<Uuid, Arc<Mutex<ExecutionPlan>>>>,
    stats: RwLock<PlannerStats>,
}

/// Statistics for the task planner
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlannerStats {
    /// Total plans created
    pub plans_created: u64,
    /// Total plans completed
    pub plans_completed: u64,
    /// Total plans failed
    pub plans_failed: u64,
    /// Total tasks executed
    pub tasks_executed: u64,
    /// Total tasks failed
    pub tasks_failed: u64,
    /// Average plan completion time
    pub avg_completion_time: Duration,
    /// Replan count
    pub replan_count: u64,
}

impl TaskPlanner {
    /// Create a new task planner with the given configuration
    #[must_use]
    pub fn new(config: PlannerConfig) -> Self {
        Self {
            config,
            executors: RwLock::new(vec![Arc::new(DefaultTaskExecutor)]),
            active_plans: RwLock::new(HashMap::new()),
            stats: RwLock::new(PlannerStats::default()),
        }
    }

    /// Register a task executor
    pub async fn register_executor(&self, executor: Arc<dyn TaskExecutor>) {
        let mut executors = self.executors.write().await;
        executors.push(executor);
    }

    /// Create an execution plan for a goal
    ///
    /// This method analyzes the goal and generates a structured plan
    /// with tasks, dependencies, and execution stages.
    pub async fn create_plan(&self, goal: Goal) -> Result<ExecutionPlan> {
        info!(goal_id = %goal.id, goal = %goal.description, "Creating execution plan");

        let mut plan = ExecutionPlan::new(goal.clone());

        // Generate tasks based on goal analysis
        let tasks = self.decompose_goal(&goal).await?;

        if tasks.len() > self.config.max_tasks {
            return Err(Error::config(format!(
                "Plan exceeds maximum tasks: {} > {}",
                tasks.len(),
                self.config.max_tasks
            )));
        }

        // Add all tasks to the plan
        for task in tasks {
            plan.add_task(task);
        }

        // Calculate execution stages
        plan.calculate_stages();

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.plans_created += 1;
        }

        // Store the plan
        {
            let mut active = self.active_plans.write().await;
            active.insert(plan.id, Arc::new(Mutex::new(plan.clone())));
        }

        info!(
            plan_id = %plan.id,
            tasks = plan.tasks.len(),
            stages = plan.execution_stages.len(),
            "Execution plan created"
        );

        Ok(plan)
    }

    /// Decompose a goal into tasks
    async fn decompose_goal(&self, goal: &Goal) -> Result<Vec<Task>> {
        // In a real implementation, this would use an LLM to analyze the goal
        // For now, we create a sample decomposition

        let mut tasks = Vec::new();

        // Analysis phase
        let analysis = Task::new("Analyze Requirements")
            .with_description(format!("Analyze the goal: {}", goal.description))
            .with_type(TaskType::Analysis)
            .with_priority(TaskPriority::High)
            .with_complexity(TaskComplexity::Simple)
            .with_tag("phase:analysis");
        let analysis_id = analysis.id;
        tasks.push(analysis);

        // Planning phase
        let planning = Task::new("Create Implementation Plan")
            .with_description("Create detailed implementation plan based on analysis")
            .with_type(TaskType::Planning)
            .with_priority(TaskPriority::High)
            .with_complexity(TaskComplexity::Medium)
            .depends_on(analysis_id)
            .with_tag("phase:planning");
        let planning_id = planning.id;
        tasks.push(planning);

        // Implementation phase - multiple parallel tasks
        let impl1 = Task::new("Implement Core Logic")
            .with_description("Implement the core functionality")
            .with_type(TaskType::Generation)
            .with_priority(TaskPriority::Normal)
            .with_complexity(TaskComplexity::Complex)
            .depends_on(planning_id)
            .with_rollback()
            .with_tag("phase:implementation");
        let impl1_id = impl1.id;
        tasks.push(impl1);

        let impl2 = Task::new("Implement Supporting Features")
            .with_description("Implement supporting features and utilities")
            .with_type(TaskType::Generation)
            .with_priority(TaskPriority::Normal)
            .with_complexity(TaskComplexity::Medium)
            .depends_on(planning_id)
            .with_rollback()
            .with_tag("phase:implementation");
        let impl2_id = impl2.id;
        tasks.push(impl2);

        // Integration phase
        let integration = Task::new("Integrate Components")
            .with_description("Integrate all implemented components")
            .with_type(TaskType::Integration)
            .with_priority(TaskPriority::Normal)
            .with_complexity(TaskComplexity::Medium)
            .depends_on(impl1_id)
            .depends_on(impl2_id)
            .with_tag("phase:integration");
        let integration_id = integration.id;
        tasks.push(integration);

        // Validation phase
        let validation = Task::new("Validate Implementation")
            .with_description("Validate the implementation against success criteria")
            .with_type(TaskType::Validation)
            .with_priority(TaskPriority::High)
            .with_complexity(TaskComplexity::Simple)
            .depends_on(integration_id)
            .with_tag("phase:validation");
        let validation_id = validation.id;
        tasks.push(validation);

        // Review phase
        let review = Task::new("Final Review")
            .with_description("Perform final review and documentation")
            .with_type(TaskType::Review)
            .with_priority(TaskPriority::Normal)
            .with_complexity(TaskComplexity::Simple)
            .depends_on(validation_id)
            .with_tag("phase:review");
        tasks.push(review);

        // Update dependents
        let task_ids: HashMap<TaskId, usize> =
            tasks.iter().enumerate().map(|(i, t)| (t.id, i)).collect();

        for i in 0..tasks.len() {
            let deps = tasks[i].dependencies.clone();
            for dep_id in deps {
                if let Some(&dep_idx) = task_ids.get(&dep_id) {
                    let task_id = tasks[i].id;
                    tasks[dep_idx].dependents.push(task_id);
                }
            }
        }

        Ok(tasks)
    }

    /// Execute a plan with progress tracking
    pub async fn execute(
        &self,
        mut plan: ExecutionPlan,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<ExecutionPlan> {
        let start_time = Instant::now();

        info!(plan_id = %plan.id, "Starting plan execution");

        // Calculate stages if not done
        if plan.execution_stages.is_empty() {
            plan.calculate_stages();
        }

        let total_tasks = plan.tasks.len();
        let mut completed_tasks = 0;
        let mut current_stage = 0;

        for stage in &plan.execution_stages.clone() {
            current_stage += 1;

            if self.config.parallel_execution && stage.len() > 1 {
                // Execute tasks in parallel
                completed_tasks += self
                    .execute_stage_parallel(
                        &mut plan,
                        stage,
                        &progress_callback,
                        completed_tasks,
                        total_tasks,
                        current_stage,
                        start_time,
                    )
                    .await?;
            } else {
                // Execute tasks sequentially
                for task_id in stage {
                    if let Err(e) = self.execute_task(&mut plan, *task_id).await {
                        if !self.config.continue_on_failure {
                            return Err(e);
                        }
                        warn!(task_id = %task_id, error = %e, "Task failed, continuing");
                    }
                    completed_tasks += 1;

                    // Send progress update
                    if let Some(ref callback) = progress_callback {
                        let progress = ExecutionProgress {
                            percentage: (completed_tasks as f64 / total_tasks as f64) * 100.0,
                            completed_tasks,
                            total_tasks,
                            running_tasks: Vec::new(),
                            estimated_remaining: Duration::ZERO,
                            elapsed: start_time.elapsed(),
                            current_stage,
                            total_stages: plan.execution_stages.len(),
                            messages: Vec::new(),
                        };
                        callback(progress);
                    }
                }
            }
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            if plan.has_failed() {
                stats.plans_failed += 1;
            } else {
                stats.plans_completed += 1;
            }
            stats.tasks_executed += completed_tasks as u64;
        }

        info!(
            plan_id = %plan.id,
            duration = ?start_time.elapsed(),
            completed = completed_tasks,
            "Plan execution complete"
        );

        Ok(plan)
    }

    /// Execute a stage of tasks in parallel
    async fn execute_stage_parallel(
        &self,
        plan: &mut ExecutionPlan,
        stage: &[TaskId],
        progress_callback: &Option<ProgressCallback>,
        mut completed_tasks: usize,
        total_tasks: usize,
        current_stage: usize,
        start_time: Instant,
    ) -> Result<usize> {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(
            self.config.max_concurrent_tasks,
        ));
        let (tx, mut rx) =
            mpsc::channel::<(TaskId, Result<HashMap<String, serde_json::Value>>)>(stage.len());

        // Send progress with running tasks
        let running_tasks: Vec<TaskId> = stage.to_vec();
        if let Some(ref callback) = progress_callback {
            let progress = ExecutionProgress {
                percentage: (completed_tasks as f64 / total_tasks as f64) * 100.0,
                completed_tasks,
                total_tasks,
                running_tasks: running_tasks.clone(),
                estimated_remaining: Duration::ZERO,
                elapsed: start_time.elapsed(),
                current_stage,
                total_stages: plan.execution_stages.len(),
                messages: vec![format!(
                    "Executing stage {} with {} parallel tasks",
                    current_stage,
                    stage.len()
                )],
            };
            callback(progress);
        }

        // Spawn all tasks
        for &task_id in stage {
            let permit = semaphore
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| Error::Internal("Semaphore closed".into()))?;

            let task = plan.tasks.get(&task_id).cloned();
            let executors = self.executors.read().await.clone();
            let tx = tx.clone();
            let timeout = self.config.task_timeout;

            tokio::spawn(async move {
                let _permit = permit;

                if let Some(task) = task {
                    let result = tokio::time::timeout(timeout, async {
                        for executor in &executors {
                            if executor.can_execute(&task.task_type) {
                                return executor.execute(&task).await;
                            }
                        }
                        Err(Error::NotFound(format!(
                            "No executor for task type {:?}",
                            task.task_type
                        )))
                    })
                    .await;

                    let result = match result {
                        Ok(r) => r,
                        Err(_) => Err(Error::Timeout("Task execution timed out")),
                    };

                    let _ = tx.send((task_id, result)).await;
                }
            });
        }

        // Drop our copy of tx so the channel closes when all tasks complete
        drop(tx);

        // Collect results
        let mut stage_completed = 0;
        while let Some((task_id, result)) = rx.recv().await {
            if let Some(task) = plan.tasks.get_mut(&task_id) {
                match result {
                    Ok(outputs) => {
                        task.status = TaskStatus::Completed;
                        task.outputs = outputs;
                        task.completed_at = Some(chrono::Utc::now());
                    }
                    Err(e) => {
                        task.status = TaskStatus::Failed(e.to_string());
                        if !self.config.continue_on_failure {
                            return Err(e);
                        }
                    }
                }
            }
            stage_completed += 1;
            completed_tasks += 1;

            // Send progress update
            if let Some(ref callback) = progress_callback {
                let progress = ExecutionProgress {
                    percentage: (completed_tasks as f64 / total_tasks as f64) * 100.0,
                    completed_tasks,
                    total_tasks,
                    running_tasks: running_tasks
                        .iter()
                        .filter(|id| plan.tasks.get(id).is_some_and(|t| !t.status.is_terminal()))
                        .copied()
                        .collect(),
                    estimated_remaining: Duration::ZERO,
                    elapsed: start_time.elapsed(),
                    current_stage,
                    total_stages: plan.execution_stages.len(),
                    messages: Vec::new(),
                };
                callback(progress);
            }
        }

        Ok(stage_completed)
    }

    /// Execute a single task
    async fn execute_task(&self, plan: &mut ExecutionPlan, task_id: TaskId) -> Result<()> {
        // Extract task info before mutable borrow
        let (task_type, task_clone) = {
            let task = plan
                .tasks
                .get(&task_id)
                .ok_or_else(|| Error::NotFound(format!("Task not found: {}", task_id)))?;
            (task.task_type.clone(), task.clone())
        };

        // Mark as running
        if let Some(t) = plan.tasks.get_mut(&task_id) {
            t.status = TaskStatus::Running;
            t.started_at = Some(chrono::Utc::now());
        }

        // Find executor
        let executors = self.executors.read().await;
        let executor = executors
            .iter()
            .find(|e| e.can_execute(&task_type))
            .ok_or_else(|| Error::NotFound(format!("No executor for task type {:?}", task_type)))?;

        // Execute with timeout
        let result =
            tokio::time::timeout(self.config.task_timeout, executor.execute(&task_clone)).await;

        match result {
            Ok(Ok(outputs)) => {
                if let Some(t) = plan.tasks.get_mut(&task_id) {
                    t.status = TaskStatus::Completed;
                    t.outputs = outputs;
                    t.completed_at = Some(chrono::Utc::now());
                }
                Ok(())
            }
            Ok(Err(e)) => {
                if let Some(t) = plan.tasks.get_mut(&task_id) {
                    t.status = TaskStatus::Failed(e.to_string());
                    t.retry_count += 1;
                }
                Err(e)
            }
            Err(_) => {
                if let Some(t) = plan.tasks.get_mut(&task_id) {
                    t.status = TaskStatus::Failed("Task timed out".to_string());
                }
                Err(Error::Timeout("Task execution timed out"))
            }
        }
    }

    /// Replan based on execution state
    pub async fn replan(&self, plan: &mut ExecutionPlan) -> Result<()> {
        if !self.config.auto_replan {
            return Err(Error::config("Auto replan is disabled"));
        }

        info!(plan_id = %plan.id, version = plan.version, "Replanning");

        // Find failed tasks
        let failed: Vec<TaskId> = plan
            .tasks
            .iter()
            .filter(|(_, t)| matches!(t.status, TaskStatus::Failed(_)))
            .map(|(id, _)| *id)
            .collect();

        // Reset failed tasks and their dependents
        let mut to_reset: HashSet<TaskId> = failed.iter().copied().collect();
        let mut queue: VecDeque<TaskId> = failed.into_iter().collect();

        while let Some(task_id) = queue.pop_front() {
            if let Some(task) = plan.tasks.get(&task_id) {
                for dep in &task.dependents {
                    if to_reset.insert(*dep) {
                        queue.push_back(*dep);
                    }
                }
            }
        }

        // Reset tasks
        for task_id in to_reset {
            if let Some(task) = plan.tasks.get_mut(&task_id) {
                task.status = TaskStatus::Pending;
                task.outputs.clear();
                task.started_at = None;
                task.completed_at = None;
            }
        }

        // Increment version
        plan.version += 1;
        plan.modified_at = chrono::Utc::now();

        // Recalculate stages
        plan.calculate_stages();

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.replan_count += 1;
        }

        Ok(())
    }

    /// Get planner statistics
    pub async fn stats(&self) -> PlannerStats {
        self.stats.read().await.clone()
    }

    /// Get an active plan by ID
    pub async fn get_plan(&self, plan_id: Uuid) -> Option<ExecutionPlan> {
        let active = self.active_plans.read().await;
        if let Some(plan) = active.get(&plan_id) {
            Some(plan.lock().await.clone())
        } else {
            None
        }
    }
}

// ============================================================================
// Plan Builder
// ============================================================================

/// Builder for creating execution plans manually
pub struct TaskPlanBuilder {
    goal: Goal,
    tasks: Vec<Task>,
}

impl TaskPlanBuilder {
    /// Create a new plan builder for a goal
    #[must_use]
    pub fn new(goal: Goal) -> Self {
        Self {
            goal,
            tasks: Vec::new(),
        }
    }

    /// Add a task
    #[must_use]
    pub fn add_task(mut self, task: Task) -> Self {
        self.tasks.push(task);
        self
    }

    /// Build the execution plan
    #[must_use]
    pub fn build(self) -> ExecutionPlan {
        let mut plan = ExecutionPlan::new(self.goal);

        for task in self.tasks {
            plan.add_task(task);
        }

        plan.calculate_stages();
        plan
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let task = Task::new("Test Task")
            .with_description("A test task")
            .with_type(TaskType::Analysis)
            .with_priority(TaskPriority::High)
            .with_complexity(TaskComplexity::Simple)
            .with_tag("test");

        assert_eq!(task.name, "Test Task");
        assert_eq!(task.description, "A test task");
        assert!(matches!(task.task_type, TaskType::Analysis));
        assert!(matches!(task.priority, TaskPriority::High));
        assert!(task.tags.contains("test"));
    }

    #[test]
    fn test_goal_creation() {
        let goal = Goal::new("Build a REST API")
            .with_context("Using Rust")
            .with_constraint("Must be fast")
            .with_success_criterion("All tests pass");

        assert_eq!(goal.description, "Build a REST API");
        assert_eq!(goal.context, "Using Rust");
        assert_eq!(goal.constraints.len(), 1);
        assert_eq!(goal.success_criteria.len(), 1);
    }

    #[test]
    fn test_task_dependencies() {
        let task1 = Task::new("Task 1");
        let task1_id = task1.id;

        let task2 = Task::new("Task 2").depends_on(task1_id);

        assert!(task2.dependencies.contains(&task1_id));

        let completed: HashSet<TaskId> = [task1_id].into_iter().collect();
        assert!(task2.dependencies_met(&completed));

        let empty: HashSet<TaskId> = HashSet::new();
        assert!(!task2.dependencies_met(&empty));
    }

    #[test]
    fn test_execution_plan_stages() {
        let goal = Goal::new("Test Goal");
        let mut plan = ExecutionPlan::new(goal);

        let task1 = Task::new("Task 1");
        let task1_id = task1.id;

        let task2 = Task::new("Task 2");
        let task2_id = task2.id;

        let task3 = Task::new("Task 3")
            .depends_on(task1_id)
            .depends_on(task2_id);

        plan.add_task(task1);
        plan.add_task(task2);
        plan.add_task(task3);

        plan.calculate_stages();

        // First stage should have task1 and task2 (no deps)
        assert_eq!(plan.execution_stages.len(), 2);
        assert_eq!(plan.execution_stages[0].len(), 2);
        assert_eq!(plan.execution_stages[1].len(), 1);
    }

    #[tokio::test]
    async fn test_planner_create_plan() {
        let planner = TaskPlanner::new(PlannerConfig::default());
        let goal = Goal::new("Build a web application");

        let plan = planner.create_plan(goal).await.unwrap();

        assert!(!plan.tasks.is_empty());
        assert!(!plan.execution_stages.is_empty());
    }

    #[tokio::test]
    async fn test_planner_execute() {
        let config = PlannerConfig::builder()
            .parallel_execution(false)
            .task_timeout(Duration::from_secs(5))
            .build();

        let planner = TaskPlanner::new(config);
        let goal = Goal::new(String::from("Simple test"));

        let plan = planner.create_plan(goal).await.unwrap();
        let result = planner.execute(plan, None).await.unwrap();

        assert!(result.completion_percentage() > 99.0);
        assert!(result.is_complete());
    }

    #[tokio::test]
    async fn test_planner_parallel_execution() {
        let config = PlannerConfig::builder()
            .parallel_execution(true)
            .max_concurrent_tasks(4)
            .build();

        let planner = TaskPlanner::new(config);
        let goal = Goal::new("Parallel test");

        let plan = planner.create_plan(goal).await.unwrap();

        let progress_updates = Arc::new(RwLock::new(Vec::new()));
        let progress_clone = progress_updates.clone();

        let callback: ProgressCallback = Arc::new(move |progress| {
            let updates = progress_clone.clone();
            tokio::spawn(async move {
                updates.write().await.push(progress);
            });
        });

        let result = planner.execute(plan, Some(callback)).await.unwrap();

        assert!(result.is_complete());
    }

    #[test]
    fn test_task_status() {
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Failed("error".into()).is_terminal());
        assert!(TaskStatus::Cancelled.is_terminal());
        assert!(!TaskStatus::Pending.is_terminal());
        assert!(!TaskStatus::Running.is_terminal());

        assert!(TaskStatus::Ready.is_runnable());
        assert!(!TaskStatus::Pending.is_runnable());
    }

    #[test]
    fn test_task_complexity_duration() {
        assert!(
            TaskComplexity::Trivial.estimated_duration()
                < TaskComplexity::Simple.estimated_duration()
        );
        assert!(
            TaskComplexity::Simple.estimated_duration()
                < TaskComplexity::Medium.estimated_duration()
        );
        assert!(
            TaskComplexity::Medium.estimated_duration()
                < TaskComplexity::Complex.estimated_duration()
        );
        assert!(
            TaskComplexity::Complex.estimated_duration()
                < TaskComplexity::VeryComplex.estimated_duration()
        );
    }

    #[test]
    fn test_plan_builder() {
        let goal = Goal::new(String::from("Test"));
        let task1 = Task::new(String::from("Task 1"));
        let task2 = Task::new(String::from("Task 2")).depends_on(task1.id);

        let plan = TaskPlanBuilder::new(goal)
            .add_task(task1)
            .add_task(task2)
            .build();

        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.execution_stages.len(), 2);
    }
}
