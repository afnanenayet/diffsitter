//! AST navigation utilities for querying and traversing tree-sitter syntax trees.
//!
//! This module provides high-level functions for inspecting nodes, navigating the tree,
//! running tree-sitter queries, listing symbols, and caching parsed files.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tree_sitter::{Language, Node, Point, Query, QueryCursor, StreamingIterator, Tree};

use crate::parse::{self, GrammarConfig, LoadingError};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum length of inline text stored in a [`NodeInfo`]. Text beyond this limit is truncated.
const MAX_INLINE_TEXT_LEN: usize = 500;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during AST navigation operations.
#[derive(Error, Debug)]
pub enum NavigationError {
    #[error("Failed to parse file: {0}")]
    ParseError(#[from] LoadingError),

    #[error("No node found at position {line}:{column}")]
    NoNodeAtPosition { line: usize, column: usize },

    #[error("No enclosing scope found at position {line}:{column}")]
    NoScopeAtPosition { line: usize, column: usize },

    #[error("Navigation {direction} from {line}:{column} yielded no node")]
    NavigationFailed {
        direction: String,
        line: usize,
        column: usize,
    },

    #[error("Invalid tree-sitter query: {0}")]
    InvalidQuery(String),

    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// A position in a source file (zero-indexed line and column).
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl From<Point> for Position {
    fn from(p: Point) -> Self {
        Self {
            line: p.row,
            column: p.column,
        }
    }
}

/// A contiguous range in a source file.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

/// Summary information about a single child node.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
pub struct ChildSummary {
    pub kind: String,
    pub field_name: Option<String>,
    pub span: Span,
}

/// Detailed information about a single AST node.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
pub struct NodeInfo {
    pub kind: String,
    pub is_named: bool,
    pub span: Span,
    pub text: String,
    pub child_count: usize,
    pub field_name: Option<String>,
    pub named_children: Vec<ChildSummary>,
}

/// Information about a symbol (function, struct, class, etc.) in a source file.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
pub struct SymbolInfo {
    pub name: String,
    pub kind: String,
    pub span: Span,
    pub signature: String,
}

/// Summary of a scope node used in the parent chain of [`ScopeInfo`].
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
pub struct ScopeSummary {
    pub kind: String,
    pub name: Option<String>,
    pub span: Span,
}

/// Detailed scope information including the scope node and its parent chain.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
pub struct ScopeInfo {
    pub node: NodeInfo,
    pub parent_chain: Vec<ScopeSummary>,
}

/// A single capture from a tree-sitter query match.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
pub struct CaptureResult {
    pub name: String,
    pub node: NodeInfo,
}

/// A complete match from a tree-sitter query, containing the pattern index and all captures.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
pub struct QueryMatchResult {
    pub pattern_index: usize,
    pub captures: Vec<CaptureResult>,
}

/// Direction for navigating from one node to a related node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum NavigationDirection {
    Parent,
    FirstChild,
    NextSibling,
    PrevSibling,
    NextNamedSibling,
    PrevNamedSibling,
}

impl std::fmt::Display for NavigationDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parent => write!(f, "parent"),
            Self::FirstChild => write!(f, "first_child"),
            Self::NextSibling => write!(f, "next_sibling"),
            Self::PrevSibling => write!(f, "prev_sibling"),
            Self::NextNamedSibling => write!(f, "next_named_sibling"),
            Self::PrevNamedSibling => write!(f, "prev_named_sibling"),
        }
    }
}

// ---------------------------------------------------------------------------
// Core functions
// ---------------------------------------------------------------------------

