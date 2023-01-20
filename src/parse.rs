//! Utilities for reading and parsing files with the diffsitter parser

// Loads codegen methods from the build script
// We only load for either static-grammar-libs or dynamic-grammar-libs. This is required
// because both of these feature enable functions that need imports and functions
#[cfg(feature = "static-grammar-libs")]
include!(concat!(env!("OUT_DIR"), "/generated_grammar.rs"));

#[cfg(not(feature = "static-grammar-libs"))]
use phf::phf_map;

#[cfg(not(feature = "static-grammar-libs"))]
use tree_sitter::Language;

use anyhow::Result;
use log::{debug, error, info};
use logging_timer::time;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};
use thiserror::Error;
use tree_sitter::{Parser, Tree};

/// A mapping of file extensions to their associated languages
///
/// The languages correspond to grammars from `tree-sitter`
static FILE_EXTS: phf::Map<&'static str, &'static str> = phf_map! {
    "hs" => "haskell",
    "rs" => "rust",
    "go" => "go",
    "c" => "c",
    "cc" => "cpp",
    "cpp" => "cpp",
    "cs" => "c_sharp",
    "java" => "java",
    "py" => "python",
    "css" => "css",
    "sh" => "bash",
    "bash" => "bash",
    "jl" => "julia",
    "ml" => "ocaml",
    "rb" => "ruby",
    "scala" => "scala",
    "sc" => "scala",
    "swift" => "swift",
    "php" => "php",
    "json" => "json",
    "hcl" => "hcl",
    "ts" => "typescript",
    "tsx" => "tsx",
    "hpp" => "cpp",
    "tpp" => "tpp",
    "h" => "c",
    "tf" => "hcl",
    "js" => "javascript",
};

/// Possible errors that can arise when loading grammars
#[derive(Error, Debug)]
pub enum LoadingError {
    #[cfg(feature = "static-grammar-libs")]
    #[error("The program was not compiled with support for {0}")]
    StaticNotCompiled(String),

    #[error("This program was not compiled with support for any grammars")]
    NoGrammars,

    #[error("Unsupported extension: {0}")]
    UnsupportedExt(String),

    #[error("Did not find a valid file extension from filename {0}")]
    NoFileExt(String),

