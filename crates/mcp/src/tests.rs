//! Integration tests for MCP client ↔ server over the in-memory channel transport.

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::json;

    use openswarm_core::{ContentBlock, ToolDefinition, ToolExecutor};

    use crate::{McpClient, McpServer};

    // ── Minimal in-process tool executor ──────────────────────────────────────

    struct EchoExecutor;

    #[async_trait]
    impl ToolExecutor for EchoExecutor {
        fn tool_definitions(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition {
                name: "echo".into(),
                description: "Returns the input message unchanged.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "message": { "type": "string", "description": "Text to echo" }
                    },
                    "required": ["message"]
                }),
                examples: vec![],
            }]
        }

        async fn execute(
            &self,
            name: &str,
            tool_use_id: &str,
            arguments: serde_json::Value,
        ) -> ContentBlock {
            if name != "echo" {
                return ContentBlock::ToolResult {
                    tool_use_id: tool_use_id.into(),
                    content: format!("Unknown tool: {name}"),
                    is_error: true,
                };
            }
            let msg = arguments["message"].as_str().unwrap_or("(empty)");
            ContentBlock::ToolResult {
                tool_use_id: tool_use_id.into(),
                content: format!("echo: {msg}"),
                is_error: false,
            }
        }
    }

    fn make_server() -> McpServer {
        McpServer::new("test-server", "0.1.0", Arc::new(EchoExecutor))
    }

    // ── Test 1: initialize handshake ──────────────────────────────────────────

    #[tokio::test]
    async fn test_initialize() {
        let (transport, _handle) = make_server().serve_channel();
        let client = McpClient::channel(transport);

        let info = client.initialize().await.expect("initialize failed");
        assert_eq!(info.server_info.name, "test-server");
    }

    // ── Test 2: list tools ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_list_tools() {
        let (transport, _handle) = make_server().serve_channel();
        let client = McpClient::channel(transport);

        client.initialize().await.unwrap();
        let tools = client.list_tools().await.expect("list_tools failed");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");
    }

    // ── Test 3: call tool ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_call_tool() {
        let (transport, _handle) = make_server().serve_channel();
        let client = McpClient::channel(transport);

        client.initialize().await.unwrap();
        let result = client
            .call_tool("echo", json!({"message": "hello MCP"}))
            .await
            .expect("call_tool failed");

        assert!(result.is_error.unwrap_or(false) == false);
        let text = result.content[0].as_text().expect("text content");
        assert_eq!(text, "echo: hello MCP");
    }

    // ── Test 4: call unknown tool → is_error ──────────────────────────────────

    #[tokio::test]
    async fn test_call_unknown_tool() {
        let (transport, _handle) = make_server().serve_channel();
        let client = McpClient::channel(transport);

        client.initialize().await.unwrap();
        let result = client
            .call_tool("nonexistent", json!({}))
            .await
            .expect("call_tool should succeed at transport level");
        assert_eq!(result.is_error, Some(true));
    }

    // ── Test 5: resources ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_resources() {
        use crate::protocol::McpResource;

        let resource = McpResource {
            uri: "file:///readme".into(),
            name: "README".into(),
            description: None,
            mime_type: Some("text/plain".into()),
        };

        let server = McpServer::new("resource-server", "0.1.0", Arc::new(EchoExecutor))
            .add_resource(resource, || "Hello from resource!".into());

        let (transport, _handle) = server.serve_channel();
        let client = McpClient::channel(transport);
        client.initialize().await.unwrap();

        let resources = client.list_resources().await.expect("list_resources failed");
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].uri, "file:///readme");

        let content =
            client.read_resource("file:///readme").await.expect("read_resource failed");
        let text = content.contents[0].text.as_deref().unwrap_or_default();
        assert_eq!(text, "Hello from resource!");
    }

    // ── Test 6: JSON-RPC serialization round-trip ─────────────────────────────

    #[tokio::test]
    async fn test_jsonrpc_roundtrip() {
        use crate::jsonrpc::{Request, Response, ResponseBody};

        let req = Request::new(42i64, "tools/list", None);
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(back.method, "tools/list");

        let resp = Response::ok(42i64, json!({"tools": []}));
        let json = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&json).unwrap();
        assert!(matches!(back.body, ResponseBody::Ok { .. }));
    }
}
