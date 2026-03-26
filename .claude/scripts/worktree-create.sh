#!/usr/bin/env bash
# Claude Code WorktreeCreate hook
# Replaces the default git worktree creation to add submodule initialization.
# Receives JSON on stdin with "name" field, must output the worktree path to stdout.
set -euo pipefail

# Parse the worktree name from stdin JSON
INPUT=$(cat)
NAME=$(echo "$INPUT" | python3 -c "import sys,json; print(json.load(sys.stdin)['name'])" 2>/dev/null)

if [ -z "$NAME" ]; then
  echo "Error: could not parse worktree name from hook input" >&2
  exit 1
fi

REPO_ROOT=$(git rev-parse --show-toplevel)
WORKTREE_DIR="${REPO_ROOT}/.claude/worktrees/${NAME}"
BRANCH_NAME="worktree-${NAME}"

# Determine the default remote branch to base the worktree on
DEFAULT_BRANCH=$(git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's|refs/remotes/origin/||' || echo "main")

# Create the worktree directory
mkdir -p "$(dirname "$WORKTREE_DIR")"

# Create a new branch and worktree from the default remote branch
git worktree add -b "$BRANCH_NAME" "$WORKTREE_DIR" "origin/${DEFAULT_BRANCH}" 2>&1 >&2

# Initialize submodules in the new worktree (critical for diffsitter builds)
(cd "$WORKTREE_DIR" && git submodule update --init --recursive) >&2

# Output the absolute path (required by Claude Code)
echo "$WORKTREE_DIR"
