---
name: add-grammar
description: "Step-by-step guide for adding a new tree-sitter language grammar to diffsitter. Use when adding support for a new programming language."
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
user-invocable: true
argument-hint: "[language] Language name (e.g., 'yaml', 'lua')"
---

# Adding a New Tree-sitter Grammar to diffsitter

Follow this checklist to add support for a new language `$ARGUMENTS`. If the user did not provide a language name, ask for one before proceeding.

## Prerequisites

Read these files first:

- `build.rs` -- `grammars()` function and `GrammarCompileInfo` struct
- `src/parse.rs` -- `FILE_EXTS` map and language resolution

## Step-by-Step Checklist

### Step 1: Add the grammar as a git submodule

```sh
git submodule add https://github.com/tree-sitter/tree-sitter-$ARGUMENTS grammars/tree-sitter-$ARGUMENTS
git submodule update --init --recursive
```

Verify the submodule has a `src/` directory containing at minimum `parser.c`. Check for a `scanner.c` or `scanner.cc` file -- this determines whether you need C or C++ compilation.

### Step 2: Strip Rust bindings from the grammar repo

Tree-sitter grammar repos typically include Rust bindings that would interfere with diffsitter's custom build process. Remove them:

```sh
cd grammars/tree-sitter-$ARGUMENTS
rm -rf bindings/rust Cargo.toml build.rs
cd ../..
```

This is critical -- if `Cargo.toml` or `build.rs` remain in the grammar directory, Cargo may try to compile the grammar as a separate crate and conflict with diffsitter's build script.

### Step 3: Add `GrammarCompileInfo` to `build.rs`

In `build.rs`, add a new entry to the `grammars()` function's vector. The struct has these fields:

```rust
GrammarCompileInfo {
    /// The language name -- must match what you use in FILE_EXTS and the
    /// tree_sitter_$ARGUMENTS() constructor function name
    display_name: "$ARGUMENTS",
    /// Path to the grammar root (contains src/ directory)
    path: PathBuf::from("grammars/tree-sitter-$ARGUMENTS"),
    /// C source files in src/ to compile
    c_sources: vec!["parser.c"],           // always include parser.c
    /// C++ source files in src/ to compile (empty if no C++ scanner)
    cpp_sources: vec![],
    ..Default::default()
}
```

**Determining sources:**

- `parser.c` is always present and always compiled as C.
- If `src/scanner.c` exists, add `"scanner.c"` to `c_sources`.
- If `src/scanner.cc` exists, add `"scanner.cc"` to `cpp_sources`. Do NOT put `.cc` files in `c_sources`.
- Some grammars have no scanner at all (e.g., `json`, `go`, `java`, `c`).

**Examples from the codebase:**

C-only (parser + C scanner):
```rust
GrammarCompileInfo {
    display_name: "rust",
    path: PathBuf::from("grammars/tree-sitter-rust"),
    c_sources: vec!["parser.c", "scanner.c"],
    ..Default::default()
}
```

C + C++ scanner:
```rust
GrammarCompileInfo {
    display_name: "ruby",
    path: PathBuf::from("grammars/tree-sitter-ruby"),
    c_sources: vec!["parser.c"],
    cpp_sources: vec!["scanner.cc"],
    ..GrammarCompileInfo::default()
}
```

Parser only (no scanner):
```rust
GrammarCompileInfo {
    display_name: "json",
    path: PathBuf::from("grammars/tree-sitter-json"),
    c_sources: vec!["parser.c"],
    ..Default::default()
}
```

### Step 4: Add file extension mappings to `src/parse.rs`

Add entries to the `FILE_EXTS` static `phf_map!` in `src/parse.rs`. The key is the file extension (without dot), the value is the `display_name` from Step 3:

```rust
static FILE_EXTS: phf::Map<&'static str, &'static str> = phf_map! {
    // ... existing entries ...
    "$EXT" => "$ARGUMENTS",
};
```

Add all common extensions for the language. For example, C++ has `"cc"`, `"cpp"`, `"hpp"`, and `"tpp"`.

