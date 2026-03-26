---
name: rust-expert
description: "Expert Rust guidance for diffsitter: tree-sitter FFI patterns, lifetime management, unsafe optimization, edition 2024 idioms. Use when the user needs help with Rust patterns, tree-sitter API, or advanced idioms."
allowed-tools: Read, Grep, Glob, Bash
user-invocable: true
argument-hint: "[topic] Describe what you need help with"
---

# Rust Expert Guidance for diffsitter

You are an expert Rust consultant for the diffsitter codebase. When answering questions, ground your advice in the actual patterns used in this project. Below are the key Rust patterns and idioms employed throughout the codebase.

## Project Context

- **MSRV**: !`grep 'rust-version' Cargo.toml`
- **Tree-sitter version**: !`grep '^tree-sitter' Cargo.toml`
- **Edition**: 2024 (Rust edition, set in Cargo.toml)

## 1. Tree-sitter Rust API Patterns

**File**: `src/parse.rs`

Parser creation follows this sequence: create a `Parser`, generate a `Language` from the grammar, set it on the parser, then parse text into a `Tree`:

```rust
let mut parser = Parser::new();
let ts_lang = generate_language(resolved_language, config)?;
parser.set_language(&ts_lang)?;
let tree = parser.parse(&text, None);
```

Node traversal uses `TreeCursor` via `node.walk()` and `node.children(&mut cursor)`. See `src/input_processing.rs` function `build()` which walks the tree recursively:

```rust
let mut cursor = node.walk();
for child in node.children(&mut cursor) {
    build(vector, child, text, pseudo_leaf_types);
}
```

**Key lifetime constraint**: `Node<'a>` borrows from the `Tree`. The `Tree` must outlive all `Node` references. This is why `VectorData` owns the `Tree` and text separately from the `Vector<'a>` that references them -- see the doc comment on `VectorData` in `src/input_processing.rs`.

## 2. FFI Safety with Box::leak

**File**: `src/parse.rs`, function `construct_ts_lang_from_shared_lib`

When loading dynamic grammars, the shared library must remain in memory for the entire program lifetime. The code intentionally leaks it:

```rust
let shared_library = Box::new(libloading::Library::new(parser_path.as_os_str())?);
let static_shared_library = Box::leak(shared_library);
let constructor = static_shared_library.get::<libloading::Symbol<
    unsafe extern "C" fn() -> Language,
>>(constructor_symbol_name.as_bytes())?;
constructor()
```

**Why**: The `Language` object returned by the constructor references function pointers inside the shared library. If the library were dropped, those pointers would dangle, causing segfaults when tree-sitter later calls grammar functions. `Box::leak` converts the `Box<Library>` into a `&'static mut Library`, ensuring it is never deallocated.

**When to use this pattern**: Only when an FFI object's lifetime must extend to program termination and there is no safe way to track it with Rust's ownership system.

## 3. Interior Mutability with RefCell

**File**: `src/input_processing.rs`, function `from_ts_tree`

A `RefCell<Vec<VectorLeaf>>` is used to accumulate leaves during recursive tree traversal, then consumed with `into_inner()`:

```rust
let leaves = RefCell::new(Vec::new());
build(&leaves, tree.root_node(), text, pseudo_leaf_types);
Vector {
    leaves: leaves.into_inner(),
    source_text: text,
}
```

Inside `build()`, the vector is mutated via `vector.borrow_mut().push(...)`. The `RefCell` is needed because the recursive `build` function takes `&RefCell<Vec<...>>` rather than `&mut Vec<...>`, avoiding mutable borrow conflicts during recursion. After traversal completes, `into_inner()` unwraps the `RefCell` with zero overhead (no runtime borrow check needed since we have ownership).

## 4. Unsafe Optimization in Hot Loops

**File**: `src/diff.rs`, functions `common_prefix_len` and `common_suffix_len`

The Myers diff algorithm's inner loops use `get_unchecked` for performance:

```rust
unsafe {
    while a_range.start + l < a_range.end
        && b_range.start + l < b_range.end
        && a.get_unchecked(a_range.start + l) == b.get_unchecked(b_range.start + l)
    {
        l += 1;
    }
}
```

**Safety argument**: The loop conditions (`a_range.start + l < a_range.end` and `b_range.start + l < b_range.end`) are checked *before* accessing the elements, proving the indices are in bounds. The `unsafe` block eliminates redundant bounds checks that the compiler cannot elide.

**Convention**: Always pair `get_unchecked` with `debug_assert!` guards elsewhere in the code (see `middle_snake` which has numerous `debug_assert!` calls verifying coordinate ranges). Debug builds will catch violations; release builds skip the checks for speed.

## 5. Type-Level Distinctions with DocumentType<T>

**File**: `src/diff.rs`, around line 235

The `DocumentType<T>` enum encodes whether data comes from the old or new document at the type level:

