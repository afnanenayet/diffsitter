#![no_main]

use libdiffsitter::ast_navigation::run_query;
use libdiffsitter::parse::{self, GrammarConfig};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Need at least 1 byte of input.
    if data.is_empty() {
        return;
    }

    // Split at the first null byte, or at the midpoint if no null byte is found.
    let (source_bytes, query_bytes) = match data.iter().position(|&b| b == 0) {
        Some(pos) => (&data[..pos], &data[pos + 1..]),
        None => {
            let mid = data.len() / 2;
            (&data[..mid], &data[mid..])
        }
    };

    // Both halves must be valid UTF-8.
    let source_text = match std::str::from_utf8(source_bytes) {
        Ok(s) => s,
        Err(_) => return,
    };
    let query_str = match std::str::from_utf8(query_bytes) {
        Ok(s) => s,
        Err(_) => return,
    };

    let config = GrammarConfig::default();

    // Parse as Rust -- a single language suffices for query fuzzing.
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

    // Run the fuzzed query string. Malformed queries should produce errors, not panics.
    let _ = run_query(&tree, source_text, &ts_language, query_str);
});
