# Configuration Commands

Manage argo-rs settings from the command line.

## Gemini API Key

Required for AI-powered features (commit messages, PR descriptions).

```bash
# Set your API key
argo config set gemini-key YOUR_API_KEY

# Check if key is configured
argo config get gemini-key
```

Get an API key from [Google AI Studio](https://aistudio.google.com/).

## Gemini Model

Choose which Gemini model to use for AI features.

```bash
# Set the model
argo config set gemini-model gemini-2.5-flash

# Check current model
argo config get gemini-model
```

### Available Models

| Model | Description |
|-------|-------------|
| `gemini-2.0-flash` | Fast, efficient model |
| `gemini-2.5-flash` | Default, balanced performance |
| `gemini-3-flash-preview` | Latest preview features |

## Configuration Storage

Settings are stored in:
- **macOS**: `~/Library/Application Support/com.argo-rs.argo-rs/config.toml`
- **Linux**: `~/.config/argo-rs/config.toml`

See [File Locations](../configuration/paths.md) for details.
