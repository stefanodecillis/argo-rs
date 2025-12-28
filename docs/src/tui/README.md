# TUI Guide

argo-rs includes an interactive Terminal User Interface (TUI) for managing GitHub repositories visually.

## Launching the TUI

Navigate to a git repository with a GitHub remote and run:

```bash
argo
```

The TUI provides:
- Visual navigation of pull requests
- Real-time polling for PR updates
- Keyboard-driven workflow with vim-style navigation
- Quick access to all argo-rs features

## Screens

The TUI consists of multiple screens:

| Screen | Access | Description |
|--------|--------|-------------|
| Home | Launch | Main menu |
| PR List | `p` | View and manage pull requests |
| Commit | `c` | Stage and commit changes |
| Settings | `s` | Configure argo-rs |

## Requirements

- Terminal with at least 80x24 characters
- GitHub authentication (run `argo auth login` first)
- Valid git repository with GitHub remote

## Next Steps

- Learn [Navigation & Keybindings](navigation.md)
- Or use the [CLI](../cli/README.md) for scripting
