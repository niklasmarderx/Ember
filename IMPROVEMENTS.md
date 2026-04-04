# 🔥 Ember Improvement Plan — Towards a World-Class AI Assistant

> Analysis based on full codebase review (April 2026)

## Executive Summary

Ember already has an impressive foundation: multi-provider LLM support (10+ providers), a rich tool system, session persistence, MCP integration, a learning system, and more. However, to become a **truly strong AI assistant** comparable to Claude Code, Cursor Agent, or Cline, several critical areas need deepening. This document identifies **13 high-impact improvement areas** with concrete implementation plans.

---

## 🏗️ Architecture Overview (Current)

```
ember-cli ──→ ember-core (agent loop, context, conversation)
           ──→ ember-llm  (providers, routing, streaming)
           ──→ ember-tools (filesystem, shell, git, web, code exec, db, image)
           ──→ ember-mcp   (Model Context Protocol)
           ──→ ember-learn (profile, preferences, patterns)
           ──→ ember-storage (SQLite persistence)
           ──→ ember-code  (analyzer, refactor, testgen)
           ──→ ember-browser (headless browser)
           ──→ ember-plugins (plugin registry)
           ──→ ember-voice, ember-i18n, ember-enterprise, ember-telemetry
```

---

## 1. ✅ 🧠 Smarter Agentic Loop (CRITICAL) — IMPLEMENTED

### Problem
The current agent loop (`agent_interactive`/`agent_one_shot` in `chat.rs`) is a simple request→tool→response cycle with a 50-iteration cap. It lacks:
- **Self-reflection** — the agent doesn't evaluate whether its approach is working
- **Planning before acting** — no structured decomposition of complex tasks
- **Error recovery strategies** — retries the same approach on failure
- **Progress tracking** — no awareness of how far through a task it is

### Solution

#### 1a. Introduce a `ThinkingStep` before tool calls
```rust
pub enum AgentStep {
    Think(ThinkingResult),      // Internal reasoning (not shown to user)
    Plan(Vec<PlanStep>),        // Task decomposition
    Act(ToolCall),              // Tool execution
    Observe(ToolResult),        // Process tool output
    Reflect(ReflectionResult),  // Evaluate progress
    Respond(String),            // Final answer to user
}
```

#### 1b. Implement ReAct-style loop with reflection
```
loop {
  1. THINK: Analyze current state, what do I know, what do I need?
  2. PLAN:  If complex task, decompose into steps
  3. ACT:   Execute next tool call
  4. OBSERVE: Process result
  5. REFLECT: Did this work? Should I change approach?
  6. If done → RESPOND, else → back to THINK
}
```

#### 1c. Automatic strategy switching on failure
- After 2 failed attempts with same tool → try alternative approach
- After file edit produces errors → read file again, re-analyze
- After shell command fails → analyze error, try different command
- Track "confidence score" that decreases with failures

### Files to modify
- `crates/ember-core/src/agent.rs` — add `AgentStep` enum, reflection logic
- `crates/ember-cli/src/commands/chat.rs` — integrate new loop into `agent_interactive`

---

## 2. 📋 Deep System Prompt Engineering (CRITICAL)

### Problem
The system prompt in `chat.rs` (`build_working_directory_context`) provides only a file listing. There's no behavioral instruction, no tool usage guidance, no coding standards. The LLM doesn't know *how* to be a great assistant.

### Solution

#### 2a. Rich behavioral system prompt
Create a `SystemPromptBuilder` that assembles context-aware system prompts:

```rust
pub struct SystemPromptBuilder {
    role_description: String,          // "You are Ember, an expert AI assistant..."
    behavioral_rules: Vec<String>,     // Rules for how to behave
    tool_guidelines: Vec<ToolGuideline>, // How to use each tool effectively
    project_context: ProjectContext,   // Language, framework, conventions
    user_preferences: UserPrefs,       // From learning system
    active_constraints: Vec<String>,   // Budget limits, safety rules
}
```

