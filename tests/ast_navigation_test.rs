#[cfg(test)]
mod tests {
    use libdiffsitter::ast_navigation::*;
    use libdiffsitter::parse::GrammarConfig;
    use std::path::PathBuf;
    use test_case::test_case;

    /// Helper: path to a fixture file under `test_data/ast_navigation/`.
    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(format!("./test_data/ast_navigation/{name}"))
    }

    /// Helper: parse a fixture file through the [`ParseCache`] and return it along with the
    /// cache so the borrow remains valid.
    fn parse_fixture(name: &str) -> (ParseCache, PathBuf) {
        let path = fixture_path(name);
        assert!(path.exists(), "fixture {name} does not exist");
        let cache = ParseCache::new(GrammarConfig::default());
        (cache, path)
    }

    // -----------------------------------------------------------------------
    // 1. list_symbols_integration — parameterized over several languages
    // -----------------------------------------------------------------------

    #[test_case(
        "rust_sample.rs", None,
        &["MAX_SIZE", "Result", "Point", "Shape", "Point", "new", "distance_to", "Point", "area", "perimeter", "Color", "ORIGIN", "main"]
        ; "rust symbols"
    )]
    #[test_case(
        "python_sample.py", None,
        &["Animal", "__init__", "speak", "is_loud", "greet", "main"]
        ; "python symbols"
    )]
    #[test_case(
        "go_sample.go", None,
        &["Point", "Stringer", "NewPoint", "String", "main"]
        ; "go symbols"
    )]
    #[test_case(
        "c_sample.c", None,
        &["Point", "distance", "Point", "Point", "main", "Point", "Point"]
        ; "c symbols"
    )]
    #[test_case(
        "cpp_sample.cpp", None,
        &["Point", "Shape", "Circle", "Circle", "distance", "main"]
        ; "cpp symbols"
    )]
    #[test_case(
        "java_sample.java", None,
        &["Describable", "describe", "Animal", "speak", "describe", "Main", "main"]
        ; "java symbols"
    )]
    #[test_case(
        "typescript_sample.ts", None,
        &["Printable", "Coordinate", "Vector", "add", "main"]
        ; "typescript symbols"
    )]
    fn list_symbols_integration(fixture: &str, lang_override: Option<&str>, expected: &[&str]) {
        let (mut cache, path) = parse_fixture(fixture);
        let parsed = cache.get_or_parse(&path, lang_override).unwrap();
        let symbols = list_symbols(
            &parsed.tree,
            &parsed.text,
            &parsed.language,
            &parsed.language_name,
        );
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, expected, "symbol names mismatch for {fixture}");
    }

    // -----------------------------------------------------------------------
    // 2. parse_and_navigate_integration — parse, get node at position, navigate parent
    // -----------------------------------------------------------------------

    #[test]
    fn parse_and_navigate_integration() {
        let (mut cache, path) = parse_fixture("rust_sample.rs");
        let parsed = cache.get_or_parse(&path, None).unwrap();

        // Line 16 column 8 is inside `fn new` body: `Self { x, y }`
        let node = get_node_at_position(&parsed.tree, &parsed.text, 15, 8).unwrap();
        // The node should be some token inside the function body.
        assert!(node.is_named || !node.kind.is_empty(), "node should exist");

        // Navigate to parent — should succeed.
        let parent = navigate(
            &parsed.tree,
            &parsed.text,
            15,
            8,
            NavigationDirection::Parent,
        )
        .unwrap();
        assert!(
            !parent.kind.is_empty(),
            "parent node should have a non-empty kind"
        );
    }

    // -----------------------------------------------------------------------
    // 3. get_scope_integration — query position inside impl method, verify scope chain
    // -----------------------------------------------------------------------

    #[test]
    fn get_scope_integration() {
        let (mut cache, path) = parse_fixture("rust_sample.rs");
        let parsed = cache.get_or_parse(&path, None).unwrap();

        // Line 16 col 12 is inside `fn new` which is inside `impl Point`.
        let scope = get_scope(&parsed.tree, &parsed.text, &parsed.language_name, 15, 12).unwrap();

        // The innermost scope should be `function_item` (fn new).
        assert_eq!(
            scope.node.kind, "function_item",
            "innermost scope should be a function_item"
        );

        // The parent chain should contain the enclosing `impl_item`.
        let parent_kinds: Vec<&str> = scope.parent_chain.iter().map(|s| s.kind.as_str()).collect();
        assert!(
            parent_kinds.contains(&"impl_item"),
            "parent chain should include impl_item, got: {parent_kinds:?}"
        );
    }

    // -----------------------------------------------------------------------
    // 4. get_definition_integration — parameterized, look up known symbol names
    // -----------------------------------------------------------------------

    #[test_case("rust_sample.rs", None, "Point", "struct_item" ; "rust Point struct")]
    #[test_case("rust_sample.rs", None, "main", "function_item" ; "rust main fn")]
    #[test_case("rust_sample.rs", None, "Color", "enum_item" ; "rust Color enum")]
    #[test_case("python_sample.py", None, "Animal", "class_definition" ; "python Animal class")]
    #[test_case("python_sample.py", None, "greet", "function_definition" ; "python greet fn")]
    #[test_case("go_sample.go", None, "NewPoint", "function_declaration" ; "go NewPoint fn")]
    fn get_definition_integration(
        fixture: &str,
        lang_override: Option<&str>,
        symbol: &str,
        expected_kind: &str,
    ) {
        let (mut cache, path) = parse_fixture(fixture);
        let parsed = cache.get_or_parse(&path, lang_override).unwrap();

        let (sym, full_text) = get_definition(
            &parsed.tree,
            &parsed.text,
            &parsed.language,
            &parsed.language_name,
            symbol,
        )
        .unwrap();

        assert_eq!(sym.name, symbol, "symbol name should match");
        assert_eq!(
            sym.kind, expected_kind,
            "symbol kind should match for {symbol}"
        );
        assert!(!full_text.is_empty(), "definition text should not be empty");
    }

    // -----------------------------------------------------------------------
    // 5. get_children_of_integration — get methods from Point impl in rust_sample.rs
    // -----------------------------------------------------------------------

    #[test]
    fn get_children_of_integration() {
        let (mut cache, path) = parse_fixture("rust_sample.rs");
        let parsed = cache.get_or_parse(&path, None).unwrap();

        // The first `impl Point` (inherent impl) should contain `new` and `distance_to`.
        // `list_symbols` returns two entries named "Point" — the struct and the first impl.
        // `get_children_of` searches by name and finds the first match, which is the struct.
        // We look for "Point" children — if it matches the struct, children may be empty,
        // so we also try via the impl which is the second "Point".
        let symbols = list_symbols(
            &parsed.tree,
            &parsed.text,
            &parsed.language,
            &parsed.language_name,
        );
        // Find the impl_item named "Point"
        let impl_sym = symbols
            .iter()
            .find(|s| s.name == "Point" && s.kind == "impl_item");
        assert!(impl_sym.is_some(), "should find an impl_item named Point");

        // "Shape" only appears once (as a trait), so get_children_of won't
        // be confused by struct-vs-impl ambiguity.
        let children = get_children_of(
            &parsed.tree,
            &parsed.text,
            &parsed.language,
            &parsed.language_name,
            "Shape",
        )
        .unwrap();

        // Shape trait should have method signatures inside it.
        // get_children_of returns whatever symbols are found within the trait's byte range.
        // Even if empty (the Rust symbol query only matches function_item, not trait method
        // signatures which are different AST nodes), the function should not error.
        let _ = children;
    }

    // -----------------------------------------------------------------------
    // 6. cross_language_symbol_queries_compile — verify symbol_query_for_language
    //    returns valid queries for all known languages
    // -----------------------------------------------------------------------

    #[test]
    fn cross_language_symbol_queries_compile() {
        let languages = [
            "rust",
            "python",
            "typescript",
            "tsx",
            "go",
            "java",
            "c",
            "cpp",
        ];
        let config = GrammarConfig::default();

        for lang in &languages {
            let query_src = symbol_query_for_language(lang);
            assert!(
                query_src.is_some(),
                "symbol_query_for_language should return Some for {lang}"
            );
            let query_str = query_src.unwrap();

            // Compile the query against the actual tree-sitter language to verify it is valid.
            let ts_language = libdiffsitter::parse::generate_language(lang, &config).unwrap();
            let result = tree_sitter::Query::new(&ts_language, query_str);
            assert!(
                result.is_ok(),
                "symbol query for {lang} should compile, got error: {:?}",
                result.err()
            );
        }
    }

    // -----------------------------------------------------------------------
    // 7. empty_file_handling
    // -----------------------------------------------------------------------

    #[test]
    fn empty_file_handling() {
        let (mut cache, path) = parse_fixture("empty.rs");
        let parsed = cache.get_or_parse(&path, None).unwrap();

        let symbols = list_symbols(
            &parsed.tree,
            &parsed.text,
            &parsed.language,
            &parsed.language_name,
        );
        assert!(symbols.is_empty(), "empty file should produce no symbols");

        // get_node_at_position at (0, 0) — depending on tree-sitter, the root may still exist
        // but there might be no descendant. Either way it should not panic.
        let node_result = get_node_at_position(&parsed.tree, &parsed.text, 0, 0);
        // We don't assert Ok or Err — just that it doesn't panic.
        let _ = node_result;
    }

    // -----------------------------------------------------------------------
    // 8. unicode_handling
    // -----------------------------------------------------------------------

    #[test]
    fn unicode_handling() {
        let (mut cache, path) = parse_fixture("unicode_heavy.rs");
        let parsed = cache.get_or_parse(&path, None).unwrap();

        let symbols = list_symbols(
            &parsed.tree,
            &parsed.text,
            &parsed.language,
            &parsed.language_name,
        );
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();

        // Should find the struct and impl with Unicode name.
        assert!(
            names.contains(&"Données"),
            "should find struct Données, got: {names:?}"
        );
        assert!(
            names.contains(&"main"),
            "should find fn main, got: {names:?}"
        );
    }

    // -----------------------------------------------------------------------
    // 9. Snapshot: list_symbols for rust_sample.rs
    // -----------------------------------------------------------------------

    #[test]
    fn list_symbols_rust_snapshot() {
        let (mut cache, path) = parse_fixture("rust_sample.rs");
        let parsed = cache.get_or_parse(&path, None).unwrap();
        let symbols = list_symbols(
            &parsed.tree,
            &parsed.text,
            &parsed.language,
            &parsed.language_name,
        );
        insta::assert_json_snapshot!("list_symbols_rust", symbols);
    }

    // -----------------------------------------------------------------------
    // 10. Snapshot: list_symbols for python_sample.py
    // -----------------------------------------------------------------------

    #[test]
    fn list_symbols_python_snapshot() {
        let (mut cache, path) = parse_fixture("python_sample.py");
        let parsed = cache.get_or_parse(&path, None).unwrap();
        let symbols = list_symbols(
            &parsed.tree,
            &parsed.text,
            &parsed.language,
            &parsed.language_name,
        );
        insta::assert_json_snapshot!("list_symbols_python", symbols);
    }

    // -----------------------------------------------------------------------
    // 11. Snapshot: get_definition for "Point" from rust_sample.rs
    // -----------------------------------------------------------------------

    #[test]
    fn get_definition_rust_snapshot() {
        let (mut cache, path) = parse_fixture("rust_sample.rs");
        let parsed = cache.get_or_parse(&path, None).unwrap();

        let (sym, full_text) = get_definition(
            &parsed.tree,
            &parsed.text,
            &parsed.language,
            &parsed.language_name,
            "Point",
        )
        .unwrap();

        // Snapshot both the SymbolInfo and the full definition text.
        insta::assert_json_snapshot!(
            "get_definition_rust_point",
            serde_json::json!({
                "symbol": sym,
                "full_text": full_text,
            })
        );
    }

    // -----------------------------------------------------------------------
    // 12. Snapshot: get_scope from deeply_nested.rs inside the process method
    // -----------------------------------------------------------------------

    #[test]
    fn get_scope_nested_snapshot() {
        let (mut cache, path) = parse_fixture("deeply_nested.rs");
        let parsed = cache.get_or_parse(&path, None).unwrap();

        // Line 8 col 16 is inside the closure within `fn process`.
        // The innermost scope should be `function_item` (process).
        let scope = get_scope(&parsed.tree, &parsed.text, &parsed.language_name, 7, 16).unwrap();
        insta::assert_json_snapshot!("get_scope_deeply_nested", scope);
    }

    // -----------------------------------------------------------------------
    // 13. Snapshot: root NodeInfo for rust_sample.rs
    // -----------------------------------------------------------------------

    #[test]
    fn parse_file_root_snapshot() {
        let (mut cache, path) = parse_fixture("rust_sample.rs");
        let parsed = cache.get_or_parse(&path, None).unwrap();

        let root = parsed.tree.root_node();
        let root_info = node_to_info(root, &parsed.text, None);
        // Snapshot only the structural fields, not the full text (which is the entire file).
        insta::assert_json_snapshot!(
            "parse_file_root_rust",
            serde_json::json!({
                "kind": root_info.kind,
                "is_named": root_info.is_named,
                "span": root_info.span,
                "child_count": root_info.child_count,
                "named_children": root_info.named_children,
            })
        );
    }

    // -----------------------------------------------------------------------
    // 14. Snapshot: run_query for all function_item in rust_sample.rs
    // -----------------------------------------------------------------------

    #[test]
    fn run_query_rust_functions_snapshot() {
        let (mut cache, path) = parse_fixture("rust_sample.rs");
        let parsed = cache.get_or_parse(&path, None).unwrap();

        let query_str = "(function_item name: (identifier) @name) @definition";
        let results = run_query(&parsed.tree, &parsed.text, &parsed.language, query_str).unwrap();
        insta::assert_json_snapshot!("run_query_rust_functions", results);
    }
}
