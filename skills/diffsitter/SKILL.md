---
name: diffsitter
description: >
  diffsitter generates semantically meaningful diffs instead of raw text, helping ignore formatting-only changes such as spacing.
  Tree-sitter parsers for Bash, C#, C++, CSS, Go, Java, OCaml, PHP, Python, Ruby, Rust, Typescript/TSX, and HCL programming languages.
  Use this skill for AST-based (abstract syntax tree) code diffing and structural change review.
license: MIT
metadata:
  author: Damien Berezenko
---

# diffsitter

`diffsitter` computes diffs on a tree-sitter AST instead of raw text, so it highlights semantic changes while suppressing formatting-only churn. It is most useful when a formatter, refactor, or code generator makes line-based diffs noisy and you need a review signal that tracks actual code structure.

## Supported languages

Bash, C#, C++, CSS, Go, Java, OCaml, PHP, Python, Ruby, Rust, Typescript/TSX, and HCL.

## Example: AST diff removes formatting noise

Take the following files:

[`a.rs`](test_data/short/rust/a.rs)

```rust
fn main() {
    let x = 1;
}

fn add_one {
}
```

[`b.rs`](test_data/short/rust/b.rs)

```rust
fn



main

()

{
}

fn addition() {
}

fn add_two() {
}
```

A traditional `diff` reports the reformatting in `main` as a large change:

```text
1,2c1,12
< fn main() {
<     let x = 1;
---
> fn
>
>
>
> main
>
> ()
>
> {
> }
>
> fn addition() {
5c15
< fn add_one {
---
> fn add_two() {
```

`diffsitter` instead suppresses the layout-only rewrite and shows the semantic edits:

```
test_data/short/rust/a.rs -> test_data/short/rust/b.rs
======================================================

9:
--
+ }

11:
---
+ fn addition() {

1:
--
-     let x = 1;

14:
---
+ fn add_two() {

4:
--
- fn add_one {
```

*Note: the numbers correspond to line numbers from the original files.*

The key review benefit is that `main` disappears from the AST-based diff because only its formatting changed.

## Practical development workflows

- Review formatter-heavy or refactor-heavy changes with `diffsitter old_file.py new_file.py` when a raw diff is dominated by whitespace or line wrapping.
- Reproduce a project-specific setup with `diffsitter --config path/to/config.json5 old.ts new.ts` or by setting `DIFFSITTER_CONFIG`.
- Isolate config effects with `diffsitter --no-config old.rs new.rs`.
- Debug parsing or unexpected output with `diffsitter --debug old.go new.go`; `--debug` (`-d`) enables detailed logs for parser behavior, timing, and config handling.

## CLI options worth knowing

- `--config <PATH>`: use a specific config file.
- `--no-config`: ignore config files and use built-in defaults.
- `--debug`, `-d`: enable verbose debug/trace logging.
- `--color <auto|on|off>`: control color output for terminals and CI logs.
- `dump-default-config`: print the default JSON5 config.
- `list`: show which languages this build supports.
- `gen-completion <bash|zsh|fish|elvish|powershell>`: generate shell completion scripts.
- `--help`, `-h`: show the current CLI help text.

On macOS and Linux, diffsitter looks for a config at `${XDG_CONFIG_HOME:-$HOME}/.config/diffsitter/config.json5` by default. `--config` and `DIFFSITTER_CONFIG` override that location.

## Optional: filter noisy nodes

Node filtering is useful when AST diffs are still too chatty for a specific language or codebase. Typical cases are string-heavy files or reviews where you only care about a narrow set of leaf node kinds.

```json5
"input-processing": {
    // You can exclude different tree sitter node types - this rule takes precedence over `include_kinds`.
    "exclude-kinds": ["string"],
    // You can specifically allow only certain tree sitter node types
    "include-kinds": ["method_definition"],
    "strip-whitespace": true,
}
```

`exclude-kinds` takes precedence over `include-kinds`. A practical workflow is to start from `diffsitter dump-default-config`, add the smallest filter that removes noise, and rerun the same comparison.
