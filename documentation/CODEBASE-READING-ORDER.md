# Codebase reading order

Read the listed `.rs` files **in order** to understand the framework from foundations to application. Each section builds on the previous one.

**Convention:** paths are relative to the repo root. Skip test/example files on a first pass; they are listed at the end as optional.

---

## Part 1 — Core foundations (rustmastra-core)

Start here: shared types and the contract everything else uses.

| Order | File | Why read here |
|-------|------|----------------|
| 1 | [error.rs](../crates/core/src/error.rs) | `FrameworkError` and `Result<T>`; no crate deps. |
| 2 | [config.rs](../crates/core/src/config.rs) | `ModelConfig`, `AgentConfig`, `ProviderCredentials`. |
| 3 | [message.rs](../crates/core/src/message.rs) | `Message`, `ContentBlock`, `CompletionRequest`/`CompletionResponse`, `ToolDefinition`, streaming, `extract_xml_blocks`. |

---

## Part 2 — Core traits

The type-system contract: runnable, agent, tool, workflow.

| Order | File | Why read here |
|-------|------|----------------|
| 4 | [traits/runnable.rs](../crates/core/src/traits/runnable.rs) | Base `Runnable` trait. |
| 5 | [traits/agent.rs](../crates/core/src/traits/agent.rs) | `Agent`, `AgentContext`, `AgentAction`, `RunMetrics`, `ToolCall`. |
| 6 | [traits/tool.rs](../crates/core/src/traits/tool.rs) | `Tool`, `ToolExecutor`, `LocalToolRegistry`, `FilteredToolExecutor`. |
| 7 | [traits/workflow.rs](../crates/core/src/traits/workflow.rs) | `Workflow`, `WorkflowStatus`, `WorkflowStep`. |
| 8 | [traits/mod.rs](../crates/core/src/traits/mod.rs) | Re-exports; confirms trait surface. |

---

## Part 3 — Model providers

How the core talks to LLMs.

| Order | File | Why read here |
|-------|------|----------------|
| 9 | [providers/mod.rs](../crates/core/src/providers/mod.rs) | `ModelProvider` trait and provider wiring. |
| 10 | [providers/openai.rs](../crates/core/src/providers/openai.rs) | OpenAI completion and tool calls. |
| 11 | [providers/anthropic.rs](../crates/core/src/providers/anthropic.rs) | Anthropic Messages API and tool calls. |
| 12 | [providers/gemini.rs](../crates/core/src/providers/gemini.rs) | Gemini API and tool calls. |

---

## Part 4 — Tools (built-in)

Concrete tools and tool module layout.

| Order | File | Why read here |
|-------|------|----------------|
| 13 | [tools/builtin.rs](../crates/core/src/tools/builtin.rs) | `TimeTool`, `ReadFileTool`, `SearchTool`. |
| 14 | [tools/mod.rs](../crates/core/src/tools/mod.rs) | Tools re-exports. |

---

## Part 5 — Durable execution

Log-centric replay and journal.

| Order | File | Why read here |
|-------|------|----------------|
| 15 | [durable/mod.rs](../crates/core/src/durable/mod.rs) | `JournalBackend`, `JournalEntry`, `JournalKind`, `InMemoryJournal`, `FileJournal`, `DurableContext` (call_tool, sleep, timestamp, run_once, resume). |

---

## Part 6 — Evaluators, supervisor, guardrails, patterns, telemetry

Scoring, routing, safety, and observability.

| Order | File | Why read here |
|-------|------|----------------|
| 16 | [evaluators.rs](../crates/core/src/evaluators.rs) | `Scorer`, `ScoreInput`/`ScoreResult`, `batch_score`, SPL, built-in scorers, golden dataset. |
| 17 | [supervisor.rs](../crates/core/src/supervisor.rs) | `Router`, `Route`, `AlwaysTier1`, `KeywordRouter`, `LlmRouter`, TQGR. |
| 18 | [guardrails.rs](../crates/core/src/guardrails.rs) | `GuardRail`, `GuardedModelProvider`, keyword and prompt-injection guardrails. |
| 19 | [patterns.rs](../crates/core/src/patterns.rs) | `EvaluatorOptimizerLoop`, `PlanAndExecute`, voting/consensus helpers. |
| 20 | [telemetry.rs](../crates/core/src/telemetry.rs) | `RunTrace`, `RunTraceBuilder`, `TraceStore`, pricing, sampling. |

---

## Part 7 — ReAct loop (heart of the agent)

Where the agent loop and entrypoints live.

| Order | File | Why read here |
|-------|------|----------------|
| 21 | [react/mod.rs](../crates/core/src/react/mod.rs) | `ReActAgent`, `run_agent`, `run_agent_with_metrics`, `run_agent_traced`; full Thought → Action → Observation loop. |

---

## Part 8 — Core crate surface

Public API and module map.

| Order | File | Why read here |
|-------|------|----------------|
| 22 | [core/lib.rs](../crates/core/src/lib.rs) | All core re-exports and module list; entry point for the crate. |

---

## Part 9 — Orchestrator (graph engine)

Graph-based workflows and runner.

