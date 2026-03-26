# Rust Quality Standards

## Error Handling

Use `thiserror` with `#[derive(Error)]` for all error types in library code (`src/`). Each variant must have a `#[error("...")]` message with interpolated context describing what failed. Use `#[from]` to wrap upstream errors.

Reference pattern -- `LoadingError` in `src/parse.rs`:

```rust
#[derive(Error, Debug)]
pub enum LoadingError {
    #[error("Unsupported extension: {0}")]
    UnsupportedExt(String),

    #[error("tree-sitter had an error")]
    LanguageError(#[from] tree_sitter::LanguageError),
}
```

Also see `ReadError` in `src/config.rs`, `HunkInsertionError` in `src/diff.rs`, and `CompileParamError` in `build.rs` for the same pattern with named fields in variants.

Use `anyhow::Result` only in binary crates (`src/bin/`). The library currently uses `anyhow::Result` in a few places (e.g., `generate_ast_vector_data` in `src/lib.rs`, `Renderer::render`) -- these are acknowledged tech debt, not a pattern to follow. New library code must use typed errors.

Never `.unwrap()` in library code unless the invariant is proven by preceding logic and guarded by `debug_assert!`. See `src/input_processing.rs` `split_on_graphemes` where `debug_assert!(!grapheme.is_empty())` precedes operations that depend on that invariant.

## Lifetime Patterns

When tree-sitter nodes reference borrowed text, use the VectorData ownership pattern: an owning struct holds `String` + `Tree`, and borrowing structs reference it.

- `VectorData` in `src/input_processing.rs` (line ~374) owns `text: String` and `tree: TSTree`
- `Vector<'a>` (line ~357) borrows with `leaves: Vec<VectorLeaf<'a>>` and `source_text: &'a str`
- `Entry<'a>` (line ~202) borrows with `reference: TSNode<'a>` and `text: Cow<'a, str>`

This split exists because tree-sitter uses FFI and self-referential structs are not feasible. New code that needs to hold parsed AST data must follow this same owned-data / borrowed-view split.

## Unsafe Discipline

Every `unsafe` block must have preceding logic that proves the invariant. Use `debug_assert!` guards before the unsafe block or within `#[cfg(debug_assertions)]` blocks.

Reference pattern -- `common_prefix_len` in `src/diff.rs` (lines 23-32): the `while` condition inside the `unsafe` block performs bounds checking before each `get_unchecked` call. The same applies to `common_suffix_len` (lines 72-81).

Reference pattern -- `src/input_processing.rs` `split_on_graphemes` (line ~294): a `#[cfg(debug_assertions)]` block with `debug_assert!` validates ordering invariants before pushing entries.

For FFI declarations, use `unsafe extern "C"` blocks (edition 2024 syntax) as seen in `build.rs` line ~408 and `src/parse.rs` line ~226.

## Serde Conventions

Config structs use `#[serde(rename_all = "kebab-case")]`. Optional config sections use `#[serde(default)]` so missing keys fall back to defaults.

- `Config` in `src/config.rs`: `#[serde(rename_all = "kebab-case", default)]`
- `TreeSitterProcessor` in `src/input_processing.rs`: `#[serde(rename_all = "kebab-case", default)]`
- `GrammarConfig` in `src/parse.rs`: `#[serde(rename_all = "kebab-case")]`
- `RenderConfig` in `src/render/mod.rs`: `#[serde(rename_all = "snake_case", default)]` (renderers use snake_case, not kebab-case)

Enum tags for renderer variants use `#[serde(tag = "type", rename_all = "snake_case")]` (see `Renderers` in `src/render/mod.rs`).

## Feature Gating

Use `#[cfg(feature = "...")]` on entire `use` blocks, struct definitions, and `impl` blocks -- not on individual lines within a function. Keep the `static-grammar-libs` / `dynamic-grammar-libs` boundary clean.

Reference pattern -- `build.rs` gates all grammar-related structs and functions behind `#[cfg(feature = "static-grammar-libs")]` at the item level. `src/parse.rs` gates `include!`, `lazy_static!`, and the `SUPPORTED_LANGUAGES` static behind the same feature.

Never mix both grammar features in the same compilation unit. They are mutually exclusive by design.
