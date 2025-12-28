# Configuration

argo-rs stores configuration and credentials securely on your system.

## Configuration Files

Settings are stored in TOML format:

- **macOS**: `~/Library/Application Support/com.argo-rs.argo-rs/config.toml`
- **Linux**: `~/.config/argo-rs/config.toml`

## Credentials

Sensitive data (tokens, API keys) are stored in your system keychain:

- **macOS**: Keychain
- **Linux**: Secret Service (GNOME Keyring, KWallet, etc.)

## Available Settings

| Setting | Command | Description |
|---------|---------|-------------|
| Gemini API Key | `argo config set gemini-key` | Required for AI features |
| Gemini Model | `argo config set gemini-model` | AI model selection |

## Sections

- [File Locations](paths.md) - Where config files are stored
- [Credential Storage](credentials.md) - How secrets are protected
- [AI Setup](ai-setup.md) - Configure Gemini integration
