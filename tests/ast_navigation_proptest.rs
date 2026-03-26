//! Property-based tests for the `ast_navigation` module using `proptest`.

#[cfg(test)]
mod tests {
    use std::io::Write;

    use libdiffsitter::ast_navigation::*;
    use libdiffsitter::parse::{self, GrammarConfig};
    use proptest::prelude::*;
    use tempfile::NamedTempFile;
    use tree_sitter::{Language, Parser, Tree};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Parse a Rust source string and return the tree-sitter `Tree` and `Language`.
    fn parse_rust(source: &str) -> (Tree, Language) {
        let config = GrammarConfig::default();
        let language = parse::generate_language("rust", &config).expect("rust grammar available");
        let mut parser = Parser::new();
        parser
            .set_language(&language)
            .expect("set language on parser");
        let tree = parser
            .parse(source, None)
            .expect("tree-sitter can parse any input");
        (tree, language)
    }

    // -----------------------------------------------------------------------
    // Strategies
    // -----------------------------------------------------------------------

    /// Generate a source string containing 1-5 simple Rust functions with random names.
    fn arb_rust_fn_source() -> impl Strategy<Value = String> {
        (1..=5usize).prop_flat_map(|n| {
            proptest::collection::vec("[a-z][a-z0-9_]{0,10}", n).prop_map(|names| {
                names
                    .iter()
                    .map(|name| format!("fn {name}() {{}}\n"))
                    .collect::<String>()
            })
        })
    }

    /// Generate a source string with a struct containing an impl block with 1-3 methods,
    /// suitable for testing nested scopes.
    fn arb_nested_rust_source() -> impl Strategy<Value = String> {
        (
            "[A-Z][a-z]{1,8}",
            proptest::collection::vec("[a-z][a-z0-9_]{0,8}", 1..=3),
        )
            .prop_map(|(struct_name, method_names)| {
                let methods: String = method_names
                    .iter()
                    .map(|m| format!("    fn {m}(&self) {{}}\n"))
                    .collect();
                format!("struct {struct_name} {{}}\nimpl {struct_name} {{\n{methods}}}\n")
            })
    }

    /// Generate a string of arbitrary length (including very long ones) for text truncation
    /// testing.
    fn arb_varying_length_string() -> impl Strategy<Value = String> {
        prop_oneof![
            // Short strings
            "[a-z]{0,50}",
            // Medium strings around the 500-char boundary
            "[a-z]{490,510}",
            // Long strings well beyond the limit
            "[a-z]{1000,2000}",
        ]
    }

    /// Given a source text, return a strategy that produces a valid (line, column) position
    /// within the text bounds.
    #[allow(dead_code)]
    fn arb_valid_position(text: &str) -> BoxedStrategy<(usize, usize)> {
        let lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() {
            return Just((0usize, 0usize)).boxed();
        }
        // Build a vec of (line_index, max_col) pairs for non-empty lines.
        let valid_lines: Vec<(usize, usize)> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| !l.is_empty())
            .map(|(i, l)| (i, l.len() - 1))
            .collect();

        if valid_lines.is_empty() {
            // All lines are empty; position (0, 0) should still work for the root node.
            return Just((0usize, 0usize)).boxed();
        }

