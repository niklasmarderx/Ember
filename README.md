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

| Traditional Agents | Ember |
|:---:|:---:|
| Minutes to install | **Seconds** |
| Gigabytes of RAM | **Megabytes** |
| Hundreds of dependencies | **Zero** |
| Requires internet | **Works offline** |
| Python runtime needed | **Single binary** |
| "It worked on my machine" | **If it compiles, it runs** |

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

| | LangChain | AutoGPT | CrewAI | **Ember** |
|---|---|---|---|---|
| **Install Time** | 15 min | 20 min | 10 min | **5 sec** |
| **Cold Start** | 2.3s | 4.1s | 1.8s | **80ms** |
| **Memory** | 450MB | 800MB | 380MB | **45MB** |
| **Dependencies** | 500+ | 300+ | 200+ | **0** |
| **Works Offline** | No | No | No | **Yes** |

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

## Contributing

We welcome contributions! Ember is designed to be easy to understand and extend.

```bash
git clone https://github.com/niklasmarderx/Ember
cd Ember
./quickstart.sh  # Builds everything, sets up config
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

---

## License

MIT License - see [LICENSE-MIT](LICENSE-MIT)

---

<div align="center">

**Small spark, big fire.**

Built with Rust. Built for speed. Built for developers who ship.

[Get Started](#quick-start-30-seconds) | [Star on GitHub](https://github.com/niklasmarderx/Ember)

---

**Contact:** [niklas.marder@gmail.com](mailto:niklas.marder@gmail.com)

</div>