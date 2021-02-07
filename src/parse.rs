//! Utilities for reading and parsing files with the diffsitter parser

include!(concat!(env!("OUT_DIR"), "/generated_grammar.rs"));

use anyhow::{format_err, Result};
use log::info;
use logging_timer::time;
use std::collections::HashMap;
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
};

fn generate_language(lang: &str) -> Result<Language> {
    info!("Using tree-sitter parser for language {}", lang);
    match LANGUAGES.get(lang) {
        Some(grammar_fn) => Ok(unsafe { grammar_fn() }),
        None => Err(format_err!("Unsupported language {}", lang)),
    }
}

/// Create an instance of a language from a file extension
///
/// The user may optionally provide a hashmap with overrides
pub fn language_from_ext(
    ext: &str,
    overrides: Option<&HashMap<String, String>>,
) -> Result<Language> {
    if let Some(Some(language_str)) = overrides.map(|x| x.get(ext)) {
        info!(
            "Deduced language \"{}\" from extension \"{}\" provided from user mappings",
            language_str, ext
        );
        return generate_language(language_str);
    };
    let language_str = match FILE_EXTS.get(ext) {
        Some(&language_str) => {
            info!(
                "Deduced language \"{}\" from extension \"{}\" from default mappings",
                language_str, ext
            );
            Ok(language_str)
        }
        None => Err(format_err!("Unsupported filetype \"{}\"", ext)),
    }?;
    generate_language(language_str)
}

/// Parse a file to an AST
///
/// The user may optionally supply the language to use. If the language is not supplied, it will be
/// inferrred from the file's extension.
#[time("info", "parse::{}")]
pub fn parse_file(
    p: &Path,
    language: Option<&str>,
    overrides: Option<&HashMap<String, String>>,
) -> Result<Tree> {
    let text = fs::read_to_string(p)?;
    let mut parser = Parser::new();
    let language = match language {
        Some(x) => generate_language(x),
        None => {
            let ext = p.extension().unwrap_or_default().to_string_lossy();
            language_from_ext(&ext, overrides)
        }
    }?;
    parser.set_language(language).unwrap();
    info!("Constructed parser");

    match parser.parse(&text, None) {
        Some(ast) => {
            info!("Parsed AST");
            Ok(ast)
        }
        None => Err(format_err!("Failed to parse file: {}", p.to_string_lossy())),
    }
}

/// Return the languages supported by this instance of the tool in alphabetically sorted order
pub fn supported_languages() -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = LANGUAGES.keys().copied().collect();
    keys.sort_unstable();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that every parser that this program was compiled to support can be loaded by the tree
    /// sitter [parser](tree_sitter::Parser)
    #[test]
    fn test_loading_languages() {
        for (_, lang) in &LANGUAGES {
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(unsafe { lang() }).unwrap();
        }
    }
}
