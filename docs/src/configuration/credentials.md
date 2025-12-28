# Credential Storage

argo-rs stores sensitive credentials securely using your system's native keychain.

## Storage Mechanism

| Platform | Backend |
|----------|---------|
| macOS | Keychain |
| Linux | Secret Service (GNOME Keyring, KWallet, etc.) |

## What's Stored

| Credential | Purpose |
|------------|---------|
| GitHub Token | Authentication for GitHub API |
| Gemini API Key | AI feature integration |

## Security Features

### Secret Handling

argo-rs uses the `secrecy` crate to prevent accidental token exposure:
- Tokens are wrapped in `SecretString`
- Debug output redacts sensitive values
- Memory is zeroed when tokens are dropped

### Three-Tier Fallback

Credentials are retrieved in order:

1. **Environment Variables** - `GITHUB_TOKEN`, `GEMINI_API_KEY`
2. **In-Memory Cache** - For performance during a session
3. **System Keychain** - Persistent secure storage

This allows CI/CD environments to use environment variables while desktop users benefit from keychain storage.

## Managing Credentials

### GitHub Token

```bash
# Login (stores token in keychain)
argo auth login

# Logout (removes token from keychain)
argo auth logout

# Check status
argo auth status
```

### Gemini API Key

```bash
# Set key (stores in keychain)
argo config set gemini-key YOUR_KEY

# Check if configured
argo config get gemini-key
```

## Environment Variables

For CI/CD or if you prefer not to use the keychain:

```bash
export GITHUB_TOKEN="ghp_xxxxx"
export GEMINI_API_KEY="xxxxx"
```

These take precedence over keychain-stored credentials.
