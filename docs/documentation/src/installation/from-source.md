# Build from Source

Build argo-rs from source for contributors or unsupported platforms.

## Prerequisites

- [Rust](https://rustup.rs/) 1.70 or later
- Git

## Build Steps

```bash
# Clone the repository
git clone https://github.com/stefanodecillis/argo-rs.git
cd argo-rs

# Build release binary (optimized)
cargo build --release

# The binary is at target/release/argo
```

## Install to PATH

Copy the binary to a location in your PATH:

```bash
# Create ~/.local/bin if it doesn't exist
mkdir -p ~/.local/bin

# Copy the binary
cp target/release/argo ~/.local/bin/
```

Ensure `~/.local/bin` is in your PATH:

```bash
# Add to ~/.bashrc or ~/.zshrc
export PATH="$HOME/.local/bin:$PATH"
```

## Development Build

For development with faster compile times (but slower runtime):

```bash
cargo build
cargo run -- --help
```

## Running Tests

```bash
cargo test
```

## Code Quality

```bash
# Format code
cargo fmt

# Lint
cargo clippy
```
