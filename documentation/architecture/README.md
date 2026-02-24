# Architecture documentation (Mermaid diagrams)

This folder contains **implementation documentation** for the Rust Agent Framework: what exists in the codebase today, described with Mermaid diagrams and short prose.

## Index

| Document | Description |
|----------|-------------|
| [00-overview.md](00-overview.md) | Workspace, crate graph, high-level architecture |
| [01-core.md](01-core.md) | Core: traits, config, messages, providers, ReAct, durable execution |
| [02-orchestrator.md](02-orchestrator.md) | Graph builder, ExecutionGraph, FlowRunner, Task, NextAction |
| [03-memory.md](03-memory.md) | Memory trait, EpisodicMemory (Tier 1 stub) |
| [04-mcp.md](04-mcp.md) | McpClient, McpServer, McpToolExecutor, transports |
| [05-runtime.md](05-runtime.md) | WASM sandbox, SandboxConfig, run_json protocol |
| [06-macros.md](06-macros.md) | `#[tool]` and `#[workflow]` macros |

Start with **00-overview.md** for the big picture, then open the per-crate files for details.

## Related docs

- **documentation/FRAMEWORK-BUILD-CHECKLIST.md** — Checklist of planned work and what’s done.
- **documentation/framework/** — Strategy, technical spec, and product docs.
