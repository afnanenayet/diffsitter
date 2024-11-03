#[cfg(feature = "static-grammar-libs")]
use anyhow::bail;

#[cfg(feature = "static-grammar-libs")]
use thiserror::Error;

#[cfg(feature = "static-grammar-libs")]
use cargo_emit::{rerun_if_changed, rerun_if_env_changed};

#[cfg(feature = "static-grammar-libs")]
use rayon::prelude::*;

#[cfg(feature = "static-grammar-libs")]
use std::{
    env,
    fmt::Display,
    fs,
    path::{Path, PathBuf},
    vec,
};

use anyhow::Result;
use std::fmt::Write;

/// Compilation information as it pertains to a tree-sitter grammar
///
/// This contains information about a parser that is required at build time
#[cfg(feature = "static-grammar-libs")]
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

    /// Additional include paths to pass to the compiler.
    ///
    /// By default this is set to <path>/include, but some repos may have a different include path.
    include_paths: Option<Vec<PathBuf>>,
}

/// The compilation parameters that are passed into the `compile_grammar` function
///
/// This is a convenience method that was created so we can store parameters in a vector and use
/// a parallel iterator to compile all of the grammars at once over a threadpool.
#[cfg(feature = "static-grammar-libs")]
struct CompileParams {
    pub include_dirs: Vec<PathBuf>,
    pub c_sources: Vec<PathBuf>,
    pub cpp_sources: Vec<PathBuf>,
    pub display_name: String,
}

/// An error that can arise when sanity check compilation parameters
#[cfg(feature = "static-grammar-libs")]
#[derive(Debug, Error)]
enum CompileParamError {
    #[error("Subdirectory for grammar {0} was not found")]
    SubdirectoryNotFound(String),

    #[error("Source files {source_files:?} not found for {grammar}")]
    SourceFilesNotFound {
        /// The name of the grammar that had an error
        grammar: String,

        /// The missing source files
        source_files: Vec<String>,
    },
}

/// Environment variables that the build system relies on
///
/// If any of these are changed, Cargo will rebuild the project.
#[cfg(feature = "static-grammar-libs")]
const BUILD_ENV_VARS: &[&str] = &["CC", "CXX", "LD_LIBRARY_PATH", "PATH"];

/// Generated the code fo the map between the language identifiers and the function to initialize
/// the language parser
#[cfg(feature = "static-grammar-libs")]
fn codegen_language_map<T: ToString + Display>(languages: &[T]) -> String {
    let body: String = languages.iter().fold(String::new(), |mut buffer, lang| {
        writeln!(buffer, "\"{lang}\" => tree_sitter_{lang},").unwrap();
        buffer
    });
    let map_decl = format!(
        "\nstatic LANGUAGES: phf::Map<&'static str, unsafe extern \"C\" fn() -> Language> = phf_map! {{\n {body}\n }};\n");
    map_decl
}

/// Compile a language's grammar
///
/// This builds the grammars and statically links them into the Rust binary using Crichton's cc
/// library. We name the libraries "{`grammar_name`}-[cc|cxx]-diffsitter" to prevent clashing with
/// any existing installed tree sitter libraries.
#[cfg(feature = "static-grammar-libs")]
fn compile_grammar(
    includes: &[PathBuf],
    c_sources: &[PathBuf],
    cpp_sources: &[PathBuf],
    output_name: &str,
) -> Result<(), cc::Error> {
    // NOTE: that we have to compile the C sources first because the scanner depends on the parser.
    // Right now the only C libraries are parsers, so we build them before the C++ files, which
    // are only scanners. This resolves a linker error we were seeing on Linux.
    if !c_sources.is_empty() {
        cc::Build::new()
            .includes(includes)
            .files(c_sources)
            .flag_if_supported("-std=c11")
            .warnings(false)
            .extra_warnings(false)
            .try_compile(&format!("{output_name}-cc-diffsitter"))?;
    }

    if !cpp_sources.is_empty() {
        cc::Build::new()
            .cpp(true)
            .includes(includes)
            .files(cpp_sources)
            .flag_if_supported("-std=c++17")
            .warnings(false)
            .extra_warnings(false)
            .try_compile(&format!("{}-cxx-diffsitter", &output_name))?;
    }

    Ok(())
}

/// Print any other cargo-emit directives
#[cfg(feature = "static-grammar-libs")]
fn extra_cargo_directives() {
    for &env_var in BUILD_ENV_VARS {
        rerun_if_env_changed!(env_var);
    }
}