#### 2b. Context-aware instructions
- **Coding tasks**: Include project language, framework, test conventions, linting rules
- **File editing**: "Always read a file before editing. Use targeted edits, not full rewrites. Verify changes compile."
- **Shell commands**: "Prefer non-interactive commands. Check exit codes. Use `--no-pager` flags."
- **Git operations**: "Always check status before commits. Write conventional commit messages."

#### 2c. Project detection & adaptation
Auto-detect from `Cargo.toml`, `package.json`, `.gitignore`, etc.:
- Programming language(s) and frameworks
- Build system and test runner
- Code style (from `.editorconfig`, `rustfmt.toml`, `.prettierrc`)
- CI/CD configuration

### New files
- `crates/ember-core/src/system_prompt.rs` — SystemPromptBuilder
- `crates/ember-core/src/project_detector.rs` — project analysis

---

## 3. ✅ 🪟 Intelligent Context Window Management (CRITICAL) — IMPLEMENTED

### Problem
No automatic context window management. Long conversations will hit token limits and fail. The `compaction.rs` exists but isn't integrated into the main chat loop.

### Solution

#### 3a. Token budget tracking
```rust
pub struct ContextBudget {
    max_tokens: usize,           // Model's context window
    system_prompt_tokens: usize, // Reserved for system prompt
    tool_results_tokens: usize,  // Reserved for pending tool results
    response_tokens: usize,      // Reserved for model response
    available_tokens: usize,     // What's left for conversation history
}
```

#### 3b. Automatic conversation compaction
When approaching token limit:
1. **Summarize old messages** — Use a cheap/fast model to summarize earlier turns
2. **Drop tool results** — Replace verbose tool outputs with summaries
3. **Preserve recent context** — Keep last N turns verbatim
4. **Keep key decisions** — Mark important messages as "pinned"

#### 3c. Sliding window with smart truncation
```rust
pub fn compact_history(
    messages: &[Message],
    budget: &ContextBudget,
    provider: &dyn LLMProvider,
) -> Vec<Message> {
    // 1. Always keep system message
    // 2. Keep last 6-8 turns verbatim
    // 3. Summarize older turns into a single "conversation so far" message
    // 4. Truncate large tool outputs (keep first/last 50 lines)
}
```

### Files to modify
- `crates/ember-core/src/compaction.rs` — enhance with LLM-based summarization
- `crates/ember-core/src/context_manager.rs` — add token budget tracking
- `crates/ember-cli/src/commands/chat.rs` — integrate into main loop

---

## 4. 📂 Deep Project Understanding (HIGH)

### Problem
`auto_context.rs` exists but only provides basic file listings. The assistant doesn't understand the codebase structure, dependencies, or architecture.

### Solution

#### 4a. Project indexing on first run
```rust
pub struct ProjectIndex {
    languages: Vec<Language>,
    frameworks: Vec<Framework>,
    entry_points: Vec<PathBuf>,
    dependency_graph: HashMap<String, Vec<String>>,
    module_map: HashMap<PathBuf, ModuleInfo>,
    test_files: Vec<PathBuf>,
    config_files: Vec<PathBuf>,
    build_commands: BuildConfig,
    git_info: GitInfo,
}
```

#### 4b. Smart file relevance scoring
When the user asks a question, automatically identify which files are most relevant:
- Parse imports/dependencies to find related files
- Use file path patterns (test files for source files, etc.)
- Track which files were recently edited
- Use semantic similarity if embeddings available

#### 4c. Codebase summary generation
Generate and cache a "project overview" that includes:
- Architecture description
- Key modules and their purposes
- Coding conventions detected
- Build/test/deploy commands

### New files
- `crates/ember-code/src/project_index.rs`
- `crates/ember-code/src/relevance.rs`

---

## 5. ✅ ✏️ Precise File Editing (HIGH) — IMPLEMENTED

### Problem
The filesystem tool writes entire files. For large files, this is error-prone and wasteful. The `patch.rs` system exists in ember-tools but isn't the primary edit method.

### Solution