/// Extract a [`NodeInfo`] from a tree-sitter [`Node`].
///
/// The `text` parameter should be the full source text of the file. If `field_name` is
/// provided it is stored on the returned struct; otherwise the field is `None`.
pub fn node_to_info(node: Node, text: &str, field_name: Option<&str>) -> NodeInfo {
    let start = node.start_position();
    let end = node.end_position();

    let node_text = node.utf8_text(text.as_bytes()).unwrap_or("").to_string();
    let truncated = if node_text.len() > MAX_INLINE_TEXT_LEN {
        let mut t = String::with_capacity(MAX_INLINE_TEXT_LEN + 3);
        // Truncate at a char boundary
        let boundary = node_text
            .char_indices()
            .take_while(|(i, _)| *i < MAX_INLINE_TEXT_LEN)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        t.push_str(&node_text[..boundary]);
        t.push_str("...");
        t
    } else {
        node_text
    };

    let mut named_children = Vec::new();
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.is_named() {
                let child_field = cursor.field_name().map(String::from);
                named_children.push(ChildSummary {
                    kind: child.kind().to_string(),
                    field_name: child_field,
                    span: Span {
                        start: child.start_position().into(),
                        end: child.end_position().into(),
                    },
                });
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    NodeInfo {
        kind: node.kind().to_string(),
        is_named: node.is_named(),
        span: Span {
            start: start.into(),
            end: end.into(),
        },
        text: truncated,
        child_count: node.child_count(),
        field_name: field_name.map(String::from),
        named_children,
    }
}

/// Find the most specific node at the given `(line, column)` position.
///
/// Returns a [`NodeInfo`] describing the node, or [`NavigationError::NoNodeAtPosition`] if no
/// node covers that position.
pub fn get_node_at_position(
    tree: &Tree,
    text: &str,
    line: usize,
    column: usize,
) -> Result<NodeInfo, NavigationError> {
    let point = Point { row: line, column };
    let node = tree
        .root_node()
        .descendant_for_point_range(point, point)
        .ok_or(NavigationError::NoNodeAtPosition { line, column })?;
    Ok(node_to_info(node, text, None))
}

/// Find the innermost enclosing scope at the given position and build the parent scope chain.
///
/// "Scope" is defined per-language via [`scope_kinds_for_language`]. If no scope-like ancestor
/// is found, returns [`NavigationError::NoScopeAtPosition`].
pub fn get_scope(
    tree: &Tree,
    text: &str,
    language_name: &str,
    line: usize,
    column: usize,
) -> Result<ScopeInfo, NavigationError> {
    let point = Point { row: line, column };
    let start_node = tree
        .root_node()
        .descendant_for_point_range(point, point)
        .ok_or(NavigationError::NoNodeAtPosition { line, column })?;

    let scope_kinds = scope_kinds_for_language(language_name);

    // Walk up to find the innermost scope.
    let mut current = Some(start_node);
    let mut scope_node = None;
    while let Some(n) = current {
        if scope_kinds.contains(&n.kind()) {
            scope_node = Some(n);
            break;
        }
        current = n.parent();
    }

    let scope_node = scope_node.ok_or(NavigationError::NoScopeAtPosition { line, column })?;

    // Build the parent chain from the scope node upward.
    let mut parent_chain = Vec::new();
    let mut ancestor = scope_node.parent();
    while let Some(a) = ancestor {
        if scope_kinds.contains(&a.kind()) {
            let name = a
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(text.as_bytes()).ok())
                .map(String::from);
            parent_chain.push(ScopeSummary {
                kind: a.kind().to_string(),
                name,
                span: Span {
                    start: a.start_position().into(),
                    end: a.end_position().into(),
                },
            });
        }
        ancestor = a.parent();
    }

    // Determine the scope name from a "name" field child.
    let scope_field_name = scope_node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(text.as_bytes()).ok())
        .map(String::from);

    // For the node info, try to use the scope name as a contextual field name.
    let info = node_to_info(scope_node, text, scope_field_name.as_deref());

    Ok(ScopeInfo {
        node: info,
        parent_chain,
    })
}

/// Navigate from the node at `(line, column)` in the given `direction`.
///
/// Returns the [`NodeInfo`] for the target node, or [`NavigationError::NavigationFailed`] if
/// the requested navigation is not possible (e.g., no parent from the root).
pub fn navigate(
    tree: &Tree,
    text: &str,
    line: usize,
    column: usize,
    direction: NavigationDirection,
) -> Result<NodeInfo, NavigationError> {
    let point = Point { row: line, column };
    let node = tree
        .root_node()
        .descendant_for_point_range(point, point)
        .ok_or(NavigationError::NoNodeAtPosition { line, column })?;

    let target = match &direction {
        NavigationDirection::Parent => node.parent(),
        NavigationDirection::FirstChild => node.child(0),
        NavigationDirection::NextSibling => node.next_sibling(),
        NavigationDirection::PrevSibling => node.prev_sibling(),
        NavigationDirection::NextNamedSibling => node.next_named_sibling(),
        NavigationDirection::PrevNamedSibling => node.prev_named_sibling(),
    };

    let target = target.ok_or_else(|| NavigationError::NavigationFailed {
        direction: direction.to_string(),
        line,
        column,
    })?;

    Ok(node_to_info(target, text, None))
}

