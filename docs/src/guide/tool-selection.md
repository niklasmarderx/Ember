# Intelligent Tool Selection

Ember includes an intelligent tool selector that automatically recommends the best tools for a given task using semantic matching and capability scoring.

## Overview

The Tool Selector analyzes user requests and context to recommend the most appropriate tools. It considers:

- Semantic similarity between the request and tool descriptions
- Tool capabilities and their relevance to the task
- Historical success rates and performance data
- Current context and constraints

## Basic Usage

```rust
use ember_core::tool_selector::{ToolSelector, ToolSelectorConfig, SelectionContext};

// Create a tool selector with default config
let selector = ToolSelector::new(ToolSelectorConfig::default());

// Register tools
selector.register_tool(shell_tool)?;
selector.register_tool(filesystem_tool)?;
selector.register_tool(web_tool)?;

// Create selection context
let context = SelectionContext {
    user_request: "List all files in the current directory".to_string(),
    conversation_history: vec![],
    available_context: serde_json::json!({
        "working_directory": "/home/user/project"
    }),
    constraints: vec![],
};

// Get tool recommendations
let recommendations = selector.recommend(&context)?;

for rec in recommendations {
    println!("Tool: {} (score: {:.2})", rec.tool_name, rec.score);
    println!("Reason: {}", rec.reason);
}
```

## Configuration

```rust
use ember_core::tool_selector::ToolSelectorConfig;

let config = ToolSelectorConfig {
    // Minimum score threshold for recommendations
    min_score_threshold: 0.3,
    
    // Maximum number of tools to recommend
    max_recommendations: 5,
    
    // Weight for semantic similarity (0.0 - 1.0)
    semantic_weight: 0.4,
    
    // Weight for capability matching (0.0 - 1.0)
    capability_weight: 0.3,
    
    // Weight for historical performance (0.0 - 1.0)
    history_weight: 0.2,
    
    // Weight for context relevance (0.0 - 1.0)
    context_weight: 0.1,
    
    // Enable learning from execution results
    enable_learning: true,
};

let selector = ToolSelector::new(config);
```

## Registering Tools

Tools must provide metadata for the selector to work effectively:

```rust
use ember_core::tool_selector::{ToolMetadata, ToolCapability};

let metadata = ToolMetadata {
    name: "shell".to_string(),
    description: "Execute shell commands on the system".to_string(),
    
    // Keywords help with matching
    keywords: vec![
        "command".to_string(),
        "terminal".to_string(),
        "execute".to_string(),
        "run".to_string(),
        "bash".to_string(),
    ],
    
    // Capabilities define what the tool can do
    capabilities: vec![
        ToolCapability {
            name: "execute_command".to_string(),
            description: "Run arbitrary shell commands".to_string(),
            parameters: vec!["command".to_string()],
        },
        ToolCapability {
            name: "pipe_commands".to_string(),
            description: "Chain multiple commands with pipes".to_string(),
            parameters: vec!["commands".to_string()],
        },
    ],
    
    // Example use cases
    examples: vec![
        "List files: ls -la".to_string(),
        "Find text: grep -r 'pattern' .".to_string(),
        "Install package: npm install express".to_string(),
    ],
    
    // Constraints and limitations
    constraints: vec![
        "Requires shell access".to_string(),
        "May have permission restrictions".to_string(),
    ],
};

selector.register_tool_with_metadata(shell_tool, metadata)?;
```

## Selection Context

Provide rich context for better recommendations:

```rust
use ember_core::tool_selector::{SelectionContext, Constraint};

let context = SelectionContext {
    // The user's current request
    user_request: "Download the image and resize it to 800x600".to_string(),
    
    // Recent conversation for context
    conversation_history: vec![
        "User: I need to process some images".to_string(),
        "Assistant: I can help with that. What would you like to do?".to_string(),
    ],
    
    // Current environment context
    available_context: serde_json::json!({
        "working_directory": "/home/user/images",
        "available_memory": "8GB",
        "network_available": true,
    }),
    
    // Any constraints to consider
    constraints: vec![
        Constraint::MustHaveCapability("image_processing".to_string()),
        Constraint::PreferLocal,
        Constraint::MaxExecutionTime(30),
    ],
};
```

