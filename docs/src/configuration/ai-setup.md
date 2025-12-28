# AI Setup (Gemini)

argo-rs integrates with Google's Gemini AI for generating commit messages and PR descriptions.

## Getting an API Key

1. Visit [Google AI Studio](https://aistudio.google.com/)
2. Sign in with your Google account
3. Create a new API key
4. Copy the key

## Configuring argo-rs

```bash
# Set your API key
argo config set gemini-key YOUR_API_KEY

# Verify it's configured
argo config get gemini-key
```

## Selecting a Model

Choose which Gemini model to use:

```bash
# Set the model
argo config set gemini-model gemini-2.5-flash

# Check current model
argo config get gemini-model
```

### Available Models

| Model | Description |
|-------|-------------|
| `gemini-2.0-flash` | Fast, efficient responses |
| `gemini-2.5-flash` | **Default** - Balanced performance |
| `gemini-3-flash-preview` | Latest features (preview) |

## Using AI Features

### AI Commit Messages

Generate a commit message from your staged changes:

```bash
# Stage files first
git add .

# Generate and commit
argo commit --ai

# Or stage all and generate
argo commit -a --ai
```

### AI PR Descriptions

Generate a PR title and description from your branch commits:

```bash
argo pr create --ai
```

## How It Works

1. argo-rs analyzes your changes (diff for commits, commits for PRs)
2. Sends context to Gemini API
3. Returns a formatted message following conventional commit style
4. Shows you the result for confirmation before applying

## Troubleshooting

### "API key not configured"

Run `argo config set gemini-key YOUR_KEY` with your API key.

### "Rate limit exceeded"

Wait a moment and try again. Free tier has usage limits.

### Poor quality suggestions

Try a different model with `argo config set gemini-model gemini-3-flash-preview`.
