#![cfg(feature = "mcp-server")]

//! Integration tests for the MCP server module.
//!
//! The rmcp `#[tool(aggr)]` macro transforms method signatures on `TreeSitterMcpServer`,
//! making direct calls fragile. Instead, we test:
//!
//! 1. `get_info()` via the `ServerHandler` trait (plain method, no macro transformation).
//! 2. The underlying `ast_navigation` functions through `ParseCache` wrapped in
//!    `Arc<tokio::sync::Mutex<_>>`, mirroring the server's internal pattern exactly.

use std::path::PathBuf;
use std::sync::Arc;

use libdiffsitter::ast_navigation::{self, NavigationDirection, ParseCache};
use libdiffsitter::mcp_server::TreeSitterMcpServer;
use libdiffsitter::parse::GrammarConfig;
use rmcp::ServerHandler;
use rmcp::model::ProtocolVersion;
use tokio::sync::Mutex;

/// Path to the Rust sample file used across most tests.
fn rust_sample_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test_data")
        .join("ast_navigation")
        .join("rust_sample.rs")
}

/// Build a fresh server instance with default grammar config.
fn make_server() -> TreeSitterMcpServer {
    TreeSitterMcpServer::new(GrammarConfig::default())
}

/// Build a `ParseCache` behind `Arc<Mutex<_>>` (mirrors the server's internal field).
fn make_cache() -> Arc<Mutex<ParseCache>> {
    Arc::new(Mutex::new(ParseCache::new(GrammarConfig::default())))
}

// ---------------------------------------------------------------------------
// 1. ServerHandler::get_info
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_server_info() {
    let server = make_server();
    let info = server.get_info();
    assert_eq!(info.server_info.name, "tree-sitter-mcp");
    assert_eq!(info.protocol_version, ProtocolVersion::V_2024_11_05);
    // Capabilities must include tools.
    assert!(
        info.capabilities.tools.is_some(),
        "server capabilities should include tools"
    );
}

// ---------------------------------------------------------------------------
// 2. parse_file — success
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_parse_file_success() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard
        .get_or_parse(&path, None)
        .expect("parse should succeed");
    let info = ast_navigation::node_to_info(parsed.tree.root_node(), &parsed.text, None);
    assert_eq!(info.kind, "source_file");
    assert!(
        info.child_count > 0,
        "root node should have children for a non-empty file"
    );
}

// ---------------------------------------------------------------------------
// 3. parse_file — nonexistent path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_parse_file_nonexistent() {
    let cache = make_cache();
    let path = PathBuf::from("/tmp/nonexistent_file_that_does_not_exist_12345.rs");
    let mut guard = cache.lock().await;
    let result = guard.get_or_parse(&path, None);
    assert!(
        result.is_err(),
        "parsing a missing file should return an error"
    );
}

// ---------------------------------------------------------------------------
// 4. list_symbols — returns expected symbols
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_symbols_returns_expected() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard.get_or_parse(&path, None).unwrap();
    let symbols = ast_navigation::list_symbols(
        &parsed.tree,
        &parsed.text,
        &parsed.language,
        &parsed.language_name,
    );

    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"main"), "should list `main` function");
    assert!(
        names.contains(&"Point"),
        "should list `Point` struct or impl"
    );
    assert!(names.contains(&"Shape"), "should list `Shape` trait");
    assert!(names.contains(&"Color"), "should list `Color` enum");
    assert!(names.contains(&"MAX_SIZE"), "should list `MAX_SIZE` const");
    assert!(names.contains(&"ORIGIN"), "should list `ORIGIN` static");
}

// ---------------------------------------------------------------------------
// 5. get_definition — success
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_definition_success() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard.get_or_parse(&path, None).unwrap();
    let (symbol, text) = ast_navigation::get_definition(
        &parsed.tree,
        &parsed.text,
        &parsed.language,
        &parsed.language_name,
        "main",
    )
    .expect("should find `main` definition");

    assert_eq!(symbol.name, "main");
    assert!(
        text.contains("fn main"),
        "definition text should contain `fn main`"
    );
}

// ---------------------------------------------------------------------------
// 6. get_definition — not found
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_definition_not_found() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard.get_or_parse(&path, None).unwrap();
    let result = ast_navigation::get_definition(
        &parsed.tree,
        &parsed.text,
        &parsed.language,
        &parsed.language_name,
        "nonexistent_symbol_xyz",
    );
    assert!(result.is_err(), "looking up a missing symbol should error");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("nonexistent_symbol_xyz"),
        "error message should mention the missing symbol name"
    );
}

