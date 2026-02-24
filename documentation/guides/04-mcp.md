# MCP (Model Context Protocol)

This guide covers **McpClient**, **McpToolExecutor** (using MCP tools from an agent), and **McpServer** (exposing your tools as MCP).

---

## 1. Overview

MCP standardises how agents discover and call external tools. VanSwarm provides:

- **McpClient** — connect to an MCP server (stdio, HTTP, or in-memory channel), list tools, call tools.
- **McpToolExecutor** — implements `ToolExecutor`; your ReActAgent uses it to call the MCP server’s tools.
- **McpServer** — turn a `ToolExecutor` (e.g. `LocalToolRegistry`) into an MCP server for IDEs or other clients.

---

## 2. Connecting to an MCP server (McpClient)

### Stdio (spawn a subprocess)

Typical for local servers (e.g. filesystem, rust-mcp):

```rust
use vanswarm_mcp::McpClient;

// Filesystem server (read-only under /tmp)
let client = McpClient::stdio(
    "npx",
    &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
).await?;

// Or rust-mcp (Rust dev tools)
let bin = std::env::var("RUST_MCP_BIN").unwrap_or_else(|_| "../rust-mcp/target/release/rust-mcp".into());
let client = McpClient::stdio(&bin, &["--client=cursor"]).await?;
```

### HTTP

```rust
let client = McpClient::http("https://mcp.example.com/sse").await?;
```

### In-memory (tests)

```rust
use vanswarm_mcp::transport::ChannelTransport;

let (client, _server_handle) = ChannelTransport::pair().await?;
```

Always call **initialize()** before listing or calling tools:

```rust
let init = client.initialize().await?;
println!("Server: {} {}", init.server_info.name, init.server_info.version);
let tools = client.list_tools().await?;
```

---

## 3. Using MCP tools in an agent (McpToolExecutor)

**McpToolExecutor** implements core’s **ToolExecutor**. Give it an **McpClient**, then call **refresh_tools()** after the client is initialized so the agent sees the server’s tools.

```rust
use std::sync::Arc;
use vanswarm_core::{
    config::{AgentConfig, ModelConfig},
    providers::AnthropicProvider,
    react::{run_agent, ReActAgent},
};
use vanswarm_mcp::{McpClient, McpToolExecutor};

#[tokio::main]
async fn main() -> vanswarm_core::Result<()> {
    let client = McpClient::stdio(
        "npx",
        &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
    ).await?;
    client.initialize().await?;

    let executor = McpToolExecutor::new(Arc::new(client));
    executor.refresh_tools().await?;

    let provider = vanswarm_core::providers::AnthropicProvider::from_env()?;
    let config = AgentConfig::new("mcp-agent", ModelConfig::new("claude-sonnet-4-20250514"))
        .with_max_iterations(15);
    let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(executor));

    let answer = run_agent(&agent, "List the top-level files in /tmp.").await?;
    println!("{}", answer);
    Ok(())
}
```

You can **combine** a local registry and MCP: wrap both in a composite executor or register local tools in a registry and use that for some agents while using McpToolExecutor for MCP-only tools (architecture depends on how you compose executors).

---

## 4. Calling a tool directly (without an agent)

```rust
let client = McpClient::stdio("npx", &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]).await?;
client.initialize().await?;

let result = client.call_tool("read_file", serde_json::json!({ "path": "hello.txt" })).await?;
match result {
    vanswarm_mcp::CallToolResult::Content(blocks) => {
        for b in blocks {
            println!("{:?}", b);
        }
    }
    vanswarm_mcp::CallToolResult::Error { text, .. } => eprintln!("Tool error: {}", text),
}
```

---

## 5. Exposing your tools as an MCP server (McpServer)

Use **McpServer** to serve a **ToolExecutor** (e.g. **LocalToolRegistry**) so IDEs (Cursor, etc.) or other MCP clients can call your tools.

```rust
use std::sync::Arc;
use vanswarm_core::traits::tool::LocalToolRegistry;
use vanswarm_core::TimeTool;
use vanswarm_mcp::McpServer;

let registry = LocalToolRegistry::new().register(TimeTool);
let server = McpServer::new("my-tools", "0.1.0", Arc::new(registry));
server.serve_stdio(); // blocks: reads stdin, writes stdout (JSON-RPC)
```

For tests or in-process use, **serve_channel** returns a transport that you can pass to **McpClient** on the other end.

---

## 6. Runnable example in this repo

The **vanswarm-mcp** crate includes an example that connects to the rust-mcp server and runs `cargo_workspace_action`:

```bash
# Build rust-mcp first (in its repo):
#   cd ../rust-mcp && cargo build --release

# From this workspace root:
cargo run -p vanswarm-mcp --example rust_mcp_client
```

Or set the server path explicitly:

```bash
RUST_MCP_BIN=/path/to/rust-mcp/target/release/rust-mcp cargo run -p vanswarm-mcp --example rust_mcp_client
```

---

## 7. Context rot mitigation

When a server exposes many tools or resources, vague or overlapping descriptions can hurt model performance (“context rot”). Prefer:

- Clear, **action-oriented** tool names and descriptions.
- **FilteredToolExecutor** (or a custom executor) to expose only a subset of tools per agent/task.

See [documentation/architecture/04-mcp.md](../architecture/04-mcp.md) for the full context-rot section.

---

## 8. Next steps

- Add local tools alongside MCP: [03-tools](03-tools.md).
- Durable workflows that call tools: [05-durable-workflows](05-durable-workflows.md).
