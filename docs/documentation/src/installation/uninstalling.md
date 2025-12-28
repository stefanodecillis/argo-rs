# Uninstalling

Remove argo-rs from your system.

## Quick Uninstall

```bash
curl -sSL https://raw.githubusercontent.com/stefanodecillis/argo-rs/main/uninstall.sh | bash
```

This will:
- Remove the `argo` binary
- Optionally remove configuration files
- Provide instructions for removing stored credentials

## Manual Uninstall

### Remove Binary

```bash
rm -f ~/.local/bin/argo
```

### Remove Configuration

**macOS:**
```bash
rm -rf ~/Library/Application\ Support/com.argo-rs.argo-rs
```

**Linux:**
```bash
rm -rf ~/.config/argo-rs
```

### Remove Credentials

Credentials are stored securely in your system keychain:

- **macOS**: Open Keychain Access and search for "argo-rs"
- **Linux**: Use your Secret Service manager (GNOME Keyring, KWallet, etc.) to find and remove "argo-rs" entries

The uninstall script cannot automatically remove keychain entries for security reasons.
