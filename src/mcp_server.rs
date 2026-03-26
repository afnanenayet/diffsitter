//! MCP (Model Context Protocol) server exposing tree-sitter AST navigation tools.
//!
//! This module implements an MCP server that provides eight tools for parsing files,
//! inspecting AST nodes, navigating the syntax tree, running tree-sitter queries,
//! and listing/resolving symbols. It is gated behind the `mcp-server` feature flag.

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::model::*;
use rmcp::{Error as McpError, ServerHandler, tool};
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::ast_navigation::{self, NavigationDirection, NavigationError, ParseCache};
use crate::parse::GrammarConfig;

// ---------------------------------------------------------------------------
// Parameter types
// ---------------------------------------------------------------------------

/// Parameters for parsing a file and returning its root AST node.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ParseFileParams {
    /// Absolute or relative path to the source file.
    pub path: String,
    /// Optional language override (e.g. "rust", "python"). If omitted, the
    /// language is inferred from the file extension.
    pub language: Option<String>,
}

/// Parameters that identify a position within a source file.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PositionParams {
    /// Absolute or relative path to the source file.
    pub path: String,
    /// Zero-based line number.
    pub line: usize,
    /// Zero-based column number (in bytes).
    pub column: usize,
    /// Optional language override.
    pub language: Option<String>,
}

/// Parameters for navigating from a position in a given direction.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct NavigateParams {
    /// Absolute or relative path to the source file.
    pub path: String,
    /// Zero-based line number of the starting position.
    pub line: usize,
    /// Zero-based column number of the starting position.
    pub column: usize,
    /// Navigation direction. One of: "parent", "first_child", "next_sibling",
    /// "prev_sibling", "next_named_sibling", "prev_named_sibling".
    pub direction: String,
    /// Optional language override.
    pub language: Option<String>,
}

/// Parameters for running a tree-sitter S-expression query.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryParams {
    /// Absolute or relative path to the source file.
    pub path: String,
    /// Tree-sitter S-expression query string.
    pub query: String,
    /// Optional language override.
    pub language: Option<String>,
}

/// Parameters for listing all top-level symbols in a file.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListSymbolsParams {
    /// Absolute or relative path to the source file.
    pub path: String,
    /// Optional language override.
    pub language: Option<String>,
}

/// Parameters for retrieving a specific symbol definition.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetDefinitionParams {
    /// Absolute or relative path to the source file.
    pub path: String,
    /// Name of the symbol to look up.
    pub symbol_name: String,
    /// Optional language override.
    pub language: Option<String>,
}

/// Parameters for listing child symbols of a named parent.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetChildrenOfParams {
    /// Absolute or relative path to the source file.
    pub path: String,
    /// Name of the parent symbol whose children should be listed.
    pub parent_name: String,
    /// Optional language override.
    pub language: Option<String>,
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

/// The tree-sitter MCP server.
///
/// Exposes AST navigation capabilities over the Model Context Protocol via
/// eight tools. Parsed files are cached so that repeated queries against the
/// same file avoid redundant parsing.
#[derive(Clone)]
pub struct TreeSitterMcpServer {
    cache: Arc<Mutex<ParseCache>>,
}

