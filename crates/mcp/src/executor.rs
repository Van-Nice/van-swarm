//! MCP-backed `ToolExecutor` — use any MCP server's tools with a RustMastra agent.
//!
//! After connecting and initializing an `McpClient`, wrap it in `McpToolExecutor`,
//! call `refresh_tools().await`, then pass the executor to `ReActAgent`. The agent
//! will see the server's tools and execute them via MCP when the model requests.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rustmastra_core::message::{ContentBlock, ToolDefinition, ToolExample};

use crate::client::McpClient;
use crate::protocol::McpTool;

// ─────────────────────────────────────────────────────────────────────────────
// McpToolExecutor
// ─────────────────────────────────────────────────────────────────────────────

/// A `ToolExecutor` that forwards tool definitions and calls to an MCP server.
///
/// Use this to give an agent access to rust-mcp, filesystem, or any other
/// MCP server's tools without implementing them in Rust.
pub struct McpToolExecutor {
    client: Arc<McpClient>,
    /// Cached tool list so `tool_definitions()` can return sync (populate via `refresh_tools()`).
    cache: Mutex<Option<Vec<McpTool>>>,
}

impl McpToolExecutor {
    /// Create an executor that uses the given MCP client.
    ///
    /// Call `refresh_tools().await` after `client.initialize().await?` so the
    /// agent sees the server's tools.
    pub fn new(client: Arc<McpClient>) -> Self {
        Self {
            client,
            cache: Mutex::new(None),
        }
    }

    /// Refresh the cached tool list from the MCP server.
    ///
    /// Call this after `client.initialize().await?` so that `tool_definitions()`
    /// returns the server's tools. Required before using this executor with an agent.
    pub async fn refresh_tools(&self) -> rustmastra_core::Result<()> {
        let tools = self.client.list_tools().await?;
        if let Ok(mut g) = self.cache.lock() {
            *g = Some(tools);
        }
        Ok(())
    }
}

#[async_trait]
impl rustmastra_core::ToolExecutor for McpToolExecutor {
    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        if let Ok(g) = self.cache.lock() {
            if let Some(ref tools) = *g {
                return tools
                    .iter()
                    .map(|t| ToolDefinition {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.input_schema.clone(),
                        examples: Vec::<ToolExample>::new(),
                    })
                    .collect();
            }
        }
        Vec::new()
    }

    async fn execute(
        &self,
        tool_name: &str,
        tool_use_id: &str,
        arguments: serde_json::Value,
    ) -> ContentBlock {
        match self.client.call_tool(tool_name, arguments).await {
            Ok(result) => {
                let text = result
                    .content
                    .iter()
                    .filter_map(|c| c.as_text().map(String::from))
                    .collect::<Vec<_>>()
                    .join("\n");
                let is_error = result.is_error == Some(true);
                if is_error {
                    ContentBlock::tool_error(tool_use_id, text)
                } else {
                    ContentBlock::tool_result(tool_use_id, text)
                }
            }
            Err(e) => ContentBlock::tool_error(tool_use_id, e.to_string()),
        }
    }
}
