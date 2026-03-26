# tree-sitter-mcp

A Claude Code plugin that gives Claude structural understanding of code through tree-sitter AST navigation.

## What it does

Exposes 8 MCP tools that let Claude navigate code as an AST rather than flat text:

| Tool | Description |
|------|-------------|
| `parse_file` | Parse a file and get its top-level AST structure |
| `list_symbols` | List all functions, classes, structs, traits, etc. |
| `get_definition` | Get the full source text of a symbol by name |
| `get_children_of` | Get methods/fields inside a class or impl block |
| `get_node_at_position` | Get the deepest AST node at a line/column |
| `get_scope` | Get the enclosing scope with full parent chain |
| `navigate` | Move through the AST (parent, child, sibling) |
| `query` | Run raw tree-sitter S-expression queries |

Supports 14+ languages: Rust, Python, TypeScript, TSX, JavaScript, Go, Java, C, C++, C#, Ruby, Bash, PHP, OCaml, CSS, HCL.

## Usage

### As a plugin (includes skill + agent + auto-build hook)

Load the plugin for your Claude Code session:

```sh
claude --plugin-dir ./plugins/tree-sitter-mcp
```

The `SessionStart` hook automatically builds the `tree-sitter-mcp` binary on first use. This requires:

- Rust toolchain (1.85.1+)
- C/C++ compiler (for tree-sitter grammars)
- Git submodules initialized (`git submodule update --init --recursive`)

### As a standalone MCP server (no plugin needed)

If you have a pre-built `tree-sitter-mcp` binary, register it directly:

```sh
# Build
cargo build --release --features mcp-server --bin tree-sitter-mcp

# Register with Claude Code
claude mcp add tree-sitter-mcp -- ./target/release/tree-sitter-mcp
```

### Development

To rebuild the binary manually:

```sh
cargo build --release --features mcp-server --bin tree-sitter-mcp
```

Use `/reload-plugins` inside Claude Code to pick up plugin changes without restarting.
