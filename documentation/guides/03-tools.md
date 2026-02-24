# Tools

This guide covers implementing **Tool**, registering tools with **LocalToolRegistry**, using **built-in tools**, and the **#[tool]** macro.

---

## 1. Tool trait

A **Tool** has:

- **definition()** — JSON schema and description the model uses to discover and call the tool.
- **execute(arguments)** — run the tool with the parsed JSON; return a string (or error) the model can read.

```rust
use async_trait::async_trait;
use vanswarm_core::{
    message::ToolDefinition,
    traits::tool::Tool,
};

struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "echo".into(),
            description: "Echo back the given message.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string", "description": "Text to echo" }
                },
                "required": ["message"]
            }),
            examples: vec![],
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> vanswarm_core::Result<String> {
        let msg = arguments
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        Ok(msg.to_string())
    }
}
```

**Important:** Validate `arguments` in `execute`; the model output is untrusted. Return `Err(FrameworkError::ToolExecution { ... })` so the agent gets a tool_error block and can retry.

---

## 2. Registering tools

Use **LocalToolRegistry** and pass it to **ReActAgent** as the **ToolExecutor**:

```rust
use vanswarm_core::traits::tool::LocalToolRegistry;

let executor = LocalToolRegistry::new()
    .register(EchoTool)
    .register(TimeTool)  // built-in
    .register(MyOtherTool);

let agent = ReActAgent::new(config, provider, Arc::new(executor));
```

---

## 3. Built-in tools

| Tool             | Purpose                                                          | Parameters                |
| ---------------- | ---------------------------------------------------------------- | ------------------------- |
| **TimeTool**     | Current UTC time (ISO 8601)                                      | None                      |
| **ReadFileTool** | Read file under a root path (path traversal rejected)            | `path` (relative to root) |
| **SearchTool**   | Stub search (returns a message; use MCP or custom in production) | `query`                   |

Example:

```rust
use vanswarm_core::tools::builtin::{ReadFileTool, TimeTool};

let executor = LocalToolRegistry::new()
    .register(TimeTool)
    .register(ReadFileTool::new("/allowed/root"));
```

---

## 4. #[tool] macro

The **vanswarm-macros** crate provides **#[tool]** to derive the schema from your function signature and Rustdoc, and optionally add **examples** for the model.

Add dependency:

```toml
vanswarm-macros = { path = "../macros" }  # or your path
vanswarm-core = { path = "../core" }
```

Example:

```rust
use vanswarm_core::Result;
use vanswarm_macros::tool;

/// Multiplies two integers.
#[tool]
async fn multiply(a: i64, b: i64) -> Result<String> {
    Ok((a * b).to_string())
}
```

The macro generates a struct and `Tool` impl with:

- **name**: `multiply`
- **description**: from the doc comment
- **input_schema**: from parameter types (schemars)

Optional **examples** (for few-shot tool use):

```rust
/// Get the current weather for a city (stub).
#[tool(example(
    description = "User asks for weather in Paris",
    input = r#"{"city": "Paris"}"#,
    output = r#"Sunny, 22°C"#
))]
async fn get_weather(city: String) -> Result<String> {
    Ok(format!("Weather for {}: stub", city))
}
```

Register the generated type:

```rust
let executor = LocalToolRegistry::new()
    .register(multiply_tool())  // or the type name the macro generates
```

(Check the macro crate docs for the exact generated name, e.g. a wrapper type that implements `Tool`.)

---

## 5. FilteredToolExecutor — limit which tools the agent sees

Wrap another executor and restrict by keyword:

```rust
use vanswarm_core::traits::tool::{FilteredToolExecutor, LocalToolRegistry};

let base = LocalToolRegistry::new().register(TimeTool).register(ReadFileTool::new("/tmp"));
let executor = FilteredToolExecutor::new(
    base,
    vec!["time".into()],  // only expose tools whose name contains "time"
);
```

Useful when you have many tools but want to give the agent only a subset per task.

---

## 6. Full example: custom tool + built-in

```rust
use async_trait::async_trait;
use std::sync::Arc;
use vanswarm_core::{
    config::{AgentConfig, ModelConfig},
    message::ToolDefinition,
    providers::AnthropicProvider,
    react::{run_agent, ReActAgent},
    traits::tool::{LocalToolRegistry, Tool},
    TimeTool,
};

struct GreetTool;

#[async_trait]
impl Tool for GreetTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "greet".into(),
            description: "Greet someone by name.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "name": { "type": "string" } },
                "required": ["name"]
            }),
            examples: vec![],
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> vanswarm_core::Result<String> {
        let name = arguments.get("name").and_then(|v| v.as_str()).unwrap_or("World");
        Ok(format!("Hello, {}!", name))
    }
}

#[tokio::main]
async fn main() -> vanswarm_core::Result<()> {
    let provider = AnthropicProvider::from_env()?;
    let executor = LocalToolRegistry::new()
        .register(GreetTool)
        .register(TimeTool);
    let config = AgentConfig::new("tools-demo", ModelConfig::new("claude-sonnet-4-20250514"))
        .with_max_iterations(10);
    let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(executor));

    let answer = run_agent(&agent, "Greet Alice and tell me the current time.").await?;
    println!("{}", answer);
    Ok(())
}
```

---

## 7. Next steps

- Expose your tools to IDEs via MCP: [04-mcp](04-mcp.md).
- Call external MCP tools from the agent: [04-mcp](04-mcp.md).
