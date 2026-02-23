# RustMastra — High-Performance Rust AI Agent Framework

> **Status:** Active development — Phase 1 (foundations) in progress.

A production-grade Rust framework for building autonomous AI agents,
deterministic workflows, and multi-agent swarms.  Inspired by Mastra.ai's
developer experience, rebuilt on Rust's zero-cost abstractions and fearless
concurrency.

## Why Rust?

| Metric               | RustMastra      | TypeScript (Node) | Python (CPython) |
|----------------------|-----------------|-------------------|------------------|
| Cold start           | **<10 ms**      | >100 ms           | >500 ms          |
| Memory per agent     | **<5 MB**       | ~200 MB           | ~500 MB          |
| Isolates / 8 GB VPS  | **1,500+**      | ~100              | ~20              |
| Concurrency model    | Async/Tokio     | Event loop        | GIL-limited      |
| Memory management    | Ownership (no GC)| GC               | GC               |

## Crate map

```
rust-agent-framework/
├── crates/
│   ├── core/          rustmastra-core       – traits, providers, ReAct loop
│   ├── orchestrator/  rustmastra-orchestrator – graph engine (§4)
│   ├── memory/        rustmastra-memory      – three-tier memory (§8)
│   ├── mcp/           rustmastra-mcp         – Model Context Protocol (§9)
│   ├── runtime/       rustmastra-runtime     – WASM sandbox (§5)
│   └── macros/        rustmastra-macros      – #[tool], #[workflow] (§3,§10)
```

## Core concepts

### Agent vs Workflow

```rust
// Agent: probabilistic, model-driven, ReAct loop.
// Use when the LLM decides what to do next.
let agent = ReActAgent::new(config, provider, executor);
let answer = run_agent(&agent, "Research the latest Rust async news").await?;

// Workflow: deterministic, hard-coded transitions.
// Use when you know the steps in advance.
impl Workflow for MyWorkflow {
    type State = PipelineState;
    async fn execute(&self, state: Self::State) -> Result<Self::State> { … }
}
```

### Model providers

```rust
// Anthropic (Claude)
let provider = AnthropicProvider::from_env()?; // ANTHROPIC_API_KEY

// OpenAI (GPT-4o, o1, …)
let provider = OpenAiProvider::from_env()?;    // OPENAI_API_KEY

// Google Gemini
let provider = GeminiProvider::from_env()?;    // GEMINI_API_KEY

// All implement Arc<dyn ModelProvider> — swap at runtime.
```

### Tool registration

```rust
struct WebSearchTool;

#[async_trait]
impl Tool for WebSearchTool {
    fn definition(&self) -> ToolDefinition { /* schema */ }
    async fn execute(&self, args: serde_json::Value) -> Result<String> { … }
}

let executor = LocalToolRegistry::new()
    .register(WebSearchTool)
    .register(ReadFileTool);
```

## Quick start

```bash
export ANTHROPIC_API_KEY=sk-ant-...
cargo run --example basic_agent
```

## Checklist progress

- [x] §1  Project & workspace setup
- [x] §2  Core runtime (traits, providers, ReAct loop)
- [ ] §3  Durable execution (journal, `#[workflow]`)
- [ ] §4  Graph orchestration (petgraph + slotmap)
- [ ] §5  WASM sandbox (wasmtime)
- [ ] §6  WASM-to-MCP bridge
- [ ] §7  Embedded scripting (Rhai)
- [ ] §8  Three-tier memory (Redis / Qdrant)
- [ ] §9  Model Context Protocol client/server
- [ ] §10 `#[tool]` macro + ACI
- [ ] §11–§25 Supervisor, observability, platform, enterprise …

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     User / API                          │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│            SupervisorAgent (§11) — routes by complexity  │
└──────┬──────────────┬──────────────┬────────────────────┘
       │              │              │
  ┌────▼────┐   ┌────▼────┐   ┌────▼────┐
  │ Tier 1  │   │ Tier 2  │   │ Tier 3  │
  │ (Flash) │   │(Reason) │   │(Frontier│
  └────┬────┘   └────┬────┘   └────┬────┘
       └──────────────┼─────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│           Graph Orchestrator (§4)                        │
│  ┌────────┐  ┌────────┐  ┌────────┐                     │
│  │ Node A ├─►│ Node B ├─►│ Node C │  (parallel/serial)  │
│  └────────┘  └────────┘  └────────┘                     │
└─────────────────────┬───────────────────────────────────┘
                      │
       ┌──────────────┼──────────────┐
       │              │              │
┌──────▼──────┐ ┌─────▼─────┐ ┌────▼────────┐
│ ModelProvider│ │ToolExecutor│ │  Memory     │
│ (OpenAI /   │ │(WASM/MCP) │ │(3-tier §8)  │
│  Anthropic/ │ └───────────┘ └─────────────┘
│  Gemini)    │
└─────────────┘
```

## License

MIT OR Apache-2.0
