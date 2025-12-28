# File Locations

argo-rs follows platform conventions for configuration storage.

## Configuration Directory

| Platform | Path |
|----------|------|
| macOS | `~/Library/Application Support/com.argo-rs.argo-rs/` |
| Linux | `~/.config/argo-rs/` |

## Configuration File

The main configuration file is `config.toml` inside the configuration directory.

**macOS:**
```
~/Library/Application Support/com.argo-rs.argo-rs/config.toml
```

**Linux:**
```
~/.config/argo-rs/config.toml
```

## Config File Format

```toml
# Example config.toml
gemini_model = "gemini-2.5-flash"
```

## Credentials

Credentials are **not** stored in the config file. They are stored securely in your system keychain. See [Credential Storage](credentials.md).

## Resetting Configuration

To reset configuration, delete the config directory:

**macOS:**
```bash
rm -rf ~/Library/Application\ Support/com.argo-rs.argo-rs
```

**Linux:**
```bash
rm -rf ~/.config/argo-rs
```

Note: This does not remove credentials from the keychain. Use `argo auth logout` to remove GitHub credentials.
