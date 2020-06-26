use cc;
use std::{fs, io, path::PathBuf};

/// The top level directory for each language that contains the tree sitter source for each
/// language
static GRAMMARS_DIR: &'static str = "grammars";

/// Candidate source file names that might be in a tree sitter directory
static SRC_FILE_CANDS: &'static [&'static str] = &["parser", "scanner"];

/// Valid extensions for source files
static VALID_EXTENSIONS: &'static [&'static str] = &["cc", "c"];

fn main() {
    let grammars = fs::read_dir(GRAMMARS_DIR)
        .unwrap()
        .map(|res| res.map(|e| (e.file_name(), e.path())))
        .collect::<Result<Vec<_>, io::Error>>()
        .unwrap();

    // Iterate through each grammar, find the valid source files that are in it, and add them as
    // compilation targets
    for grammar in grammars {
        let output_name = grammar.0.to_string_lossy();
        let dir = grammar.1.join("src");

        // Take the cartesian product of the source names and valid extensions, and filter for the
        // ones that actually exist in each folder
        let build_files: Vec<PathBuf> = SRC_FILE_CANDS
            .iter()
            .flat_map(|&fname| {
                VALID_EXTENSIONS
                    .iter()
                    .map(move |&ext| PathBuf::from(fname).with_extension(ext))
            })
            .map(|filename| dir.join(filename))
            .filter(|candidate_file| candidate_file.is_file())
            .collect();

        cc::Build::new()
            .include(&dir)
            .files(build_files)
            .compile(&output_name);
    }
}