#### 5a. SEARCH/REPLACE as primary edit mode
Implement a `file_edit` tool that uses targeted search/replace:
```rust
pub struct FileEditTool {
    /// Apply one or more search/replace blocks to a file
    pub async fn apply_edits(&self, path: &str, edits: Vec<SearchReplace>) -> Result<ToolOutput>;
}

pub struct SearchReplace {
    pub search: String,   // Exact content to find
    pub replace: String,  // Content to replace with
}
```

#### 5b. Diff preview before apply
- Show unified diff to user before applying
- Track all edits for undo capability
- Validate that search content exists in file (error on no match)

#### 5c. Smart edit validation
After each file edit:
- If Rust: run `cargo check` on the file
- If TypeScript: run `tsc --noEmit`
- If Python: run `python -m py_compile`
- Report errors back to agent for self-correction

### Files to modify
- `crates/ember-tools/src/filesystem.rs` — add `edit_file` operation
- `crates/ember-tools/src/patch.rs` — integrate search/replace

---

## 6. ✅ 🔒 Smarter Safety & Auto-Approval (HIGH) — IMPLEMENTED

### Problem
Auto-approve is a binary on/off (`AtomicBool`). No risk assessment. Either everything needs confirmation or nothing does.

### Solution

#### 6a. Tiered risk assessment
```rust
pub enum RiskLevel {
    Safe,       // Read operations, ls, cat, git status
    Low,        // Write to project files, git add
    Medium,     // Shell commands, git commit
    High,       // Delete files, git push, network requests
    Critical,   // System commands, rm -rf, credential access
}

pub struct AutoApprovePolicy {
    auto_approve_up_to: RiskLevel,  // Auto-approve Safe + Low
    require_confirm: Vec<String>,   // Always confirm these tools
    never_allow: Vec<String>,       // Block these entirely
}
```

#### 6b. Tool-specific risk classification
- `filesystem.read` → Safe
- `filesystem.write` (project files) → Low
- `filesystem.write` (outside project) → High
- `filesystem.delete` → High
- `shell` (read-only: ls, cat, grep) → Safe
- `shell` (build: cargo, npm) → Low
- `shell` (destructive: rm, mv) → High
- `git.status/diff/log` → Safe
- `git.commit` → Medium
- `git.push` → High

#### 6c. Smart command analysis
Parse shell commands to assess risk before execution:
```rust
fn assess_shell_risk(command: &str) -> RiskLevel {
    // Parse command, check for pipes, redirects, destructive operations
    // Use allowlist of safe commands
}
```

### Files to modify
- `crates/ember-tools/src/shell.rs` — add risk assessment
- `crates/ember-cli/src/commands/chat.rs` — replace binary auto_approve

---

## 7. 💰 Smart Model Routing & Cost Control (MEDIUM)

### Problem
The router and scorer exist in `ember-llm` but aren't integrated into the chat flow. Users manually pick models. No automatic cost optimization.

### Solution

#### 7a. Integrate CascadeRouter into chat
- Simple questions → use cheap/fast model (Haiku, GPT-4o-mini, Groq)
- Complex coding → use powerful model (Sonnet, GPT-4o, DeepSeek)
- Use `TaskAnalyzer` + `ModelScorer` from ember-llm

#### 7b. Budget tracking with limits
```rust
pub struct BudgetTracker {
    session_budget: f64,      // Max spend per session
    daily_budget: f64,        // Max spend per day
    total_spent: f64,         // Running total
    model_costs: HashMap<String, f64>,  // Per-model tracking
}
```

When approaching budget:
1. Warn user
2. Automatically switch to cheaper model
3. Hard-stop if budget exceeded

#### 7c. Token usage display
Show after each response:
```
📊 Tokens: 1,234 in / 567 out | Cost: $0.003 | Session: $0.12 / $5.00
```

### Files to modify
- `crates/ember-cli/src/commands/chat.rs` — integrate router, add budget display
- `crates/ember-llm/src/router.rs` — connect to chat flow

---

## 8. 🔄 Checkpoint & Undo System (MEDIUM)

### Problem
`checkpoint.rs` exists in ember-core but integration with the chat flow is unclear. Users can't easily undo multi-file changes.

### Solution

