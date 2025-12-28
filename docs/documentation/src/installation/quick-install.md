# Quick Install

The easiest way to install argo-rs on macOS or Linux.

## One-Line Install

```bash
curl -sSL https://raw.githubusercontent.com/stefanodecillis/argo-rs/main/install.sh | bash
```

This will:

1. Detect your platform (macOS/Linux, x86_64/aarch64)
2. Download the latest release from GitHub
3. Install `argo` to `~/.local/bin/`
4. Sign the binary on macOS for Keychain compatibility

## Verify Installation

After installation, verify it works:

```bash
argo --version
```

If the command is not found, ensure `~/.local/bin` is in your PATH:

```bash
# Add to ~/.bashrc or ~/.zshrc
export PATH="$HOME/.local/bin:$PATH"
```

Then reload your shell:

```bash
source ~/.bashrc  # or source ~/.zshrc
```

## Next Steps

1. [Authenticate with GitHub](../cli/authentication.md)
2. Navigate to a git repository and run `argo` to launch the TUI
