# Quick start

Get the RustMastra framework into your project and run your first agent in a few minutes.

**Scaffold a new agent:** From the framework repo, run `cargo run -p rustmastra-cli -- new my_agent` to generate a ready-to-build project (see [CLI proposal](../proposals/CLI-NEW-AGENT-BOILERPLATE.md) for `--provider`, `--with-tools`, and other flags).

---

## 1. Add the dependency

If you are **inside this repo** (rust-agent-framework), use workspace path dependencies. From a **separate project**, add the framework as a path or git dependency.

### Option A: New binary in this workspace

Create a new crate under the workspace (e.g. `apps/demo`) and add to the root `Cargo.toml`:

```toml
# In workspace Cargo.toml members:
members = [ "crates/core", "crates/orchestrator", ... , "apps/demo" ]
```

In `apps/demo/Cargo.toml`:

```toml
[package]
name = "demo"
version = "0.1.0"
edition = "2021"

[dependencies]
rustmastra-core = { path = "../../crates/core" }
tokio = { version = "1", features = ["full"] }
```

### Option B: External project (path or git)

```toml
[dependencies]
rustmastra-core = { path = "../rust-agent-framework/crates/core" }
# Or:
# rustmastra-core = { git = "https://github.com/your-org/rust-agent-framework", branch = "main" }
tokio = { version = "1", features = ["full"] }
```

---

## 2. Set an API key

Pick one provider and set the corresponding env var:

```bash
# Anthropic (Claude)
export ANTHROPIC_API_KEY=sk-ant-...

# OpenAI (GPT-4, etc.)
export OPENAI_API_KEY=sk-...

# Google Gemini
export GEMINI_API_KEY=...
```

---

## 3. Minimal agent (no tools)

Create `src/main.rs` (or run inside this repo with the crate that has `rustmastra-core`):

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
    // Optional: enable logging
    tracing_subscriber::fmt::init();

    let provider = AnthropicProvider::from_env()?;
    let executor = LocalToolRegistry::new(); // no tools yet
    let config = AgentConfig::new(
        "quickstart",
        ModelConfig::new("claude-sonnet-4-20250514"),
    )
    .with_max_iterations(10);

    let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(executor));
    let answer = run_agent(&agent, "What is 2 + 2? Reply in one sentence.").await?;
    println!("{}", answer);
    Ok(())
}
```

Run (from workspace root):

```bash
cargo run -p rustmastra-core --example basic_agent
```

Or from a crate that depends on `rustmastra-core`, run your own binary (e.g. `cargo run`).

---

## 4. Agent with one built-in tool

Use the built-in **TimeTool** so the model can ask for the current time:

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
    tracing_subscriber::fmt::init();

    let provider = AnthropicProvider::from_env()?;
    let executor = LocalToolRegistry::new().register(TimeTool);
    let config = AgentConfig::new(
        "quickstart-with-tools",
        ModelConfig::new("claude-sonnet-4-20250514"),
    )
    .with_max_iterations(10);

    let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(executor));
    let answer = run_agent(
        &agent,
        "What is the current UTC time? Use the tool if available and say it in one sentence.",
    )
    .await?;
    println!("{}", answer);
    Ok(())
}
```

---

## 5. Using a different provider

Swap the provider; the rest stays the same:

```rust
// OpenAI
use rustmastra_core::providers::OpenAiProvider;
let provider = OpenAiProvider::from_env()?;
// ModelConfig::new("gpt-4o") or "gpt-4o-mini", etc.

// Gemini
use rustmastra_core::providers::GeminiProvider;
let provider = GeminiProvider::from_env()?;
// ModelConfig::new("gemini-2.0-flash") or your model id
```

---

## 6. Next steps

- Add your own tools: [03-tools](03-tools.md).
- Connect to MCP servers: [04-mcp](04-mcp.md).
- Full agent options: [02-building-an-agent](02-building-an-agent.md).
