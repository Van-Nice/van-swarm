//! Example: connect to the rust-mcp server (Rust development MCP), list tools,
//! and call `cargo_workspace_action` to run `cargo check` on this workspace.
//!
//! ## Usage
//!
//! Build the rust-mcp server first (in the rust-mcp repo):
//!
//!   cd ../rust-mcp && cargo build --release
//!
//! Run this example (from the rust-agent-framework workspace root):
//!
//!   cargo run -p openswarm-mcp --example rust_mcp_client
//!
//! Or set the server binary path explicitly:
//!
//!   RUST_MCP_BIN=/path/to/rust-mcp/target/release/rust-mcp cargo run -p openswarm-mcp --example rust_mcp_client

use openswarm_mcp::{CallToolResult, McpClient};

fn rust_mcp_bin() -> String {
    std::env::var("RUST_MCP_BIN").unwrap_or_else(|_| {
        let cwd = std::env::current_dir().unwrap_or_default();
        let path = cwd.join("../rust-mcp/target/release/rust-mcp");
        path.canonicalize()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string_lossy().into_owned())
    })
}

#[tokio::main]
async fn main() -> openswarm_core::Result<()> {
    let bin = rust_mcp_bin();
    let args = ["--client=cursor"];

    println!("Spawning MCP server: {} {:?}", bin, &args);
    let client = McpClient::stdio(&bin, &args).await?;

    println!("Initializing...");
    let init = client.initialize().await?;
    println!(
        "Server: {} {} (protocol {})",
        init.server_info.name,
        init.server_info.version,
        init.protocol_version
    );

    println!("\nListing tools...");
    let tools = client.list_tools().await?;
    println!("Found {} tools:", tools.len());
    for t in &tools {
        println!("  - {}: {}", t.name, t.description.lines().next().unwrap_or("").trim());
    }

    // Call cargo_workspace_action to run `cargo check` on this workspace
    let workspace_root = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let workspace_root = workspace_root.to_string_lossy();

    let tool_name = tools
        .iter()
        .find(|t| t.name == "cargo_workspace_action")
        .map(|t| t.name.as_str())
        .unwrap_or("cargo_workspace_action");

    println!("\nCalling {} (cargo check, toon format)...", tool_name);
    let params = serde_json::json!({
        "command": "check",
        "workspace_root": workspace_root.as_ref(),
        "output_format": "toon"
    });

    let result = client.call_tool(tool_name, params).await?;
    print_call_result(&result);

    Ok(())
}

fn print_call_result(r: &CallToolResult) {
    for c in &r.content {
        match c {
            openswarm_mcp::ToolContent::Text { text } => println!("{}", text),
            _ => {}
        }
    }
    if r.is_error == Some(true) {
        eprintln!("(Tool returned error content)");
    }
}