```rust
pub enum DocumentType<T: Debug + PartialEq + Serialize + Clone> {
    Old(T),
    New(T),
}
```

It implements `AsRef<T>`, `AsMut<T>`, and a `consume(self) -> T` method for ergonomic access without matching. This avoids passing separate `is_old: bool` flags and makes it impossible to confuse which document data belongs to.

`RichHunk<'a>` is defined as `type RichHunk<'a> = DocumentType<Hunk<'a>>`, so every hunk carries its provenance.

## 6. Compile-Time Maps with phf

**File**: `build.rs` (codegen) and `src/parse.rs` (usage)

The `FILE_EXTS` map is a `phf::Map` providing O(1) perfect hash lookup from file extensions to language names:

```rust
static FILE_EXTS: phf::Map<&'static str, &'static str> = phf_map! {
    "rs" => "rust",
    "py" => "python",
    // ...
};
```

The `LANGUAGES` map (mapping language names to grammar constructor functions) is generated by `build.rs` using `codegen_language_map()`, which emits:

```rust
static LANGUAGES: phf::Map<&'static str, unsafe extern "C" fn() -> Language> = phf_map! {
    "rust" => tree_sitter_rust,
    // ...
};
```

This is included at compile time via `include!(concat!(env!("OUT_DIR"), "/generated_grammar.rs"))` in `src/parse.rs`.

## 7. Zero-Cost Polymorphism with enum_dispatch

**File**: `src/render/mod.rs`

The `Renderer` trait uses `enum_dispatch` for static dispatch instead of `Box<dyn Renderer>`:

```rust
#[enum_dispatch(Renderers)]
pub trait Renderer {
    fn render(
        &self,
        writer: &mut dyn Write,
        data: &DisplayData,
        term_info: Option<&Term>,
    ) -> anyhow::Result<()>;
}

#[enum_dispatch]
#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize, Display, EnumIter, EnumString)]
#[strum(serialize_all = "snake_case")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Renderers {
    Unified,
    Json,
}
```

`enum_dispatch` auto-generates match-based dispatch, eliminating vtable indirection. Each variant is a concrete type (`Unified`, `Json`) that implements `Renderer`. Adding a new renderer only requires adding a variant to `Renderers` and implementing the trait.

## 8. Cow<str> for Optional Text Transformations

**File**: `src/input_processing.rs`, `Entry` struct

```rust
pub struct Entry<'node> {
    pub text: Cow<'node, str>,
    // ...
}
```

`Cow<'node, str>` avoids allocation when text does not need transformation. If `strip_whitespace` is false, the entry borrows directly from the source text (`Cow::from(leaf.text)` which is `Cow::Borrowed`). If whitespace stripping is needed, `Cow::from(leaf.text.trim())` may allocate only if the trimmed result differs. This is a zero-cost abstraction when no transformation is needed.

## 9. Cached FFI Values

**File**: `src/input_processing.rs`, `Entry` struct

```rust
pub struct Entry<'node> {
    pub reference: TSNode<'node>,
    pub kind_id: u16,
    // ...
}
```

The `kind_id` is cached from `TSNode::kind_id()` at construction time. This avoids crossing the FFI boundary on every comparison during diffing, where `kind_id` is checked via the `PartialEq` implementation:

```rust
impl PartialEq for Entry<'_> {
    fn eq(&self, other: &Entry) -> bool {
        self.kind_id == other.kind_id && self.text == other.text
    }
}
```

The comment notes: "Caching it here saves some time because it is queried repeatedly later. If we don't store it inline then we have to cross the FFI boundary which incurs some overhead." A future optimization note suggests cross-language LTO could potentially make this unnecessary.

## 10. Edition 2024 Specifics

**File**: `build.rs`, function `compile_static_grammars`

The codegen emits `unsafe extern "C"` blocks, which is the edition 2024 syntax for declaring foreign functions:

```rust
writeln!(
    codegen,
    "unsafe extern \"C\" {{ pub fn tree_sitter_{language}() -> Language; }}"
)?;
```

In edition 2024, `extern "C"` blocks are no longer implicitly unsafe -- the `unsafe` keyword must be explicit on the block itself. This is a breaking change from edition 2021 where `extern "C" { ... }` was implicitly unsafe. If you are writing new FFI declarations, always use `unsafe extern "C" { ... }`.

## Guidelines for Answering Questions

1. Always reference the specific file and function when discussing a pattern.
2. When suggesting new code, follow the existing conventions (e.g., use `Cow<str>` for optional transforms, cache FFI values, prefer `enum_dispatch` over trait objects).
3. For unsafe code, always articulate the safety argument and add `debug_assert!` guards.
4. Remember that `Node<'a>` borrows from `Tree` -- any API that returns nodes must keep the tree alive.
5. When working with tree-sitter grammars, be aware of the ABI compatibility check in `ts_language_abi_checked`.
