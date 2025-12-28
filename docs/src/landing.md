# argo-rs

<div class="hero">
  <img src="assets/argo.png" alt="argo mascot" class="mascot">
  <h2>A Terminal Application for Managing GitHub Repositories</h2>
  <p class="tagline">Interactive TUI and powerful CLI for pull requests, branches, commits, and AI-powered development workflows.</p>
</div>

<div class="features-grid">
  <div class="feature">
    <h3>GitHub Authentication</h3>
    <p>Secure OAuth Device Flow for browser-based login</p>
  </div>
  <div class="feature">
    <h3>Pull Request Management</h3>
    <p>List, create, view, comment, and merge PRs</p>
  </div>
  <div class="feature">
    <h3>AI Integration</h3>
    <p>Generate commit messages and PR descriptions with Gemini AI</p>
  </div>
  <div class="feature">
    <h3>Interactive TUI</h3>
    <p>Vim-style navigation in a beautiful terminal interface</p>
  </div>
</div>

## Quick Start

```bash
# Install
curl -sSL https://raw.githubusercontent.com/stefanodecillis/argo-rs/main/install.sh | bash

# Authenticate
argo auth login

# Launch TUI
cd your-repo && argo
```

<div class="download-section">

### Download

<a href="https://github.com/stefanodecillis/argo-rs/releases/latest" class="download-button">Latest Release</a>

| Platform | Architecture | Download |
|----------|--------------|----------|
| macOS | Apple Silicon (M1/M2/M3) | `argo-macos-aarch64.tar.gz` |
| Linux | x86_64 | `argo-linux-x86_64.tar.gz` |

For other platforms, please [build from source](installation/from-source.md).

</div>

## Why argo-rs?

- **Fast**: Written in Rust for blazing performance
- **Secure**: OAuth authentication, credentials stored in system keychain
- **Smart**: AI-powered commit messages and PR descriptions
- **Intuitive**: Vim-style navigation, works exactly like you'd expect

## Get Started

1. [Install argo-rs](installation/README.md) on your system
2. [Authenticate](cli/authentication.md) with GitHub
3. Launch the [TUI](tui/README.md) or use [CLI commands](cli/README.md)
