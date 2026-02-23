//! `McpServer` — exposes `rustmastra_core` tools/resources as an MCP server.
//!
//! ## Usage
//!
//! ```rust,no_run
//! # use std::sync::Arc;
//! # use rustmastra_mcp::McpServer;
//! # use rustmastra_core::LocalToolRegistry;
//! let registry = Arc::new(LocalToolRegistry::new());
//! // registry.register(my_tool);
//! let server = McpServer::new("my-agent", "0.1.0", registry);
//!
//! // For unit tests — communicate in-process:
//! let (transport, handle) = server.serve_channel();
//!
//! // For production — serve over stdio (compatible with Claude Desktop etc.):
//! // server.serve_stdio().await;
//! ```

use std::sync::Arc;

use tokio::task::JoinHandle;
use tracing::{debug, warn};

use rustmastra_core::ToolExecutor;

use crate::{
    jsonrpc::{codes, parse_incoming, IncomingMessage, Request, Response, RpcError},
    protocol::{
        CallToolParams, CallToolResult, EmbeddedResource, InitializeResult, ListResourcesResult,
        ListToolsResult, McpResource, McpTool, PromptsCapability, ResourcesCapability,
        ServerCapabilities, ServerInfo, ToolContent, ToolsCapability, PROTOCOL_VERSION,
    },
    transport::ChannelTransport,
};

// ─────────────────────────────────────────────────────────────────────────────
// McpServer
// ─────────────────────────────────────────────────────────────────────────────

/// An in-process MCP server backed by a `ToolExecutor`.
///
/// Serves tools from any `LocalToolRegistry` (or custom `ToolExecutor`) over
/// the JSON-RPC protocol.
pub struct McpServer {
    name: String,
    version: String,
    executor: Arc<dyn ToolExecutor>,
    /// Static resources registered with the server.
    resources: Vec<(McpResource, Arc<dyn Fn() -> String + Send + Sync>)>,
}

