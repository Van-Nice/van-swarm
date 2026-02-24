# Building an agent

This guide covers **ReActAgent**, configuration, model providers, and the main entry points: `run_agent`, `run_agent_with_metrics`, and `run_agent_traced`.

---

## 1. The ReAct loop

RustMastra agents use the **ReAct** pattern: the model receives conversation history and tool definitions, then either:

- Returns a **final answer** (text), or  
- Issues **tool calls**; the runner executes them, appends results to the conversation, and calls the model again.

This repeats until the model answers or the iteration limit is reached.

---

## 2. Creating a ReActAgent

You need three things:

1. **AgentConfig** — name, model id, system prompt, max iterations, etc.  
2. **ModelProvider** — OpenAI, Anthropic, or Gemini (or your own impl).  
3. **ToolExecutor** — e.g. `LocalToolRegistry` or `McpToolExecutor`.

```rust
use std::sync::Arc;
use rustmastra_core::{
    config::{AgentConfig, ModelConfig},
    providers::AnthropicProvider,
    react::ReActAgent,
    traits::tool::LocalToolRegistry,
};

let provider = AnthropicProvider::from_env()?;
let executor = LocalToolRegistry::new(); // add tools with .register(...)
let config = AgentConfig::new("my-agent", ModelConfig::new("claude-sonnet-4-20250514"))
    .with_system_prompt("You are a helpful assistant.")
    .with_max_iterations(20);

let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(executor));
```

---

## 3. AgentConfig in detail

| Method | Purpose |
|--------|---------|
| `AgentConfig::new(name, model_config)` | Required: agent name and model id. |
| `.with_system_prompt(s)` | System message the model sees every run. |
| `.with_max_iterations(n)` | Cap on ReAct steps (default is finite; avoid runaway loops). |
| `.with_chain_of_thought(true)` | Asks the model to reason in `<thinking>…</thinking>` tags. |
| `.with_model_config(c)` | Replace model config (temperature, max_tokens, etc.). |

**ModelConfig:**

```rust
use rustmastra_core::config::ModelConfig;

// Minimal: model id only
let model = ModelConfig::new("claude-sonnet-4-20250514");

// With options
let model = ModelConfig::new("gpt-4o")
    .with_temperature(0.2)
    .with_max_tokens(4096);
```

---

## 4. Running the agent

### run_agent — final answer only

```rust
use rustmastra_core::react::run_agent;

let answer: String = run_agent(&agent, "Summarize the Rust ownership rules in 3 bullets.").await?;
println!("{}", answer);
```

### run_agent_with_metrics — answer + RunMetrics (for SPL / evals)

```rust
use rustmastra_core::react::run_agent_with_metrics;

let (answer, metrics) = run_agent_with_metrics(&agent, "What is the current year?").await?;
println!("Answer: {}", answer);
println!("Iterations: {}", metrics.iterations);
println!("Tool calls: {}", metrics.tool_call_count); // path length for SPL
```

Use `metrics.tool_call_count` as L_exec when computing **SPL** (Success weighted by Path Length); see [08-evaluation](08-evaluation.md).

### run_agent_traced — full APM trace

```rust
use rustmastra_core::react::run_agent_traced;

let (answer, trace) = run_agent_traced(&agent, "Explain async Rust in one paragraph.").await?;
println!("{}", answer);
println!("{}", trace.summary()); // iterations, tokens, duration, estimated cost
```

You can persist `trace` with a `TraceStore` (e.g. `InMemoryTraceStore`, `FileTraceStore`) for observability.

---

## 5. Extracting chain-of-thought

If you enable chain-of-thought, the model may emit `<thinking>…</thinking>` in its reply. Parse it with:

```rust
use rustmastra_core::message::extract_xml_blocks;

let assistant_text = "..."; // from the final message
let thoughts = extract_xml_blocks(assistant_text, "thinking");
for t in &thoughts {
    println!("Thought: {}", t);
}
```

---

## 6. Using the Router (supervisor)

To route by complexity and choose a cheaper or more capable model:

```rust
use rustmastra_core::supervisor::{Router, Route, AlwaysTier1};

// Stub: always Tier1 (e.g. fast/cheap model)
let router: AlwaysTier1 = AlwaysTier1;
let route = router.route("What is 2+2?").await?;
match route {
    Route::Tier1 => { /* use claude-3-haiku or gpt-4o-mini */ }
    Route::Tier2 => { /* use claude-sonnet */ }
    Route::Tier3 => { /* use claude-opus or o1 */ }
}
```

You can implement `Router` yourself (e.g. keyword-based or LLM-based) and build the appropriate `ReActAgent` per tier.

---

## 7. Full example: config + metrics + provider swap

```rust
use std::sync::Arc;
use rustmastra_core::{
    config::{AgentConfig, ModelConfig},
    providers::{AnthropicProvider, OpenAiProvider},
    react::{run_agent_with_metrics, ReActAgent},
    traits::tool::LocalToolRegistry,
};

#[tokio::main]
async fn main() -> rustmastra_core::Result<()> {
    let use_openai = std::env::var("USE_OPENAI").is_ok();
    let provider: Arc<dyn rustmastra_core::providers::ModelProvider> = if use_openai {
        Arc::new(OpenAiProvider::from_env()?)
    } else {
        Arc::new(AnthropicProvider::from_env()?)
    };

    let model_id = if use_openai { "gpt-4o-mini" } else { "claude-sonnet-4-20250514" };
    let config = AgentConfig::new("multi-provider", ModelConfig::new(model_id))
        .with_max_iterations(15);

    let executor = LocalToolRegistry::new();
    let agent = ReActAgent::new(config, provider, Arc::new(executor));

    let (answer, metrics) = run_agent_with_metrics(&agent, "What is 2+2?").await?;
    println!("{}", answer);
    println!("Iterations: {}  Tool calls: {}", metrics.iterations, metrics.tool_call_count);
    Ok(())
}
```

---

## 8. Next steps

- Register custom tools: [03-tools](03-tools.md).  
- Use MCP tools: [04-mcp](04-mcp.md).  
- Evaluate runs (SPL, batch): [08-evaluation](08-evaluation.md).
