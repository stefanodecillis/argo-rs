# CLI Reference

argo-rs provides a comprehensive command-line interface for GitHub repository management.

## Command Structure

```
argo [COMMAND] [OPTIONS]
```

Running `argo` without arguments launches the [TUI mode](../tui/README.md).

## Available Commands

| Command | Description |
|---------|-------------|
| `auth` | [Authentication](authentication.md) with GitHub |
| `pr` | [Pull request](pull-requests.md) operations |
| `branch` | [Branch](branches.md) management |
| `commit` | [Create commits](commits.md) with optional AI |
| `config` | [Configuration](configuration.md) management |

## Global Options

```bash
argo --help     # Show help
argo --version  # Show version
```

## Quick Examples

```bash
# Authenticate
argo auth login

# List PRs
argo pr list

# Create commit with AI-generated message
argo commit -a --ai

# Launch TUI
argo
```
