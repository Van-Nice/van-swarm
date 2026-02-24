# Examples reference

This page indexes **runnable examples** and **copy-paste snippets** you can use with the framework.

---

## 1. In-repo examples

| Crate | Example | How to run |
|-------|---------|------------|
| **rustmastra-core** | `basic_agent` | `cargo run -p rustmastra-core --example basic_agent` |
| **rustmastra-mcp** | `rust_mcp_client` | `cargo run -p rustmastra-mcp --example rust_mcp_client` |

**rust_mcp_client** connects to the rust-mcp server (Rust dev MCP), lists tools, and calls `cargo_workspace_action` (e.g. `cargo check`). Set `RUST_MCP_BIN` to the path of the rust-mcp binary if it is not under `../rust-mcp/target/release/rust-mcp`.

---

## 2. Minimal agent (no tools)

Create a binary that depends on `rustmastra-core` and paste:

```rust
use std::sync::Arc;
use rustmastra_core::{
    config::{AgentConfig, ModelConfig},
    providers::AnthropicProvider,
    react::{run_agent, ReActAgent},
    traits::tool::LocalToolRegistry,
};

#[tokio::main]
async fn main() -> rustmastra_core::Result<()> {
    let provider = AnthropicProvider::from_env()?;
    let executor = LocalToolRegistry::new();
    let config = AgentConfig::new("minimal", ModelConfig::new("claude-sonnet-4-20250514"));
    let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(executor));
    println!("{}", run_agent(&agent, "What is 2+2?").await?);
    Ok(())
}
```

---

## 3. Agent with built-in TimeTool

```rust
use std::sync::Arc;
use rustmastra_core::{
    config::{AgentConfig, ModelConfig},
    providers::AnthropicProvider,
    react::{run_agent, ReActAgent},
    traits::tool::LocalToolRegistry,
    TimeTool,
};

#[tokio::main]
async fn main() -> rustmastra_core::Result<()> {
    let provider = AnthropicProvider::from_env()?;
    let executor = LocalToolRegistry::new().register(TimeTool);
    let config = AgentConfig::new("with-time", ModelConfig::new("claude-sonnet-4-20250514"));
    let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(executor));
    println!("{}", run_agent(&agent, "What time is it in UTC?").await?);
    Ok(())
}
```

---

## 4. Agent with metrics (for SPL / evals)

```rust
use rustmastra_core::react::run_agent_with_metrics;

let (answer, metrics) = run_agent_with_metrics(&agent, "Question?").await?;
println!("Answer: {}", answer);
println!("Iterations: {}  Tool calls: {}", metrics.iterations, metrics.tool_call_count);
```

---

## 5. Custom Tool impl

```rust
use async_trait::async_trait;
use rustmastra_core::message::ToolDefinition;
use rustmastra_core::traits::tool::Tool;

struct GreetTool;

#[async_trait]
impl Tool for GreetTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "greet".into(),
            description: "Greet by name.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "name": { "type": "string" } },
                "required": ["name"]
            }),
            examples: vec![],
        }
    }
    async fn execute(&self, args: serde_json::Value) -> rustmastra_core::Result<String> {
        let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("World");
        Ok(format!("Hello, {}!", name))
    }
}
```

---

## 6. MCP client + list tools

```rust
use rustmastra_mcp::McpClient;

let client = McpClient::stdio("npx", &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]).await?;
client.initialize().await?;
for t in client.list_tools().await? {
    println!("{}: {}", t.name, t.description.lines().next().unwrap_or(""));
}
```

---

## 7. MCP server (expose tools)

```rust
use std::sync::Arc;
use rustmastra_core::traits::tool::LocalToolRegistry;
use rustmastra_core::TimeTool;
use rustmastra_mcp::McpServer;

let registry = LocalToolRegistry::new().register(TimeTool);
McpServer::new("my-server", "0.1.0", Arc::new(registry)).serve_stdio();
```

---

## 8. DurableContext + run_once

```rust
use std::sync::Arc;
use rustmastra_core::durable::{DurableContext, InMemoryJournal};

let journal = Arc::new(InMemoryJournal::new());
let ctx = DurableContext::new("run-1", journal, None);
let x = ctx.run_once("step1", async { Ok::<_, rustmastra_core::FrameworkError>("done") }).await?;
```

---

## 9. Orchestrator: two-node graph

```rust
use async_trait::async_trait;
use rustmastra_orchestrator::{FlowRunner, GraphBuilder, NextAction, NodeKey, Task};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Default, Serialize, Deserialize)]
struct S { value: String }

struct Node1;
#[async_trait]
impl Task for Node1 {
    type State = S;
    async fn run(&self, _: NodeKey, mut s: S) -> rustmastra_core::Result<(S, NextAction)> {
        s.value = "one".into();
        Ok((s, NextAction::Continue))
    }
    fn name(&self) -> &str { "n1" }
}
struct Node2;
#[async_trait]
impl Task for Node2 {
    type State = S;
    async fn run(&self, _: NodeKey, mut s: S) -> rustmastra_core::Result<(S, NextAction)> {
        s.value = format!("{} -> two", s.value);
        Ok((s, NextAction::End))
    }
    fn name(&self) -> &str { "n2" }
}

let mut b = GraphBuilder::new();
let k1 = b.add_node(Node1);
let k2 = b.add_node(Node2);
b.edge(k1, k2);
b.start(k1);
let result = FlowRunner::new(Arc::new(b.build())).run(serde_json::json!({})).await?;
println!("{:?}", result.state);
```

---

## 10. Memory: EpisodicMemory store and search

```rust
use rustmastra_memory::{EpisodicMemory, Memory, MemoryEntry};

let mem = EpisodicMemory::new(100);
mem.store(MemoryEntry::new("User asked about Rust")).await?;
let recent = mem.recent(5).await?;
let hits = mem.search("Rust", 5).await?;
```

---

## 11. Evaluation: ContainsScorer + spl

```rust
use rustmastra_core::evaluators::{ContainsScorer, ScoreInput, Scorer, SplRun, spl};

let scorer = ContainsScorer::default();
let input = ScoreInput {
    messages: vec![],
    final_answer: "The capital is Paris.".into(),
    expected: Some("Paris".into()),
};
let r = scorer.score(&input).await?;
let runs = vec![SplRun { score: r.score, path_length: 2, optimal_path_length: 1 }];
println!("SPL: {}", spl(&runs));
```

---

## 12. Where each snippet is explained

| Snippet | Guide |
|--------|--------|
| Minimal agent, TimeTool, metrics | [01-quick-start](01-quick-start.md), [02-building-an-agent](02-building-an-agent.md) |
| Custom Tool, registry | [03-tools](03-tools.md) |
| MCP client, executor, server | [04-mcp](04-mcp.md) |
| DurableContext, run_once | [05-durable-workflows](05-durable-workflows.md) |
| GraphBuilder, FlowRunner, Task | [06-orchestrator](06-orchestrator.md) |
| EpisodicMemory, search | [07-memory](07-memory.md) |
| Scorer, SPL | [08-evaluation](08-evaluation.md) |
| Sandbox, run_compiled | [09-runtime-wasm](09-runtime-wasm.md) |

For a single high-level overview, see [documentation/HOW-IT-WORKS.md](../HOW-IT-WORKS.md).