/// Preprocess grammar compilation info so the build script can find all of the source files.
///
/// This will augment the C and C++ source files so that they have the full relative path from the
/// repository root rather, which prepends the repository path and `src/` to the file.
///
/// For example, a `GrammarCompileInfo` instance for Rust:
///
/// ```rust
/// GrammarCompileInfo {
///     display_name: "rust",
///     path: PathBuf::from("grammars/tree-sitter-rust"),
///     c_sources: vec!["parser.c", "scanner.c"],
///     ..GrammarCompileInfo::default()
/// };
/// ```
///
/// will get turned to:
///
/// ```rust
/// CompileParams {
///     display_name: "rust",
///     path: PathBuf::from("grammars/tree-sitter-rust"),
///     c_sources: vec![
///         "grammars/tree-sitter-rust/src/parser.c",
///         "grammars/tree-sitter-rust/src/scanner.c"
///     ],
///     cpp_sources: vec![],
/// };
/// ```
#[cfg(feature = "static-grammar-libs")]
fn preprocess_compile_info(grammar: &GrammarCompileInfo) -> CompileParams {
    let dir = grammar.path.join("src");
    // The directory to the source files
    let include_dirs = if let Some(includes) = grammar.include_paths.clone() {
        includes.clone()
    } else {
        vec![dir.clone()]
    };

    let cpp_sources: Vec<_> = grammar
        .cpp_sources
        .iter()
        // Prepend {grammar-repo}/src path to each file
        .map(|&filename| dir.join(filename))
        .collect();
    let c_sources: Vec<_> = grammar
        .c_sources
        .iter()
        // Prepend {grammar-repo}/src path to each file
        .map(|&filename| dir.join(filename))
        .collect();

    CompileParams {
        include_dirs,
        c_sources,
        cpp_sources,
        display_name: grammar.display_name.into(),
    }
}

/// Sanity check the contents of a compilation info unit.
///
/// This should give clearer errors up front compared to the more obscure errors you can get from
/// the C/C++ toolchains when files are missing.
#[cfg(feature = "static-grammar-libs")]
fn verify_compile_params(compile_params: &CompileParams) -> Result<(), CompileParamError> {
    for include_dir in &compile_params.include_dirs {
        if !include_dir.exists() {
            return Err(CompileParamError::SubdirectoryNotFound(
                compile_params.display_name.to_string(),
            ));
        }
    }

    let missing_sources = compile_params
        .c_sources
        .iter()
        .chain(compile_params.cpp_sources.iter())
        .filter_map(|file| {
            // Filter for files that *don't* exist
            if file.exists() {
                None
            } else {
                Some(file.to_string_lossy().to_string())
            }
        })
        .collect::<Vec<String>>();

    if !missing_sources.is_empty() {
        return Err(CompileParamError::SourceFilesNotFound {
            grammar: compile_params.display_name.to_string(),
            source_files: missing_sources,
        });
    }

    Ok(())
}

/// Grammar compilation information for diffsitter.
///
/// This defines all of the grammars that are used by the build script. If you want to add new
/// grammars, add them to this list. This would ideally be a global static vector, but we can't
/// create a `const static` because the `PathBuf` constructors can't be evaluated at compile time.
#[cfg(feature = "static-grammar-libs")]
fn grammars() -> Vec<GrammarCompileInfo<'static>> {
    let grammars = vec![
        GrammarCompileInfo {
            display_name: "rust",
            path: PathBuf::from("grammars/tree-sitter-rust"),
            c_sources: vec!["parser.c", "scanner.c"],
            ..Default::default()
        },
        GrammarCompileInfo {
            display_name: "cpp",
            path: PathBuf::from("grammars/tree-sitter-cpp"),
            c_sources: vec!["parser.c", "scanner.c"],
            ..Default::default()
        },
        GrammarCompileInfo {
            display_name: "python",
            path: PathBuf::from("grammars/tree-sitter-python"),
            c_sources: vec!["parser.c", "scanner.c"],
            ..Default::default()
        },
        GrammarCompileInfo {
            display_name: "bash",
            path: PathBuf::from("grammars/tree-sitter-bash"),
            c_sources: vec!["parser.c", "scanner.c"],
            ..Default::default()
        },
        GrammarCompileInfo {
            display_name: "ocaml",
            path: PathBuf::from("grammars/tree-sitter-ocaml/grammars/ocaml"),
            c_sources: vec!["parser.c", "scanner.c"],
            ..Default::default()
        },
        GrammarCompileInfo {
            display_name: "go",
            path: PathBuf::from("grammars/tree-sitter-go"),
            c_sources: vec!["parser.c"],
            ..Default::default()
        },
        GrammarCompileInfo {
            display_name: "ruby",
            path: PathBuf::from("grammars/tree-sitter-ruby"),
            c_sources: vec!["parser.c"],
            cpp_sources: vec!["scanner.cc"],
            ..GrammarCompileInfo::default()
        },
        GrammarCompileInfo {
            display_name: "java",
            path: PathBuf::from("grammars/tree-sitter-java"),
            c_sources: vec!["parser.c"],
            ..Default::default()
        },
        GrammarCompileInfo {
            display_name: "c_sharp",
            path: PathBuf::from("grammars/tree-sitter-c-sharp"),
            c_sources: vec!["parser.c", "scanner.c"],
            ..Default::default()
        },
        GrammarCompileInfo {
            display_name: "css",
            path: PathBuf::from("grammars/tree-sitter-css"),
            c_sources: vec!["parser.c", "scanner.c"],
            ..Default::default()
        },
        GrammarCompileInfo {
            display_name: "php",
            path: PathBuf::from("grammars/tree-sitter-php/php"),
            c_sources: vec!["parser.c", "scanner.c"],
            ..Default::default()
        },
        GrammarCompileInfo {
            display_name: "json",
            path: PathBuf::from("grammars/tree-sitter-json"),
            c_sources: vec!["parser.c"],
            ..Default::default()
        },
        GrammarCompileInfo {
            display_name: "hcl",
            path: PathBuf::from("grammars/tree-sitter-hcl"),
            c_sources: vec!["parser.c"],
            cpp_sources: vec!["scanner.cc"],
            ..Default::default()
        },
        GrammarCompileInfo {
            display_name: "typescript",
            path: PathBuf::from("grammars/tree-sitter-typescript/typescript"),
            c_sources: vec!["parser.c", "scanner.c"],
            ..Default::default()
        },
        GrammarCompileInfo {
            display_name: "tsx",
            path: PathBuf::from("grammars/tree-sitter-typescript/tsx"),
            c_sources: vec!["parser.c", "scanner.c"],
            ..Default::default()
        },
        GrammarCompileInfo {
            display_name: "c",
            path: PathBuf::from("grammars/tree-sitter-c"),
            c_sources: vec!["parser.c"],
            ..Default::default()
        },
        GrammarCompileInfo {
            display_name: "markdown",
            path: PathBuf::from("grammars/tree-sitter-markdown/tree-sitter-markdown"),
            c_sources: vec!["parser.c", "scanner.c"],
            ..Default::default()
        }, // Add new grammars here...
    ];
    grammars
}

