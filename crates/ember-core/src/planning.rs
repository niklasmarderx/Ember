//! Plan/Act mode implementation for Ember agents.
//!
//! This module provides a planning system that allows agents to create
//! and execute plans before taking action, similar to Cline's plan mode.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Mode of operation for the agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum AgentMode {
    /// Act mode: Execute actions immediately
    #[default]
    Act,
    /// Plan mode: Create a plan first, then ask for approval
    Plan,
}

impl std::fmt::Display for AgentMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Act => write!(f, "act"),
            Self::Plan => write!(f, "plan"),
        }
    }
}

/// A single step in a plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// Unique ID for this step
    pub id: Uuid,
    /// Step number (1-indexed)
    pub number: usize,
    /// Description of what this step will do
    pub description: String,
    /// Tool to be used (if any)
    pub tool: Option<String>,
    /// Estimated complexity (1-10)
    pub complexity: u8,
    /// Whether this step is completed
    pub completed: bool,
    /// Dependencies on other step IDs
    pub depends_on: Vec<Uuid>,
    /// Optional notes or reasoning
    pub notes: Option<String>,
}

impl PlanStep {
    /// Create a new plan step
    pub fn new(number: usize, description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            number,
            description: description.into(),
            tool: None,
            complexity: 5,
            completed: false,
            depends_on: Vec::new(),
            notes: None,
        }
    }

    /// Set the tool for this step
    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }

    /// Set the complexity
    pub fn with_complexity(mut self, complexity: u8) -> Self {
        self.complexity = complexity.clamp(1, 10);
        self
    }

    /// Add a dependency
    pub fn depends_on(mut self, step_id: Uuid) -> Self {
        self.depends_on.push(step_id);
        self
    }

    /// Set notes
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    /// Mark as completed
    pub fn complete(&mut self) {
        self.completed = true;
    }
}

/// A complete plan for accomplishing a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// Unique ID for this plan
    pub id: Uuid,
    /// The original goal/task
    pub goal: String,
    /// Steps in the plan
    pub steps: Vec<PlanStep>,
    /// Estimated total complexity
    pub total_complexity: u32,
    /// Estimated token usage
    pub estimated_tokens: Option<u32>,
    /// Tools that will be needed
    pub tools_needed: Vec<String>,
    /// Whether the plan has been approved
    pub approved: bool,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Optional approval timestamp
    pub approved_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Plan {
    /// Create a new plan
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            goal: goal.into(),
            steps: Vec::new(),
            total_complexity: 0,
            estimated_tokens: None,
            tools_needed: Vec::new(),
            approved: false,
            created_at: chrono::Utc::now(),
            approved_at: None,
            metadata: HashMap::new(),
        }
    }

    /// Add a step to the plan
    pub fn add_step(&mut self, step: PlanStep) {
        self.total_complexity += step.complexity as u32;
        if let Some(ref tool) = step.tool {
            if !self.tools_needed.contains(tool) {
                self.tools_needed.push(tool.clone());
            }
        }
        self.steps.push(step);
    }

    /// Create a step and add it
    pub fn step(mut self, description: impl Into<String>) -> Self {
        let number = self.steps.len() + 1;
        let step = PlanStep::new(number, description);
        self.add_step(step);
        self
    }

    /// Set estimated tokens
    pub fn with_estimated_tokens(mut self, tokens: u32) -> Self {
        self.estimated_tokens = Some(tokens);
        self
    }

    /// Approve the plan
    pub fn approve(&mut self) {
        self.approved = true;
        self.approved_at = Some(chrono::Utc::now());
    }

    /// Reject the plan
    pub fn reject(&mut self) {
        self.approved = false;
        self.approved_at = None;
    }

    /// Get the next incomplete step
    pub fn next_step(&self) -> Option<&PlanStep> {
        self.steps.iter().find(|s| !s.completed)
    }

    /// Get the next incomplete step (mutable)
    pub fn next_step_mut(&mut self) -> Option<&mut PlanStep> {
        self.steps.iter_mut().find(|s| !s.completed)
    }

    /// Mark a step as completed by index
    pub fn complete_step(&mut self, index: usize) -> Result<()> {
        if index >= self.steps.len() {
            return Err(Error::config("Step index out of bounds"));
        }
        self.steps[index].complete();
        Ok(())
    }

    /// Check if all steps are completed
    pub fn is_complete(&self) -> bool {
        self.steps.iter().all(|s| s.completed)
    }

    /// Get completion percentage
    pub fn completion_percentage(&self) -> f32 {
        if self.steps.is_empty() {
            return 100.0;
        }
        let completed = self.steps.iter().filter(|s| s.completed).count();
        (completed as f32 / self.steps.len() as f32) * 100.0
    }

    /// Format as a checklist (Markdown)
    pub fn to_checklist(&self) -> String {
        let mut output = format!("# Plan: {}\n\n", self.goal);

        for step in &self.steps {
            let checkbox = if step.completed { "[x]" } else { "[ ]" };
            let tool_info = step
                .tool
                .as_ref()
                .map(|t| format!(" (tool: {})", t))
                .unwrap_or_default();

            output.push_str(&format!(
                "- {} Step {}: {}{}\n",
                checkbox, step.number, step.description, tool_info
            ));

            if let Some(notes) = &step.notes {
                output.push_str(&format!("  - Notes: {}\n", notes));
            }
        }

        output.push_str(&format!(
            "\nProgress: {:.0}% ({}/{})\n",
            self.completion_percentage(),
            self.steps.iter().filter(|s| s.completed).count(),
            self.steps.len()
        ));

        output
    }
}

