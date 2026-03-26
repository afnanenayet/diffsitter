---
name: add-renderer
description: "Step-by-step guide for adding a new diff output renderer to diffsitter. Use when adding a new output format."
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
user-invocable: true
argument-hint: "[name] Name of the new renderer (e.g., 'delta', 'html')"
---

# Adding a New Renderer to diffsitter

Follow this checklist to add a new diff output renderer named `$ARGUMENTS`. If the user did not provide a name, ask for one before proceeding.

## Prerequisites

Read these files first to understand the existing patterns:

- `src/render/mod.rs` -- trait definition, enum, config
- `src/render/json.rs` -- minimal renderer example
- `src/render/unified.rs` -- full-featured renderer example
- `src/config.rs` -- top-level config struct
- `assets/sample_config.json5` -- sample config (CI parses this as a test)

## Step-by-Step Checklist

### Step 1: Create the renderer module

Create `src/render/$ARGUMENTS.rs` with a struct that derives the required traits:

```rust
use super::DisplayData;
use crate::render::Renderer;
use console::Term;
use serde::{Deserialize, Serialize};
use std::io::Write;

/// A renderer that outputs diffs in $ARGUMENTS format.
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug, Default)]
pub struct $ARGUMENTS_PASCAL_CASE {
    // Add configuration fields here.
    // Each field should be serializable for the config system.
}

impl Renderer for $ARGUMENTS_PASCAL_CASE {
    fn render(
        &self,
        writer: &mut dyn Write,
        data: &DisplayData,
        term_info: Option<&Term>,
    ) -> anyhow::Result<()> {
        // Implementation goes here.
        // `data.hunks` contains `RichHunks` (Vec<RichHunk> where RichHunk = DocumentType<Hunk>)
        // `data.old` and `data.new` contain DocumentDiffData { filename, text }
        // `term_info` provides terminal dimensions if the output is a TTY
        todo!()
    }
}
```

**Key types available in `DisplayData`:**
- `data.hunks.0` -- `Vec<RichHunk<'a>>` where `RichHunk` is `DocumentType<Hunk>`
- Each `Hunk` contains `Vec<Line>`, each `Line` has `line_index: usize` and `entries: Vec<&Entry>`
- Each `Entry` has `text: Cow<str>`, `start_position: Point`, `end_position: Point`, `kind_id: u16`
- `DocumentType::Old(hunk)` / `DocumentType::New(hunk)` distinguishes old vs new document hunks

Use `src/render/json.rs` as a minimal reference (just serializes `DisplayData` to JSON). Use `src/render/unified.rs` for a full-featured example with terminal colors, hunk titles, and line-by-line rendering.

### Step 2: Register the module in `src/render/mod.rs`

Add the module declaration and use statement near the top:

```rust
mod $ARGUMENTS;
use self::$ARGUMENTS::$ARGUMENTS_PASCAL_CASE;
```

These go alongside the existing:
```rust
mod json;
mod unified;
use self::json::Json;
use unified::Unified;
```

### Step 3: Add a variant to the `Renderers` enum

Add your variant to the `Renderers` enum in `src/render/mod.rs`:

```rust
#[enum_dispatch]
#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize, Display, EnumIter, EnumString)]
#[strum(serialize_all = "snake_case")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Renderers {
    Unified,
    Json,
    $ARGUMENTS_PASCAL_CASE,  // <-- add this
}
```

The `enum_dispatch` attribute automatically generates the `Renderer` trait dispatch for the new variant. The `strum` and `serde` derives handle string conversion and serialization using the `snake_case` name.

### Step 4: Add a field to `RenderConfig`

In `src/render/mod.rs`, add a field to `RenderConfig`:

```rust
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case", default)]
pub struct RenderConfig {
    default: String,
    unified: unified::Unified,
    json: json::Json,
    $ARGUMENTS: $ARGUMENTS::$ARGUMENTS_PASCAL_CASE,  // <-- add this
}
```

Update the `Default` impl for `RenderConfig`:

```rust
impl Default for RenderConfig {
    fn default() -> Self {
        let default_renderer = Renderers::default();
        RenderConfig {
            default: default_renderer.to_string(),
            unified: Unified::default(),
            json: Json::default(),
            $ARGUMENTS: $ARGUMENTS_PASCAL_CASE::default(),  // <-- add this
        }
    }
}
```

### Step 5: Update `assets/sample_config.json5`

Add a section for the new renderer's configuration under the `"formatting"` key. CI parses this file as a test (`test_sample_config` in `src/config.rs`), so it must be valid.

### Step 6: Add tests

At minimum:

1. Add a `#[test_case("$ARGUMENTS")]` line to the `test_get_renderer_custom_tag` test in `src/render/mod.rs`:

```rust
#[test_case("unified")]
#[test_case("json")]
#[test_case("$ARGUMENTS")]  // <-- add this
fn test_get_renderer_custom_tag(tag: &str) {
```

2. Add unit tests in your renderer module for any non-trivial logic.

3. Consider adding snapshot tests with `insta` if the output format is complex.

### Step 7: Build and test

```sh
cargo build
cargo test --all
```

If you updated `sample_config.json5`, the `test_sample_config` test will verify it parses correctly.

## Common Pitfalls

- **Forgetting `Default` derive/impl**: The `RenderConfig` uses `#[serde(default)]`, so your struct must implement `Default`.
- **Case sensitivity**: The `Renderers` enum uses `snake_case` serialization via strum/serde. Your variant `MyRenderer` becomes `"my_renderer"` as a string tag.
- **The `writer` is generic**: Don't assume stdout. The renderer receives `&mut dyn Write` which could be a buffered terminal, a pager, or a file.
- **`term_info` may be `None`**: If the output is piped or redirected, there is no terminal. Handle gracefully (see how `unified.rs` handles missing terminal width).
