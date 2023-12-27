//! Utility functions that complement the diffsitter binary.

use anyhow::Result;
use clap::CommandFactory;
use clap::FromArgMatches;
use clap::Parser;
use libdiffsitter::parse::construct_ts_lang_from_shared_lib;
use std::path::PathBuf;

/// Utility functions that complement the diffsitter binary.
#[derive(Debug, Parser)]
#[clap(author, version, about)]
pub enum DiffsitterUtilsApp {
    /// Try loading a tree-sitter parser shared library object.
    ///
    /// You can use this command to check the validity of a tree-sitter parser shared library
    /// object file.
    ///
    /// If this operation succeeds, the binary will exist with code 0. Otherwise the exit code will
    /// be non-zero and an error will be printed.
    LoadParser {
        /// The name of the language/parser.
        ///
        /// This is used to get the name of the constructor that corresponds to the name of the
        /// symbol that is constructor method for the parser.
        ///
        /// This *must* be the tree-sitter name.
        ///
        /// For example: "C" will is turned to "tree-sitter-c".
        language_name: String,

        /// The path to the shared library object.
        parser_path: PathBuf,
    },
}

fn main() -> Result<()> {
    let command = DiffsitterUtilsApp::command();
    let matches = command.get_matches();
    let args = DiffsitterUtilsApp::from_arg_matches(&matches)?;
    match args {
        DiffsitterUtilsApp::LoadParser {
            language_name,
            parser_path,
        } => {
            construct_ts_lang_from_shared_lib(&language_name, &parser_path)?;
        }
    };
    Ok(())
}