/// Run a tree-sitter query against the given tree and return all matches.
///
/// `query_str` is a tree-sitter S-expression query string. Invalid queries produce
/// [`NavigationError::InvalidQuery`].
pub fn run_query(
    tree: &Tree,
    text: &str,
    language: &Language,
    query_str: &str,
) -> Result<Vec<QueryMatchResult>, NavigationError> {
    let query = Query::new(language, query_str)
        .map_err(|e| NavigationError::InvalidQuery(format!("{e}")))?;

    let capture_names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut results = Vec::new();

    collect_query_matches(&query, &mut cursor, tree, text, capture_names, &mut results);

    Ok(results)
}

/// Collect query matches from the tree into `results`.
///
/// Iterates all matches produced by `QueryCursor::matches` using the
/// `StreamingIterator` trait (re-exported by tree-sitter) and converts each
/// match into a [`QueryMatchResult`].
fn collect_query_matches(
    query: &Query,
    cursor: &mut QueryCursor,
    tree: &Tree,
    text: &str,
    capture_names: &[&str],
    results: &mut Vec<QueryMatchResult>,
) {
    let root = tree.root_node();
    let text_bytes = text.as_bytes();
    let mut matches = cursor.matches(query, root, text_bytes);
    while let Some(m) = matches.next() {
        let mut captures = Vec::new();
        for cap in m.captures {
            let cap_name = capture_names.get(cap.index as usize).unwrap_or(&"unknown");
            captures.push(CaptureResult {
                name: (*cap_name).to_string(),
                node: node_to_info(cap.node, text, None),
            });
        }
        results.push(QueryMatchResult {
            pattern_index: m.pattern_index,
            captures,
        });
    }
}

/// List all symbols (functions, types, etc.) in a source file.
///
/// If a language-specific symbol query is available (see [`symbol_query_for_language`]), it is
/// used. Otherwise a heuristic fallback iterates the root node's named children looking for
/// nodes with a `"name"` field.
pub fn list_symbols(
    tree: &Tree,
    text: &str,
    language: &Language,
    language_name: &str,
) -> Vec<SymbolInfo> {
    if let Some(query_src) = symbol_query_for_language(language_name) {
        if let Ok(matches) = run_query(tree, text, language, query_src) {
            return symbols_from_matches(&matches, text, tree);
        }
    }
    // Fallback: iterate root named children.
    symbols_from_root_children(tree, text)
}

/// Extract [`SymbolInfo`] values from query match results.
///
/// Looks for captures named `"name"` and `"definition"`. The `@definition` capture provides
/// the span and kind; the `@name` capture provides the symbol name.
fn symbols_from_matches(matches: &[QueryMatchResult], text: &str, tree: &Tree) -> Vec<SymbolInfo> {
    let mut symbols = Vec::new();

    for m in matches {
        let name_capture = m.captures.iter().find(|c| c.name == "name");
        let def_capture = m.captures.iter().find(|c| c.name == "definition");

        let (name, kind, span, def_text) = match (name_capture, def_capture) {
            (Some(nc), Some(dc)) => {
                let full_text = &dc.node.text;
                (
                    nc.node.text.clone(),
                    dc.node.kind.clone(),
                    dc.node.span.clone(),
                    full_text.clone(),
                )
            }
            (Some(nc), None) => {
                // Only name capture — use its parent info if available.
                (
                    nc.node.text.clone(),
                    nc.node.kind.clone(),
                    nc.node.span.clone(),
                    nc.node.text.clone(),
                )
            }
            (None, Some(dc)) => {
                // Only definition — try to get a name from the node's "name" field.
                let def_name = find_name_in_node(tree, text, &dc.node);
                (
                    def_name,
                    dc.node.kind.clone(),
                    dc.node.span.clone(),
                    dc.node.text.clone(),
                )
            }
            (None, None) => continue,
        };

        let signature = def_text.lines().next().unwrap_or("").to_string();

        symbols.push(SymbolInfo {
            name,
            kind,
            span,
            signature,
        });
    }
    symbols
}

