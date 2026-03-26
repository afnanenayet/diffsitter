#!/usr/bin/env bash
# Validates the tree-sitter-mcp plugin structure and manifest.
# Run from the repo root: bash plugins/tree-sitter-mcp/scripts/validate-plugin.sh
set -euo pipefail

PLUGIN_DIR="$(cd "$(dirname "$0")/.." && pwd)"
ERRORS=0

err() {
    echo "ERROR: $1" >&2
    ERRORS=$((ERRORS + 1))
}

echo "Validating plugin at: ${PLUGIN_DIR}"

# ── 1. Required files exist ─────────────────────────────────────────────────

for f in \
    ".claude-plugin/plugin.json" \
    ".mcp.json" \
    "hooks.json" \
    "README.md" \
    "scripts/ensure-binary.sh"
do
    if [[ ! -f "${PLUGIN_DIR}/${f}" ]]; then
        err "missing required file: ${f}"
    fi
done

# ── 2. JSON files are valid ─────────────────────────────────────────────────

for f in \
    ".claude-plugin/plugin.json" \
    ".mcp.json" \
    "hooks.json"
do
    path="${PLUGIN_DIR}/${f}"
    if [[ -f "$path" ]] && ! python3 -m json.tool "$path" > /dev/null 2>&1; then
        err "${f} is not valid JSON"
    fi
done

# ── 3. plugin.json JSON Schema validation ─────────────────────────────────────

MANIFEST="${PLUGIN_DIR}/.claude-plugin/plugin.json"
SCHEMA="${PLUGIN_DIR}/scripts/plugin.schema.json"

VALIDATE_PY='
import json, sys
from jsonschema import Draft202012Validator

with open(sys.argv[1]) as f:
    manifest = json.load(f)
with open(sys.argv[2]) as f:
    schema = json.load(f)

v = Draft202012Validator(schema)
errors = sorted(v.iter_errors(manifest), key=lambda e: list(e.path))
for e in errors:
    path = ".".join(str(p) for p in e.absolute_path) or "(root)"
    print(f"{path}: {e.message}")
'

validate_basic_fields() {
    for field in name version description; do
        if ! python3 -c "import json,sys; d=json.load(open(sys.argv[1])); assert d.get('$field')" "$MANIFEST" 2>/dev/null; then
            err "plugin.json missing required field: ${field}"
        fi
    done
}

if [[ -f "$MANIFEST" ]] && [[ -f "$SCHEMA" ]]; then
    ran_schema=false
    schema_errors=""

    # Try jsonschema via: uv (preferred) → pip-installed → basic fallback
    if command -v uv > /dev/null 2>&1; then
        schema_errors=$(uv run --with jsonschema python3 -c "$VALIDATE_PY" "$MANIFEST" "$SCHEMA" 2>&1)
        ran_schema=true
    elif python3 -c "import jsonschema" 2>/dev/null; then
        schema_errors=$(python3 -c "$VALIDATE_PY" "$MANIFEST" "$SCHEMA" 2>&1)
        ran_schema=true
    else
        echo "  WARN: neither uv nor jsonschema found, falling back to basic field checks"
        echo "        Install uv (https://docs.astral.sh/uv/) or: pip install jsonschema"
        validate_basic_fields
    fi

    if [[ "$ran_schema" == true ]]; then
        if [[ -n "$schema_errors" ]]; then
            while IFS= read -r line; do
                err "plugin.json schema: ${line}"
            done <<< "$schema_errors"
        else
            echo "  plugin.json passes JSON Schema validation"
        fi
    fi
elif [[ -f "$MANIFEST" ]]; then
    echo "  WARN: schema file not found at ${SCHEMA}, falling back to basic field checks"
    validate_basic_fields
fi

# ── 4. Paths referenced in plugin.json actually exist ───────────────────────

if [[ -f "$MANIFEST" ]]; then
    for ref in mcpServers hooks skills; do
        path_val=$(python3 -c "
import json, sys
d = json.load(open(sys.argv[1]))
v = d.get('$ref', '')
if isinstance(v, str) and v.startswith('./'):
    print(v)
" "$MANIFEST" 2>/dev/null)
        if [[ -n "$path_val" ]]; then
            resolved="${PLUGIN_DIR}/${path_val}"
            if [[ ! -e "$resolved" ]]; then
                err "plugin.json references ${ref}=${path_val} but ${resolved} does not exist"
            fi
        fi
    done
fi

# ── 5. .mcp.json references a valid command placeholder ────────────────────

MCP_JSON="${PLUGIN_DIR}/.mcp.json"
if [[ -f "$MCP_JSON" ]]; then
    if ! grep -q 'CLAUDE_PLUGIN_DATA\|CLAUDE_PLUGIN_ROOT' "$MCP_JSON"; then
        err ".mcp.json command does not use CLAUDE_PLUGIN_DATA or CLAUDE_PLUGIN_ROOT variable"
    fi
fi

# ── 6. Agent files have valid YAML frontmatter ──────────────────────────────

if [[ -d "${PLUGIN_DIR}/agents" ]]; then
    for agent_file in "${PLUGIN_DIR}"/agents/*.md; do
        [[ -f "$agent_file" ]] || continue
        basename_f="$(basename "$agent_file")"

        # Must start with ---
        if ! head -1 "$agent_file" | grep -q '^---$'; then
            err "agents/${basename_f}: missing YAML frontmatter delimiter"
            continue
        fi

        # Must have closing ---
        if ! tail -n +2 "$agent_file" | grep -q '^---$'; then
            err "agents/${basename_f}: missing closing YAML frontmatter delimiter"
            continue
        fi

        # Must have name and description
        frontmatter=$(sed -n '2,/^---$/p' "$agent_file" | sed '$ d')
        for field in name description; do
            if ! echo "$frontmatter" | grep -q "^${field}:"; then
                err "agents/${basename_f}: frontmatter missing required field '${field}'"
            fi
        done

        # tools field must be comma-separated string, not YAML list
        if echo "$frontmatter" | grep -q "^tools:$" || echo "$frontmatter" | grep -q "^  - "; then
            err "agents/${basename_f}: 'tools' must be a comma-separated string (e.g., 'tools: Read, Grep'), not a YAML list"
        fi
    done
fi

# ── 7. Skill files have valid YAML frontmatter ─────────────────────────────

if [[ -d "${PLUGIN_DIR}/skills" ]]; then
    find "${PLUGIN_DIR}/skills" -name "SKILL.md" | while read -r skill_file; do
        rel="${skill_file#"${PLUGIN_DIR}/"}"

        if ! head -1 "$skill_file" | grep -q '^---$'; then
            err "${rel}: missing YAML frontmatter delimiter"
            continue
        fi

        frontmatter=$(sed -n '2,/^---$/p' "$skill_file" | sed '$ d')
        if ! echo "$frontmatter" | grep -q "^description:"; then
            err "${rel}: frontmatter missing required field 'description'"
        fi
    done
fi

# ── 8. ensure-binary.sh is executable ───────────────────────────────────────

BUILD_SCRIPT="${PLUGIN_DIR}/scripts/ensure-binary.sh"
if [[ -f "$BUILD_SCRIPT" ]] && [[ ! -x "$BUILD_SCRIPT" ]]; then
    err "scripts/ensure-binary.sh is not executable"
fi

# ── Summary ─────────────────────────────────────────────────────────────────

echo ""
if [[ $ERRORS -eq 0 ]]; then
    echo "Plugin validation passed."
else
    echo "Plugin validation failed with ${ERRORS} error(s)."
    exit 1
fi
