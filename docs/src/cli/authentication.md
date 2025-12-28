# Authentication

argo-rs uses GitHub's OAuth Device Flow for secure browser-based authentication.

## Commands

### Login

```bash
argo auth login
```

This will:
1. Display a device code
2. Open your browser to GitHub's authorization page
3. Wait for you to enter the code and authorize
4. Securely store the token in your system keychain

### Logout

```bash
argo auth logout
```

Removes stored GitHub credentials from the keychain.

### Check Status

```bash
argo auth status
```

Shows whether you're currently authenticated and displays your GitHub username.

## How OAuth Device Flow Works

1. argo-rs requests a device code from GitHub
2. You visit `https://github.com/login/device` and enter the code
3. GitHub authenticates you and grants access to argo-rs
4. The token is stored securely in your system keychain

This flow is more secure than storing personal access tokens because:
- No token is ever displayed or copied
- Tokens have limited, well-defined scopes
- Authorization can be revoked from GitHub settings

## Token Storage

Tokens are stored securely using your system's native credential storage:

- **macOS**: Keychain
- **Linux**: Secret Service (GNOME Keyring, KWallet, etc.)

See [Credential Storage](../configuration/credentials.md) for details.

## Troubleshooting

### "Not authenticated" Error

Run `argo auth login` to authenticate.

### Token Expired

Re-run `argo auth login`. The old token will be replaced.

### Browser Doesn't Open

Manually visit `https://github.com/login/device` and enter the displayed code.
