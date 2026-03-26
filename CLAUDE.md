# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

diffsitter is an AST-based diff tool written in Rust. It uses tree-sitter to parse files into ASTs and computes semantically meaningful diffs that ignore formatting changes. Supports 14+ languages via tree-sitter grammars vendored as git submodules in `grammars/`.

## Prerequisites

- Rust toolchain (MSRV: 1.85.1, edition 2024)
- C99+ and C++14+ compiler (needed to compile tree-sitter grammars)
- Git submodules must be initialized: `git submodule update --init --recursive`

## Build Commands

```sh
cargo build                    # Build with static grammars (default)
cargo build --no-default-features --features dynamic-grammar-libs  # Dynamic grammar loading
cargo build --profile production  # Release build with LTO + strip
```

## Testing

```sh
cargo test --all               # Run all tests
cargo test --all-features      # Run with all features enabled
cargo insta review             # Review/update snapshot test changes (requires cargo-insta)
```

Nextest is configured (`.config/nextest.toml`) and used in CI. Snapshot tests use `insta` and may break when grammars are updated — review and accept with `cargo insta review`.

## Linting/Formatting

```sh
cargo fmt --all                # Format code
cargo clippy                   # Lint
pre-commit install             # Set up pre-commit hooks (formatting, TOML validation, etc.)
```

## Architecture

**Pipeline** (`src/bin/diffsitter.rs` → library modules):
1. CLI parsing (`cli.rs`, clap with derive) → config loading (`config.rs`, figment)
2. Language detection + tree-sitter parsing (`parse.rs`) — uses compile-time `phf` maps for extension→language lookup
3. AST node processing and filtering (`input_processing.rs`) — grapheme splitting, whitespace stripping, pseudo-leaf handling
4. Diff computation (`diff.rs`) — edit script algorithm producing hunks
5. Output rendering (`render/`) — trait via `enum_dispatch`, implementations: `unified.rs`, `json.rs`

**Library crate** (`libdiffsitter` in `src/lib.rs`): Exposes `generate_ast_vector_data()` as the main public API.

**Three binaries**: `diffsitter` (main), `diffsitter_completions` (shell completions), `diffsitter-utils`.

**Build script** (`build.rs`): Compiles tree-sitter C/C++ grammars in parallel (rayon), generates `phf` language maps. This is why builds require a C/C++ toolchain.

**Feature flags**:
- `static-grammar-libs` (default): Compiles grammars into the binary
- `dynamic-grammar-libs`: Loads grammars from system shared libraries at runtime
- `better-build-info`: Extended build metadata via shadow-rs

## Key Conventions

- Config changes must also update `assets/sample_config.json5` — CI parses it as a test.
- Grammar submodules have Rust bindings stripped so Cargo doesn't interfere with the custom build process.
- Grammar submodule updates are coordinated via the [diffsitter-grammars](https://github.com/afnanenayet/diffsitter-grammars) repo using nvchecker.
- Platform-specific config paths: `xdg` on Unix, `directories-next` on Windows.
