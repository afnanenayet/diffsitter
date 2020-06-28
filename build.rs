use cc;
use phf::{phf_set, Set};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

/// The top level directory for each language that contains the tree sitter source for each
/// language
static GRAMMARS_DIR: &'static str = "grammars";

/// Candidate source file names that might be in a tree sitter directory
static SRC_FILE_CANDS: &'static [&'static str] = &["parser", "scanner"];

/// Valid extensions for source files
static VALID_EXTENSIONS: &'static [&'static str] = &["cc", "c"];

/// Tree sitter grammars are *supposed* to be valid C, but it seems like some parsers need to be
/// compiled with C++ to avoid build errors
static COMPILE_WITH_CPP: Set<&'static str> = phf_set! {
    "agda",
    "ruby",
    //"ocaml",
    "php",
    "python",
    //"haskell",
    //"bash",
};

/// Generated the code fo the map between the language identifiers and the function to initialize
/// the language parser
fn codegen_language_map(languages: &[String]) -> String {
    // Build a vector of the languages for code gen
    let mut map_decl =
        "\nstatic LANGUAGES: phf::Map<&'static str, unsafe extern \"C\" fn() -> Language> = phf_map! {\n".to_owned();

    for language in languages {
        map_decl += &format!("\"{}\" => tree_sitter_{},\n", language, language);
    }
    map_decl += "};\n";
    map_decl
}

fn main() {
    // Create a tuple of (folder name, folder relative path) that we can reference the desired
    // output name for each compiled grammar and the path to the source code for that compiled unit
    let grammars = fs::read_dir(GRAMMARS_DIR)
        .unwrap()
        .map(|res| res.map(|e| (e.file_name(), e.path())))
        .collect::<Result<Vec<_>, io::Error>>()
        .unwrap();

    // The string represented the generated code that we get from the tree sitter grammars
    let mut codegen = String::from(
        r#"
use tree_sitter::Language;
use phf::phf_map;
"#,
    );
    let mut languages = Vec::new();

    // Iterate through each grammar, find the valid source files that are in it, and add them as
    // compilation targets
    for grammar in grammars {
        let output_name = grammar.0.to_string_lossy();
        let dir = grammar.1.join("src");

        // The folder names for the grammars are hyphenated, we want to conver those to underscores
        // so we can form valid rust identifiers
        let language = output_name
            .trim_start_matches("tree-sitter-")
            .replace("-", "_");

        // Take the cartesian product of the source names and valid extensions, and filter for the
        // ones that actually exist in each folder
        let build_files: Vec<PathBuf> = VALID_EXTENSIONS
            .iter()
            .flat_map(|&ext| {
                SRC_FILE_CANDS
                    .iter()
                    .map(move |&fname| PathBuf::from(fname).with_extension(ext))
            })
            .map(|filename| dir.join(filename))
            .filter(|candidate_file| candidate_file.is_file())
            .collect();

        // If building with C++ fails, try building with C
        if COMPILE_WITH_CPP.contains(language.as_str()) {
            let _ = cc::Build::new()
                .include(&dir)
                .files(build_files.clone())
                .flag_if_supported("-std=c11")
                .cpp(true)
                .compile(&output_name);
        } else {
            let _ = cc::Build::new()
                .include(&dir)
                .files(build_files.clone())
                .flag_if_supported("-std=c++17")
                .compile(&output_name);
        }

        codegen += &format!(
            "extern \"C\" {{ pub fn tree_sitter_{}() -> Language; }}\n",
            language
        );
        languages.push(language.to_owned());
    }
    codegen += &codegen_language_map(&languages);

    // Write the generated code to a file called `grammar.rs`
    let codegen_out_dir = env::var_os("OUT_DIR").unwrap();
    let codegen_path = Path::new(&codegen_out_dir).join("generated_grammar.rs");
    fs::write(&codegen_path, codegen).unwrap();
    println!("cargo:rerun-if-changed=build.rs");
}
