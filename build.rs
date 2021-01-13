use anyhow::Result;
use std::{
    env,
    fmt::Display,
    fs,
    path::{Path, PathBuf},
};

/// Compilation information as it pertains to a tree-sitter grammar
///
/// This contains information about a parser that is required at build time
#[derive(Debug, Default)]
struct GrammarCompileInfo<'a> {
    /// The language's display name
    display_name: &'a str,
    /// The location of the grammar's source relative to `build.rs`
    path: PathBuf,
    /// The sources to compile with a C compiler
    c_sources: Vec<&'a str>,
    /// The sources to compile with a C++ compiler
    ///
    /// The files supplied here will be compiled into a library named
    /// "tree-sitter-{language}-cpp-compile-diffsitter" to avoid clashing with other symbols.
    cpp_sources: Vec<&'a str>,
}

/// Generated the code fo the map between the language identifiers and the function to initialize
/// the language parser
fn codegen_language_map<T: ToString + Display>(languages: &[T]) -> String {
    let body: String = languages
        .iter()
        .map(|lang| format!("\"{}\" => tree_sitter_{},\n", lang, lang))
        .collect();
    let map_decl = format!(
        "\nstatic LANGUAGES: phf::Map<&'static str, unsafe extern \"C\" fn() -> Language> = phf_map! {{\n {}\n }};\n", body);
    map_decl
}

/// Compile a language's grammar
fn compile_grammar(
    include: &Path,
    c_sources: &[PathBuf],
    cpp_sources: &[PathBuf],
    output_name: &str,
) -> Result<(), cc::Error> {
    if !cpp_sources.is_empty() {
        cc::Build::new()
            .cpp(true)
            .include(include)
            .files(cpp_sources)
            .warnings(false)
            .try_compile(&format!("{}-cpp-compile-diffsiter", &output_name))?;
    }

    if !c_sources.is_empty() {
        cc::Build::new()
            .include(include)
            .files(c_sources)
            .warnings(false)
            .try_compile(&output_name)?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let grammars = vec![
        GrammarCompileInfo {
            display_name: "rust",
            path: PathBuf::from("grammars/tree-sitter-rust"),
            c_sources: vec!["parser.c", "scanner.c"],
            ..GrammarCompileInfo::default()
        },
        GrammarCompileInfo {
            display_name: "cpp",
            path: PathBuf::from("grammars/tree-sitter-cpp"),
            c_sources: vec!["parser.c"],
            cpp_sources: vec!["scanner.cc"],
        },
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
        if grammar.c_sources.is_empty() && grammar.cpp_sources.is_empty() {
            return Err(anyhow::format_err!(
                "Supplied source files for {} parser is empty",
                grammar.display_name
            ));
        }
        // Prepend {grammar-repo}/src path to each file
        let c_sources: Vec<_> = grammar
            .c_sources
            .iter()
            .map(|&filename| dir.join(filename))
            .collect();
        let cpp_sources: Vec<_> = grammar
            .cpp_sources
            .iter()
            .map(|&filename| dir.join(filename))
            .collect();
        compile_grammar(
            &dir,
            &c_sources[..],
            &cpp_sources[..],
            &grammar.display_name,
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
