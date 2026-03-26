#![no_main]

use libdiffsitter::ast_navigation::node_to_info;
use libdiffsitter::parse::{self, GrammarConfig};
use libfuzzer_sys::fuzz_target;
use tree_sitter::Node;

/// Recursively walk the tree, calling `node_to_info` on every node.
///
/// This exercises text truncation at character boundaries, including multi-byte
/// characters that might land on awkward boundary positions.
fn walk_and_convert(node: Node, text: &str) {
    let _ = node_to_info(node, text, None);
    let _ = node_to_info(node, text, Some("test_field"));

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            walk_and_convert(cursor.node(), text);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fuzz_target!(|data: &[u8]| {
    // Input must be valid UTF-8.
    let source_text = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };

    let config = GrammarConfig::default();

    // Parse as Rust.
    let ts_language = match parse::generate_language("rust", &config) {
        Ok(lang) => lang,
        Err(_) => return,
    };

    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&ts_language).is_err() {
        return;
    }

    let tree = match parser.parse(source_text, None) {
        Some(t) => t,
        None => return,
    };

    // Walk the entire tree, calling node_to_info on every node.
    walk_and_convert(tree.root_node(), source_text);
});
