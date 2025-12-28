# Development

Guide for contributing to argo-rs.

## Setup

```bash
# Clone the repository
git clone https://github.com/stefanodecillis/argo-rs.git
cd argo-rs

# Build
cargo build

# Run
cargo run -- --help

# Test
cargo test

# Format
cargo fmt

# Lint
cargo clippy
```

## Architecture

```
src/
├── main.rs          # Entry point, CLI dispatch
├── error.rs         # Unified error handling
├── core/            # Business logic
│   ├── config.rs    # TOML config management
│   ├── credentials.rs # Credential storage
│   ├── git.rs       # Git operations
│   └── repository.rs # GitHub URL parsing
├── github/          # GitHub API
│   ├── client.rs    # Octocrab wrapper
│   ├── auth.rs      # OAuth Device Flow
│   ├── pull_request.rs
│   └── branch.rs
├── cli/             # Command handlers
├── tui/             # Terminal UI
│   ├── app.rs       # State machine
│   ├── event.rs     # Input handling
│   └── screens/     # Screen implementations
└── ai/              # Gemini integration
    ├── gemini.rs    # API client
    └── prompts.rs   # Prompt templates
```

## Key Patterns

### Error Handling

All errors flow through `GhrustError` in `src/error.rs`:

```rust
pub enum GhrustError {
    Git(String),
    GitHub(String),
    Config(String),
    // ...
}
```

### Credential Management

Three-tier fallback: environment → cache → keyring. Uses `secrecy::SecretString` to prevent accidental exposure.

### Async Architecture

Tokio runtime with channel-based message passing between TUI and async operations.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run `cargo fmt` and `cargo clippy`
5. Run `cargo test`
6. Submit a pull request

## License

MIT - see [LICENSE](https://github.com/stefanodecillis/argo-rs/blob/main/LICENSE) for details.
