#![no_main]

use libdiffsitter::input_processing::{TreeSitterProcessor, VectorData};
use libdiffsitter::parse::{self, GrammarConfig};
use libdiffsitter::tree_diff::{TreeDiffError, TreeDiffOptions, tree_diff};
use libfuzzer_sys::fuzz_target;
use std::path::PathBuf;

fuzz_target!(|data: &[u8]| {
    // Layout: [split byte][old source bytes][new source bytes].
    if data.len() < 3 {
        return;
    }
    let body = &data[1..];
    let split = (data[0] as usize) % (body.len() + 1);
    let (old_bytes, new_bytes) = body.split_at(split);
    let (Ok(old_text), Ok(new_text)) = (
        std::str::from_utf8(old_bytes),
        std::str::from_utf8(new_bytes),
    ) else {
        return;
    };

    let Ok(language) = parse::generate_language("rust", &GrammarConfig::default()) else {
        return;
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return;
    }
    let (Some(old_tree), Some(new_tree)) =
        (parser.parse(old_text, None), parser.parse(new_text, None))
    else {
        return;
    };
    let old = VectorData {
        text: old_text.to_string(),
        tree: old_tree,
        path: PathBuf::from("a.rs"),
        resolved_language: "rust".into(),
    };
    let new = VectorData {
        text: new_text.to_string(),
        tree: new_tree,
        path: PathBuf::from("b.rs"),
        resolved_language: "rust".into(),
    };

    // Small bound keeps each fuzz iteration fast; BoundExceeded is a valid
    // outcome, any other error or panic is a bug.
    let processor = TreeSitterProcessor::default();
    let options = TreeDiffOptions { max_tau: 64 };
    match tree_diff(&processor, &old, &new, &options) {
        Ok(diff) => {
            // Unit costs: every edit costs exactly 1, so counts must agree.
            assert_eq!(diff.edits.len() as u32, diff.distance);
            // Determinism.
            let again = tree_diff(&processor, &old, &new, &options).unwrap();
            assert_eq!(diff, again);
        }
        Err(TreeDiffError::BoundExceeded { .. }) => {}
        Err(other) => panic!("unexpected tree diff error: {other}"),
    }
});