/// Try to find a "name" field child from a node described by the given [`NodeInfo`].
fn find_name_in_node(tree: &Tree, text: &str, info: &NodeInfo) -> String {
    let point_start = Point {
        row: info.span.start.line,
        column: info.span.start.column,
    };
    let point_end = Point {
        row: info.span.end.line,
        column: info.span.end.column,
    };
    if let Some(node) = tree
        .root_node()
        .descendant_for_point_range(point_start, point_end)
    {
        if let Some(name_node) = node.child_by_field_name("name") {
            if let Ok(name_text) = name_node.utf8_text(text.as_bytes()) {
                return name_text.to_string();
            }
        }
    }
    String::new()
}

/// Fallback symbol listing: iterate the root node's named children and look for a `"name"`
/// field.
fn symbols_from_root_children(tree: &Tree, text: &str) -> Vec<SymbolInfo> {
    let root = tree.root_node();
    let mut symbols = Vec::new();
    let mut cursor = root.walk();

    for child in root.named_children(&mut cursor) {
        let name = child
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(text.as_bytes()).ok())
            .unwrap_or("")
            .to_string();

        if name.is_empty() {
            continue;
        }

        let full_text = child.utf8_text(text.as_bytes()).unwrap_or("");
        let signature = full_text.lines().next().unwrap_or("").to_string();

        symbols.push(SymbolInfo {
            name,
            kind: child.kind().to_string(),
            span: Span {
                start: child.start_position().into(),
                end: child.end_position().into(),
            },
            signature,
        });
    }
    symbols
}

/// Find the definition of a symbol by name and return its [`SymbolInfo`] along with the full
/// text of the defining node.
///
/// Returns [`NavigationError::SymbolNotFound`] if no symbol with the given name exists.
pub fn get_definition(
    tree: &Tree,
    text: &str,
    language: &Language,
    language_name: &str,
    symbol_name: &str,
) -> Result<(SymbolInfo, String), NavigationError> {
    let symbols = list_symbols(tree, text, language, language_name);
    let sym = symbols
        .into_iter()
        .find(|s| s.name == symbol_name)
        .ok_or_else(|| NavigationError::SymbolNotFound(symbol_name.to_string()))?;

    // Retrieve the full text of the definition node.
    let point_start = Point {
        row: sym.span.start.line,
        column: sym.span.start.column,
    };
    let point_end = Point {
        row: sym.span.end.line,
        column: sym.span.end.column,
    };
    let full_text = tree
        .root_node()
        .descendant_for_point_range(point_start, point_end)
        .and_then(|n| n.utf8_text(text.as_bytes()).ok())
        .unwrap_or("")
        .to_string();

    Ok((sym, full_text))
}

