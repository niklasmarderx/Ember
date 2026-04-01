# Contributing to Ember

First off, thank you for considering contributing to Ember! It's people like you that make Ember such a great tool.

## Code of Conduct

This project and everyone participating in it is governed by our [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## How Can I Contribute?

### Reporting Bugs

Before creating bug reports, please check the issue list as you might find out that you don't need to create one. When you are creating a bug report, please include as many details as possible:

- **Use a clear and descriptive title**
- **Describe the exact steps which reproduce the problem**
- **Provide specific examples to demonstrate the steps**
- **Describe the behavior you observed after following the steps**
- **Explain which behavior you expected to see instead and why**
- **Include your environment details** (OS, Rust version, Ember version)

### Suggesting Enhancements

Enhancement suggestions are tracked as GitHub issues. When creating an enhancement suggestion:

- **Use a clear and descriptive title**
- **Provide a detailed description of the suggested enhancement**
- **Explain why this enhancement would be useful**
- **List any alternatives you've considered**

### Pull Requests

1. **Fork the repo** and create your branch from `main`
2. **Follow the coding style** outlined in `.clinerules`
3. **Write tests** for any new functionality
4. **Ensure the test suite passes** with `cargo test --workspace`
5. **Run lints** with `cargo clippy -- -D warnings`
6. **Format your code** with `cargo fmt`
7. **Update documentation** if needed

## Development Setup

```bash
# Clone the repository
git clone https://github.com/niklasmarderx/Ember.git
cd Ember

# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Run the CLI
cargo run -p ember-cli -- --help
```

## Project Structure

```
ember/
├── crates/
│   ├── ember-core/      # Agent runtime, memory, context
│   ├── ember-llm/       # LLM provider abstraction
│   ├── ember-tools/     # Built-in tools (shell, filesystem, web)
│   ├── ember-storage/   # SQLite, vector database
│   ├── ember-plugins/   # WASM plugin system
│   ├── ember-cli/       # Command-line interface
│   └── ember-web/       # Web server and API
├── docs/                # Documentation
├── examples/            # Example configurations
└── tests/               # Integration tests
```

## Coding Standards

### Rust Conventions

- Use Rust 2021 Edition
- Maximum line length: 100 characters
- Use 4 spaces for indentation

### Error Handling

```rust
// Good
let config = Config::load(&path)
    .context("Failed to load configuration")?;

// Bad
let config = Config::load(&path).unwrap();
```

### Documentation

All public APIs must have doc comments:

```rust
/// Creates a new agent with the specified configuration.
///
/// # Arguments
///
/// * `config` - The agent configuration
///
/// # Errors
///
/// Returns an error if the configuration is invalid.
pub fn new(config: AgentConfig) -> Result<Self> {
    // ...
}
```

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`

Examples:
- `feat(llm): add Anthropic provider support`
- `fix(memory): prevent memory leak in long conversations`
- `docs(readme): update installation instructions`

## License

By contributing, you agree that your contributions will be licensed under the MIT OR Apache-2.0 license.

## Questions?

Feel free to open an issue or reach out to the maintainers.

Thank you for contributing!
