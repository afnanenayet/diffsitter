---
paths:
  - "src/**/*.rs"
---

# Code Review Standards

## Naming

Follow RFC 430: PascalCase for types/traits/enum variants, snake_case for functions/methods/variables, SCREAMING_SNAKE_CASE for constants and statics.

Enum variants that wrap values use descriptive nouns, not verbs:

- `DocumentType::Old(T)`, `DocumentType::New(T)` in `src/diff.rs` line ~240
- `EditType::Addition(T)`, `EditType::Deletion(T)` in `src/input_processing.rs` line ~504
- `LoadingError::UnsupportedExt(String)`, `LoadingError::TSParseFailure(PathBuf)` in `src/parse.rs`

Error enum variants with multiple fields use named fields (not positional) for clarity. See `HunkInsertionError::NonAdjacentHunk { incoming_line, last_line }` in `src/diff.rs` line ~112.

## Imports

Group imports in this order, separated by blank lines when readability benefits:

1. `std` crate
2. External crates (e.g., `anyhow`, `serde`, `tree_sitter`, `thiserror`)
3. `crate::` internal modules
4. `super::` (in submodules/tests)

Reference: `src/diff.rs` lines 4-12 shows `crate::` imports first then external then `std` -- the grouping exists but ordering varies slightly. The key rule is: no glob imports (`use foo::*`). Always import specific items.

## Clone/Copy Discipline

Prefer borrowing over cloning. When wrapping `Copy` types like `TSNode`, make the wrapper `Copy` too. See `VectorLeaf<'a>` in `src/input_processing.rs` line ~180: it is `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` because it only holds a `TSNode<'a>` (which is `Copy`) and `&'a str`.

Use `Cow<'a, str>` for text that may or may not need transformation. See `Entry.text: Cow<'node, str>` in `src/input_processing.rs` line ~218 -- most entries borrow the original text directly, but `process_leaf` and `split_on_graphemes` may produce owned variants when stripping whitespace.

## Builder Patterns

Builders consume `self` in their finalizer method -- use `build(self)`, not `build(&self)`. The builder must be marked with `#[must_use]` on `new()` and `build()`.

Reference: `RichHunksBuilder` in `src/diff.rs` line ~291:

```rust
#[must_use]
pub fn new() -> Self { ... }

#[must_use]
pub fn build(self) -> RichHunks<'a> {
    self.hunks
}
```

## Polymorphism

Prefer `enum_dispatch` for closed sets of implementations over `dyn Trait`. This gives static dispatch with enum ergonomics.

Reference: `Renderers` enum and `Renderer` trait in `src/render/mod.rs`:

```rust
#[enum_dispatch]
pub enum Renderers {
    Unified,
    Json,
}

#[enum_dispatch(Renderers)]
pub trait Renderer {
    fn render(&self, writer: &mut dyn Write, data: &DisplayData, term_info: Option<&Term>) -> anyhow::Result<()>;
}
```

When adding a new renderer, add its variant to the `Renderers` enum and implement the `Renderer` trait on the struct -- `enum_dispatch` handles the rest.

## Dependencies

Never introduce new dependencies without justification. Before adding a crate, check whether an existing dependency already solves the problem. The project already includes:

- `thiserror` / `anyhow` for errors
- `serde` + `figment` for config
- `clap` (derive) for CLI
- `console` + `strum` for terminal/enum utilities
- `phf` for compile-time maps
- `test_case`, `rstest`, `mockall`, `pretty_assertions`, `insta`, `proptest`, `criterion`, `tempfile` for testing

If an existing dep covers the need, use it rather than adding a new one.