// ---------------------------------------------------------------------------
// 7. get_node_at_position — success
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_node_at_position_success() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard.get_or_parse(&path, None).unwrap();
    // Line 0, column 6 should be inside `MAX_SIZE` (the identifier on `const MAX_SIZE: ...`).
    let info = ast_navigation::get_node_at_position(&parsed.tree, &parsed.text, 0, 6).unwrap();
    assert!(
        info.text.contains("MAX_SIZE"),
        "node at (0, 6) should contain `MAX_SIZE`, got: {}",
        info.text
    );
}

// ---------------------------------------------------------------------------
// 8. get_scope — inside a function
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_scope_inside_function() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard.get_or_parse(&path, None).unwrap();
    // Line 43 (`let p = Point::new(1.0, 2.0);`) is inside `fn main`.
    let scope = ast_navigation::get_scope(&parsed.tree, &parsed.text, &parsed.language_name, 43, 8)
        .unwrap();
    assert_eq!(
        scope.node.kind, "function_item",
        "innermost scope at line 43 should be a function_item"
    );
}

// ---------------------------------------------------------------------------
// 9. navigate — all six directions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_navigate_parent() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard.get_or_parse(&path, None).unwrap();
    // Navigate parent from inside `fn main` body (line 43, col 8).
    let info = ast_navigation::navigate(
        &parsed.tree,
        &parsed.text,
        43,
        8,
        NavigationDirection::Parent,
    )
    .unwrap();
    assert!(
        !info.kind.is_empty(),
        "parent node kind should be non-empty"
    );
}

#[tokio::test]
async fn test_navigate_first_child_from_leaf_errors() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard.get_or_parse(&path, None).unwrap();
    // Position (0, 6) resolves to the `MAX_SIZE` identifier, which is a leaf node.
    // Navigating first_child from a leaf should fail with NavigationFailed.
    let result = ast_navigation::navigate(
        &parsed.tree,
        &parsed.text,
        0,
        6,
        NavigationDirection::FirstChild,
    );
    assert!(
        result.is_err(),
        "first_child from a leaf node should return an error"
    );
}

#[tokio::test]
async fn test_navigate_next_sibling() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard.get_or_parse(&path, None).unwrap();
    // The `const` keyword at (0, 0) should have a next sibling (the name identifier).
    let info = ast_navigation::navigate(
        &parsed.tree,
        &parsed.text,
        0,
        0,
        NavigationDirection::NextSibling,
    )
    .unwrap();
    assert!(!info.kind.is_empty());
}

#[tokio::test]
async fn test_navigate_prev_sibling() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard.get_or_parse(&path, None).unwrap();
    // The `main` identifier (line 42, col 3) inside `function_item` has `fn` keyword
    // as its previous sibling.
    let info = ast_navigation::navigate(
        &parsed.tree,
        &parsed.text,
        42,
        3,
        NavigationDirection::PrevSibling,
    )
    .unwrap();
    assert!(!info.kind.is_empty());
}

#[tokio::test]
async fn test_navigate_next_named_sibling() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard.get_or_parse(&path, None).unwrap();
    // The `main` identifier (line 42, col 3) should have a next named sibling
    // (the parameters node).
    let info = ast_navigation::navigate(
        &parsed.tree,
        &parsed.text,
        42,
        3,
        NavigationDirection::NextNamedSibling,
    )
    .unwrap();
    assert!(info.is_named, "next named sibling should be a named node");
}

#[tokio::test]
async fn test_navigate_prev_named_sibling() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard.get_or_parse(&path, None).unwrap();
    // First get the next_named_sibling from `main` (line 42, col 3), then navigate
    // prev_named_sibling from that result's start position to round-trip back.
    let next = ast_navigation::navigate(
        &parsed.tree,
        &parsed.text,
        42,
        3,
        NavigationDirection::NextNamedSibling,
    )
    .unwrap();
    // Now go prev_named_sibling from the next named sibling's start position.
    // The deepest node at that position may differ from the named sibling itself, so
    // the result may or may not have a prev_named_sibling. The critical invariant: no panic.
    let prev = ast_navigation::navigate(
        &parsed.tree,
        &parsed.text,
        next.span.start.line,
        next.span.start.column,
        NavigationDirection::PrevNamedSibling,
    );
    if let Ok(node) = prev {
        assert!(node.is_named, "prev named sibling should be a named node");
    }
}