        // Pick a random valid line, then a random column within that line.
        let idx_range = 0..valid_lines.len();
        idx_range
            .prop_flat_map(move |idx| {
                let (line, max_col) = valid_lines[idx];
                (Just(line), 0..=max_col)
            })
            .boxed()
    }

    // -----------------------------------------------------------------------
    // Property tests
    // -----------------------------------------------------------------------

    proptest! {
        /// For any generated Rust source and any valid position within it,
        /// `get_node_at_position` must return `Ok`.
        #[test]
        fn node_at_valid_position_never_errors(source in arb_rust_fn_source()) {
            let (tree, _lang) = parse_rust(&source);

            // Derive a deterministic position from the source so we can test many.
            let lines: Vec<&str> = source.lines().collect();
            for (line_idx, line) in lines.iter().enumerate() {
                for col in 0..line.len() {
                    let result = get_node_at_position(&tree, &source, line_idx, col);
                    prop_assert!(
                        result.is_ok(),
                        "get_node_at_position failed at ({}, {}): {:?}",
                        line_idx,
                        col,
                        result.err()
                    );
                }
            }
        }

        /// Parsing the same source twice with separate parsers and calling `list_symbols`
        /// must produce identical results in the same order.
        #[test]
        fn list_symbols_is_deterministic(source in arb_rust_fn_source()) {
            let (tree1, lang1) = parse_rust(&source);
            let (tree2, lang2) = parse_rust(&source);

            let symbols1 = list_symbols(&tree1, &source, &lang1, "rust");
            let symbols2 = list_symbols(&tree2, &source, &lang2, "rust");

            prop_assert_eq!(symbols1.len(), symbols2.len(), "symbol count mismatch");
            for (s1, s2) in symbols1.iter().zip(symbols2.iter()) {
                prop_assert_eq!(&s1.name, &s2.name, "symbol name mismatch");
                prop_assert_eq!(&s1.kind, &s2.kind, "symbol kind mismatch");
                prop_assert_eq!(&s1.signature, &s2.signature, "signature mismatch");
            }
        }

        /// For any position where `get_scope` succeeds, each parent in the `parent_chain`
        /// must have a span that fully contains the child (monotonically widening spans).
        #[test]
        fn scope_chain_spans_are_monotonically_widening(source in arb_nested_rust_source()) {
            let (tree, _lang) = parse_rust(&source);

            // Find a position inside a method body: scan for "fn " inside the impl block.
            let lines: Vec<&str> = source.lines().collect();
            for (line_idx, line) in lines.iter().enumerate() {
                if let Some(col) = line.find("fn ") {
                    // Position at the start of a function keyword.
                    let result = get_scope(&tree, &source, "rust", line_idx, col);
                    if let Ok(scope_info) = result {
                        // The scope node itself is the innermost; parents should be wider.
                        let mut prev_start_line = scope_info.node.span.start.line;
                        let mut prev_start_col = scope_info.node.span.start.column;
                        let mut prev_end_line = scope_info.node.span.end.line;
                        let mut prev_end_col = scope_info.node.span.end.column;

                        for parent in &scope_info.parent_chain {
                            let p_start = (parent.span.start.line, parent.span.start.column);
                            let p_end = (parent.span.end.line, parent.span.end.column);

                            // Parent start must be <= child start.
                            prop_assert!(
                                p_start <= (prev_start_line, prev_start_col),
                                "parent start {:?} is after child start ({}, {})",
                                p_start,
                                prev_start_line,
                                prev_start_col
                            );
                            // Parent end must be >= child end.
                            prop_assert!(
                                p_end >= (prev_end_line, prev_end_col),
                                "parent end {:?} is before child end ({}, {})",
                                p_end,
                                prev_end_line,
                                prev_end_col
                            );

                            prev_start_line = parent.span.start.line;
                            prev_start_col = parent.span.start.column;
                            prev_end_line = parent.span.end.line;
                            prev_end_col = parent.span.end.column;
                        }
                    }
                }
            }
        }

        /// For any parsed node, `node_to_info` produces text whose byte length is at most
        /// 503 bytes (500 + "...").
        #[test]
        fn node_info_text_length_bounded(content in arb_varying_length_string()) {
            // Wrap the content in a Rust function so tree-sitter has something to parse.
            let source = format!("fn f() {{ let _x = \"{content}\"; }}");
            let (tree, _lang) = parse_rust(&source);

            // Walk every node in the tree and check the text length invariant.
            let root = tree.root_node();
            let mut stack = vec![root];

            while let Some(node) = stack.pop() {
                let info = node_to_info(node, &source, None);
                prop_assert!(
                    info.text.len() <= 503,
                    "node_to_info text too long: {} bytes for node kind '{}'",
                    info.text.len(),
                    info.kind
                );

                // Push children to visit the full tree.
                let child_count = node.child_count();
                for i in 0..child_count {
                    if let Some(child) = node.child(i as u32) {
                        stack.push(child);
                    }
                }
            }
        }

        /// For any node that is not the root, navigating `Parent` always succeeds.
        #[test]
        fn navigate_parent_always_succeeds_for_non_root(source in arb_rust_fn_source()) {
            let (tree, _lang) = parse_rust(&source);
            let root = tree.root_node();

            // Collect positions of all non-root named nodes.
            let mut stack = Vec::new();
            let child_count = root.child_count();
            for i in 0..child_count {
                if let Some(child) = root.child(i as u32) {
                    stack.push(child);
                }
            }

            while let Some(node) = stack.pop() {
                // This node is not the root, so Parent should succeed.
                let line = node.start_position().row;
                let col = node.start_position().column;
                let result = navigate(
                    &tree,
                    &source,
                    line,
                    col,
                    NavigationDirection::Parent,
                );
                prop_assert!(
                    result.is_ok(),
                    "navigate Parent failed at ({}, {}) for node kind '{}': {:?}",
                    line,
                    col,
                    node.kind(),
                    result.err()
                );

                // Push children for deeper traversal.
                let cc = node.child_count();
                for i in 0..cc {
                    if let Some(child) = node.child(i as u32) {
                        stack.push(child);
                    }
                }
            }
        }
    }

    /// For each language that has a `symbol_query_for_language`, the returned query string
    /// must compile as a valid tree-sitter `Query` for that language's grammar.
    #[test]
    fn symbol_queries_compile_for_all_known_languages() {
        let config = GrammarConfig::default();
        let languages_with_queries = ["rust", "python", "go", "java", "c", "cpp"];

        for lang_name in &languages_with_queries {
            let query_src = symbol_query_for_language(lang_name);
            assert!(
                query_src.is_some(),
                "expected a symbol query for language '{lang_name}'"
            );
            let query_src = query_src.unwrap();

            let ts_language = parse::generate_language(lang_name, &config).unwrap_or_else(|e| {
                panic!("failed to load grammar for '{lang_name}': {e}");
            });

            let result = tree_sitter::Query::new(&ts_language, query_src);
            assert!(
                result.is_ok(),
                "symbol query for '{lang_name}' failed to compile: {:?}",
                result.err()
            );
        }
    }

    /// Writing a file, parsing it via `ParseCache` twice (without modification) yields the
    /// same `language_name` and root node kind.
    #[test]
    fn cache_idempotence() {
        let source = "fn hello() {}\nfn world() {}\n";
        let mut tmp = NamedTempFile::with_suffix(".rs").expect("create temp file");
        tmp.write_all(source.as_bytes()).expect("write temp file");
        tmp.flush().expect("flush temp file");

        let config = GrammarConfig::default();
        let mut cache = ParseCache::new(config);

        let path = tmp.path();

        // First parse.
        let parsed1 = cache.get_or_parse(path, Some("rust")).expect("first parse");
        let lang_name1 = parsed1.language_name.clone();
        let root_kind1 = parsed1.tree.root_node().kind().to_string();

        // Second parse (should hit the cache).
        let parsed2 = cache
            .get_or_parse(path, Some("rust"))
            .expect("second parse");
        let lang_name2 = parsed2.language_name.clone();
        let root_kind2 = parsed2.tree.root_node().kind().to_string();

        assert_eq!(lang_name1, lang_name2, "language_name must be stable");
        assert_eq!(root_kind1, root_kind2, "root node kind must be stable");
    }
}
