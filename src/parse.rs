//! Utilities for reading and parsing files with the diffsitter parser

use tree_sitter::Language;

// Declare foreign functions for building parsers

extern "C" {
    pub fn tree_sitter_c() -> Language;
}

extern "C" {
    pub fn tree_sitter_rust() -> Language;
}
