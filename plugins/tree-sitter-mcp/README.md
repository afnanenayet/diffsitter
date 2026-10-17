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

### Researching dependency APIs alongside local code

AST navigation shows how your code uses a library. To look up that library's
documentation in the same Claude Code session, optionally add
[Parallel Search MCP](https://docs.parallel.ai/integrations/mcp/search-mcp).
It provides `web_search` and `web_fetch` for free, with no account or API key
required. Anonymous searches use Fast mode and have rate limits.

From the project directory, register the companion server:

```sh
claude mcp add --transport http --scope local parallel-search https://search.parallel.ai/mcp \
  --header "User-Agent: diffsitter/tree-sitter-mcp"
```

The local scope keeps this registration specific to your project and user.
Load tree-sitter-mcp using either setup above, then run `/mcp` in Claude Code
to check that both servers are connected. The companion is optional; AST
navigation continues to work without it.

For example, from a diffsitter checkout, ask:

> Use tree-sitter-mcp's `get_definition` to inspect `run_query` in
> `src/ast_navigation.rs`. Check the tree-sitter version in `Cargo.lock`, then
> use Parallel's `web_search` and `web_fetch` to find the corresponding query
> API documentation. Explain how captures are returned, citing local file
> paths and line numbers as well as documentation URLs. Use only public library
> names, versions and API names in web requests; keep local source code local.

Match documentation to the installed dependency version before suggesting an
API change. Local definitions and fetched documentation provide complementary
context; the AST tools do not resolve definitions in external libraries.

To remove the companion registration:

```sh
claude mcp remove --scope local parallel-search
```

### Development

To rebuild the binary manually:

```sh
cargo build --release --features mcp-server --bin tree-sitter-mcp
```

Use `/reload-plugins` inside Claude Code to pick up plugin changes without restarting.
