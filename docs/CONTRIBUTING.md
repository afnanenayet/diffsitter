# Contributing

## Development setup

This repo uses [pre-commit](https://pre-commit.com)
to automatically apply linters and formatters before every commit. Install
`pre-commit`. If you have it installed, then initialize the git hooks for
this repo with:

```sh
pre-commit install
```

Now your files will be automatically formatted before each commit. If they are not
formatted then the commit check will fail and you will have to commit the updated
formatted file again.

## Building

This project uses a mostly standard Rust toolchain. At the time of writing, the
easiest way to get set up with the Rust toolchain is
[rustup](https://rustup.rs/). The rustup website has the instructions to get
you set up with Rust on any platform that Rust supports. This project uses
Cargo to build, like most Rust projects.

The only small caveat with this projects that isn't standard is that it has
bindings to tree-sitter, so it compiles tree-sitter grammars that are written
in C and C++, and uses the C FFI to link from the Rust codebase to the
compiled tree-sitter grammars. As such, you'll need to have a C compiler that
supports `C99` or later, and a C++ compiler that supports `C++14` or later.
Compilation is handled by the `cc` crate, and you can find the details on how
compilers are selected in the [cc docs](https://docs.rs/cc).

These tree-sitter grammars are included as [git
submodules](https://git-scm.com/book/en/v2/Git-Tools-Submodules), so you need
to make sure you initialize submodules when checking out the git repo.

If you're cloning the repository for the first time:

```sh
git clone --recurse-submodules https://github.com/afnanenayet/diffsitter.git
```

If you've already checked out the repository, you can initialize submodules
with the following command:

```sh
git submodule update --init --recursive
```

This command can also be used to update to the latest commits for each
submodule as the repository gets updated. Sometimes you may run into build
errors that complain about trying to link to nonexistent symbols, this error
can be incurred if a new grammar is added to the repository but the source
files aren't present, so you should run the update command to see if that fixes
the error. If it doesn't, I've messed up and you should file an issue
(with as much detail as possible).

### Dynamic Libraries/Grammars

If you want to use dynamic libraries you don't have to clone the submodules.
You can build this binary with support for dynamic libraries with the following
command:

```sh
cargo build --no-default-features --features dynamic-grammar-libs
```

There is an optional test that checks to see if the default library locations
can be loaded correctly for every language that `diffsitter` is configured to
handle by default. This will look for a shared library file in the user's
default library lookup path in the form `libtree-sitter-{lang}.{ext}` where
`ext` is determined by the user's platform (`.so` on Linux, `.dylib` on MacOS,
and `.dll` on Windows). The test will then try to find and call the function to
construct the grammar object from that file if it is able to find it.

You can invoke the test with this command:

```sh
cargo test --features dynamic-grammar-libs -- --ignored --exact parse::tests::dynamic_load_parsers
```

This test is marked `#[ignore]` because people may decide to package their
shared libraries for `tree-sitter` differently or may want to specify different
file paths for these shared libraries in their config.

### C/C++ Toolchains

If you're on Mac and have [Homebrew](https://brew.sh) installed:

```sh
brew install llvm

# or

brew install gcc
```

The built-in Apple clang that comes with XCode is also fine.

If you're on Ubuntu:

```sh
sudo apt install gcc
```

If you're on Arch Linux:

```sh
sudo pacman -S gcc
```

## Development

### Required tools

| Tool | Install | Purpose |
|------|---------|---------|
| [cargo-nextest](https://nexte.st) | `cargo install cargo-nextest` | Test runner (used in CI) |
| [cargo-insta](https://insta.rs) | `cargo install cargo-insta` | Snapshot test review TUI |

### Running CI checks locally

Before submitting a PR, make sure formatting, linting, and tests all pass:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
cargo test --doc --all-features
```

This matches what CI runs. Having these checks pass is a prerequisite for
getting any PR merged.

This project targets the latest stable version of `rustc` (MSRV 1.85.1).

Note that if you update anything to do with the project config, you'll have to
update the [sample config](../assets/sample_config.json5) as well to ensure
that tests pass (the project will actually parse the sample config) and that
it documents the various options available to users.

### Submodules

We are currently vendoring tree sitter grammars in the diffsitter repository so
we can compile everything statically. We strip the Rust bindings from the
repository if it contains them, otherwise Rust will not include any files from
these folders in the target directory, and we will not be able to compile these
dependencies ourselves.

We maintain these vendors and ensure they stay up to date using
[nvchecker](https://github.com/lilydjwg/nvchecker). We have a repository for
the grammars at:
[github.com/afnanenayet/diffsitter-grammars](https://github.com/afnanenayet/diffsitter-grammars).
If you update a tree sitter fork, you should file a pull request in the
`diffsitter-grammars` repository and a PR in this repository with the updated
submodule. You can also use that repository with `nvchecker` to find
forks that are out of date, which makes for an easy first issue that you can
tackle in this project.

### Testing

Tests are run with [nextest](https://nexte.st):

```sh
cargo nextest run --all-features
```

The nextest configuration lives in `.config/nextest.toml` and defines test
groups with concurrency limits for property-based tests and MCP server tests.

We use a combination of unit testing, snapshot testing, and property-based
testing:

- **Unit tests** verify expected behavior of individual functions
- **Snapshot tests** ([insta](https://docs.rs/insta)) verify consistent output between changes — these typically break when grammars are updated
- **Property tests** ([proptest](https://docs.rs/proptest)) verify invariants hold across randomly generated inputs
- **Benchmarks** ([criterion](https://docs.rs/criterion)) measure parsing and navigation performance

If snapshot tests change, review and accept the new snapshots:

```sh
cargo insta review
```

This opens a TUI tool that lets you review snapshots and accept or reject
the changes.