| Order | File | Why read here |
|-------|------|----------------|
| 23 | [orchestrator/lib.rs](../crates/orchestrator/src/lib.rs) | `NodeKey`, `NextAction`, `Task` trait; crate overview. |
| 24 | [orchestrator/task.rs](../crates/orchestrator/src/task.rs) | `ErasedTask`, `TaskAdapter` (type erasure for graph nodes). |
| 25 | [orchestrator/graph.rs](../crates/orchestrator/src/graph.rs) | `GraphBuilder`, `ExecutionGraph`, `EdgeKind`, `Predicate`. |
| 26 | [orchestrator/runner.rs](../crates/orchestrator/src/runner.rs) | `FlowRunner`, `RunResult`, `RunStatus`, `GraphCheckpoint`, ready queue, parallel execution. |
| 27 | [orchestrator/patterns.rs](../crates/orchestrator/src/patterns.rs) | `majority_vote`, `similarity_vote` (optional). |

---

## Part 10 — Memory (three-tier)

Episodic, mid-term, semantic memory and conflict resolution.

| Order | File | Why read here |
|-------|------|----------------|
| 28 | [memory/lib.rs](../crates/memory/src/lib.rs) | `Memory` trait, `MemoryEntry`, `EpisodicMemory`, `MidTermMemory`, `SemanticMemory`, `MemoryManager`, `cosine_similarity`; single-file crate. |

---

## Part 11 — MCP (Model Context Protocol)

Client, server, and executor that bridges to the agent.

| Order | File | Why read here |
|-------|------|----------------|
| 29 | [mcp/protocol.rs](../crates/mcp/src/protocol.rs) | MCP types: `InitializeResult`, `McpTool`, `CallToolResult`, resources, prompts, etc. |
| 30 | [mcp/jsonrpc.rs](../crates/mcp/src/jsonrpc.rs) | JSON-RPC parsing and message types. |
| 31 | [mcp/transport.rs](../crates/mcp/src/transport.rs) | `Transport` trait, `StdioTransport`, `HttpTransport`, `ChannelTransport`. |
| 32 | [mcp/client.rs](../crates/mcp/src/client.rs) | `McpClient`: connect, initialize, list_tools, call_tool, resources, prompts. |
| 33 | [mcp/executor.rs](../crates/mcp/src/executor.rs) | `McpToolExecutor`: implements `ToolExecutor` over `McpClient`. |
| 34 | [mcp/server.rs](../crates/mcp/src/server.rs) | `McpServer`: expose a `ToolExecutor` as an MCP server (stdio/channel). |
| 35 | [mcp/lib.rs](../crates/mcp/src/lib.rs) | MCP re-exports and crate overview. |

---

## Part 12 — Runtime (WASM sandbox and scripting)

Wasmtime sandbox and optional Rhai.

| Order | File | Why read here |
|-------|------|----------------|
| 36 | [runtime/lib.rs](../crates/runtime/src/lib.rs) | `SandboxConfig`, `Sandbox`, `CompiledModule`, `run_json` convention, optional MCP bridge. |
| 37 | [runtime/scripting.rs](../crates/runtime/src/scripting.rs) | `RhaiEngine`, `ScriptConfig`, `ScriptContext` (optional scripting feature). |

---

## Part 13 — Macros

Procedural macros for tools and workflows.

| Order | File | Why read here |
|-------|------|----------------|
| 38 | [macros/lib.rs](../crates/macros/src/lib.rs) | `#[tool]` (schema, examples) and `#[workflow]` (signature validation). |

---

## Part 14 — MCP server binary

Production stdio MCP server that exposes the framework.

| Order | File | Why read here |
|-------|------|----------------|
| 39 | [mcp-server/server.rs](../crates/mcp-server/src/server.rs) | Tool handlers: `rustmastra_run_agent`, `rustmastra_memory_*`, `rustmastra_framework_info`. |
| 40 | [mcp-server/main.rs](../crates/mcp-server/src/main.rs) | Provider detection, stdio transport, server startup. |

---

## Optional — Tests and examples

After the main order, these show usage and invariants:

| File | Purpose |
|------|---------|
| [core/examples/basic_agent.rs](../crates/core/examples/basic_agent.rs) | Minimal run_agent example. |
| [core/tests/integration.rs](../crates/core/tests/integration.rs) | Full ReAct loop with mock and tools. |
| [core/tests/journal_properties.rs](../crates/core/tests/journal_properties.rs) | Journal replay / determinism. |
| [core/tests/supervisor_spl_grading.rs](../crates/core/tests/supervisor_spl_grading.rs) | Supervisor and SPL. |
| [memory/tests/tier_integration.rs](../crates/memory/tests/tier_integration.rs) | Tier 1 → 2 → 3 flow. |
| [mcp/examples/rust_mcp_client.rs](../crates/mcp/examples/rust_mcp_client.rs) | MCP client against rust-mcp. |
| [runtime/benches/cold_start.rs](../crates/runtime/benches/cold_start.rs) | WASM cold start benchmark. |

---

## Summary (40 files in order)

1. **Core (1–22):** error → config → message → traits (runnable, agent, tool, workflow) → providers → tools → durable → evaluators → supervisor → guardrails → patterns → telemetry → react → lib.
2. **Orchestrator (23–27):** lib → task → graph → runner → patterns.
3. **Memory (28):** lib (single module).
4. **MCP (29–35):** protocol → jsonrpc → transport → client → executor → server → lib.
5. **Runtime (36–37):** lib → scripting.
6. **Macros (38):** lib.
7. **MCP server (39–40):** server → main.

Following this order gives you a dependency-respecting path from base types to the main agent loop, then to orchestration, memory, MCP, runtime, macros, and the packaged MCP server.
