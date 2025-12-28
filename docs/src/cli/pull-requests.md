# Pull Requests

Manage GitHub pull requests from the command line.

## List Pull Requests

```bash
# List open PRs (default)
argo pr list

# List all PRs (open, closed, merged)
argo pr list --state=all

# List closed PRs only
argo pr list --state=closed

# Filter by author
argo pr list --author=username
```

## Create Pull Request

```bash
# Create with title only
argo pr create --title "Add new feature"

# Create with title and body
argo pr create --title "Add new feature" --body "Description of changes"

# Create as draft
argo pr create --title "WIP: Feature" --draft

# Create with AI-generated title and body
argo pr create --ai
```

The `--ai` flag uses Gemini AI to analyze your commits and generate an appropriate title and description. Requires a [Gemini API key](../configuration/ai-setup.md).

## View Pull Request

```bash
# View PR details and comments
argo pr view 123
```

Displays:
- PR title and description
- Status (open, merged, closed)
- Author and reviewers
- Comments and review comments

## Comment on Pull Request

```bash
argo pr comment 123 "Looks good! Just one suggestion..."
```

## Merge Pull Request

```bash
# Merge commit (default)
argo pr merge 123

# Squash and merge
argo pr merge 123 --squash

# Rebase and merge
argo pr merge 123 --rebase

# Delete branch after merge
argo pr merge 123 --delete
```

### Merge Strategies

| Strategy | Command | Result |
|----------|---------|--------|
| Merge commit | `--merge` (default) | Creates a merge commit |
| Squash | `--squash` | Combines all commits into one |
| Rebase | `--rebase` | Rebases commits onto base branch |