/// List the child symbols of a parent symbol identified by name.
///
/// This finds the parent symbol via [`list_symbols`], locates the corresponding tree-sitter
/// node by byte range, then either runs the symbol query scoped to that node's byte range or
/// falls back to iterating named children.
pub fn get_children_of(
    tree: &Tree,
    text: &str,
    language: &Language,
    language_name: &str,
    parent_name: &str,
) -> Result<Vec<SymbolInfo>, NavigationError> {
    let symbols = list_symbols(tree, text, language, language_name);
    let parent_sym = symbols
        .iter()
        .find(|s| s.name == parent_name)
        .ok_or_else(|| NavigationError::SymbolNotFound(parent_name.to_string()))?;

    // Locate the parent node in the tree using its span.
    let point_start = Point {
        row: parent_sym.span.start.line,
        column: parent_sym.span.start.column,
    };
    let point_end = Point {
        row: parent_sym.span.end.line,
        column: parent_sym.span.end.column,
    };
    let parent_node = tree
        .root_node()
        .descendant_for_point_range(point_start, point_end)
        .ok_or_else(|| NavigationError::SymbolNotFound(parent_name.to_string()))?;

    // Try the query-based approach with the cursor scoped to the parent node's byte range.
    if let Some(query_src) = symbol_query_for_language(language_name) {
        if let Ok(query) = Query::new(language, query_src) {
            let mut cursor = QueryCursor::new();
            cursor.set_byte_range(parent_node.start_byte()..parent_node.end_byte());
            let capture_names = query.capture_names();
            let mut match_results = Vec::new();
            collect_query_matches(
                &query,
                &mut cursor,
                tree,
                text,
                capture_names,
                &mut match_results,
            );
            let child_symbols: Vec<SymbolInfo> = symbols_from_matches(&match_results, text, tree)
                .into_iter()
                // Exclude the parent itself.
                .filter(|s| s.name != parent_name)
                .collect();
            if !child_symbols.is_empty() {
                return Ok(child_symbols);
            }
        }
    }

    // Fallback: iterate named children of the parent node.
    let mut child_symbols = Vec::new();
    let mut cursor = parent_node.walk();
    for child in parent_node.named_children(&mut cursor) {
        let name = child
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(text.as_bytes()).ok())
            .unwrap_or("")
            .to_string();

        if name.is_empty() {
            continue;
        }

        let full_text = child.utf8_text(text.as_bytes()).unwrap_or("");
        let signature = full_text.lines().next().unwrap_or("").to_string();

        child_symbols.push(SymbolInfo {
            name,
            kind: child.kind().to_string(),
            span: Span {
                start: child.start_position().into(),
                end: child.end_position().into(),
            },
            signature,
        });
    }
    Ok(child_symbols)
}

// ---------------------------------------------------------------------------
// Language-specific data
// ---------------------------------------------------------------------------

/// Return the set of node kinds considered "scopes" for the given language.
///
/// Unknown languages receive a reasonable default set.
pub fn scope_kinds_for_language(lang: &str) -> &'static [&'static str] {
    match lang {
        "rust" => &[
            "function_item",
            "impl_item",
            "struct_item",
            "enum_item",
            "trait_item",
            "mod_item",
        ],
        "python" => &["function_definition", "class_definition", "module"],
        "typescript" | "tsx" => &[
            "function_declaration",
            "class_declaration",
            "method_definition",
            "arrow_function",
        ],
        "go" => &[
            "function_declaration",
            "method_declaration",
            "type_declaration",
        ],
        "java" => &[
            "class_declaration",
            "method_declaration",
            "interface_declaration",
        ],
        "cpp" => &[
            "function_definition",
            "class_specifier",
            "struct_specifier",
            "namespace_definition",
        ],
        "c" => &["function_definition", "struct_specifier"],
        _ => &[
            "function_definition",
            "function_declaration",
            "method_definition",
            "class_definition",
            "class_declaration",
            "module",
            "struct_specifier",
            "impl_item",
            "trait_item",
        ],
    }
}

