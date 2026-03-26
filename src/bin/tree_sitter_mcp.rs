//! Binary entry point for the tree-sitter MCP server.
//!
//! This binary starts a tree-sitter MCP server that communicates over stdio,
//! exposing AST navigation tools via the Model Context Protocol. It requires
//! the `mcp-server` feature flag to be enabled.

use anyhow::Result;

use libdiffsitter::mcp_server::TreeSitterMcpServer;
use libdiffsitter::parse::GrammarConfig;
use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> Result<()> {
    let config = GrammarConfig::default();
    let server = TreeSitterMcpServer::new(config);
    let transport = rmcp::transport::io::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}
