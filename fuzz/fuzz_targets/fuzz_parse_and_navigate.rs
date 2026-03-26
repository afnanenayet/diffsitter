#![no_main]

use libdiffsitter::ast_navigation::{
    get_node_at_position, get_scope, list_symbols, navigate, NavigationDirection,
};
use libdiffsitter::parse::{self, GrammarConfig};
use libfuzzer_sys::fuzz_target;

/// Languages to cycle through based on the first byte of input.
const LANGUAGES: &[&str] = &["rust", "python", "c", "go", "java", "cpp"];

fuzz_target!(|data: &[u8]| {
    // Need at least 2 bytes: one for language selection, one for source text.
    if data.len() < 2 {
        return;
    }

    let lang_index = (data[0] as usize) % LANGUAGES.len();
    let lang_name = LANGUAGES[lang_index];
    let source_bytes = &data[1..];

    // Only proceed with valid UTF-8 input.
    let source_text = match std::str::from_utf8(source_bytes) {
        Ok(s) => s,
        Err(_) => return,
    };

    let config = GrammarConfig::default();

    // Load the tree-sitter language. This may fail for legitimate reasons; skip if so.
    let ts_language = match parse::generate_language(lang_name, &config) {
        Ok(lang) => lang,
        Err(_) => return,
    };

    // Parse the source text.
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&ts_language).is_err() {
        return;
    }

    let tree = match parser.parse(source_text, None) {
        Some(t) => t,
        None => return,
    };

    let root = tree.root_node();
    let start = root.start_position();
    let end = root.end_position();

    // Compute several positions to exercise: start, end, midpoint, quarter points.
    let positions = [
        (start.row, start.column),
        (end.row, end.column),
        (
            (start.row + end.row) / 2,
            (start.column + end.column) / 2,
        ),
        (
            start.row + (end.row - start.row) / 4,
            start.column,
        ),
        (
            start.row + 3 * (end.row - start.row) / 4,
            end.column,
        ),
    ];

    for &(line, column) in &positions {
        // Exercise get_node_at_position.
        let _ = get_node_at_position(&tree, source_text, line, column);

        // Exercise navigate in all directions.
        let directions = [
            NavigationDirection::Parent,
            NavigationDirection::FirstChild,
            NavigationDirection::NextSibling,
            NavigationDirection::PrevSibling,
            NavigationDirection::NextNamedSibling,
            NavigationDirection::PrevNamedSibling,
        ];
        for direction in directions {
            let _ = navigate(&tree, source_text, line, column, direction);
        }

        // Exercise get_scope.
        let _ = get_scope(&tree, source_text, lang_name, line, column);
    }

    // Exercise list_symbols.
    let _ = list_symbols(&tree, source_text, &ts_language, lang_name);
});
