---
description: Use tree-sitter MCP tools for AST-aware code navigation — finding symbols, jumping to definitions, inspecting scopes, and running structural queries across 14+ languages
---

# AST Navigation with tree-sitter MCP

You have access to tree-sitter MCP tools that let you navigate code structurally rather than as flat text. Use these tools when you need precise, language-aware understanding of code structure.

## Available Tools

### `parse_file`
Parse a file and get its top-level AST structure. Returns the root node with its children's kinds and spans.

**When to use:** Starting point for exploring an unfamiliar file. Shows you what top-level constructs exist (functions, classes, imports, etc.).

```
file_path: "/path/to/file.rs"
language: "rust"  # optional — auto-detected from extension
```

### `list_symbols`
List all top-level symbols (functions, structs, classes, traits, enums, constants) with their names, kinds, and locations.

**When to use:** Getting an overview of what a file defines. Much faster than reading the whole file when you just need to know what's there.

```
file_path: "/path/to/file.rs"
```

### `get_definition`
Get the full source text of a specific symbol by name.

**When to use:** Reading one specific function/struct/class without loading the entire file.

```
file_path: "/path/to/file.rs"
symbol_name: "MyStruct"
```

### `get_children_of`
Get child definitions within a named scope (methods inside a class/impl, fields in a struct).

**When to use:** Exploring the contents of a class, impl block, or module.

```
file_path: "/path/to/file.rs"
symbol_name: "MyStruct"
```

### `get_node_at_position`
Get the deepest AST node at a specific line/column position.

**When to use:** Understanding what syntax construct exists at a specific location (e.g., when investigating a compiler error at a given line).

```
file_path: "/path/to/file.rs"
line: 42
column: 8
```

### `get_scope`
Get the enclosing scope (function, class, module) at a position, with the full parent chain.

**When to use:** Understanding the nesting context at a specific location — what function am I in? What class is that function in?

```
file_path: "/path/to/file.rs"
line: 42
column: 8
```

### `navigate`
Move from a position in a direction: `parent`, `first_child`, `next_sibling`, `prev_sibling`, `next_named_sibling`, `prev_named_sibling`.

**When to use:** Traversing the AST step by step from a known position. Useful for exploring neighboring constructs.

```
file_path: "/path/to/file.rs"
line: 42
column: 8
direction: "parent"
```

### `query`
Run a raw tree-sitter S-expression query and get all matches with captures.

**When to use:** Complex structural searches that the other tools don't cover. For example, finding all functions that take a specific parameter type, or all assignments to a variable.

```
file_path: "/path/to/file.rs"
query: "(function_item name: (identifier) @name parameters: (parameters (parameter type: (type_identifier) @type))) @fn"
```

## Supported Languages

Rust, Python, TypeScript, TSX, JavaScript, Go, Java, C, C++, C#, Ruby, Bash, PHP, OCaml, CSS, HCL.

## Usage Patterns

**Exploring an unfamiliar codebase:**
1. `list_symbols` on key files to understand structure
2. `get_definition` for specific symbols you need to understand
3. `get_children_of` to drill into classes/modules

**Investigating a specific location:**
1. `get_node_at_position` to see what's at a line/column
2. `get_scope` to understand the nesting context
3. `navigate` with `parent` to walk up the tree

**Structural search:**
1. `query` with S-expressions for pattern matching across the AST
