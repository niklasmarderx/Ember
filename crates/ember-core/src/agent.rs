//! Agent runtime implementation with ReAct pattern.

use crate::{
    config::AgentConfig,
    context::Context,
    conversation::{Conversation, ConversationId},
    memory::Memory,
    Error, Result,
};
use ember_llm::{
    CompletionRequest, LLMProvider, Message, TokenUsage, ToolCall, ToolDefinition, ToolResult,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Current state of the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    /// Agent is idle, waiting for input
    Idle,
    /// Agent is thinking (processing user input)
    Thinking,
    /// Agent is executing a tool
    ExecutingTool,
    /// Agent is generating a response
    Generating,
    /// Agent is in an error state
    Error,
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Thinking => write!(f, "thinking"),
            Self::ExecutingTool => write!(f, "executing_tool"),
            Self::Generating => write!(f, "generating"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Tool executor function type.
///
/// Takes a tool call and returns a result. The executor is responsible for
/// parsing arguments and executing the tool's logic.
pub type ToolExecutor = Box<dyn Fn(&ToolCall) -> Result<ToolResult> + Send + Sync>;

/// The main agent struct that coordinates LLM interactions.
pub struct Agent {
    /// LLM provider
    provider: Arc<dyn LLMProvider>,

    /// Agent configuration
    config: AgentConfig,

    /// Current state
    state: RwLock<AgentState>,

    /// Current conversation
    conversation: RwLock<Option<Conversation>>,

    /// Context manager
    context: RwLock<Context>,

    /// Memory store
    memory: RwLock<Memory>,

    /// Registered tools
    tools: Vec<ToolDefinition>,

    /// Tool executors
    tool_executors: std::collections::HashMap<String, ToolExecutor>,
}

impl Agent {
    /// Create a new agent builder.
    pub fn builder() -> AgentBuilder {
        AgentBuilder::new()
    }

    /// Create a new agent with the given provider and config.
    pub fn new(provider: Arc<dyn LLMProvider>, config: AgentConfig) -> Result<Self> {
        config.validate()?;

        let context = Context::new(&config.system_prompt, config.max_context_tokens);

        Ok(Self {
            provider,
            config,
            state: RwLock::new(AgentState::Idle),
            conversation: RwLock::new(None),
            context: RwLock::new(context),
            memory: RwLock::new(Memory::new()),
            tools: Vec::new(),
            tool_executors: std::collections::HashMap::new(),
        })
    }

    /// Get the current state.
    pub async fn state(&self) -> AgentState {
        *self.state.read().await
    }

    /// Get the configuration.
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// Get the current conversation ID.
    pub async fn conversation_id(&self) -> Option<ConversationId> {
        self.conversation.read().await.as_ref().map(|c| c.id)
    }

    /// Start a new conversation.
    pub async fn new_conversation(&self) -> ConversationId {
        let conv = Conversation::new(&self.config.system_prompt);
        let id = conv.id;

        // Reset context
        let mut context = self.context.write().await;
        context.clear();
        context.set_system_prompt(&self.config.system_prompt);

        *self.conversation.write().await = Some(conv);

        info!(conversation_id = %id, "Started new conversation");
        id
    }

    /// Send a chat message and get a response.
    pub async fn chat(&self, message: impl Into<String>) -> Result<AgentResponse> {
        let message = message.into();

        // Ensure we have a conversation
        if self.conversation.read().await.is_none() {
            self.new_conversation().await;
        }

        // Set state to thinking
        self.set_state(AgentState::Thinking).await?;

        // Start a new turn
        {
            let mut conv = self.conversation.write().await;
            if let Some(ref mut c) = *conv {
                c.start_turn(&message);
            }
        }

        // Add user message to context
        self.context.write().await.add_user_message(&message);

        // Run the agent loop
        let response = self.run_agent_loop().await;

        // Set state back to idle
        let _ = self.set_state(AgentState::Idle).await;

        response
    }

    /// Run the main agent loop (ReAct pattern).
    async fn run_agent_loop(&self) -> Result<AgentResponse> {
        let mut iterations = 0;
        let max_iterations = self.config.max_iterations;

        loop {
            iterations += 1;
            if iterations > max_iterations {
                return Err(Error::LoopLimitExceeded { iterations });
            }

            debug!(
                iteration = iterations,
                max = max_iterations,
                "Agent loop iteration"
            );

            // Build the completion request
            let request = self.build_request().await?;

            // Set state to generating
            self.set_state(AgentState::Generating).await?;

            // Call the LLM
            let response = self.provider.complete(request).await.map_err(Error::from)?;

            // Process the response
            let (content, tool_calls) = self.process_response(&response).await?;

            // If there are tool calls, execute them
            if !tool_calls.is_empty() {
                self.set_state(AgentState::ExecutingTool).await?;

                for call in tool_calls {
                    let result = self.execute_tool(&call).await?;

                    // Add tool result to context and conversation
                    self.context.write().await.add_message(
                        Message::tool_result(&call.id, &result.output).with_name(&call.name),
                    );

                    // Update conversation
                    if let Some(ref mut conv) = *self.conversation.write().await {
                        if let Some(turn) = conv.current_turn_mut() {
                            turn.add_tool_call(call);
                            turn.add_tool_result(result);
                        }
                    }
                }

                // Continue the loop to get final response
                continue;
            }

            // No tool calls, we have the final response
            if let Some(content) = content {
                // Add assistant message to context
                self.context.write().await.add_assistant_message(&content);

                // Complete the turn
                {
                    let mut conv = self.conversation.write().await;
                    if let Some(ref mut c) = *conv {
                        if let Some(turn) = c.current_turn_mut() {
                            turn.assistant_response = content.clone();
                            turn.tokens_used = Some(crate::conversation::TokenUsage::new(
                                response.usage.prompt_tokens,
                                response.usage.completion_tokens,
                            ));
                            turn.complete();
                        }
                    }
                }

                return Ok(AgentResponse {
                    content,
                    tool_calls_made: iterations > 1,
                    iterations,
                    usage: Some(response.usage),
                });
            }

            // Empty response
            warn!("Received empty response from LLM");
            return Err(Error::config("Received empty response from LLM"));
        }
    }

    /// Build a completion request from current context.
    async fn build_request(&self) -> Result<CompletionRequest> {
        let context = self.context.read().await;
        let messages = context.messages();

        let mut request =
            CompletionRequest::from_messages(messages).with_temperature(self.config.temperature);

        // Add tools if enabled
        if self.config.tools_enabled && !self.tools.is_empty() {
            request = request.with_tools(self.tools.clone());
        }

        Ok(request)
    }

    /// Process LLM response and extract content/tool calls.
    async fn process_response(
        &self,
        response: &ember_llm::CompletionResponse,
    ) -> Result<(Option<String>, Vec<ToolCall>)> {
        let content = if response.content.is_empty() {
            None
        } else {
            Some(response.content.clone())
        };

        let tool_calls = response.tool_calls.clone();

        Ok((content, tool_calls))
    }

    /// Execute a tool call.
    async fn execute_tool(&self, call: &ToolCall) -> Result<ToolResult> {
        let tool_name = &call.name;

        debug!(tool = %tool_name, "Executing tool");

        if let Some(executor) = self.tool_executors.get(tool_name) {
            executor(call)
        } else {
            Err(Error::tool_execution(
                tool_name,
                format!("Tool '{}' not found", tool_name),
            ))
        }
    }

    /// Set the agent state with validation.
    async fn set_state(&self, new_state: AgentState) -> Result<()> {
        let current = *self.state.read().await;

        // Validate state transition
        let valid = matches!(
            (current, new_state),
            (AgentState::Idle, AgentState::Thinking)
                | (AgentState::Thinking, AgentState::Generating)
                | (AgentState::Thinking, AgentState::Error)
                | (AgentState::Generating, AgentState::ExecutingTool)
                | (AgentState::Generating, AgentState::Idle)
                | (AgentState::ExecutingTool, AgentState::Generating)
                | (AgentState::ExecutingTool, AgentState::Thinking)
                | (AgentState::ExecutingTool, AgentState::Error)
                | (AgentState::Error, AgentState::Idle)
                | (_, AgentState::Idle) // Always allow transition to Idle
        );

        if !valid {
            return Err(Error::InvalidStateTransition {
                from: current.to_string(),
                to: new_state.to_string(),
            });
        }

        *self.state.write().await = new_state;
        debug!(from = %current, to = %new_state, "State transition");

        Ok(())
    }

    /// Register a tool with the agent.
    pub fn register_tool(&mut self, tool: ToolDefinition, executor: ToolExecutor) {
        let name = tool.name.clone();
        self.tools.push(tool);
        self.tool_executors.insert(name, executor);
    }

    /// Get the memory store.
    pub async fn memory(&self) -> tokio::sync::RwLockReadGuard<'_, Memory> {
        self.memory.read().await
    }

    /// Get mutable access to the memory store.
    pub async fn memory_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, Memory> {
        self.memory.write().await
    }
}

/// Response from an agent chat.
#[derive(Debug)]
pub struct AgentResponse {
    /// The response content
    pub content: String,

    /// Whether tool calls were made
    pub tool_calls_made: bool,

    /// Number of iterations in the agent loop
    pub iterations: usize,

    /// Token usage statistics
    pub usage: Option<TokenUsage>,
}

/// Builder for creating an Agent.
pub struct AgentBuilder {
    provider: Option<Arc<dyn LLMProvider>>,
    config: AgentConfig,
    tools: Vec<(ToolDefinition, ToolExecutor)>,
}

impl AgentBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            provider: None,
            config: AgentConfig::default(),
            tools: Vec::new(),
        }
    }

    /// Set the LLM provider.
    pub fn provider(mut self, provider: impl LLMProvider + 'static) -> Self {
        self.provider = Some(Arc::new(provider));
        self
    }

    /// Set the LLM provider from an Arc.
    pub fn provider_arc(mut self, provider: Arc<dyn LLMProvider>) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Set the configuration.
    pub fn config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    /// Set the system prompt.
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.config.system_prompt = prompt.into();
        self
    }

    /// Set the temperature.
    pub fn temperature(mut self, temp: f32) -> Self {
        self.config.temperature = temp;
        self
    }

    /// Set max iterations.
    pub fn max_iterations(mut self, max: usize) -> Self {
        self.config.max_iterations = max;
        self
    }

    /// Enable or disable streaming.
    pub fn streaming(mut self, enabled: bool) -> Self {
        self.config.streaming = enabled;
        self
    }

    /// Add a tool.
    pub fn tool(mut self, tool: ToolDefinition, executor: ToolExecutor) -> Self {
        self.tools.push((tool, executor));
        self
    }

    /// Build the agent.
    pub fn build(self) -> Result<Agent> {
        let provider = self
            .provider
            .ok_or_else(|| Error::config("LLM provider is required"))?;

        let mut agent = Agent::new(provider, self.config)?;

        for (tool, executor) in self.tools {
            agent.register_tool(tool, executor);
        }

        Ok(agent)
    }
}

impl Default for AgentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests would require mocking the LLM provider

    #[test]
    fn test_agent_state_display() {
        assert_eq!(AgentState::Idle.to_string(), "idle");
        assert_eq!(AgentState::Thinking.to_string(), "thinking");
        assert_eq!(AgentState::ExecutingTool.to_string(), "executing_tool");
    }

    #[test]
    fn test_builder_requires_provider() {
        let result = Agent::builder().system_prompt("Test").build();

        assert!(result.is_err());
    }
}
