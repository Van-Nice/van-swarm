//! # rustmastra-mcp
//!
//! Model Context Protocol (MCP) client and server implementation.
//!
//! MCP standardises the connection between AI agents and external tools /
//! resources.  This crate provides:
//!
//! * **[`McpClient`]** — connects to any MCP server via stdio, HTTP, or an
//!   in-memory channel.
//! * **[`McpServer`]** — exposes `rustmastra_core` tools as an MCP server;
//!   can serve over stdio (for IDE integration) or in-memory (for tests).
//!
//! ## Quick start — connect to an existing server
//!
//! ```rust,no_run
//! # async fn example() -> rustmastra_core::Result<()> {
//! let client = rustmastra_mcp::McpClient::stdio(
//!     "npx",
//!     &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
//! ).await?;
//! client.initialize().await?;
//! let tools = client.list_tools().await?;
//! println!("Found {} tools", tools.len());
//! # Ok(()) }
//! ```
//!
//! ## Quick start — expose your tools as an MCP server
//!
//! ```rust,no_run
//! # use std::sync::Arc;
//! # use rustmastra_mcp::McpServer;
//! # use rustmastra_core::LocalToolRegistry;
//! let registry = Arc::new(LocalToolRegistry::new());
//! McpServer::new("my-server", "0.1.0", registry)
//!     .serve_stdio(); // blocks; reads from stdin, writes to stdout
//! ```

pub mod client;
mod tests;
pub mod jsonrpc;
pub mod protocol;
pub mod server;
pub mod transport;

// ── Re-exports ────────────────────────────────────────────────────────────────

pub use client::McpClient;
pub use protocol::{
    CallToolResult, EmbeddedResource, InitializeResult, ListResourcesResult, ListToolsResult,
    McpPrompt, McpResource, McpTool, PromptArgument, ReadResourceResult, ServerCapabilities,
    ServerInfo, ToolContent, PROTOCOL_VERSION,
};
pub use server::McpServer;
pub use transport::{ChannelTransport, HttpTransport, StdioTransport, Transport};
