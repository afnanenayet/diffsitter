#!/usr/bin/env bash
# Claude Code WorktreeRemove hook
# Cleans up the worktree and its branch.
# Receives JSON on stdin with "name" field.
set -euo pipefail

INPUT=$(cat)
NAME=$(echo "$INPUT" | python3 -c "import sys,json; print(json.load(sys.stdin)['name'])" 2>/dev/null)

if [ -z "$NAME" ]; then
  echo "Error: could not parse worktree name from hook input" >&2
  exit 1
fi

REPO_ROOT=$(git rev-parse --show-toplevel)
WORKTREE_DIR="${REPO_ROOT}/.claude/worktrees/${NAME}"
BRANCH_NAME="worktree-${NAME}"

# Remove the worktree
if [ -d "$WORKTREE_DIR" ]; then
  git worktree remove --force "$WORKTREE_DIR" 2>&1 >&2
fi

# Delete the branch
git branch -D "$BRANCH_NAME" 2>&1 >&2 || true