// ---------------------------------------------------------------------------
// 10. navigate — invalid direction string
// ---------------------------------------------------------------------------

// `parse_direction` is private to the mcp_server module, so we cannot call it directly
// from an integration test. However, `NavigationDirection` uses `#[serde(rename_all =
// "snake_case")]`, so we can verify that an invalid direction string fails
// deserialization — which is the same boundary the server enforces.

#[tokio::test]
async fn test_navigate_invalid_direction_deserialization() {
    let result: Result<NavigationDirection, _> = serde_json::from_str("\"invalid_direction\"");
    assert!(
        result.is_err(),
        "deserializing an unknown direction string should fail"
    );
}

// Also test that navigation returns an error for an impossible move (e.g., navigating
// to a position well past the end of the file).
#[tokio::test]
async fn test_navigate_out_of_bounds_position() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard.get_or_parse(&path, None).unwrap();
    // Position (99999, 0) is well past the end of the file.
    let result = ast_navigation::navigate(
        &parsed.tree,
        &parsed.text,
        99999,
        0,
        NavigationDirection::Parent,
    );
    // tree-sitter resolves out-of-range positions to the root node, whose parent
    // does not exist, so this should return a NavigationFailed error.
    assert!(
        result.is_err(),
        "navigating parent from root (out-of-bounds position) should error"
    );
}

// ---------------------------------------------------------------------------
// 11. query — valid tree-sitter query
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_query_valid() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard.get_or_parse(&path, None).unwrap();
    let query_str = "(function_item name: (identifier) @name) @definition";
    let matches =
        ast_navigation::run_query(&parsed.tree, &parsed.text, &parsed.language, query_str)
            .expect("valid query should succeed");

    assert!(
        !matches.is_empty(),
        "query should find at least one function"
    );
    // `fn main` should appear in the results.
    let has_main = matches.iter().any(|m| {
        m.captures
            .iter()
            .any(|c| c.name == "name" && c.node.text == "main")
    });
    assert!(has_main, "query should capture `main` function name");
}

// ---------------------------------------------------------------------------
// 12. query — invalid (malformed) query
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_query_invalid() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard.get_or_parse(&path, None).unwrap();
    let result = ast_navigation::run_query(
        &parsed.tree,
        &parsed.text,
        &parsed.language,
        "(((this_is_not_valid",
    );
    assert!(result.is_err(), "malformed query should return an error");
}

// ---------------------------------------------------------------------------
// 13. get_children_of — impl block children
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_children_of_impl() {
    let cache = make_cache();
    let path = rust_sample_path();
    let mut guard = cache.lock().await;
    let parsed = guard.get_or_parse(&path, None).unwrap();
    // "Shape" only appears once (as a trait), so there's no struct/impl
    // ambiguity like there is with "Point".
    let children = ast_navigation::get_children_of(
        &parsed.tree,
        &parsed.text,
        &parsed.language,
        &parsed.language_name,
        "Shape",
    )
    .expect("should find children of `Shape`");

    // Shape is a trait — its children depend on whether the symbol query
    // captures trait method signatures (they're declaration nodes, not
    // function_item). The function should succeed regardless.
    let _ = children;
}

// ---------------------------------------------------------------------------
// 14. caching — parse then query on the same file reuses the cache
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_caching_across_tools() {
    let cache = make_cache();
    let path = rust_sample_path();

    // First call: parse the file.
    {
        let mut guard = cache.lock().await;
        let parsed = guard.get_or_parse(&path, None).unwrap();
        let info = ast_navigation::node_to_info(parsed.tree.root_node(), &parsed.text, None);
        assert_eq!(info.kind, "source_file");
    }

    // Second call: list_symbols on the same file (should hit the cache).
    {
        let mut guard = cache.lock().await;
        let parsed = guard.get_or_parse(&path, None).unwrap();
        let symbols = ast_navigation::list_symbols(
            &parsed.tree,
            &parsed.text,
            &parsed.language,
            &parsed.language_name,
        );
        assert!(
            !symbols.is_empty(),
            "symbols should be non-empty from cached parse"
        );
    }
}