    #[error("tree-sitter had an error")]
    LanguageError(#[from] tree_sitter::LanguageError),

    #[error("could not parse {0} with tree-sitter")]
    TSParseFailure(PathBuf),

    #[error("Some IO error was encountered")]
    IoError(#[from] io::Error),

    #[cfg(feature = "dynamic-grammar-libs")]
    #[error("Unable to dynamically load grammar")]
    LibloadingError(#[from] libloading::Error),
}

type StringMap = HashMap<String, String>;

/// Configuration options pertaining to loading grammars and parsing files.
#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct GrammarConfig {
    /// Set which dynamic library files should be used for different languages.
    ///
    /// This is a mapping from language strings to absolute file paths, relative filepaths, or
    /// file names.
    pub dylib_overrides: Option<StringMap>,

    /// Override the languages that get resolved for different extensions.
    ///
    /// This is a mapping from extension names to language strings. For example:
    /// ```txt
    /// "cpp" => "cpp"
    /// ```
    pub file_associations: Option<StringMap>,
}

/// Generate a [tree sitter language](Language) from a language string for a static language.
///
/// This will return an error if an unknown string is provided.
#[cfg(feature = "static-grammar-libs")]
fn generate_language_static(lang: &str) -> Result<Language, LoadingError> {
    info!("Using tree-sitter parser for language {}", lang);
    match LANGUAGES.get(lang) {
        Some(grammar_fn) => Ok(unsafe { grammar_fn() }),
        None => Err(LoadingError::StaticNotCompiled(lang.to_string())),
    }
}

/// Generate the method name to load a parser given the name of the language.
///
/// "tree-sitter-" will be prepended to the language and any dashes (-) will be converted
/// to underscores (_).
#[cfg(feature = "dynamic-grammar-libs")]
fn fn_name_from_lang(lang: &str) -> String {
    format!("tree_sitter_{}", lang.replace("-", "_"))
}

/// Generate the name of the library to `dlopen` given the name of the langauge.
///
/// "lib" will be prepended to the name of the language, and any underscores (_) will be converted
/// to dashes (-) and the appropriate extension will be applied based on the platform this binary
/// was compiled for.
#[cfg(feature = "dynamic-grammar-libs")]
fn lib_name_from_lang(lang: &str) -> String {
    let extension = if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(any(target_os = "linux", target_os = "netbsd")) {
        "so"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        panic!("Dynamic libraries are not supported for this platform.");
    };
    format!("libtree-sitter-{}.{}", lang.replace("_", "-"), extension)
}

/// Attempt to generate a tree-sitter grammar from a shared library
#[cfg(feature = "dynamic-grammar-libs")]
fn generate_language_dynamic(
    lang: &str,
    overrides: Option<&StringMap>,
) -> Result<Language, LoadingError> {
    let default_fname = lib_name_from_lang(lang);

    let lib_fname = if let Some(d) = overrides {
        debug!("Overriding dynamic library name because of user config");
        d.get(lang).unwrap_or(&default_fname)
    } else {
        &default_fname
    };
    info!("Loading dynamic library from {}", lib_fname);
    let fn_name = fn_name_from_lang(lang);
    debug!("Using name {} for dynamic function", fn_name);

    let grammar = unsafe {
        // We leak the library because the symbol table has to be loaded in memory for the
        // entire duration of the program up until the very end. There is probably a better way
        // to do this that doesn't involve leaking memory, but I wasn't able to figure it out.
        let ptr = Box::new(libloading::Library::new(lib_fname)?);
        let lib = Box::leak(ptr);
        let constructor =
            lib.get::<libloading::Symbol<unsafe extern "C" fn() -> Language>>(fn_name.as_bytes())?;
        constructor()
    };
    Ok(grammar)
}

/// Generate a langauge from a language string.
///
/// This is a dispatch method that will attempt to load a statically linked grammar, and then fall
/// back to loading the dynamic library for the grammar. If the user specifies an override for the
/// dynamic library then that will be prioritized first.
#[allow(clippy::vec_init_then_push)]
// `config` is not used if the `dynamic-grammar-libs` build flag isn't enabled
#[allow(unused)]
pub fn generate_language(lang: &str, config: &GrammarConfig) -> Result<Language, LoadingError> {
    // The candidates for the grammar, in order of precedence.
    let mut grammar_candidates = Vec::new();

    // Try the dynamic grammar first if there's a user override
    #[cfg(feature = "dynamic-grammar-libs")]
    if config.dylib_overrides.is_some() {
        grammar_candidates.push(generate_language_dynamic(
            lang,
            config.dylib_overrides.as_ref(),
        ));
    }

    // If there's no user override we prioritize the static/vendored grammar since there's much
    // better guarantees of that working correctly.
    #[cfg(feature = "static-grammar-libs")]
    grammar_candidates.push(generate_language_static(lang));

    #[cfg(feature = "dynamic-grammar-libs")]
    if config.dylib_overrides.is_none() {
        grammar_candidates.push(generate_language_dynamic(
            lang,
            config.dylib_overrides.as_ref(),
        ));
    }

    // Need to get the length of the vector here to prevent issues with borrowing in the loop
    let last_cand_idx = grammar_candidates.len() - 1;

    for (i, candidate_result) in grammar_candidates.into_iter().enumerate() {
        let is_last_cand = i == last_cand_idx;

        match candidate_result {
            Ok(grammar) => {
                info!("Succeeded loading grammar for {}", lang);
                return Ok(grammar);
            }
            Err(e) => {
                debug!("Failed to load candidate grammar for {}: {}", lang, &e);
                // Only error out on the last candidate, otherwise we want to keep falling back to
                // the next potential grammar
                if is_last_cand {
                    error!("Failed to load all candidate grammars for {}", lang);
                    return Err(e);
                }
            }
        };
    }
    error!("No grammars were loaded at all");
    Err(LoadingError::NoGrammars)
}

/// Get the language string that corresponds to an extension.
///
/// The user is optionally allowed to supply a map of overrides for these extensions, if none are
/// supplied, or if the given extension is not found in the map, this method will fall back to the
/// default map, `FILE_EXTS`.
pub fn resolve_language_str<'a>(
    ext: &str,
    overrides: Option<&'a HashMap<String, String>>,
) -> Option<&'a str> {
    let lang_from_override = {
        if let Some(overrides) = overrides {
            overrides.get(ext)
        } else {
            None
        }
    };
    let lang_from_defaults = FILE_EXTS.get(ext);

