//! Utilities for reading and parsing files with the diffsitter parser

include!(concat!(env!("OUT_DIR"), "/generated_grammar.rs"));

use anyhow::{format_err, Result};
use std::{fs, path::Path};
use tree_sitter::{Parser, Tree};

/// A mapping of file extensions to their associated languages
///
/// The languages correspond to grammars from `tree-sitter`
static FILE_EXTS: phf::Map<&'static str, &'static str> = phf_map! {
    "hs" => "haskell",
    "rs" => "rust",
    "go" => "go",
    "c" => "c",
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
};

fn generate_language(lang: &str) -> Result<Language> {
    match LANGUAGES.get(lang) {
        Some(grammar_fn) => Ok(unsafe { grammar_fn() }),
        None => Err(format_err!("Unsupported language {}", lang)),
    }
}

/// Create an instance of a language from a file extension
pub fn language_from_ext(ext: &str) -> Result<Language> {
    let language_str = match FILE_EXTS.get(ext) {
        Some(&language_str) => Ok(language_str),
        None => Err(format_err!("Unsupported filetype {}", ext)),
    }?;
    generate_language(language_str)
}

/// Parse a file to an AST
///
/// The user may optionally supply the language to use. If the language is not supplied, it will be
/// inferrred from the file's extension.
pub fn parse_file(p: &Path, language: Option<&str>) -> Result<Tree> {
    let text = fs::read_to_string(p)?;
    let mut parser = Parser::new();
    let language = match language {
        Some(x) => generate_language(x),
        None => {
            let ext = p.extension().unwrap_or_default().to_string_lossy();
            language_from_ext(&ext)
        }
    }?;
    parser.set_language(language).unwrap();

    match parser.parse(&text, None) {
        Some(ast) => Ok(ast),
        None => Err(format_err!("Failed to parse file: {}", p.to_string_lossy())),
    }
}
