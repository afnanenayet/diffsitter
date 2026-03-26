# Project Conventions

## Config Sync

Any change to `Config` (`src/config.rs`), `RenderConfig` (`src/render/mod.rs`), `GrammarConfig` (`src/parse.rs`), or `TreeSitterProcessor` (`src/input_processing.rs`) must also update `assets/sample_config.json5`. CI parses it as a test -- see `test_sample_config` in `src/config.rs` (line ~204), which loads the sample config and calls `Config::try_from_file`. A deserialization failure there breaks CI.

## Grammar Submodule Rules

Never add Rust bindings (`build.rs`, `Cargo.toml`, `lib.rs`) to grammar submodules under `grammars/`. The custom `build.rs` compiles grammars directly from C/C++ sources and Cargo-level Rust bindings would interfere.

To add a new grammar:

1. Add the submodule in `grammars/` (coordinate via the diffsitter-grammars repo)
2. Add a `GrammarCompileInfo` entry in the `grammars()` function in `build.rs` -- specify `display_name`, `path`, `c_sources`, and optionally `cpp_sources` and `include_paths`
3. Add extension-to-language mappings in the `FILE_EXTS` phf map in `src/parse.rs` (line ~46)

The build script compiles grammars in parallel using rayon and generates a `phf::Map` linking language names to their `unsafe extern "C" fn() -> Language` entry points.

## Test Patterns

This project uses several test frameworks. Use the right one for the situation:

- **`test_case`** for parameterized tests with inline values. See `src/diff.rs` line ~899 (`#[test_case(b"BAAA", b"CAAA" => 0)]`) and `src/render/mod.rs` line ~250.
- **`rstest` with `#[files(...)]`** for file-driven tests. See `src/config.rs` line ~218 (`#[files(r"resources/test_configs/*.json5")]`).
- **`mockall` with `#[cfg_attr(test, automock)]`** for mocking traits in unit tests. See `TSNodeTrait` in `src/input_processing.rs` line ~22.
- **`pretty_assertions`** for readable diff output on assertion failures. See `src/diff.rs` line ~794 (`use pretty_assertions::assert_eq as p_assert_eq`).
- **`insta`** for snapshot tests. See `tests/regression_test.rs` line ~3 (`use insta::assert_snapshot`). When grammar updates change snapshots, review and accept with `cargo insta review`.

Grammar-dependent integration tests (those that parse actual files) live in `tests/regression_test.rs` and require the `static-grammar-libs` feature to be enabled at compile time.

## Build Shortcuts

- `cargo check` -- fast feedback loop; skips grammar compilation entirely since it only checks Rust code
- `cargo build --no-default-features --features dynamic-grammar-libs` -- skips compiling C/C++ grammars; useful for iterating on Rust-only changes
- `cargo build` -- full build with static grammars (default); requires C/C++ toolchain and initialized git submodules
- `cargo build --profile production` -- release build with LTO and symbol stripping

## Edition 2024

This crate uses Rust edition 2024 with MSRV 1.85.1 (see `Cargo.toml`). Key implications:

- Use `unsafe extern "C"` blocks for FFI declarations (not bare `extern "C"`). See `build.rs` line ~408.
- `gen` is a reserved keyword -- do not use it as an identifier.
- Lifetime elision rules are stricter in some edge cases -- be explicit when the compiler requires it.
