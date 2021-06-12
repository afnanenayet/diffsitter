use anyhow::Result;
use cargo_emit::{rerun_if_changed, rerun_if_env_changed};
use rayon::prelude::*;
use std::{
    env,
    fmt::Display,
    fs,
    path::{Path, PathBuf},
    vec,
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

/// The compilation parameters that are passed into the `compile_grammar` function
///
/// This is a convenience method that was created so we can store parameters in a vector and use
/// a parallel iterator to compile all of the grammars at once over a threadpool.
struct CompileParams {
    pub dir: PathBuf,
    pub c_sources: Vec<PathBuf>,
    pub cpp_sources: Vec<PathBuf>,
    pub display_name: String,
}

/// Environment variables that the build system relies on
///
/// If any of these are changed, Cargo will rebuild the project.
const BUILD_ENV_VARS: &[&str] = &["CC", "CXX", "LD_LIBRARY_PATH", "PATH"];

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
            .flag_if_supported("-std=c++14")
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

/// Print any other cargo-emit directives
fn extra_cargo_directives() {
    for &env_var in BUILD_ENV_VARS {
        rerun_if_env_changed!(env_var);
    }
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
        GrammarCompileInfo {
            display_name: "python",
            path: PathBuf::from("grammars/tree-sitter-python"),
            c_sources: vec!["parser.c"],
            cpp_sources: vec!["scanner.cc"],
        },
        GrammarCompileInfo {
            display_name: "bash",
            path: PathBuf::from("grammars/tree-sitter-bash"),
            c_sources: vec!["parser.c"],
            cpp_sources: vec!["scanner.cc"],
        },
        GrammarCompileInfo {
            display_name: "ocaml",
            path: PathBuf::from("grammars/tree-sitter-ocaml/ocaml"),
            c_sources: vec!["parser.c"],
            cpp_sources: vec!["scanner.cc"],
        },
        GrammarCompileInfo {
            display_name: "go",
            path: PathBuf::from("grammars/tree-sitter-go"),
            c_sources: vec!["parser.c"],
            ..GrammarCompileInfo::default()
        },
        GrammarCompileInfo {
            display_name: "ruby",
            path: PathBuf::from("grammars/tree-sitter-ruby"),
            c_sources: vec!["parser.c"],
            cpp_sources: vec!["scanner.cc"],
        },
        GrammarCompileInfo {
            display_name: "java",
            path: PathBuf::from("grammars/tree-sitter-java"),
            c_sources: vec!["parser.c"],
            ..GrammarCompileInfo::default()
        },
        GrammarCompileInfo {
            display_name: "c_sharp",
            path: PathBuf::from("grammars/tree-sitter-c-sharp"),
            c_sources: vec!["parser.c", "scanner.c"],
            ..GrammarCompileInfo::default()
        },
        GrammarCompileInfo {
            display_name: "css",
            path: PathBuf::from("grammars/tree-sitter-css"),
            c_sources: vec!["parser.c", "scanner.c"],
            ..GrammarCompileInfo::default()
        },
        GrammarCompileInfo {
            display_name: "php",
            path: PathBuf::from("grammars/tree-sitter-php"),
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

    // A vector of language strings that are used later for codegen, so we can dynamically created
    // the unsafe functions that load the grammar for each language
    let mut languages = Vec::new();
    languages.reserve(grammars.len());

    // We create a vector of parameters so we can use Rayon's parallel iterators to compile
    // grammars in parallel
    let compile_params: Vec<CompileParams> = grammars
        .iter()
        .map(|grammar| {
            // The directory to the source files
            let dir = grammar.path.join("src");

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
            CompileParams {
                dir,
                c_sources,
                cpp_sources,
                display_name: grammar.display_name.into(),
            }
        })
        .collect();

    // Any of the compilation steps failing will short circuit the entire `collect` function and
    // error out
    compile_params
        .par_iter()
        .map(|p| {
            compile_grammar(
                &p.dir,
                &p.c_sources[..],
                &p.cpp_sources[..],
                &p.display_name,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Run the follow up tasks for the compiled sources
    for params in &compile_params {
        let language = &params.display_name;

        // If compilation succeeded with either case, link the language. If it failed, we'll never
        // get to this step.
        codegen += &format!(
            "extern \"C\" {{ pub fn tree_sitter_{}() -> Language; }}\n",
            language
        );
        languages.push(language);

        // We recompile the libraries if any grammar sources or this build file change, since Cargo
        // will cache based on the Rust modules and isn't aware of the linked C libraries.
        for source in params.c_sources.iter().chain(params.cpp_sources.iter()) {
            rerun_if_changed!(&source.as_path().to_string_lossy());
        }
    }

    extra_cargo_directives();
    codegen += &codegen_language_map(&languages[..]);

    // Write the generated code to a file in the resulting build directory
    let codegen_out_dir = env::var_os("OUT_DIR").unwrap();
    let codegen_path = Path::new(&codegen_out_dir).join("generated_grammar.rs");
    fs::write(&codegen_path, codegen)?;

    build_info_build::build_script();
    Ok(())
}
