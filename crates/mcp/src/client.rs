//! `McpClient` — connects to an MCP server and exposes its tools/resources.
//!
//! ## Usage
//!
//! ```rust,no_run
//! # use openswarm_mcp::McpClient;
//! # async fn example() -> openswarm_core::Result<()> {
//! // Connect to an MCP server over stdio.
//! let mut client = McpClient::stdio("uvx", &["mcp-server-fetch"]).await?;
//! let tools = client.list_tools().await?;
//! for t in &tools {
//!     println!("{}: {}", t.name, t.description);
//! }
//! let result = client.call_tool("fetch", serde_json::json!({"url":"https://example.com"})).await?;
//! println!("{}", result.content[0].as_text().unwrap_or_default());
//! # Ok(()) }
//! ```

use tracing::instrument;

use crate::{
    protocol::{
        CallToolParams, CallToolResult, ClientCapabilities, ClientInfo, InitializeParams,
        InitializeResult, ListResourcesResult, ListToolsResult, McpResource, McpTool,
        ReadResourceResult, PROTOCOL_VERSION,
    },
    transport::{ChannelTransport, HttpTransport, StdioTransport, Transport},
};

// ─────────────────────────────────────────────────────────────────────────────
// McpClient
// ─────────────────────────────────────────────────────────────────────────────

/// High-level MCP client.
///
/// Call `McpClient::stdio()` / `McpClient::http()` to create, then
/// `initialize()` to complete the handshake before calling tools.
pub struct McpClient {
    transport: Transport,
}

impl McpClient {
    // ── Constructors ─────────────────────────────────────────────────────────

    /// Connect to an MCP server by spawning a subprocess.
    ///
    /// # Example
    /// ```rust,no_run
    /// # async fn ex() -> openswarm_core::Result<()> {
    /// let mut client = openswarm_mcp::McpClient::stdio("npx", &["-y", "@modelcontextprotocol/server-filesystem", "/"]).await?;
    /// # Ok(()) }
    /// ```
    pub async fn stdio(
        command: impl AsRef<str>,
        args: &[impl AsRef<str>],
    ) -> openswarm_core::Result<Self> {
        let t = StdioTransport::spawn(command.as_ref(), args).await?;
        Ok(Self { transport: Transport::Stdio(t) })
    }

    /// Connect to an MCP server over HTTP.
    pub fn http(endpoint: impl Into<String>) -> Self {
        Self { transport: Transport::Http(HttpTransport::new(endpoint)) }
    }

    /// Create a client backed by an in-memory channel (for tests).
    pub fn channel(transport: ChannelTransport) -> Self {
        Self { transport: Transport::Channel(transport) }
    }

    // ── Protocol ─────────────────────────────────────────────────────────────

    /// Complete the MCP handshake.  Must be called before any tool/resource
    /// operations.  Idempotent — calling it twice is harmless.
    #[instrument(skip(self))]
    pub async fn initialize(&self) -> openswarm_core::Result<InitializeResult> {
        let params = serde_json::to_value(InitializeParams {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            client_info: ClientInfo {
                name: "openswarm".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            capabilities: ClientCapabilities::default(),
        })
        .map_err(|e| openswarm_core::FrameworkError::Serialization(e.into()))?;

        let raw = self.transport.send("initialize", Some(params)).await?;
        let result: InitializeResult = serde_json::from_value(raw)
            .map_err(|e| openswarm_core::FrameworkError::Config(format!("initialize parse: {e}")))?;

        // Send the required `notifications/initialized` notification.
        self.transport.notify("notifications/initialized", None).await?;

        Ok(result)
    }

    // ── Tools ─────────────────────────────────────────────────────────────────

    /// List all tools exposed by the server.
    #[instrument(skip(self))]
    pub async fn list_tools(&self) -> openswarm_core::Result<Vec<McpTool>> {
        let raw = self.transport.send("tools/list", None).await?;
        let list: ListToolsResult = serde_json::from_value(raw)
            .map_err(|e| openswarm_core::FrameworkError::Config(format!("tools/list parse: {e}")))?;
        Ok(list.tools)
    }

    /// Call a tool on the server.
    ///
    /// `arguments` is a JSON object matching the tool's `inputSchema`.
    #[instrument(skip(self, arguments), fields(tool = name))]
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> openswarm_core::Result<CallToolResult> {
        let params = serde_json::to_value(CallToolParams {
            name: name.to_owned(),
            arguments: Some(arguments),
        })
        .map_err(|e| openswarm_core::FrameworkError::Serialization(e.into()))?;

        let raw = self.transport.send("tools/call", Some(params)).await?;
        serde_json::from_value(raw)
            .map_err(|e| openswarm_core::FrameworkError::Config(format!("tools/call parse: {e}")))
    }

    // ── Resources ─────────────────────────────────────────────────────────────

    /// List all resources exposed by the server.
    #[instrument(skip(self))]
    pub async fn list_resources(&self) -> openswarm_core::Result<Vec<McpResource>> {
        let raw = self.transport.send("resources/list", None).await?;
        let list: ListResourcesResult = serde_json::from_value(raw)
            .map_err(|e| openswarm_core::FrameworkError::Config(format!("resources/list parse: {e}")))?;
        Ok(list.resources)
    }

    /// Read the content of a resource by URI.
    #[instrument(skip(self), fields(uri))]
    pub async fn read_resource(
        &self,
        uri: &str,
    ) -> openswarm_core::Result<ReadResourceResult> {
        let params = serde_json::json!({ "uri": uri });
        let raw = self.transport.send("resources/read", Some(params)).await?;
        serde_json::from_value(raw)
            .map_err(|e| openswarm_core::FrameworkError::Config(format!("resources/read parse: {e}")))
    }

    /// Fetch one or more resources and return their text concatenated for context injection (§9.6).
    ///
    /// Use this to inject read-only MCP resource content into the agent's context or system
    /// prompt. Each resource's text (from `EmbeddedResource::text`) is prefixed with
    /// `--- Resource: {uri} ---` and concatenated. Binary or missing text is skipped.
    #[instrument(skip(self), fields(uris = uris.len()))]
    pub async fn fetch_resources_for_context(
        &self,
        uris: &[impl AsRef<str>],
    ) -> openswarm_core::Result<String> {
        let mut out = String::new();
        for uri in uris {
            let uri = uri.as_ref();
            let result = self.read_resource(uri).await?;
            for content in &result.contents {
                if let Some(ref text) = content.text {
                    if !out.is_empty() {
                        out.push_str("\n\n");
                    }
                    out.push_str("--- Resource: ");
                    out.push_str(uri);
                    out.push_str(" ---\n");
                    out.push_str(text);
                }
            }
        }
        Ok(out)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ToolContent helper
// ─────────────────────────────────────────────────────────────────────────────

use crate::protocol::ToolContent;

impl ToolContent {
    /// Convenience: extract plain text from a `Text` variant.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ToolContent::Text { text } => Some(text.as_str()),
            _ => None,
        }
    }
}
