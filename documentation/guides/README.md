# RustMastra Framework — User Guides

This directory contains **how-to guides and examples** for using the RustMastra framework to build agents, workflows, and tool-integrated applications.

---

## Is the framework ready for others to use?

**Yes, with clear expectations.**

| Area | Status | Notes |
|------|--------|--------|
| **Core agent (ReAct)** | ✅ Ready | `ReActAgent`, `run_agent`, `run_agent_with_metrics`, three providers (OpenAI, Anthropic, Gemini), streaming. |
| **Tools** | ✅ Ready | `Tool` / `ToolExecutor`, `LocalToolRegistry`, built-ins (time, read_file, search), `#[tool]` macro with schema + examples. |
| **MCP** | ✅ Ready | `McpClient` (stdio/HTTP/channel), `McpToolExecutor` (bridge to agent), `McpServer` (expose your tools). |
| **Durable workflows** | ✅ Ready | `DurableContext`, `JournalBackend` (in-memory + file NDJSON), `#[workflow]` validation. |
| **Orchestrator** | ✅ Ready | `GraphBuilder`, `ExecutionGraph`, `FlowRunner`, `Task`, `NextAction` (including `WaitForInput`). |
| **Memory** | ✅ Ready | `Memory` trait, `EpisodicMemory`, `MidTermMemory`, `SemanticMemory`, `MemoryManager`. |
| **Evaluators** | ✅ Ready | `Scorer`, `batch_score`, SPL, `RunMetrics`, built-in scorers (NonEmpty, Contains, LLM judge, etc.). |
| **Runtime (WASM)** | ✅ Ready | Wasmtime sandbox, `run_json` convention, optional MCP bridge. |
| **Supervisor / Router** | ✅ Ready | `Router` trait, `Route` (Tier1/2/3), `AlwaysTier1`, `KeywordRouter`, `LlmRouter`. |

**What to tell users:**

- **You can build production-style agents today:** ReAct loop, tools (local or MCP), multiple providers, durable workflows, graph orchestration, memory, and evaluation are implemented and documented.
- **Publish as a library:** Add your crate to [crates.io](https://crates.io) (or use path/ git deps). Depend on `rustmastra-core`, `rustmastra-mcp`, etc. as needed. See [01-quick-start](01-quick-start.md) for a minimal `Cargo.toml`.
- **Caveats:** Some checklist items (e.g. wasmtime-wasi-http, Redis/Qdrant backends) are optional or future work. The framework is in **active development**; semver will apply once you tag a stable release (e.g. 0.1.x for early adopters).

---

## Guide index

| # | Guide | Contents |
|---|-------|----------|
| 01 | [Quick start](01-quick-start.md) | Add the framework to your project and run your first agent. |
| 02 | [Building an agent](02-building-an-agent.md) | ReActAgent, config, providers, `run_agent` and `run_agent_with_metrics`. |
| 03 | [Tools](03-tools.md) | Implementing `Tool`, `LocalToolRegistry`, built-ins, `#[tool]` macro. |
| 04 | [MCP (Model Context Protocol)](04-mcp.md) | McpClient, McpToolExecutor, McpServer, transports, examples. |
| 05 | [Durable workflows](05-durable-workflows.md) | DurableContext, JournalBackend, `#[workflow]`, replay. |
| 06 | [Orchestrator](06-orchestrator.md) | GraphBuilder, Task, FlowRunner, NextAction, human-in-the-loop. |
| 07 | [Memory](07-memory.md) | EpisodicMemory, MidTerm, Semantic, MemoryManager. |
| 08 | [Evaluation](08-evaluation.md) | Scorer, SPL, batch_score, RunMetrics. |
| 09 | [Runtime (WASM)](09-runtime-wasm.md) | Sandbox, run_json, optional MCP bridge. |
| 10 | [Examples reference](10-examples-reference.md) | Runnable examples and copy-paste snippets. |
| 11 | [Workflow vs Agent](11-workflow-vs-agent.md) | When to use deterministic workflows vs probabilistic agents. |
| 12 | [Swarm patterns](12-swarm-patterns.md) | Orchestrator-Worker, Hierarchical Swarm, Blackboard, Forest Swarm, consensus. |
| 13 | [Prompting best practices](13-prompting-best-practices.md) | XML tags, prescriptive prompts, effort control, context caching. |

---

## Where to read next

- **High-level overview:** [documentation/HOW-IT-WORKS.md](../HOW-IT-WORKS.md)
- **Architecture (per crate):** [documentation/architecture/](../architecture/README.md)
- **Build checklist:** [documentation/FRAMEWORK-BUILD-CHECKLIST.md](../FRAMEWORK-BUILD-CHECKLIST.md)