/// Return a tree-sitter S-expression query that captures top-level symbols for the given
/// language.
///
/// The query uses `@name` for the symbol's name node and `@definition` for the enclosing
/// definition node. Returns `None` for languages without a known symbol query.
pub fn symbol_query_for_language(lang: &str) -> Option<&'static str> {
    match lang {
        "rust" => Some(concat!(
            "(function_item name: (identifier) @name) @definition\n",
            "(struct_item name: (type_identifier) @name) @definition\n",
            "(enum_item name: (type_identifier) @name) @definition\n",
            "(trait_item name: (type_identifier) @name) @definition\n",
            "(impl_item type: (type_identifier) @name) @definition\n",
            "(const_item name: (identifier) @name) @definition\n",
            "(static_item name: (identifier) @name) @definition\n",
            "(type_item name: (type_identifier) @name) @definition\n",
        )),
        "python" => Some(concat!(
            "(function_definition name: (identifier) @name) @definition\n",
            "(class_definition name: (identifier) @name) @definition\n",
        )),
        "typescript" | "tsx" => Some(concat!(
            "(function_declaration name: (identifier) @name) @definition\n",
            "(class_declaration name: (type_identifier) @name) @definition\n",
            "(interface_declaration name: (type_identifier) @name) @definition\n",
            "(type_alias_declaration name: (type_identifier) @name) @definition\n",
        )),
        "go" => Some(concat!(
            "(function_declaration name: (identifier) @name) @definition\n",
            "(method_declaration name: (field_identifier) @name) @definition\n",
            "(type_declaration (type_spec name: (type_identifier) @name)) @definition\n",
        )),
        "java" => Some(concat!(
            "(class_declaration name: (identifier) @name) @definition\n",
            "(method_declaration name: (identifier) @name) @definition\n",
            "(interface_declaration name: (identifier) @name) @definition\n",
        )),
        "c" => Some(concat!(
            "(function_definition declarator: (function_declarator declarator: (identifier) @name)) @definition\n",
            "(struct_specifier name: (type_identifier) @name) @definition\n",
        )),
        "cpp" => Some(concat!(
            "(function_definition declarator: (function_declarator declarator: (identifier) @name)) @definition\n",
            "(struct_specifier name: (type_identifier) @name) @definition\n",
            "(class_specifier name: (type_identifier) @name) @definition\n",
        )),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Parse cache
// ---------------------------------------------------------------------------

/// A parsed file together with its metadata, suitable for caching.
pub struct ParsedFile {
    pub text: String,
    pub tree: Tree,
    pub language_name: String,
    pub language: Language,
    pub last_modified: SystemTime,
}

/// A cache of parsed files keyed by canonical path.
///
/// When a file is requested, the cache checks whether the entry is still fresh by comparing
/// the file's modification time. Stale entries are automatically re-parsed.
pub struct ParseCache {
    entries: HashMap<PathBuf, ParsedFile>,
    config: GrammarConfig,
}

impl ParseCache {
    /// Create a new empty cache using the given grammar configuration.
    #[must_use]
    pub fn new(config: GrammarConfig) -> Self {
        Self {
            entries: HashMap::new(),
            config,
        }
    }

    /// Get a cached parse result or parse the file on demand.
    ///
    /// The `language` parameter optionally overrides automatic language detection from the
    /// file extension. If the file has been modified since the last parse, it is re-parsed.
    pub fn get_or_parse(
        &mut self,
        path: &Path,
        language: Option<&str>,
    ) -> Result<&ParsedFile, NavigationError> {
        let canonical = fs::canonicalize(path).map_err(NavigationError::Io)?;
        let current_mtime = fs::metadata(&canonical)
            .and_then(|m| m.modified())
            .map_err(NavigationError::Io)?;

        // Check if we have a fresh entry.
        let needs_parse = match self.entries.get(&canonical) {
            Some(entry) => entry.last_modified < current_mtime,
            None => true,
        };

        if needs_parse {
            let text = fs::read_to_string(&canonical).map_err(NavigationError::Io)?;

            // Resolve the language name.
            let lang_name = match language {
                Some(l) => l.to_string(),
                None => {
                    let ext = canonical
                        .extension()
                        .and_then(|e| e.to_str())
                        .ok_or_else(|| {
                            NavigationError::ParseError(LoadingError::NoFileExt(
                                canonical.to_string_lossy().to_string(),
                            ))
                        })?;
                    parse::lang_name_from_file_ext(ext, &self.config)?.to_string()
                }
            };

            let ts_language = parse::generate_language(&lang_name, &self.config)?;

            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&ts_language)
                .map_err(LoadingError::from)?;

            let tree = parser.parse(&text, None).ok_or_else(|| {
                NavigationError::ParseError(LoadingError::TSParseFailure(canonical.clone()))
            })?;

            self.entries.insert(
                canonical.clone(),
                ParsedFile {
                    text,
                    tree,
                    language_name: lang_name,
                    language: ts_language,
                    last_modified: current_mtime,
                },
            );
        }

        // The entry is guaranteed to exist after the block above.
        Ok(self
            .entries
            .get(&canonical)
            .expect("entry was just inserted"))
    }

    /// Remove a specific path from the cache.
    pub fn evict(&mut self, path: &Path) {
        if let Ok(canonical) = fs::canonicalize(path) {
            self.entries.remove(&canonical);
        }
    }

    /// Remove all entries from the cache.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