/// Compile the submodules as static grammars for the binary.
#[cfg(feature = "static-grammar-libs")]
fn compile_static_grammars() -> Result<()> {
    let grammars = grammars();
    // The string represented the generated code that we get from the tree sitter grammars
    let mut codegen = String::from(
        r"
use tree_sitter::Language;
use phf::phf_map;
",
    );

    // A vector of language strings that are used later for codegen, so we can dynamically created
    // the unsafe functions that load the grammar for each language
    let mut languages = Vec::with_capacity(grammars.len());

    // We create a vector of parameters so we can use Rayon's parallel iterators to compile
    // grammars in parallel
    let compile_params: Vec<CompileParams> = grammars.iter().map(preprocess_compile_info).collect();

    // Verify each preprocessed compile param entry -- this will short circuit the build script if
    // there are any errors
    compile_params
        .iter()
        .map(verify_compile_params)
        .collect::<Result<Vec<_>, CompileParamError>>()?;

    // Any of the compilation steps failing will short circuit the entire `collect` function and
    // error out
    compile_params
        .par_iter()
        .map(|p| {
            compile_grammar(
                &p.include_dirs,
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
        writeln!(
            codegen,
            "extern \"C\" {{ pub fn tree_sitter_{language}() -> Language; }}"
        )?;
        languages.push(language.as_str());

        // We recompile the libraries if any grammar sources or this build file change, since Cargo
        // will cache based on the Rust modules and isn't aware of the linked C libraries.
        for source in params.c_sources.iter().chain(params.cpp_sources.iter()) {
            if let Some(grammar_path) = &source.as_path().to_str() {
                rerun_if_changed!((*grammar_path).to_string());
            } else {
                bail!("Path to grammar for {} is not a valid string", language);
            }
        }
    }

    extra_cargo_directives();
    codegen += &codegen_language_map(&languages[..]);

    // Write the generated code to a file in the resulting build directory
    let codegen_out_dir = env::var_os("OUT_DIR").unwrap();
    let codegen_path = Path::new(&codegen_out_dir).join("generated_grammar.rs");
    fs::write(codegen_path, codegen)?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "static-grammar-libs")]
    compile_static_grammars()?;

    #[cfg(feature = "better-build-info")]
    shadow_rs::ShadowBuilder::builder()
        .build_pattern(shadow_rs::BuildPattern::RealTime)
        .build()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // TODO(afnan): add generaetd shell completion scripts

    // Needed so tests are rerun if we change/update any of the test input files. Otherwise Cargo
    // won't know to actually rerun the tests.
    println!("cargo::rerun-if-changed=resources/test_configs");
    println!("cargo::rerun-if-changed=assets/sample_config.json5");
    println!("cargo::rerun-if-env-changed=BASE_TEST_DIR");
    Ok(())
}
