# argo-rs

A terminal application with TUI for managing GitHub repositories.

> **Note:** You can use either `argo` or `gr` command - they are aliases for the same binary.

## Installation

### Quick Install (macOS/Linux)

```bash
curl -sSL https://raw.githubusercontent.com/stefanodecillis/argo-rs/main/install.sh | bash
```

This will:
- Detect your platform (macOS/Linux, x86_64/aarch64)
- Download the latest release
- Install `gr` and `argo` to `~/.local/bin/`
- Sign the binary on macOS for Keychain compatibility

### Build from Source

**Prerequisites:**
- [Rust](https://rustup.rs/) (1.70 or later)
- Git

```bash
# Clone the repository
git clone https://github.com/stefanodecillis/argo-rs.git
cd argo-rs

# Build release binary
cargo build --release

# Install to your PATH
cp target/release/gr ~/.local/bin/

# Optional: create the argo alias
ln -sf ~/.local/bin/gr ~/.local/bin/argo
```

Make sure `~/.local/bin` is in your PATH:

```bash
# Add to ~/.bashrc or ~/.zshrc
export PATH="$HOME/.local/bin:$PATH"
```

### Updating

To update to the latest version, simply run the install script again:

```bash
curl -sSL https://raw.githubusercontent.com/stefanodecillis/argo-rs/main/install.sh | bash
```

### Uninstalling

**Quick Uninstall (macOS/Linux):**

```bash
curl -sSL https://raw.githubusercontent.com/stefanodecillis/argo-rs/main/uninstall.sh | bash
```

This will remove binaries and optionally remove configuration files. Stored credentials must be removed manually (the script will provide instructions).

**Manual Uninstall:**

```bash
# Remove binaries
rm -f ~/.local/bin/gr ~/.local/bin/argo

# Remove configuration (macOS)
rm -rf ~/Library/Application\ Support/com.argo-rs.argo-rs

# Remove configuration (Linux)
rm -rf ~/.config/argo-rs
```

For credentials, open your system's keychain/password manager and search for "argo-rs" entries.

## Features

- **GitHub Authentication**: OAuth Device Flow for secure browser-based login
- **Pull Request Management**: List, create, view, comment, and merge PRs
- **Branch Operations**: List and delete remote branches
- **Commit Creation**: Stage files and create commits with messages
- **AI Integration**: Generate commit messages and PR descriptions using Gemini AI
- **TUI Mode**: Interactive terminal UI with vim-style navigation
- **Polling**: Real-time updates for PR comments

## Quick Start

1. **Authenticate with GitHub**:
   ```bash
   gr auth login
   ```

2. **Navigate to a git repository and launch TUI**:
   ```bash
   cd your-repo
   gr
   ```

3. **Or use CLI commands directly**:
   ```bash
   gr pr list
   gr pr create --title "My PR" --body "Description"
   gr commit -m "feat: add new feature"
   ```

## CLI Commands

### Authentication

```bash
gr auth login     # Login via OAuth Device Flow
gr auth logout    # Remove stored credentials
gr auth status    # Check authentication status
```

### Pull Requests

```bash
gr pr list                          # List open PRs
gr pr list --state=all              # List all PRs
gr pr list --author=username        # Filter by author

gr pr create --title "Title"        # Create PR with title
gr pr create --ai                   # Create PR with AI-generated title/body
gr pr create --draft                # Create as draft PR

gr pr view 123                      # View PR #123 with comments
gr pr comment 123 "Great work!"     # Add comment to PR #123

gr pr merge 123                     # Merge PR #123 (merge commit)
gr pr merge 123 --squash            # Squash and merge
gr pr merge 123 --rebase            # Rebase and merge
gr pr merge 123 --delete            # Delete branch after merge
```

### Branches

```bash
gr branch list                      # List remote branches
gr branch delete feature-branch     # Delete remote branch
gr branch delete old-branch --force # Delete without confirmation
```

### Commits

```bash
gr commit -m "commit message"       # Commit staged changes
gr commit -a -m "message"           # Stage all and commit
gr commit --ai                      # Generate message with AI
gr commit -a --ai                   # Stage all + AI message
```

### Configuration

```bash
gr config set gemini-key YOUR_KEY   # Set Gemini API key for AI features
gr config get gemini-key            # Check if key is configured
gr config set gemini-model MODEL    # Set AI model
gr config get gemini-model          # Show current model
```

#### Available Gemini Models

- `gemini-2.0-flash`
- `gemini-2.5-flash` (default)
- `gemini-3-flash-preview`

## TUI Mode

Launch the interactive TUI by running `gr` without arguments:

```bash
gr
```

### Key Bindings

| Key | Action |
|-----|--------|
| `j` / `Down` | Move down |
| `k` / `Up` | Move up |
| `Enter` | Select / Confirm |
| `Esc` / `q` | Back / Quit |
| `p` | Go to PR list |
| `c` | Go to commit screen |
| `s` | Go to settings |
| `n` | New PR (in PR list) |
| `r` | Refresh |

## Requirements

- Git repository with GitHub remote
- macOS or Linux
- For AI features: Gemini API key

## Configuration

Configuration is stored in:
- **macOS**: `~/Library/Application Support/com.argo-rs.argo-rs/config.toml`
- **Linux**: `~/.config/argo-rs/config.toml`

Credentials (GitHub token, Gemini API key) are stored securely in:
- **macOS**: Keychain
- **Linux**: Secret Service (GNOME Keyring, KWallet, etc.)

### GitHub OAuth

argo-rs uses GitHub's OAuth Device Flow for authentication. The OAuth app is registered under the argo-rs project. When you run `gr auth login`, you'll be redirected to GitHub to authorize the official argo-rs application.

## Development

```bash
# Clone
git clone https://github.com/stefanodecillis/argo-rs.git
cd argo-rs

# Build
cargo build

# Run
cargo run -- --help
# Or run with specific binary name:
cargo run --bin gr -- --help
cargo run --bin argo -- --help

# Test
cargo test

# Format
cargo fmt

# Lint
cargo clippy
```

## License

MIT - see [LICENSE](LICENSE) for details.