    if let Some(lang) = lang_from_override {
        info!(
            "Deduced language \"{}\" from extension \"{}\" provided from user mappings",
            lang, ext
        );
        Some(lang)
    } else if let Some(lang) = lang_from_defaults {
        info!(
            "Deduced language \"{}\" from extension \"{}\" from default mappings",
            lang, ext
        );
        Some(lang)
    } else {
        error!(
            "Was not able to find a language string for extension {}",
            ext
        );
        None
    }
}

/// Create an instance of a language from a file extension
///
/// The user may optionally provide a map of overrides or additional file extensions.
pub fn language_from_ext(
    ext: &str,
    grammar_config: &GrammarConfig,
) -> Result<Language, LoadingError> {
    let language_str_cand = resolve_language_str(ext, grammar_config.file_associations.as_ref());

    if let Some(language_str) = language_str_cand {
        generate_language(language_str, grammar_config)
    } else {
        Err(LoadingError::UnsupportedExt(ext.to_string()))
    }
}

/// Parse a file to an AST
///
/// The user may optionally supply the language to use. If the language is not supplied, it will be
/// inferrred from the file's extension.
#[time("info", "parse::{}")]
pub fn parse_file(
    p: &Path,
    language: Option<&str>,
    config: &GrammarConfig,
) -> Result<Tree, LoadingError> {
    let text = fs::read_to_string(p)?;
    let mut parser = Parser::new();
    let language = match language {
        Some(x) => {
            info!("Using language {} with parser", x);
            generate_language(x, config)
        }
        None => {
            if let Some(ext) = p.extension() {
                let ext_str = ext.to_string_lossy();
                language_from_ext(&ext_str, config)
            } else {
                Err(LoadingError::NoFileExt(p.to_string_lossy().to_string()))
            }
        }
    }?;
    parser.set_language(language)?;
    debug!("Constructed parser");

    match parser.parse(&text, None) {
        Some(ast) => {
            debug!("Parsed AST");
            Ok(ast)
        }
        None => Err(LoadingError::TSParseFailure(p.to_owned())),
    }
}

/// Return the languages supported by this instance of the tool in alphabetically sorted order
#[cfg(feature = "static-grammar-libs")]
pub fn supported_languages() -> Vec<&'static str> {
    if cfg!(feature = "static-grammar-libs") {
        let mut keys: Vec<&'static str> = LANGUAGES.keys().copied().collect();
        keys.sort_unstable();
        keys
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that every parser that this program was compiled to support can be loaded by the tree
    /// sitter [parser](tree_sitter::Parser)
    #[cfg(feature = "static-grammar-libs")]
    #[test]
    fn static_load_parsers() {
        // Collect all of the test failures in a vector so we can show a comprehensive error with
        // all of the failed languages instead of panicking one at a time
        let mut failures = Vec::new();

        for (&name, lang) in &LANGUAGES {
            let mut parser = tree_sitter::Parser::new();
            let result = parser.set_language(unsafe { lang() });

            if let Err(e) = result {
                failures.push((name, e));
            }
        }

        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[cfg(feature = "dynamic-grammar-libs")]
    #[test]
    #[ignore] // this test is only applicable in certain packaging scenarios
    fn dynamic_load_parsers() {
        let languages = vec![
            "rust", "cpp", "python", "bash", "ocaml", "go", "ruby", "java", "c_sharp", "css",
            "php", "json", "tsx", "hcl",
        ];
        let mut failures = Vec::new();

        for &name in &languages {
            if generate_language_dynamic(name, None).is_err() {
                failures.push(name);
            }
        }

        assert!(failures.is_empty(), "{:#?}", failures);
    }
}