## Tool Recommendations

Recommendations include detailed information:

```rust
pub struct ToolRecommendation {
    // Name of the recommended tool
    pub tool_name: String,
    
    // Overall recommendation score (0.0 - 1.0)
    pub score: f64,
    
    // Human-readable explanation
    pub reason: String,
    
    // Breakdown of scoring components
    pub score_breakdown: ScoreBreakdown,
    
    // Suggested parameters for this use case
    pub suggested_params: Option<serde_json::Value>,
    
    // Confidence level
    pub confidence: Confidence,
}

pub struct ScoreBreakdown {
    pub semantic_score: f64,
    pub capability_score: f64,
    pub history_score: f64,
    pub context_score: f64,
}

pub enum Confidence {
    High,    // > 0.8
    Medium,  // 0.5 - 0.8
    Low,     // < 0.5
}
```

## Learning from Results

The selector can learn from execution results to improve future recommendations:

```rust
use ember_core::tool_selector::ExecutionFeedback;

// After tool execution, provide feedback
let feedback = ExecutionFeedback {
    tool_name: "web".to_string(),
    request_context: context.clone(),
    success: true,
    execution_time: Duration::from_secs(2),
    error_message: None,
    user_satisfaction: Some(5), // 1-5 scale
};

selector.record_feedback(feedback)?;

// The selector will use this to adjust future scores
```

## Multi-Tool Workflows

For complex tasks, the selector can recommend tool sequences:

```rust
// Get a workflow recommendation
let workflow = selector.recommend_workflow(&context)?;

println!("Recommended workflow:");
for (i, step) in workflow.steps.iter().enumerate() {
    println!("{}. {} - {}", i + 1, step.tool_name, step.description);
    if let Some(deps) = &step.depends_on {
        println!("   Depends on: {:?}", deps);
    }
}

// Example output:
// 1. web - Download the image from URL
// 2. image - Resize the downloaded image
//    Depends on: ["web"]
```

## Custom Scoring Functions

Add custom scoring logic for specific use cases:

```rust
use ember_core::tool_selector::CustomScorer;

struct DomainSpecificScorer {
    preferred_tools: Vec<String>,
}

impl CustomScorer for DomainSpecificScorer {
    fn score(&self, tool_name: &str, context: &SelectionContext) -> f64 {
        if self.preferred_tools.contains(&tool_name.to_string()) {
            0.2 // Bonus for preferred tools
        } else {
            0.0
        }
    }
}

selector.add_custom_scorer(Box::new(DomainSpecificScorer {
    preferred_tools: vec!["shell".to_string(), "filesystem".to_string()],
}))?;
```

## Integration with Agent

The tool selector integrates seamlessly with the Agent:

```rust
use ember_core::{Agent, AgentConfig};

let config = AgentConfig {
    // Enable automatic tool selection
    auto_select_tools: true,
    
    // Tool selection configuration
    tool_selection: ToolSelectorConfig::default(),
    
    // ... other config
    ..Default::default()
};

let agent = Agent::new(config)?;

// The agent will automatically select appropriate tools
let response = agent.chat("Create a new file called test.txt").await?;
```

## Configuration File

Configure tool selection in `ember.toml`:

```toml
[tool_selection]
enabled = true
min_score_threshold = 0.3
max_recommendations = 5

[tool_selection.weights]
semantic = 0.4
capability = 0.3
history = 0.2
context = 0.1

[tool_selection.learning]
enabled = true
history_size = 1000
decay_factor = 0.95
```

## Best Practices

1. **Provide Rich Metadata**: The more information tools provide, the better the selection
2. **Use Keywords Wisely**: Include synonyms and related terms
3. **Record Feedback**: Enable learning for continuous improvement
4. **Set Appropriate Thresholds**: Balance between precision and recall
5. **Consider Context**: Provide as much context as possible for better matches

## See Also

- [Context Management](./context-management.md)
- [Custom Tools](./custom-tools.md)
- [Agent Mode](./agent-mode.md)