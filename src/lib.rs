//! The supporting library for `diffsitter`.
//!
//! This library is not particularly cohesive or well organized. It exists to support the
//! diffsitter binary and I am open to refactoring it and organizing it better if there's demand.
//!
//! In the meantime, buyer beware.
//!
//! All of the methods used to create diffsitter are here and we have attempted to keep the library
//! at least somewhat sane and organized for our own usage.

pub mod cli;
pub mod config;
pub mod console_utils;
pub mod diff;
pub mod input_processing;
pub mod neg_idx_vec;
pub mod parse;
pub mod render;

use anyhow::Result;
use input_processing::VectorData;
use log::{debug, info};
use parse::GrammarConfig;
use std::{fs, path::PathBuf};

/// Create an AST vector from a path
///
/// This returns an `AstVector` and a pinned struct with the owned data, which the `AstVector`
/// references.
///
/// `data` is used as an out-parameter. We need some external struct we can reference because the
/// return type references the data in that struct.
///
/// This returns an anyhow [Result], which is bad practice for a library and will need to be
/// refactored in the future. This method was originally used in the `diffsitter` binary so we
/// didn't feel the need to specify a specific error type.
pub fn generate_ast_vector_data(
    path: PathBuf,
    file_type: Option<&str>,
    grammar_config: &GrammarConfig,
) -> Result<VectorData> {
    let text = fs::read_to_string(&path)?;
    let file_name = path.to_string_lossy();
    debug!("Reading {} to string", file_name);

    if let Some(file_type) = file_type {
        info!(
            "Using user-set filetype \"{}\" for {}",
            file_type, file_name
        );
    } else {
        info!("Will deduce filetype from file extension");
    };
    let (tree, resolved_language) = parse::parse_file(&path, file_type, grammar_config)?;
    Ok(VectorData {
        text,
        tree,
        path,
        resolved_language,
    })
}