impl McpServer {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        executor: Arc<dyn ToolExecutor>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            executor,
            resources: Vec::new(),
        }
    }

    /// Register a static resource (text content).
    pub fn add_resource(
        mut self,
        resource: McpResource,
        content: impl Fn() -> String + Send + Sync + 'static,
    ) -> Self {
        self.resources.push((resource, Arc::new(content)));
        self
    }

    // ── Request dispatch ──────────────────────────────────────────────────────

    /// Handle a single JSON-RPC `Request` and return the `Response`.
    pub async fn handle_request(&self, request: Request) -> Response {
        debug!(method = %request.method, id = %request.id, "McpServer dispatching request");

        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(&request),
            "tools/list" => self.handle_list_tools(),
            "tools/call" => self.handle_call_tool(&request).await,
            "resources/list" => self.handle_list_resources(),
            "resources/read" => self.handle_read_resource(&request),
            _ => Err(RpcError::new(
                codes::METHOD_NOT_FOUND,
                format!("Method '{}' not found", request.method),
            )),
        };

        match result {
            Ok(value) => Response::ok(request.id, value),
            Err(err) => Response::err(request.id, err),
        }
    }

    fn handle_initialize(&self, request: &Request) -> Result<serde_json::Value, RpcError> {
        // We accept any protocol version; serve our capabilities.
        let result = InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            server_info: ServerInfo {
                name: self.name.clone(),
                version: self.version.clone(),
            },
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability { list_changed: false }),
                resources: if self.resources.is_empty() {
                    None
                } else {
                    Some(ResourcesCapability { list_changed: false, subscribe: false })
                },
                prompts: None,
            },
            instructions: None,
        };
        serde_json::to_value(result).map_err(|e| {
            RpcError::new(codes::INTERNAL_ERROR, format!("Serialization error: {e}"))
        })
    }

    fn handle_list_tools(&self) -> Result<serde_json::Value, RpcError> {
        let tools: Vec<McpTool> = self
            .executor
            .tool_definitions()
            .into_iter()
            .map(|td| McpTool {
                name: td.name,
                description: td.description,
                input_schema: td.parameters,
            })
            .collect();

        let result = ListToolsResult { tools, next_cursor: None };
        serde_json::to_value(result)
            .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e.to_string()))
    }

    async fn handle_call_tool(
        &self,
        request: &Request,
    ) -> Result<serde_json::Value, RpcError> {
        let params = request.params.clone().ok_or_else(|| {
            RpcError::new(codes::INVALID_PARAMS, "tools/call requires params")
        })?;

        let call_params: CallToolParams =
            serde_json::from_value(params).map_err(|e| {
                RpcError::new(codes::INVALID_PARAMS, format!("Invalid params: {e}"))
            })?;

        let arguments = call_params.arguments.unwrap_or(serde_json::json!({}));

        // Execute through the ToolExecutor.
        let content_block = self
            .executor
            .execute(
                &call_params.name,
                &uuid::Uuid::new_v4().to_string(),
                arguments,
            )
            .await;

        // Convert ContentBlock → CallToolResult.
        let call_result = match content_block {
            rustmastra_core::ContentBlock::ToolResult { content, is_error, .. } => {
                if is_error {
                    CallToolResult::error(content)
                } else {
                    CallToolResult::text(content)
                }
            }
            rustmastra_core::ContentBlock::Text { text } => CallToolResult::text(text),
            other => CallToolResult::text(format!("{other:?}")),
        };

        serde_json::to_value(call_result)
            .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e.to_string()))
    }

    fn handle_list_resources(&self) -> Result<serde_json::Value, RpcError> {
        let resources = self.resources.iter().map(|(r, _)| r.clone()).collect();
        let result = ListResourcesResult { resources, next_cursor: None };
        serde_json::to_value(result)
            .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e.to_string()))
    }

    fn handle_read_resource(&self, request: &Request) -> Result<serde_json::Value, RpcError> {
        let uri = request
            .params
            .as_ref()
            .and_then(|p| p.get("uri"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::new(codes::INVALID_PARAMS, "Missing 'uri' param"))?;

        for (resource, reader) in &self.resources {
            if resource.uri == uri {
                let text = reader();
                let result = crate::protocol::ReadResourceResult {
                    contents: vec![EmbeddedResource {
                        uri: uri.to_owned(),
                        mime_type: resource.mime_type.clone(),
                        text: Some(text),
                        blob: None,
                    }],
                };
                return serde_json::to_value(result)
                    .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e.to_string()));
            }
        }

        Err(RpcError::new(
            codes::INVALID_PARAMS,
            format!("Resource not found: {uri}"),
        ))
    }

    // ── Serve over channels (for tests) ───────────────────────────────────────

    /// Spawn this server as a background task communicating over in-memory
    /// channels.  Returns a `(ChannelTransport, JoinHandle)` pair.
    ///
    /// The `ChannelTransport` can be passed directly to `McpClient::channel()`.
    pub fn serve_channel(self) -> (ChannelTransport, JoinHandle<()>) {
        let (client_transport, mut server_rx, server_tx) = ChannelTransport::pair();
        let server = Arc::new(self);

        let handle = tokio::spawn(async move {
            while let Some(line) = server_rx.recv().await {
                match parse_incoming(&line) {
                    Ok(IncomingMessage::Request(req)) => {
                        let resp = server.handle_request(req).await;
                        if let Ok(json) = serde_json::to_string(&resp) {
                            if server_tx.send(json).await.is_err() {
                                break;
                            }
                        }
                    }
                    Ok(IncomingMessage::Notification(_)) => {
                        // Notifications don't need a response.
                        debug!("McpServer received notification");
                    }
                    Err(e) => warn!("McpServer parse error: {e}"),
                }
            }
        });

        (client_transport, handle)
    }

    /// Serve over stdio (read from stdin, write to stdout).
    ///
    /// This is the standard way to run an MCP server that can be used with
    /// Claude Desktop, Cursor, or any other MCP host.
    pub async fn serve_stdio(self) {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut lines = BufReader::new(stdin).lines();
        let server = Arc::new(self);

        while let Ok(Some(line)) = lines.next_line().await {
            match parse_incoming(&line) {
                Ok(IncomingMessage::Request(req)) => {
                    let resp = server.handle_request(req).await;
                    if let Ok(mut json) = serde_json::to_string(&resp) {
                        json.push('\n');
                        if stdout.write_all(json.as_bytes()).await.is_err() {
                            break;
                        }
                        let _ = stdout.flush().await;
                    }
                }
                Ok(IncomingMessage::Notification(_)) => {}
                Err(e) => warn!("stdio server parse error: {e}"),
            }
        }
    }
}
