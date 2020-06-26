use cc;
/// The build script for `diffsitter`
///
/// This compiles runtimes for each language so they can be used by the tree-sitter library.
use std::path::PathBuf;

/// The top level directory for each language that contains the tree sitter source for each
/// language
static GRAMMARS_DIR: &'static str = "grammars";

/// The valid source directories for tree sitter language runtimes for this program to compile
static SOURCE_DIRECTORIES: &'static [&'static str] =
    &["tree-sitter-rust", "tree-sitter-javascript"];

fn main() {
    for source_dir in SOURCE_DIRECTORIES {
        let dir: PathBuf = [GRAMMARS_DIR, source_dir, "src"].iter().collect();

        cc::Build::new()
            .include(&dir)
            .file(dir.join("parser.c"))
            .file(dir.join("scanner.c"))
            .opt_level(3)
            .compile(source_dir);
    }
}