### Step 5: Build and verify

```sh
cargo build
```

The build script will:
1. Compile the grammar's C/C++ sources via the `cc` crate
2. Generate an `unsafe extern "C" { pub fn tree_sitter_$ARGUMENTS() -> Language; }` declaration
3. Add the language to the generated `LANGUAGES` phf_map

If the build fails, check:
- Are the source file paths correct? The build script prepends `{path}/src/` to each filename.
- Does the grammar's `src/` directory exist? Run `ls grammars/tree-sitter-$ARGUMENTS/src/`.
- Are git submodules initialized? Run `git submodule update --init --recursive`.

### Step 6: Add test data (optional but recommended)

If you want to add integration or snapshot tests, add test input files under `resources/` and write tests that parse them.

Run the full test suite:

```sh
cargo test --all
```

The `static_load_parsers` test in `src/parse.rs` will automatically test that the new grammar can be loaded by tree-sitter, since it iterates over all entries in the `LANGUAGES` map.

## Common Pitfalls

### Monorepo grammars

Some tree-sitter grammars contain multiple languages in one repository. In this case, the `path` field must point to the subdirectory containing the `src/` folder, not the repo root.

Examples from the codebase:

- **TypeScript**: The `tree-sitter-typescript` repo has `typescript/` and `tsx/` subdirectories, each with their own `src/`:
  ```rust
  GrammarCompileInfo {
      display_name: "typescript",
      path: PathBuf::from("grammars/tree-sitter-typescript/typescript"),
      // ...
  }
  GrammarCompileInfo {
      display_name: "tsx",
      path: PathBuf::from("grammars/tree-sitter-typescript/tsx"),
      // ...
  }
  ```

- **OCaml**: `tree-sitter-ocaml` has `grammars/ocaml/` subdirectory:
  ```rust
  path: PathBuf::from("grammars/tree-sitter-ocaml/grammars/ocaml"),
  ```

- **PHP**: `tree-sitter-php` has a `php/` subdirectory:
  ```rust
  path: PathBuf::from("grammars/tree-sitter-php/php"),
  ```

- **Markdown**: `tree-sitter-markdown` has a `tree-sitter-markdown/` subdirectory:
  ```rust
  path: PathBuf::from("grammars/tree-sitter-markdown/tree-sitter-markdown"),
  ```

### C++ scanner pitfalls

If the grammar has a C++ scanner (`scanner.cc`), it must go in `cpp_sources`, not `c_sources`. The build script compiles C and C++ sources with different compilers (`cc` vs `c++`) and links them into separate static libraries.

### Non-standard include paths

If the grammar's headers are not in `src/` (the default include path), use the `include_paths` field:

```rust
GrammarCompileInfo {
    display_name: "some_lang",
    path: PathBuf::from("grammars/tree-sitter-some-lang"),
    c_sources: vec!["parser.c"],
    include_paths: Some(vec![
        PathBuf::from("grammars/tree-sitter-some-lang/include"),
    ]),
    ..Default::default()
}
```

### display_name must match the constructor function

The `display_name` is used to generate the FFI symbol name `tree_sitter_{display_name}`. This must match the actual symbol exported by the compiled grammar. For languages with hyphens, use underscores in the display name (e.g., `c_sharp` for `tree-sitter-c-sharp`).

### Grammar submodule coordination

Grammar submodule updates are coordinated via the [diffsitter-grammars](https://github.com/afnanenayet/diffsitter-grammars) repository using nvchecker. For ongoing maintenance, consider adding the grammar there too.

### ABI compatibility

The grammar must have a compatible tree-sitter ABI version. The `ts_language_abi_checked` function in `src/parse.rs` verifies this at runtime, checking that the grammar's ABI version falls within `MIN_COMPATIBLE_LANGUAGE_VERSION..=LANGUAGE_VERSION`. If you see an `AbiOutOfRange` error, the grammar may need to be rebuilt with a compatible tree-sitter version.