#### 8a. Automatic checkpoints before destructive operations
- Before any file write → snapshot affected files
- Before git operations → record HEAD
- Before shell commands that modify state → checkpoint

#### 8b. Named checkpoints at conversation milestones
```
/checkpoint "before refactoring auth module"
/undo                    # Undo last tool operation
/restore "before..."     # Restore named checkpoint
/diff checkpoint_id      # Show what changed since checkpoint
```

#### 8c. Git-based checkpoints for large changes
For multi-file operations, use git stash or temporary branches as checkpoints.

### Files to modify
- `crates/ember-core/src/checkpoint.rs` — enhance
- `crates/ember-cli/src/commands/chat.rs` — integrate checkpoint creation

---

## 9. 🧩 Learning System Integration (MEDIUM)

### Problem
The learning system (`ember-learn`) is well-designed with profiles, preferences, patterns, and suggestions — but it appears disconnected from the main chat loop.

### Solution

#### 9a. Feed events from chat into learning system
```rust
// In chat loop, after each interaction:
learning_system.record_event(LearningEvent::new(
    EventType::ModelUsed,
    context,
    json!({ "model": model_name, "task_type": "coding", "satisfaction": rating })
));
```

#### 9b. Use suggestions in system prompt
Before each LLM call, query the learning system:
```rust
let suggestions = learning_system.get_suggestions(&current_context);
// Incorporate into system prompt:
// "Based on your preferences: use 4-space indentation, prefer async/await..."
```

#### 9c. Adaptive behavior
- Learn preferred verbosity level
- Learn which tools the user prefers
- Learn project-specific patterns (naming conventions, file organization)
- Adjust system prompt temperature based on task type history

### Files to modify
- `crates/ember-cli/src/commands/chat.rs` — integrate learning events
- `crates/ember-core/src/system_prompt.rs` — use learning data

---

## 10. 🌐 Enhanced MCP Integration (MEDIUM)

### Problem
MCP support exists but could be deeper — auto-discovery of MCP servers, better tool integration.

### Solution

#### 10a. MCP server auto-discovery
- Scan `~/.ember/mcp/` and project `.ember/mcp.json` for server configs
- Auto-connect to configured servers on startup
- List available MCP tools alongside built-in tools

#### 10b. MCP resource as context
- Automatically include relevant MCP resources in context
- Use MCP prompts when available

### Files to modify
- `crates/ember-mcp/src/` — enhance discovery and integration

---

## 11. 📊 Better TUI & Streaming UX (MEDIUM)

### Problem
The TUI renderer exists but the main chat experience could be more polished — better progress indicators, syntax-highlighted code output, inline diffs.

### Solution

#### 11a. Rich streaming output
- Syntax-highlight code blocks as they stream in
- Show tool execution progress with elapsed time
- Display file diffs with colors (green/red for add/remove)

#### 11b. Status bar
Persistent status bar showing:
```
🔥 Ember | Model: claude-sonnet-4-20250514 | Session: abc123 | Cost: $0.12 | Tools: 5 available
```

#### 11c. Multi-panel support (future)
- Split view: chat + file preview
- Tool execution log panel
- Project tree panel

### Files to modify
- `crates/ember-cli/src/tui/renderer.rs` — enhance rendering
- `crates/ember-cli/src/commands/chat.rs` — better streaming display

---

## 12. 🧪 Test-Driven Development Support (LOW-MEDIUM)

### Problem
`ember-code` has testgen capability but it's not integrated into the agent workflow.

### Solution

#### 12a. Auto-test after code changes
After the agent modifies code:
1. Detect test runner (`cargo test`, `npm test`, `pytest`)
2. Run relevant tests
3. If tests fail → feed errors back to agent for fixing
4. Loop until tests pass or max retries

#### 12b. Test generation suggestions
When the agent creates new functions, suggest generating tests:
```
💡 Created `fn calculate_total()` — would you like me to generate tests?
```

### Files to modify
- `crates/ember-code/src/testgen.rs` — enhance
- `crates/ember-cli/src/commands/chat.rs` — integrate test-run-fix loop

---

## 13. 📖 Conversation Forking & Branching (LOW)