impl TreeSitterMcpServer {
    /// Create a new server instance with the given grammar configuration.
    pub fn new(config: GrammarConfig) -> Self {
        Self {
            cache: Arc::new(Mutex::new(ParseCache::new(config))),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a direction string into a [`NavigationDirection`] enum variant.
fn parse_direction(s: &str) -> Result<NavigationDirection, McpError> {
    match s {
        "parent" => Ok(NavigationDirection::Parent),
        "first_child" => Ok(NavigationDirection::FirstChild),
        "next_sibling" => Ok(NavigationDirection::NextSibling),
        "prev_sibling" => Ok(NavigationDirection::PrevSibling),
        "next_named_sibling" => Ok(NavigationDirection::NextNamedSibling),
        "prev_named_sibling" => Ok(NavigationDirection::PrevNamedSibling),
        other => Err(McpError::invalid_params(
            format!(
                "Unknown direction: \"{other}\". Expected one of: parent, \
                 first_child, next_sibling, prev_sibling, next_named_sibling, \
                 prev_named_sibling"
            ),
            None,
        )),
    }
}

/// Convert a [`NavigationError`] into an MCP [`McpError`].
fn nav_err(e: NavigationError) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

#[tool(tool_box)]
impl TreeSitterMcpServer {
    /// Parse a file and return the root AST node info.
    #[tool(
        name = "parse_file",
        description = "Parse a source file with tree-sitter and return the root AST node, \
                        including its kind, span, child count, and named children summary."
    )]
    async fn parse_file(
        &self,
        #[tool(aggr)] params: ParseFileParams,
    ) -> Result<CallToolResult, McpError> {
        let path = PathBuf::from(&params.path);
        let mut cache = self.cache.lock().await;
        let parsed = cache
            .get_or_parse(&path, params.language.as_deref())
            .map_err(nav_err)?;
        let info = ast_navigation::node_to_info(parsed.tree.root_node(), &parsed.text, None);
        let json = serde_json::to_string_pretty(&info)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Get the AST node at a specific position in the file.
    #[tool(
        name = "get_node_at_position",
        description = "Return the most specific AST node at a given line and column in a \
                        source file, including its kind, span, text, and children."
    )]
    async fn get_node_at_position(
        &self,
        #[tool(aggr)] params: PositionParams,
    ) -> Result<CallToolResult, McpError> {
        let path = PathBuf::from(&params.path);
        let mut cache = self.cache.lock().await;
        let parsed = cache
            .get_or_parse(&path, params.language.as_deref())
            .map_err(nav_err)?;
        let info = ast_navigation::get_node_at_position(
            &parsed.tree,
            &parsed.text,
            params.line,
            params.column,
        )
        .map_err(nav_err)?;
        let json = serde_json::to_string_pretty(&info)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Get the scope context (enclosing scopes) at a position.
    #[tool(
        name = "get_scope",
        description = "Return the innermost scope node at a given position along with the \
                        chain of parent scopes up to the file root. Useful for understanding \
                        the nesting context of a code location."
    )]
    async fn get_scope(
        &self,
        #[tool(aggr)] params: PositionParams,
    ) -> Result<CallToolResult, McpError> {
        let path = PathBuf::from(&params.path);
        let mut cache = self.cache.lock().await;
        let parsed = cache
            .get_or_parse(&path, params.language.as_deref())
            .map_err(nav_err)?;
        let scope = ast_navigation::get_scope(
            &parsed.tree,
            &parsed.text,
            &parsed.language_name,
            params.line,
            params.column,
        )
        .map_err(nav_err)?;
        let json = serde_json::to_string_pretty(&scope)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Navigate from a position in a given direction.
    #[tool(
        name = "navigate",
        description = "Navigate the AST from a given position in a specified direction \
                        (parent, first_child, next_sibling, prev_sibling, next_named_sibling, \
                        prev_named_sibling) and return the target node."
    )]
    async fn navigate(
        &self,
        #[tool(aggr)] params: NavigateParams,
    ) -> Result<CallToolResult, McpError> {
        let path = PathBuf::from(&params.path);
        let direction = parse_direction(&params.direction)?;
        let mut cache = self.cache.lock().await;
        let parsed = cache
            .get_or_parse(&path, params.language.as_deref())
            .map_err(nav_err)?;
        let info = ast_navigation::navigate(
            &parsed.tree,
            &parsed.text,
            params.line,
            params.column,
            direction,
        )
        .map_err(nav_err)?;
        let json = serde_json::to_string_pretty(&info)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Run a tree-sitter S-expression query against a file.
    #[tool(
        name = "query",
        description = "Run a tree-sitter S-expression query against a source file and return \
                        all pattern matches with their captured nodes."
    )]
    async fn query(&self, #[tool(aggr)] params: QueryParams) -> Result<CallToolResult, McpError> {
        let path = PathBuf::from(&params.path);
        let mut cache = self.cache.lock().await;
        let parsed = cache
            .get_or_parse(&path, params.language.as_deref())
            .map_err(nav_err)?;
        let matches =
            ast_navigation::run_query(&parsed.tree, &parsed.text, &parsed.language, &params.query)
                .map_err(nav_err)?;
        let json = serde_json::to_string_pretty(&matches)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// List all top-level symbols defined in a file.
    #[tool(
        name = "list_symbols",
        description = "List all top-level symbols (functions, types, constants, etc.) \
                        defined in a source file, with their kind, span, and signature."
    )]
    async fn list_symbols(
        &self,
        #[tool(aggr)] params: ListSymbolsParams,
    ) -> Result<CallToolResult, McpError> {
        let path = PathBuf::from(&params.path);
        let mut cache = self.cache.lock().await;
        let parsed = cache
            .get_or_parse(&path, params.language.as_deref())
            .map_err(nav_err)?;
        let symbols = ast_navigation::list_symbols(
            &parsed.tree,
            &parsed.text,
            &parsed.language,
            &parsed.language_name,
        );
        let json = serde_json::to_string_pretty(&symbols)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Get the definition of a named symbol.
    #[tool(
        name = "get_definition",
        description = "Find a symbol by name in a source file and return its full definition, \
                        including metadata (kind, span, signature) and the complete source text."
    )]
    async fn get_definition(
        &self,
        #[tool(aggr)] params: GetDefinitionParams,
    ) -> Result<CallToolResult, McpError> {
        let path = PathBuf::from(&params.path);
        let mut cache = self.cache.lock().await;
        let parsed = cache
            .get_or_parse(&path, params.language.as_deref())
            .map_err(nav_err)?;
        let (symbol, text) = ast_navigation::get_definition(
            &parsed.tree,
            &parsed.text,
            &parsed.language,
            &parsed.language_name,
            &params.symbol_name,
        )
        .map_err(nav_err)?;
        let result = serde_json::json!({
            "symbol": symbol,
            "text": text,
        });
        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// List child symbols of a named parent symbol.
    #[tool(
        name = "get_children_of",
        description = "List the child symbols (methods, fields, nested types, etc.) of a \
                        named parent symbol in a source file."
    )]
    async fn get_children_of(
        &self,
        #[tool(aggr)] params: GetChildrenOfParams,
    ) -> Result<CallToolResult, McpError> {
        let path = PathBuf::from(&params.path);
        let mut cache = self.cache.lock().await;
        let parsed = cache
            .get_or_parse(&path, params.language.as_deref())
            .map_err(nav_err)?;
        let children = ast_navigation::get_children_of(
            &parsed.tree,
            &parsed.text,
            &parsed.language,
            &parsed.language_name,
            &params.parent_name,
        )
        .map_err(nav_err)?;
        let json = serde_json::to_string_pretty(&children)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

// ---------------------------------------------------------------------------
// ServerHandler
// ---------------------------------------------------------------------------

#[tool]
impl ServerHandler for TreeSitterMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
            server_info: Implementation {
                name: "tree-sitter-mcp".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: Some(
                "AST-aware code navigation using tree-sitter grammars. \
                 Supports 17+ languages. Use list_symbols for an overview, \
                 get_definition to read specific symbols, and query for \
                 custom tree-sitter S-expression pattern matching."
                    .into(),
            ),
        }
    }

    rmcp::tool_box!(@derive);
}
