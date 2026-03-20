# Developer Guide

This guide covers building, testing, and contributing to Ember.

## Contents

- [Building from Source](building.md)
- [Testing](testing.md)
- [Benchmarking](benchmarking.md)
- [Architecture](architecture.md)

## Quick Start for Developers

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs))
- Node.js 18+ (for frontend and VS Code extension)
- Git

### Clone and Build

```bash
git clone https://github.com/ember-ai/ember
cd ember
cargo build --release
```

### Run Tests

```bash
cargo test
```

### Build Documentation

```bash
cd docs
mdbook serve
```

## Project Structure

```
ember/
├── crates/                 # Rust workspace
│   ├── ember-core/        # Core agent logic
│   ├── ember-llm/         # LLM providers
│   ├── ember-tools/       # Built-in tools
│   ├── ember-storage/     # Storage & RAG
│   ├── ember-mcp/         # MCP protocol
│   ├── ember-plugins/     # Plugin system
│   ├── ember-browser/     # Browser automation
│   ├── ember-cli/         # CLI application
│   ├── ember-web/         # Web server & UI
│   ├── ember-desktop/     # Tauri desktop app
│   ├── ember-telemetry/   # Analytics
│   ├── ember-i18n/        # Internationalization
│   └── ember-benchmarks/  # Performance tests
├── extensions/
│   └── vscode-ember/      # VS Code extension
├── docs/                   # mdBook documentation
├── examples/               # Example code
└── tests/                  # Integration tests
```

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/my-feature
```

### 2. Make Changes

Follow the coding standards in `.clinerules`.

### 3. Test Locally

```bash
# Run all tests
cargo test

# Run specific crate tests
cargo test -p ember-core

# Run with logging
RUST_LOG=debug cargo test -- --nocapture
```

### 4. Format and Lint

```bash
cargo fmt
cargo clippy -- -D warnings
```

### 5. Submit PR

Push your branch and create a pull request.

## Code Style

### Rust

- Use `rustfmt` defaults
- Follow Rust API guidelines
- Document all public items
- Use meaningful error messages
- Prefer `Result` over `panic!`

### TypeScript

- Use TypeScript strict mode
- Prefer functional components
- Use proper types (no `any`)

## Common Tasks

### Adding a Provider

1. Create `crates/ember-llm/src/myprovider.rs`
2. Implement the `Provider` trait
3. Add to `crates/ember-llm/src/lib.rs`
4. Add tests
5. Document in `docs/src/providers/`

### Adding a Tool

1. Create `crates/ember-tools/src/mytool.rs`
2. Implement the `Tool` trait
3. Register in `crates/ember-tools/src/registry.rs`
4. Add tests
5. Document in `docs/src/tools/`

### Adding a CLI Command

1. Create `crates/ember-cli/src/commands/mycommand.rs`
2. Add to `crates/ember-cli/src/commands/mod.rs`
3. Register in main.rs
4. Add tests

## Environment Variables

```bash
# Development
RUST_LOG=debug              # Enable debug logging
RUST_BACKTRACE=1            # Full backtraces

# Testing
EMBER_TEST_OPENAI_KEY=...   # For integration tests
EMBER_TEST_ANTHROPIC_KEY=...
```

## Debugging

### VS Code Launch Configuration

```json
{
    "version": "0.2.0",
    "configurations": [
        {
            "type": "lldb",
            "request": "launch",
            "name": "Debug ember-cli",
            "cargo": {
                "args": ["build", "-p", "ember-cli"],
                "filter": { "kind": "bin" }
            },
            "args": ["chat", "Hello"],
            "cwd": "${workspaceFolder}"
        }
    ]
}
```

### Logging

```rust
use tracing::{debug, info, error, instrument};

#[instrument]
async fn my_function(input: &str) -> Result<()> {
    debug!("Processing input: {}", input);
    // ...
    info!("Operation complete");
    Ok(())
}
```

## Release Process

1. Update version in `Cargo.toml` files
2. Update `CHANGELOG.md`
3. Create PR for version bump
4. After merge, tag the release
5. GitHub Actions handles publishing

## Getting Help

- **Discord**: Join our community server
- **Issues**: Report bugs on GitHub
- **Discussions**: Ask questions and share ideas