### Problem
Slash command `/fork` exists but conversation branching could be more powerful.

### Solution

#### 13a. Named conversation branches
```
/fork "try-approach-A"     # Branch current conversation
/fork "try-approach-B"     # Another branch from same point
/branches                  # List all branches
/switch "try-approach-A"   # Switch to branch
/merge "try-approach-A"    # Merge branch back
```

#### 13b. A/B testing approaches
Run the same prompt against two different models/approaches and compare results.

---

## Implementation Priority

| Priority | Area | Impact | Effort |
|----------|------|--------|--------|
| 🔴 P0 | 1. Smarter Agentic Loop | Critical | Large |
| 🔴 P0 | 2. System Prompt Engineering | Critical | Medium |
| 🔴 P0 | 3. Context Window Management | Critical | Medium |
| 🟠 P1 | 4. Deep Project Understanding | High | Large |
| 🟠 P1 | 5. Precise File Editing | High | Medium |
| 🟠 P1 | 6. Smarter Safety & Auto-Approval | High | Medium |
| 🟡 P2 | 7. Smart Model Routing & Cost | Medium | Medium |
| 🟡 P2 | 8. Checkpoint & Undo | Medium | Small |
| 🟡 P2 | 9. Learning System Integration | Medium | Medium |
| 🟡 P2 | 10. Enhanced MCP | Medium | Medium |
| 🟢 P3 | 11. Better TUI & Streaming UX | Medium | Medium |
| 🟢 P3 | 12. Test-Driven Dev Support | Low-Med | Medium |
| 🟢 P3 | 13. Conversation Forking | Low | Small |

---

## Recommended First Sprint (2 weeks)

1. **System Prompt Engineering** (P0, Medium effort) — Biggest bang for buck. A well-crafted system prompt alone will make Ember dramatically more capable.

2. **Context Window Management** (P0, Medium effort) — Without this, long conversations break. Essential for real-world use.

3. **Precise File Editing** (P1, Medium effort) — Search/replace editing is far more reliable than full-file rewrites.

4. **Auto-Approval Risk Tiers** (P1, Medium effort) — Makes the agent much more usable without constant confirmations.

---

## Success Metrics

- **Task completion rate**: % of coding tasks completed without user intervention
- **Edit accuracy**: % of file edits that don't introduce errors
- **Cost efficiency**: Average cost per task across model tiers
- **User satisfaction**: From learning system feedback tracking
- **Context utilization**: % of context window used effectively (not wasted on verbose tool outputs)

---

## ✅ Implementation Log

### April 4, 2026 — System Prompt Engineering

**Implemented:** `SystemPromptBuilder` in `ember-core/src/system_prompt.rs`

What was added:
- **`ProjectKind` enum** — auto-detects 13 project types (Rust, TypeScript, Python, Go, Java, C#, Ruby, Elixir, Swift, Kotlin, C++) from filesystem markers
- **`RiskTier` enum** — classifies tools as Safe/Moderate/Dangerous for approval decisions
- **`SystemPromptBuilder`** — builds structured, Claude-Code-quality system prompts with sections for:
  - Identity & behavioural rules
  - Tool descriptions with risk annotations
  - File-editing protocol (SEARCH/REPLACE)
  - Safety & approval rules (auto-approve aware)
  - Language-specific coding conventions (Rust, TS/JS, Python, Go)
  - Environment info (OS, shell, CWD, project kind)
  - Extra context injection (auto-context, user profile, learned memory, custom instructions)
- **`detect_project_kind()`** — filesystem-based project detection with glob support
- **`classify_tool_risk()`** — maps tool names to risk tiers
- **14 unit tests** — all passing, including tempdir-based filesystem detection tests

**Integrated into `chat.rs`:**
- Replaced the flat `prompt_parts.join("---")` system prompt assembly with `SystemPromptBuilder`
- Auto-context, user profile, learned memory, and custom instructions injected as labelled context sections
- Project kind detected from CWD and displayed in context tags
- Active tool names resolved from config/CLI flags and passed to builder

---

*Generated from full codebase analysis — April 2026*
