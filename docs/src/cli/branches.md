# Branches

Manage remote branches on GitHub.

## List Branches

```bash
argo branch list
```

Lists all remote branches in the repository.

## Delete Branch

```bash
# Delete with confirmation prompt
argo branch delete feature-branch

# Delete without confirmation
argo branch delete old-branch --force
```

**Note:** This deletes the remote branch on GitHub. Local branches are not affected.

## Common Workflows

### Clean Up Merged Branches

After merging a PR, delete the feature branch:

```bash
# Merge the PR with branch deletion
argo pr merge 123 --delete

# Or delete separately
argo branch delete feature-branch
```

### Remove Stale Branches

List branches to identify old ones, then delete:

```bash
argo branch list
argo branch delete stale-feature --force
```
