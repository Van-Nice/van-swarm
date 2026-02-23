# Rust AI Agent Framework — Workflow Documentation

This directory contains markdown files that use **Mermaid diagrams** to explain how the Rust AI Agent Framework works. Each file focuses on a specific subsystem or flow.

## Documents

| File | Topic |
|------|--------|
| [01-overview-architecture.md](01-overview-architecture.md) | System layers, crate structure, Framework vs Platform, Workflow vs Agent |
| [02-durable-execution.md](02-durable-execution.md) | Replay vs Snapshot, log-centric durable RPC, journal, recovery, `#[workflow]` |
| [03-graph-orchestration.md](03-graph-orchestration.md) | Arena allocation, petgraph/slotmap, ready queue, state flow, Task trait |
| [04-wasm-mcp-bridge.md](04-wasm-mcp-bridge.md) | WASM sandbox, capability tunneling, Wasmtime Linker, WASI-Virt, wasmcp |
| [05-three-tier-memory.md](05-three-tier-memory.md) | Episodic → Mid-term → Semantic, heat-based consolidation, Memory-R1 |
| [06-supervisor-convergence.md](06-supervisor-convergence.md) | SupervisorAgent, SPL, TQGR, patience, model tiering, convergence score |
| [07-end-to-end-flow.md](07-end-to-end-flow.md) | Full request path, sequence diagram, failure/resume, code mode |
| [08-aci-and-code-mode.md](08-aci-and-code-mode.md) | ACI, Poka-yoke, `#[tool]`, programmatic tool calling, Rhai, gas metering |

## Source Documentation

These workflows are derived from:

- **documentation/framework/Rust Agent Framework Technical Specification.md** — Durable execution, graph orchestration, WASM–MCP, memory, convergence (SPL, TQGR).
- **documentation/framework/AI Agent Framework Strategy & PRD.md** — Architecture, crate layout, platform, MCP, Redis Streams, APM.
- **documentation/framework/Rust AI Framework Product Strategy.md** — RustMastra crates, Workflow vs Agent, ACI, MCP, swarm orchestration, three-tier memory, WASM.
- **documentation/framework/Anthropic Agent Building Bible.md** — Workflows vs agents, ACI, MCP, programmatic tool calling.
- **documentation/framework/Building and Managing Agent Swarms.md** — Scaling, protocols (MCP, A2A), Redis, FinOps.

## Viewing Mermaid

Mermaid diagrams render in:

- GitHub / GitLab
- VS Code (with a Mermaid extension)
- Cursor
- Any Markdown viewer that supports Mermaid
