//! # rustmastra-mcp
//!
//! Model Context Protocol (MCP) client/server (checklist §9).
//!
//! MCP standardises how agents connect to external tools and resources.
//! This crate implements the client side (discovering + calling tools on
//! MCP servers) and a server stub (exposing framework tools as an MCP server).
//!
//! Supported transports (§9.2–9.4):
//! * stdio  – subprocess pipes
//! * SSE    – HTTP Server-Sent Events
//! * WebSocket
//!
//! Stub implementation; full build in §9 of the checklist.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// MCP tool descriptor
// ─────────────────────────────────────────────────────────────────────────────

/// A tool exposed by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// A resource exposed by an MCP server (read-only data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Transport enum
// ─────────────────────────────────────────────────────────────────────────────

/// Transport used to communicate with an MCP server.
#[derive(Debug, Clone)]
pub enum McpTransport {
    /// Spawn a subprocess and communicate over stdin/stdout.
    Stdio {
        command: String,
        args: Vec<String>,
    },
    /// Connect to an SSE endpoint.
    Sse { url: String },
    /// Connect via WebSocket.
    WebSocket { url: String },
}

// ─────────────────────────────────────────────────────────────────────────────
// McpClient stub
// ─────────────────────────────────────────────────────────────────────────────

/// Client that connects to an MCP server and exposes its tools to agents.
///
/// Full implementation (JSON-RPC, tool discovery, resource fetching):
/// checklist §9.
pub struct McpClient {
    transport: McpTransport,
}

impl McpClient {
    pub fn new(transport: McpTransport) -> Self {
        Self { transport }
    }

    /// Connect and fetch the list of available tools.
    pub async fn list_tools(&self) -> rustmastra_core::Result<Vec<McpTool>> {
        // TODO: implement JSON-RPC `tools/list` call (§9.5)
        Err(rustmastra_core::FrameworkError::Config(
            "McpClient not yet implemented – coming in §9".into(),
        ))
    }

    /// Execute a tool call and return the result.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> rustmastra_core::Result<String> {
        // TODO: implement JSON-RPC `tools/call` (§9.5)
        let _ = (name, arguments);
        Err(rustmastra_core::FrameworkError::Config(
            "McpClient not yet implemented – coming in §9".into(),
        ))
    }
}
