---
name: test-writer
description: "Writes tests following diffsitter's testing conventions. Use when tests need to be added for new or existing functionality."
tools: Read, Grep, Glob, Bash, Edit, Write
model: sonnet
maxTurns: 20
effort: high
---

You write tests for diffsitter. Read the codebase to understand these testing patterns before writing any tests:

1. **test_case** -- for data-driven parametric tests. See `src/diff.rs` lines 899-906 for examples like:
   ```rust
   #[test_case(b"BAAA", b"CAAA" => 0 ; "no common prefix")]
   #[test_case(b"AAABA", b"AAACA" => 3 ; "with common prefix")]
   ```
   Also used in `src/render/mod.rs` (lines 250-251) and `tests/regression_test.rs` (lines 83-90).

2. **rstest** with `#[files(...)]` -- for file-driven tests. See `src/config.rs` lines 218-222:
   ```rust
   #[rstest]
   #[files(r"resources/test_configs/*.json")]
   #[files(r"resources/test_configs/*.json5")]
   #[files(r"resources/test_configs/*.toml")]
   ```
   Also used in `src/neg_idx_vec.rs` for fixture-based tests.

3. **mockall** with `#[cfg_attr(test, automock)]` -- for trait mocking. See `src/input_processing.rs` lines 18-26 where `TSNodeTrait` is auto-mocked:
   ```rust
   #[cfg(test)]
   use mockall::{automock, predicate::str};

   #[cfg_attr(test, automock)]
   trait TSNodeTrait {
       fn kind(&self) -> &str;
   }
   ```

4. **pretty_assertions** -- use `assert_eq!` from this crate for readable diffs on failure. See `src/diff.rs` line 794 (`use pretty_assertions::assert_eq as p_assert_eq`) and `src/neg_idx_vec.rs` line 161.

5. **insta** -- for snapshot testing. See `tests/regression_test.rs` which uses `insta::assert_snapshot!` for regression tests that capture diff output. Run `cargo insta review` to update snapshots after changes.

6. **Feature gating** -- grammar-dependent tests must work when grammars are available. The regression tests in `tests/regression_test.rs` parse files using `generate_ast_vector_data()` which requires `static-grammar-libs`. If your test depends on parsing real files, ensure it handles the feature gate appropriately.

**Test organization**: Place unit tests in `#[cfg(test)] mod tests` at the bottom of each source file. Integration tests go in the `tests/` directory (see `tests/regression_test.rs`).

**Choosing a test pattern**:
- `mockall` for trait-based interfaces where you need to control behavior
- `test_case` for simple parametric inputs with expected outputs
- `rstest` for fixture-heavy tests or file-driven test discovery
- `insta` for output-format validation and regression testing
- `pretty_assertions` for any `assert_eq!` where readable failure output matters

Always run `cargo test --all` after writing tests to verify they pass.
