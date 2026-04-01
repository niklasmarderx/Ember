<div align="center">

<img src="assets/logo.svg" alt="Ember Logo" width="128" height="128"/>

# Ember

**An AI agent framework in Rust. Fast, small, runs everywhere.**

[![Website](https://img.shields.io/badge/website-ember.dev-orange)](https://niklasmarderx.github.io/Ember/)
[![Crates.io](https://img.shields.io/crates/v/ember-cli)](https://crates.io/crates/ember-cli)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
[![CI](https://github.com/niklasmarderx/Ember/actions/workflows/ci.yml/badge.svg)](https://github.com/niklasmarderx/Ember/actions)

</div>

---

## What is Ember?

Ember is a command-line tool and framework for working with AI models — for chatting, generating code, or automating tasks on your machine.

What sets it apart: Ember is written in Rust and ships as a single executable. No Python, no Node.js, no dependencies. Download one file and it works.

---

## Quick Start

### With cloud APIs (OpenAI, Anthropic, etc.)

```bash
curl -fsSL https://ember.dev/install.sh | sh
export OPENAI_API_KEY="sk-..."
ember chat
```

### Fully offline and free

```bash
# Install Ollama (one-time setup)
curl -fsSL https://ollama.ai/install.sh | sh
ollama pull llama3.2

# Install and use Ember
curl -fsSL https://ember.dev/install.sh | sh
ember chat --provider ollama
```

### As a Docker container

```bash
docker run -it --rm ghcr.io/niklasmarderx/Ember chat "Hello!"
```

### Web UI

```bash
ember serve
# Open http://localhost:3000 in your browser
```

---

## Why Ember?

### One binary, no dependencies

Ember compiles to a single 15 MB file. Copy it to a server, a Raspberry Pi, or your laptop — and it runs. No `pip install`, no version conflicts, no `node_modules`.

### Fast

Rust programs start immediately. Ember takes about 80ms to start, not several seconds like Python-based tools. Memory usage is around 45 MB instead of several hundred.

### Works offline

With Ollama you can run local models like Llama, Qwen, or Mistral. Completely without internet, without API costs, without your data leaving your machine.

### Many providers, one interface

Ember supports OpenAI, Anthropic, Google Gemini, Mistral, Groq, DeepSeek, xAI, OpenRouter, and Ollama. Switch providers with a flag — the code stays the same.

---

## Supported LLM Providers

| Provider | Example Models | Cost |
|----------|----------------|------|
| OpenAI | GPT-4o, GPT-4o-mini, o1 | Paid |
| Anthropic | Claude 3.5 Sonnet, Haiku | Paid |
| Google Gemini | Gemini 2.0, 1.5 Pro | Free tier available |
| Groq | Llama 3.3 70B, Mixtral | Free tier available |
| DeepSeek | V3, R1 | Low cost |
| Mistral | Large, Codestral | Paid |
| xAI | Grok 2 | Paid |
| OpenRouter | 200+ models | Varies |
| Ollama | Llama, Qwen, etc. | Free (local) |

---

## What can Ember do?

### Chat and code generation

```bash
ember chat "Explain recursion"
ember chat "Write a Python function that finds prime numbers"
```

### Enable tools

Ember can execute commands, read and write files, use Git, and search the web:

```bash
ember chat --tools shell,fs "Create a new folder 'project' and initialize Git"
ember chat --tools web "What is the current Bitcoin price?"
```

### Web UI

The web UI shows chat history, cost tracking, and lets you switch between models.

### Checkpoints

Ember saves every step. You can go back at any time if something goes wrong.

### Cost tracking

With cloud providers, you see in real time what a chat costs. You can set budget limits.

---

## Installation

### One command

```bash
curl -fsSL https://ember.dev/install.sh | sh
```

### With Cargo (if you have Rust installed)

```bash
cargo install ember-cli
```

### With Homebrew (macOS/Linux)

```bash
brew install ember-agent
```

### From source

```bash
git clone https://github.com/niklasmarderx/Ember
cd Ember
cargo build --release
```

---

## Configuration

Ember reads API keys from environment variables:

```bash
# OpenAI
export OPENAI_API_KEY="sk-..."

# Anthropic
export ANTHROPIC_API_KEY="..."

# For other providers see the documentation
```

Or create a `.env` file:

```
OPENAI_API_KEY=sk-...
EMBER_DEFAULT_PROVIDER=openai
EMBER_DEFAULT_MODEL=gpt-4o-mini
```

---

## Examples

### Simple chat

```rust
use ember::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let agent = Agent::builder()
        .provider(OpenAIProvider::from_env()?)
        .build()?;

    let response = agent.chat("What is the capital of France?").await?;
    println!("{}", response);
    Ok(())
}
```

### With tools

```rust
use ember::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let agent = Agent::builder()
        .provider(OllamaProvider::new()?)
        .tool(tools::Shell::new())
        .tool(tools::Filesystem::sandboxed("./workspace"))
        .build()?;

    agent.chat("List all .rs files in the current directory").await?;
    Ok(())
}
```

---

## Project Structure

```
ember/
├── crates/
│   ├── ember-core/      # Agent, memory, configuration
│   ├── ember-llm/       # LLM providers
│   ├── ember-tools/     # Shell, filesystem, Git, web
│   ├── ember-storage/   # SQLite, vector DB, RAG
│   ├── ember-cli/       # Command-line interface
│   ├── ember-web/       # Web server and React frontend
│   └── ...
├── examples/            # Code examples
├── docs/                # Documentation
└── extensions/          # VS Code extension
```

---

## Documentation

- [Getting Started](https://ember.dev/docs/getting-started)
- [CLI Reference](https://ember.dev/docs/cli)
- [Configure Providers](https://ember.dev/docs/providers)
- [Build Custom Tools](https://ember.dev/docs/custom-tools)
- [API Documentation (Rust)](https://docs.rs/ember)

---

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for details.

```bash
git clone https://github.com/niklasmarderx/Ember
cd Ember
cargo test --workspace
cargo run -p ember-cli -- chat "Test"
```

---

## License

MIT — see [LICENSE-MIT](LICENSE-MIT)

---

<div align="center">

**Questions?** [niklas.marder@gmail.com](mailto:niklas.marder@gmail.com)

[![GitHub](https://img.shields.io/github/stars/niklasmarderx/Ember?style=social)](https://github.com/niklasmarderx/Ember)

</div>
