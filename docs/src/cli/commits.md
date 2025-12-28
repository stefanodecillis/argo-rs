# Commits

Create commits with optional AI-generated messages.

## Basic Usage

```bash
# Commit staged changes
argo commit -m "feat: add user authentication"

# Stage all changes and commit
argo commit -a -m "fix: resolve login bug"
```

## AI-Generated Messages

Let Gemini AI analyze your changes and generate a commit message:

```bash
# Generate message for staged changes
argo commit --ai

# Stage all and generate message
argo commit -a --ai
```

The AI will:
1. Analyze your staged changes (diff)
2. Generate a conventional commit message
3. Show you the message for confirmation

Requires a [Gemini API key](../configuration/ai-setup.md).

## Options

| Option | Short | Description |
|--------|-------|-------------|
| `--message` | `-m` | Commit message |
| `--all` | `-a` | Stage all modified files |
| `--ai` | | Generate message with AI |

## Commit Message Format

argo-rs follows [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description

[optional body]
```

Common types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation
- `refactor`: Code refactoring
- `test`: Adding tests
- `chore`: Maintenance tasks
