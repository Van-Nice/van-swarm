//! # openswarm-mcp
//!
//! Model Context Protocol (MCP) client and server implementation.
//!
//! MCP standardises the connection between AI agents and external tools /
//! resources.  This crate provides:
//!
//! * **[`McpClient`]** — connects to any MCP server via stdio, HTTP, or an
//!   in-memory channel. Use [`list_tools`](McpClient::list_tools) and
//!   [`call_tool`](McpClient::call_tool) for tool discovery and execution.
//! * **[`McpToolExecutor`]** — implements `ToolExecutor` so a OpenSwarm agent
//!   can use any MCP server's tools (e.g. [rust-mcp](https://github.com/your-org/rust-mcp)).
//! * **[`McpServer`]** — exposes `openswarm_core` tools as an MCP server;
//!   can serve over stdio (for IDE integration) or in-memory (for tests).
//!
//! ## Using the rust-mcp server
//!
//! To connect to the [rust-mcp](https://github.com/your-org/rust-mcp) server (Rust
//! development tools: cargo, rust-analyzer, refactor, etc.):
//!
//! ```rust,no_run
//! # async fn example() -> openswarm_core::Result<()> {
//! let bin = std::env::var("RUST_MCP_BIN").unwrap_or_else(|_| "../rust-mcp/target/release/rust-mcp".into());
//! let client = std::sync::Arc::new(openswarm_mcp::McpClient::stdio(&bin, &["--client=cursor"]).await?);
//! client.initialize().await?;
//! let tools = client.list_tools().await?;
//! // Use with an agent: let executor = McpToolExecutor::new(client); executor.refresh_tools().await?;
//! # Ok(()) }
//! ```
//!
//! Run the example: `cargo run -p openswarm-mcp --example rust_mcp_client`
//!
//! ## Quick start — connect to an existing server
//!
//! ```rust,no_run
//! # async fn example() -> openswarm_core::Result<()> {
//! let client = openswarm_mcp::McpClient::stdio(
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
//! # use openswarm_mcp::McpServer;
//! # use openswarm_core::LocalToolRegistry;
//! let registry = Arc::new(LocalToolRegistry::new());
//! McpServer::new("my-server", "0.1.0", registry)
//!     .serve_stdio(); // blocks; reads from stdin, writes to stdout
//! ```

pub mod client;
pub mod executor;
mod tests;
pub mod jsonrpc;
pub mod protocol;
pub mod server;
pub mod transport;

// ── Re-exports ────────────────────────────────────────────────────────────────

pub use client::McpClient;
pub use executor::McpToolExecutor;
pub use protocol::{
    CallToolResult, EmbeddedResource, InitializeResult, ListResourcesResult, ListToolsResult,
    McpPrompt, McpResource, McpTool, PromptArgument, ReadResourceResult, ServerCapabilities,
    ServerInfo, ToolContent, PROTOCOL_VERSION,
};
pub use server::McpServer;
pub use transport::{ChannelTransport, HttpTransport, StdioTransport, Transport};
