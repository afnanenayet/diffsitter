use anyhow::Result;
use cc;
use std::{
    env,
    fmt::Display,
    fs,
    path::{Path, PathBuf},
};

/// Compilation information as it pertains to a tree-sitter grammar
///
/// This contains information about a parser that is required at build time
struct GrammarCompileInfo<'a> {
    /// The language's display name
    display_name: &'a str,
    /// The location of the grammar's source relative to `build.rs`
    path: PathBuf,
    /// Whether the project should be compiled as a `C++` source
    compile_cpp: bool,
    /// The source files in the project that should be compiled
    source_files: Vec<&'a str>,
}

/// Generated the code fo the map between the language identifiers and the function to initialize
/// the language parser
fn codegen_language_map<T: ToString + Display>(languages: &[T]) -> String {
    // Build a vector of the languages for code gen
    let mut map_decl =
        "\nstatic LANGUAGES: phf::Map<&'static str, unsafe extern \"C\" fn() -> Language> = phf_map! {\n".to_owned();

    for language in languages {
        map_decl += &format!("\"{}\" => tree_sitter_{},\n", language, language);
    }
    map_decl += "};\n";
    map_decl
}

/// Compile a language's grammar
fn compile_grammar(
    include: &Path,
    files: &[PathBuf],
    output_name: &str,
    compile_cpp: bool,
) -> Result<(), cc::Error> {
    if compile_cpp {
        cc::Build::new()
            .cpp(true)
            .include(include)
            .files(files)
            .warnings(false)
            .try_compile(&output_name)
    } else {
        cc::Build::new()
            .include(include)
            .files(files)
            .warnings(false)
            .try_compile(&output_name)
    }
}

fn main() -> Result<()> {
    let grammars = [
        GrammarCompileInfo {
            display_name: "rust",
            path: PathBuf::from("grammars/tree-sitter-rust"),
            source_files: vec!["parser.c", "scanner.c"],
            compile_cpp: false,
        },
        //GrammarCompileInfo {
            //display_name: "c",
            //path: PathBuf::from("grammars/tree-sitter-c"),
            //source_files: vec!["parser.c"],
            //compile_cpp: false,
        //},
    ];

    // The string represented the generated code that we get from the tree sitter grammars
    let mut codegen = String::from(
        r#"
use tree_sitter::Language;
use phf::phf_map;
"#,
    );
    let mut languages = Vec::new();
    languages.reserve(grammars.len());

    // Iterate through each grammar, find the valid source files that are in it, and add them as
    // compilation targets
    for grammar in &grammars {
        // The directory to the source files
        let dir = grammar.path.join("src");

        // The folder names for the grammars are hyphenated, we want to conver those to underscores
        // so we can form valid rust identifiers
        let language = grammar.display_name;

        // If there are no valid source files, don't bother trying to compile
        if grammar.source_files.is_empty() {
            return Err(anyhow::format_err!(
                "Supplied source files for {} parser is empty",
                grammar.display_name
            ));
        }
        // Prepend {grammar-repo}/src path to each file
        let sources: Vec<_> = grammar
            .source_files
            .iter()
            .map(|&filename| dir.join(filename))
            .collect();
        compile_grammar(
            &dir,
            &sources[..],
            &grammar.display_name,
            grammar.compile_cpp,
        )?;
        // If compilation succeeded with either case, link the language
        //if successful_compilation {
        codegen += &format!(
            "extern \"C\" {{ pub fn tree_sitter_{}() -> Language; }}\n",
            language
        );
        languages.push(language);
    }
    codegen += &codegen_language_map(&languages[..]);

    // Write the generated code to a file in the resulting build directory
    let codegen_out_dir = env::var_os("OUT_DIR").unwrap();
    let codegen_path = Path::new(&codegen_out_dir).join("generated_grammar.rs");
    fs::write(&codegen_path, codegen).unwrap();
    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}
