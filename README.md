<div align="center">

<img src="assets/logo.svg" alt="Ember Logo" width="128" height="128"/>

# 🔥 Ember

> The open-source, provider-agnostic AI coding agent. Built in Rust.

[![CI](https://github.com/niklasmarderx/Ember/actions/workflows/ci.yml/badge.svg)](https://github.com/niklasmarderx/Ember/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/ember-cli)](https://crates.io/crates/ember-cli)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
[![Rust 1.75+](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org)

</div>

---

Ember is a command-line AI coding agent written in Rust. It runs an agentic loop — plan, use tools, observe results, repeat — against any LLM backend you configure. One binary, no Python runtime, no Node.js.

---

## Why Ember?

- **Provider-agnostic** — 10 LLM backends out of the box. Switch models mid-session with `/model`. Bring your own OpenAI-compatible endpoint.
- **`/compare` — A/B test providers** — Send the same prompt to two providers side-by-side. Compare quality and cost. Pick the winner. **No other CLI tool can do this.**
- **Smart Model Cascade** — `--model auto` analyzes prompt complexity and routes simple questions to fast/cheap models, complex tasks to powerful ones. Save money without losing quality.
- **EMBER.md project context** — Drop an `EMBER.md` in your project root and Ember loads it as system context. Like Claude Code's `CLAUDE.md`.
- **`/undo`** — Revert the last file change made by a tool. Every write is snapshotted.
- **Git-native** — `/commit`, `/diff` right from the REPL. Auto-commit after tool runs.
- **Session forking** — branch a conversation like a git branch. Explore an alternative approach, then restore to the fork point if it doesn't work out.
- **Voice mode** *(preview)* — `ember voice` for hands-free coding with speech-to-text and TTS.
- **RAG indexing** *(preview)* — `ember index .` to embed your codebase for semantic search.
- **Multi-agent orchestration** *(preview)* — `ember agents run "task" --roles coder,reviewer` for parallel specialized agents.
- **Plugin hooks** — intercept any tool call before or after execution. Approve, deny, log, or transform output from your own code.
- **Auto-compaction** — when the context window fills up, the oldest turns are summarised in-place. The session continues without interruption.
- **Cost tracking** — every API call records token counts and a USD estimate. `/cost` shows the running total for the session.
- **Granular permissions** — restrict what paths a tool may read or write, which commands it may run, and whether writes are allowed at all.
- **MCP support** — connect external tool servers over stdio, HTTP, or WebSocket. Tools are namespaced and auto-discovered.
- **Plan Mode** — `/plan` toggles read-only mode where Ember proposes changes without executing. `/execute` runs the plan. Like Gemini CLI's Plan Mode, but better.
- **`.ember/rules/` directory** — Modular rule files instead of one giant EMBER.md. Organize by concern: `style.md`, `testing.md`, `security.md`. Auto-merged at load.
- **`/checkpoint` + `/replay`** — Save conversation checkpoints and replay sessions as tutorials. Great for onboarding and code review.
- **`ember bench`** — Built-in benchmarking across providers. Compare quality, latency, and cost in one command. No other tool has this.
- **`ember learn`** — Tracks your coding preferences and patterns over time. Personalized AI that gets better the more you use it.
- **Semantic caching** — Similar prompts served from cache. `/cache` shows stats, `/cache clear` resets.
- **Single binary** — `cargo build --release` produces one ~15 MB executable with no runtime dependencies.

---

## Quick Start

```bash
# Install
cargo install ember-cli
# or: curl -fsSL https://ember.dev/install.sh | sh

# Set your API key
export ANTHROPIC_API_KEY="..."   # or OPENAI_API_KEY, etc.

# One-shot task
ember chat "Refactor this function to use iterators" --tools filesystem

# Interactive mode
ember chat
```

Once in interactive mode:

```
You: explain what this crate does
Ember: …

/model claude-3-5-sonnet   # switch model
/cost                       # show session cost
/fork before-refactor       # create a branch point
/compact                    # force context compaction
/forks                      # list branches
/restore <id>               # go back
```

---

## Features

### 🧠 Agentic Runtime

The core loop in `ember-core` drives a ReAct-style agent:

```
user message → LLM call → [tool calls → tool results → LLM call …] → response
```

- Configurable max tool rounds per turn (default: 25)
- Tool timeout per call (default: 120 s)
- Max output tokens per completion (default: 4096)
- Auto-compact when token count exceeds 80% of the context window

The loop is backend-agnostic: `LlmBackend` and `ToolBackend` are traits. Swap in any provider or a mock for testing.

### 🔀 Session Forking

Branch a conversation at any point. Each fork stores a snapshot of the full turn history.

```
/fork try-different-prompt   → creates a named branch
/forks                        → lists all forks with turn counts
/restore <fork-id>            → replaces current history with the snapshot
```

Forks are ordered by creation time. The most recently created fork is marked active in the list.

### 💰 Cost Tracking

Every turn records input tokens, output tokens, cache-creation tokens, and cache-read tokens. Costs are looked up from a built-in pricing table:

| Model family | Input (per 1M) | Output (per 1M) |
|---|---|---|
| Claude Opus | $15.00 | $75.00 |
| Claude Sonnet | $3.00 | $15.00 |
| Claude Haiku | $1.00 | $5.00 |
| GPT-4o | $2.50 | $10.00 |
| GPT-4o mini | $0.15 | $0.60 |

Anthropic prompt-cache tokens (creation and read) are tracked separately. The `/cost` command shows per-turn breakdown and session total.

Unknown models return a zero-cost sentinel — the tracker never panics on an unrecognised model ID.

### 📦 Plugin Hooks

Plugins intercept tool calls at three points in the lifecycle:

| Hook | When | What it can do |
|---|---|---|
| `PreToolUse` | Before execution | Approve or deny the call |
| `PostToolUse` | After success | Replace the tool output |
| `PostToolUseFailure` | After failure | Log errors, trigger fallbacks |

Hooks are registered with a priority (lower = runs first). All hooks for an event are called even when one denies — messages from every handler are collected.

```rust
runner.register(HookHandler {
    name: "policy".to_string(),
    events: vec![HookEvent::PreToolUse],
    priority: 0,
    handler: Box::new(|ctx| {
        if ctx.tool_name == "shell" && ctx.tool_input.contains("rm -rf") {
            HookRunResult::deny("destructive shell command blocked")
        } else {
            HookRunResult::allow()
        }
    }),
});
```

### 🔒 Permissions

Three modes, configurable per-tool:

- `Unrestricted` — allow everything (default, no breaking change to existing code)
- `Interactive` — surface a `NeedsApproval` result for every action; the caller handles the prompt
- `Policy` — evaluate actions against per-tool rules

Per-tool rules include:
- `allowed_paths` / `denied_paths` — component-level prefix matching (`/tmp` does not match `/tmp_other/foo`)
- `read_only` — deny all writes regardless of path rules
- `allowed_commands` — whitelist of executable names (bare name or full path, matched by basename)
- `max_execution_time` — per-action timeout

### 📝 Context Management & Compaction

When a conversation grows large, `compact_conversation` replaces the oldest turns with a summary turn and returns metrics:

- `turns_removed` — how many turns were merged
- `original_tokens` / `compacted_tokens` — before/after estimates (4 chars ≈ 1 token heuristic)
- `summary` — the text inserted at the front of the conversation

`keep_recent_turns` (default: 4) and `summary_max_tokens` (default: 2000) are configurable. The compaction only fires when the estimated token count exceeds `compact_threshold × max_context_tokens` (defaults: 0.8 × 100k).

### ⚡ CLI & Slash Commands

The REPL recognises these slash commands:

| Command | Aliases | What it does |
|---|---|---|
| `/help` | `/h` | List all commands |
| `/status` | — | Turns, tokens, cost for this session |
| `/compact` | — | Force context compaction now |
| `/model [name]` | `/m` | Show or change the active model |
| `/permissions [mode]` | `/perm` | Show or change permission mode |
| `/config [section]` | `/cfg` | Display merged configuration |
| `/memory` | `/mem` | Show context window usage |
| `/clear [--yes]` | `/c` | Clear the conversation |
| `/cost` | — | Cost breakdown for this session |
| `/fork [name]` | — | Create a named fork point |
| `/forks` | — | List all forks |
| `/restore <id>` | — | Restore to a fork |

Tab completion is handled by `SlashCompleter` — prefixes are matched against the registry, so `/mo` completes to `/model` and `/me` to `/memory`.

### 🖥️ TUI Renderer

The terminal renderer uses `pulldown-cmark` for Markdown parsing and `syntect` for syntax highlighting:

- Fenced code blocks with a `┌─ rust ─────┐` border and 24-bit colour highlighting
- Coloured headings (H1–H6), emphasis, strong, inline code, blockquotes, links
- Animated Braille spinners (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`) with success/failure finish states
- Tool output formatter: header line (`⚡ Running: bash  ls -la`), output truncation, error display

All rendering methods accept any `io::Write` so they are testable without a TTY.

### ⚙️ Configuration Merge

Three config sources are merged in order: User → Project → Local. Later sources override earlier ones. The merge is deep (nested tables are merged, not replaced) and every entry records which config file set it.

```rust
let config = ConfigLoader::new()
    .add_user_config()
    .add_project_config("./ember.toml")
    .add_local_config("./.ember.local.toml")
    .load()?;

// Know which file set a value:
let entry = config.get("model");
println!("{:?}", entry.source); // ConfigSource::Project("./ember.toml")
```

### 🚀 Bootstrap Pipeline

Startup is split into ordered phases so the critical path can be measured and optional phases can be skipped:

```
CliEntry → ConfigLoad → ProviderInit → PluginDiscovery →
McpSetup → SystemPrompt → ToolRegistry → SessionInit → MainRuntime
```

`BootstrapTimer` records wall-clock time for each phase. `BootstrapPlan::fast_path(&[PluginDiscovery, McpSetup])` skips the two slowest phases for quick one-shot commands.

---

## Architecture

```
ember-cli          CLI entry, REPL, TUI rendering, slash commands
ember-core         Agent runtime, compaction, permissions, forking, config merge, bootstrap
ember-llm          10 LLM provider adapters, streaming, token usage
ember-tools        File ops, shell, web, git, code execution
ember-plugins      Plugin system, hook pipeline
ember-mcp          MCP client, multi-transport (stdio/HTTP/WebSocket), tool registry
ember-storage      Persistent storage, checkpoints
ember-telemetry    Usage tracking, session logging
ember-browser      Browser automation (chromiumoxide)
ember-voice        Voice I/O
ember-web          Web interface
ember-desktop      Desktop app (Tauri)
ember-i18n         Internationalization
ember-enterprise   Enterprise features
```

---

## Comparison

| Feature | Ember | Claude Code | Codex CLI |
|---|---|---|---|
| Multi-Provider | ✅ 10 providers | ❌ Anthropic only | ❌ OpenAI only |
| Session Forking | ✅ | ❌ | ❌ |
| Plugin Hooks | ✅ Pre/Post/Failure | ❌ | ❌ |
| MCP Support | ✅ Multi-transport | ✅ | ❌ |
| Cost Tracking | ✅ Per-turn + session | Basic | Basic |
| Prompt Cache Tracking | ✅ Creation + read | ✅ | N/A |
| Auto-Compaction | ✅ Configurable | ✅ | ❌ |
| Per-Tool Permissions | ✅ Path/command/time | ❌ | ❌ |
| Config Merge (3 levels) | ✅ | ❌ | ❌ |
| Single Binary | ✅ | ❌ | ❌ |
| Open Source | ✅ MIT | Partial | ✅ |

---

## Installation

### From crates.io

```bash
cargo install ember-cli
```

### From source

```bash
git clone https://github.com/niklasmarderx/ember
cd ember
cargo build --release
./target/release/ember --version
```

### Docker

```bash
docker run -it --rm ghcr.io/niklasmarderx/ember chat "Hello"
```

---

## Configuration

API keys are read from environment variables:

```bash
export ANTHROPIC_API_KEY="..."
export OPENAI_API_KEY="sk-..."
export GOOGLE_API_KEY="..."
```

Or place them in `.env` at the project root. Run `ember config init` to create a starter config file, `ember config show` to inspect the merged result.

---

## Supported Providers

| Provider | Status |
|---|---|
| Anthropic (Claude) | ✅ |
| OpenAI (GPT-4o, o1) | ✅ |
| Google Gemini | ✅ |
| Ollama (local) | ✅ |
| Groq | ✅ |
| DeepSeek | ✅ |
| Mistral | ✅ |
| OpenRouter | ✅ |
| xAI (Grok) | ✅ |
| AWS Bedrock | ✅ |

Any OpenAI-compatible API endpoint also works via the OpenAI provider with a custom base URL.

---

## Contributing

```bash
git clone https://github.com/niklasmarderx/ember
cd ember
cargo test --workspace
cargo run -p ember-cli -- chat "Hello"
```

Issues and PRs are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

---

## License

MIT — see [LICENSE-MIT](LICENSE-MIT)
