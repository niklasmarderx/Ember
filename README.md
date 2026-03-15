<div align="center">

# Ember

### The AI Agent That Starts in 30 Seconds, Not 30 Minutes

[![Crates.io](https://img.shields.io/crates/v/ember-cli)](https://crates.io/crates/ember-cli)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
[![CI](https://github.com/niklasmarderx/Ember/actions/workflows/ci.yml/badge.svg)](https://github.com/niklasmarderx/Ember/actions)
[![Docker](https://img.shields.io/docker/pulls/niklasmarderx/ember)](https://hub.docker.com/r/niklasmarderx/ember)

**One binary. Zero dependencies. Rust-powered. Privacy-first.**

[Quick Start](#quick-start-30-seconds) |
[Why Ember](#why-ember) |
[Features](#feature-highlights) |
[Documentation](https://ember.dev/docs)

---

**Questions? Feedback? Enterprise inquiries?**  
Contact: [niklas.marder@gmail.com](mailto:niklas.marder@gmail.com)

</div>

---

<div align="center">

## What Makes Ember Revolutionary

> **"The first AI agent framework that respects your time, your memory, and your privacy."**

<table width="100%">
<tr>
<th width="50%">Traditional Agents</th>
<th width="50%">Ember</th>
</tr>
<tr><td align="center">Minutes to install</td><td align="center"><b>Seconds</b></td></tr>
<tr><td align="center">Gigabytes of RAM</td><td align="center"><b>Megabytes</b></td></tr>
<tr><td align="center">Hundreds of dependencies</td><td align="center"><b>Zero</b></td></tr>
<tr><td align="center">Requires internet</td><td align="center"><b>Works offline</b></td></tr>
<tr><td align="center">Python runtime needed</td><td align="center"><b>Single binary</b></td></tr>
<tr><td align="center">"It worked on my machine"</td><td align="center"><b>If it compiles, it runs</b></td></tr>
</table>

**Ember is not an incremental improvement.**  
**It's a complete reimagining of what an AI agent should be.**

</div>

We took everything developers hate about existing frameworks - the bloat, the slow starts, the dependency hell, the mandatory cloud connection - and eliminated it.

What's left is pure, fast, reliable AI tooling.

---

## The Problem

You want to build an AI agent. You try the popular Python frameworks:

```bash
# What you expect:
pip install langchain && python agent.py

# What you get:
pip install langchain  # 500+ dependencies, 15 minutes
# Dependency conflicts, version mismatches, "works on my machine"
# 2GB RAM usage, 5 second cold starts
# Internet required, API keys scattered everywhere
```

**We built Ember because we were tired of this.**

---

## The Solution

```bash
# Install (5 seconds)
curl -fsSL https://ember.dev/install.sh | sh

# Chat (25 seconds)
ember chat "Write me a Python script that finds all TODOs in my codebase"
```

**That's it.** No Python. No Node.js. No Docker. No environment variables. Works offline with local models.

---

## Speed Comparison

<table width="100%">
<tr>
<th width="20%"></th>
<th width="20%">LangChain</th>
<th width="20%">AutoGPT</th>
<th width="20%">CrewAI</th>
<th width="20%"><b>Ember</b></th>
</tr>
<tr><td><b>Install Time</b></td><td align="center">15 min</td><td align="center">20 min</td><td align="center">10 min</td><td align="center"><b>5 sec</b></td></tr>
<tr><td><b>Cold Start</b></td><td align="center">2.3s</td><td align="center">4.1s</td><td align="center">1.8s</td><td align="center"><b>80ms</b></td></tr>
<tr><td><b>Memory</b></td><td align="center">450MB</td><td align="center">800MB</td><td align="center">380MB</td><td align="center"><b>45MB</b></td></tr>
<tr><td><b>Dependencies</b></td><td align="center">500+</td><td align="center">300+</td><td align="center">200+</td><td align="center"><b>0</b></td></tr>
<tr><td><b>Works Offline</b></td><td align="center">No</td><td align="center">No</td><td align="center">No</td><td align="center"><b>Yes</b></td></tr>
</table>

*Measured on M2 MacBook Pro. [See benchmarks](docs/benchmarks.md)*

---

## Quick Start (30 Seconds)

### Option A: Cloud APIs (OpenAI/Anthropic/Groq)

```bash
# Install
curl -fsSL https://ember.dev/install.sh | sh

# Set ONE environment variable
export OPENAI_API_KEY="sk-..."

# Start chatting
ember chat
```

### Option B: 100% Free and Offline

**No API keys. No internet. No costs. Complete privacy.**

```bash
# Install Ollama (one time)
curl -fsSL https://ollama.ai/install.sh | sh
ollama pull llama3.2

# Install Ember
curl -fsSL https://ember.dev/install.sh | sh

# Chat - completely offline
ember chat --provider ollama
```

### Option C: Docker (One Command)

```bash
docker run -it --rm ghcr.io/niklasmarderx/Ember chat "Hello!"
```

---

## What Can Ember Do?

### 1. Chat with Any Model

```bash
# OpenAI
ember chat "Explain quantum computing"

# Anthropic Claude
ember chat --provider anthropic "Review my code"

# Local Ollama (free, private)
ember chat --provider ollama "Write a haiku"

# Groq (ultra-fast, free tier!)
ember chat --provider groq "Summarize this paper"
```

### 2. Execute Tasks with Tools

```bash
# Create files, run commands, browse the web
ember chat --tools shell,filesystem,web "Create a React app with dark mode"
```

### 3. Build AI Applications (10 lines of Rust)

```rust
use ember::{Agent, OllamaProvider, tools};

#[tokio::main]
async fn main() -> Result<()> {
    let agent = Agent::builder()
        .provider(OllamaProvider::new()?)
        .tool(tools::Shell::new())
        .tool(tools::Filesystem::sandboxed("./workspace"))
        .build()?;

    agent.chat("Build a REST API in Rust").await?;
    Ok(())
}
```

---

## Why Rust?

|   | Python | Rust |
|---|---|---|
| Memory Safety | Runtime errors | **Compile-time guarantees** |
| Performance | Interpreted, GC pauses | **Native speed, zero-cost abstractions** |
| Deployment | Python + pip + venv + deps | **Single 15MB binary** |
| Reliability | "It works... sometimes" | **If it compiles, it works** |

**Ember is built for developers who ship.**

---

## Feature Highlights

### Multi-Provider Support
Switch between OpenAI, Anthropic, Ollama, Groq with one flag. Add your own providers with 50 lines of code.

### Built-in Tools
Shell commands, file operations, web scraping, browser automation, Git operations, code execution - all sandboxed and secure.

### WASM Plugins
Extend Ember with plugins in any language that compiles to WASM. Hot-reload during development.

### Plan/Act Mode
For complex tasks, Ember plans before acting. Review the plan, then execute with confidence.

### Checkpoints
Undo/redo any action. Never lose progress. Perfect for experimentation.

### Privacy First
Run 100% offline with Ollama. Your data never leaves your machine.

---

## Supported Providers

| Provider | Status | Best For |
|---|---|---|
| **OpenAI** | Stable | General purpose, GPT-4o |
| **Anthropic** | Stable | Coding, Claude 3.5 Sonnet |
| **Ollama** | Stable | Privacy, offline, free |
| **Groq** | Stable | Speed (ultra-fast inference) |

---

## Installation

```bash
# One-liner (macOS/Linux)
curl -fsSL https://ember.dev/install.sh | sh

# Homebrew
brew install ember-agent

# Cargo
cargo install ember-cli

# Docker
docker pull ghcr.io/niklasmarderx/Ember
```

---

## Documentation

- [Getting Started Guide](https://ember.dev/docs/getting-started)
- [CLI Reference](https://ember.dev/docs/cli)
- [Building Custom Tools](https://ember.dev/docs/custom-tools)
- [Provider Configuration](https://ember.dev/docs/providers)
- [API Reference](https://docs.rs/ember)

---

## Comparison with Alternatives

| Feature | LangChain | AutoGPT | CrewAI | OpenClaw | **Ember** |
|---------|-----------|---------|--------|----------|-----------|
| Language | Python | Python | Python | Python | **Rust** |
| Single Binary | No | No | No | No | **Yes** |
| Zero Dependencies | No | No | No | No | **Yes** |
| Sub-100ms Start | No | No | No | No | **Yes** |
| Memory < 50MB | No | No | No | No | **Yes** |
| Works Offline | No | No | No | No | **Yes** |
| WASM Plugins | No | No | No | No | **Yes** |
| Type Safe | No | No | No | No | **Yes** |
| Memory Safe | No | No | No | No | **Yes** |

---

## Join the Ember Community

<div align="center">

**Ember is built by developers, for developers.**

We believe the best AI tooling should be open, fast, and accessible to everyone.

</div>

### Ways to Contribute

| Contribution | Description | Difficulty |
|---|---|---|
| **Report Bugs** | Found an issue? Open a GitHub issue | Easy |
| **Improve Docs** | Help others get started faster | Easy |
| **Add Examples** | Share your use cases | Easy |
| **New Providers** | Add support for more LLMs (Google, Mistral, etc.) | Medium |
| **New Tools** | Build tools others can use | Medium |
| **Core Features** | Help with the agent runtime | Advanced |
| **WASM Plugins** | Extend the plugin system | Advanced |

### Good First Issues

We label beginner-friendly issues with `good first issue`. Perfect for your first PR!

[View Good First Issues](https://github.com/niklasmarderx/Ember/labels/good%20first%20issue)

### Getting Started

```bash
# Clone the repository
git clone https://github.com/niklasmarderx/Ember
cd Ember

# Run the quickstart script (builds and configures everything)
./quickstart.sh

# Run tests
cargo test --workspace

# Run the CLI
cargo run -p ember-cli -- chat "Hello!"
```

### Project Structure

```
ember/
├── crates/
│   ├── ember-core/      # Agent runtime, memory, context
│   ├── ember-llm/       # LLM providers (OpenAI, Anthropic, Ollama, Groq)
│   ├── ember-tools/     # Built-in tools (shell, filesystem, web, git)
│   ├── ember-storage/   # SQLite, vector database
│   ├── ember-plugins/   # WASM plugin system
│   ├── ember-cli/       # Command-line interface
│   └── ember-web/       # Web UI and REST API
├── examples/            # Code examples
├── docs/                # Documentation
└── extensions/          # VS Code extension
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed guidelines.

### Contributors

<!-- ALL-CONTRIBUTORS-LIST:START -->
Your name could be here! Submit your first PR today.
<!-- ALL-CONTRIBUTORS-LIST:END -->

---

## License

MIT License - see [LICENSE-MIT](LICENSE-MIT)

---

<div align="center">

**Small spark, big fire.**

Built with Rust. Built for speed. Built for developers who ship.

**If Ember helps you, please consider giving us a star - it helps others discover the project!**

[![Star on GitHub](https://img.shields.io/github/stars/niklasmarderx/Ember?style=social)](https://github.com/niklasmarderx/Ember)

[Get Started](#quick-start-30-seconds) | [Report Bug](https://github.com/niklasmarderx/Ember/issues) | [Request Feature](https://github.com/niklasmarderx/Ember/issues)

---

**Contact:** [niklas.marder@gmail.com](mailto:niklas.marder@gmail.com)

</div>