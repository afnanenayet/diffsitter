#!/usr/bin/env bash
# Builds the tree-sitter-mcp binary if it doesn't exist or is outdated.
# Called by the SessionStart hook on each Claude Code session.
set -euo pipefail

PLUGIN_ROOT="${CLAUDE_PLUGIN_ROOT:?CLAUDE_PLUGIN_ROOT not set}"
PLUGIN_DATA="${CLAUDE_PLUGIN_DATA:?CLAUDE_PLUGIN_DATA not set}"
BINARY_PATH="${PLUGIN_DATA}/bin/tree-sitter-mcp"

# The plugin lives inside the diffsitter repo at plugins/tree-sitter-mcp/.
# Walk up to the repo root to build.
REPO_ROOT="$(cd "${PLUGIN_ROOT}/../.." && pwd)"

# Verify we're in the right repo.
if [[ ! -f "${REPO_ROOT}/Cargo.toml" ]]; then
    echo "tree-sitter-mcp: cannot find diffsitter repo root at ${REPO_ROOT}" >&2
    exit 1
fi

# Use Cargo.lock as the build fingerprint — if it hasn't changed since the
# last successful build, skip recompilation.
FINGERPRINT="${PLUGIN_DATA}/.build-fingerprint"
CURRENT_HASH="$(shasum -a 256 "${REPO_ROOT}/Cargo.lock" 2>/dev/null | cut -d' ' -f1)"

if [[ -x "${BINARY_PATH}" ]] && [[ -f "${FINGERPRINT}" ]]; then
    STORED_HASH="$(cat "${FINGERPRINT}")"
    if [[ "${CURRENT_HASH}" == "${STORED_HASH}" ]]; then
        exit 0
    fi
fi

echo "tree-sitter-mcp: building binary..." >&2

# Ensure submodules are initialized (required for grammar compilation).
cd "${REPO_ROOT}"
if [[ ! -f "grammars/tree-sitter-rust/src/parser.c" ]]; then
    git submodule update --init --recursive
fi

# Build the MCP server binary.
cargo build --release --features mcp-server --bin tree-sitter-mcp

# Install to plugin data directory.
mkdir -p "${PLUGIN_DATA}/bin"
cp "${REPO_ROOT}/target/release/tree-sitter-mcp" "${BINARY_PATH}"
chmod +x "${BINARY_PATH}"

# Record fingerprint.
echo "${CURRENT_HASH}" > "${FINGERPRINT}"

echo "tree-sitter-mcp: binary ready at ${BINARY_PATH}" >&2
