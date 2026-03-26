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

## Installation

### From the diffsitter repo

If you've cloned diffsitter, the plugin is already at `plugins/tree-sitter-mcp/`:

```sh
claude plugin install --scope user ./plugins/tree-sitter-mcp
```

The plugin automatically builds the `tree-sitter-mcp` binary on first session start. This requires:

- Rust toolchain (1.85.1+)
- C/C++ compiler (for tree-sitter grammars)
- Git submodules initialized (`git submodule update --init --recursive`)

### From a pre-built binary

If you have a pre-built `tree-sitter-mcp` binary, you can configure it directly in your Claude Code settings without the plugin:

```sh
# Add to project settings
claude mcp add tree-sitter-mcp -- /path/to/tree-sitter-mcp

# Or add to user settings
claude mcp add --scope user tree-sitter-mcp -- /path/to/tree-sitter-mcp
```

## Development

To test the plugin locally during development:

```sh
claude --plugin-dir ./plugins/tree-sitter-mcp
```

To rebuild the binary manually:

```sh
cargo build --release --features mcp-server --bin tree-sitter-mcp
```