/// Plan builder for fluent API
pub struct PlanBuilder {
    goal: String,
    steps: Vec<PlanStep>,
    estimated_tokens: Option<u32>,
}

impl PlanBuilder {
    /// Create a new plan builder
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            steps: Vec::new(),
            estimated_tokens: None,
        }
    }

    /// Add a step
    pub fn step(mut self, description: impl Into<String>) -> Self {
        let number = self.steps.len() + 1;
        self.steps.push(PlanStep::new(number, description));
        self
    }

    /// Add a step with a tool
    pub fn step_with_tool(
        mut self,
        description: impl Into<String>,
        tool: impl Into<String>,
    ) -> Self {
        let number = self.steps.len() + 1;
        self.steps
            .push(PlanStep::new(number, description).with_tool(tool));
        self
    }

    /// Set estimated tokens
    pub fn estimated_tokens(mut self, tokens: u32) -> Self {
        self.estimated_tokens = Some(tokens);
        self
    }

    /// Build the plan
    pub fn build(self) -> Plan {
        let mut plan = Plan::new(self.goal);
        for step in self.steps {
            plan.add_step(step);
        }
        if let Some(tokens) = self.estimated_tokens {
            plan.estimated_tokens = Some(tokens);
        }
        plan
    }
}

/// Planner configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerConfig {
    /// Maximum steps in a plan
    pub max_steps: usize,
    /// Whether to require approval before execution
    pub require_approval: bool,
    /// Auto-approve simple plans (complexity <= threshold)
    pub auto_approve_threshold: Option<u32>,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            max_steps: 20,
            require_approval: true,
            auto_approve_threshold: Some(10),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_creation() {
        let plan = Plan::new("Create a REST API")
            .step("Design the API endpoints")
            .step("Create the database schema")
            .step("Implement the handlers");

        assert_eq!(plan.steps.len(), 3);
        assert!(!plan.approved);
        assert_eq!(plan.completion_percentage(), 0.0);
    }

    #[test]
    fn test_plan_builder() {
        let plan = PlanBuilder::new("Build a website")
            .step("Create HTML structure")
            .step_with_tool("Style with CSS", "write_file")
            .step_with_tool("Add JavaScript", "write_file")
            .estimated_tokens(5000)
            .build();

        assert_eq!(plan.steps.len(), 3);
        assert!(plan.tools_needed.contains(&"write_file".to_string()));
        assert_eq!(plan.estimated_tokens, Some(5000));
    }

    #[test]
    fn test_plan_completion() {
        let mut plan = Plan::new("Test").step("Step 1").step("Step 2");

        assert_eq!(plan.completion_percentage(), 0.0);

        plan.complete_step(0).unwrap();
        assert_eq!(plan.completion_percentage(), 50.0);

        plan.complete_step(1).unwrap();
        assert_eq!(plan.completion_percentage(), 100.0);
        assert!(plan.is_complete());
    }

    #[test]
    fn test_plan_approval() {
        let mut plan = Plan::new("Test");
        assert!(!plan.approved);
        assert!(plan.approved_at.is_none());

        plan.approve();
        assert!(plan.approved);
        assert!(plan.approved_at.is_some());

        plan.reject();
        assert!(!plan.approved);
        assert!(plan.approved_at.is_none());
    }

    #[test]
    fn test_checklist_output() {
        let mut plan = Plan::new("Test Goal")
            .step("First step")
            .step("Second step");

        plan.complete_step(0).unwrap();

        let checklist = plan.to_checklist();
        assert!(checklist.contains("- [x] Step 1:"));
        assert!(checklist.contains("- [ ] Step 2:"));
        assert!(checklist.contains("50%"));
    }

    #[test]
    fn test_agent_mode() {
        assert_eq!(AgentMode::default(), AgentMode::Act);
        assert_eq!(AgentMode::Plan.to_string(), "plan");
        assert_eq!(AgentMode::Act.to_string(), "act");
    }
}
