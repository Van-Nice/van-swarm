# Rust Agent Framework — Implementation Overview

This document and the others in `documentation/architecture/` describe **what is currently implemented** in the codebase, using Mermaid diagrams and short prose.

## Workspace and crates

The workspace has six member crates. Dependencies between them look like this:

```mermaid
flowchart LR
    subgraph workspace["rust-agent-framework"]
        core["rustmastra-core"]
        orchestrator["rustmastra-orchestrator"]
        memory["rustmastra-memory"]
        mcp["rustmastra-mcp"]
        runtime["rustmastra-runtime"]
        macros["rustmastra-macros"]
    end

    orchestrator --> core
    memory --> core
    mcp --> core
    runtime --> core
    macros --> core
```

- **core** — No internal crate dependencies. Defines traits, config, messages, model providers, ReAct loop, and durable execution.
- **orchestrator** — Depends on core. Graph-based workflow engine (petgraph + slotmap).
- **memory** — Depends on core. Three-tier memory trait and Tier 1 (episodic) stub.
- **mcp** — Depends on core. MCP client, server, and tool executor.
- **runtime** — Depends on core. WASM sandbox (Wasmtime) for tool isolation.
- **macros** — Depends on core. Procedural macros: `#[tool]`, `#[workflow]`.

## High-level architecture

```mermaid
flowchart TB
    subgraph agents["Agents & tools"]
        ReAct[ReActAgent]
        run_agent[run_agent loop]
        ToolExec[ToolExecutor]
        Tools[Tools / LocalToolRegistry]
    end

    subgraph orchestration["Orchestration"]
        Graph[ExecutionGraph]
        Runner[FlowRunner]
        Task[Task trait]
    end

    subgraph durability["Durability & memory"]
        Durable[DurableContext]
        Journal[JournalBackend]
        Memory[Memory / EpisodicMemory]
    end

    subgraph integration["Integration"]
        McpClient[McpClient]
        McpServer[McpServer]
        Sandbox[WASM Sandbox]
    end

    ReAct --> run_agent
    run_agent --> ToolExec
    ToolExec --> Tools
    ReAct --> ModelProvider[ModelProvider]

    Runner --> Graph
    Graph --> Task
    Task --> Durable

    Durable --> Journal
    Durable --> Memory

    McpClient --> McpServer
    ToolExec -.-> McpClient
    McpServer --> ToolExec
    Sandbox --> ToolExec
```

## Document index

| File | Contents |
|------|----------|
| [00-overview.md](00-overview.md) | Workspace, crate graph, high-level architecture (this file) |
| [01-core.md](01-core.md) | Core: traits, config, messages, providers, ReAct, durable execution |
| [02-orchestrator.md](02-orchestrator.md) | Graph builder, ExecutionGraph, FlowRunner, Task, NextAction |
| [03-memory.md](03-memory.md) | Memory trait, EpisodicMemory (Tier 1 stub) |
| [04-mcp.md](04-mcp.md) | McpClient, McpServer, McpToolExecutor, transports |
| [05-runtime.md](05-runtime.md) | WASM sandbox, SandboxConfig, run_json protocol |
| [06-macros.md](06-macros.md) | `#[tool]` and `#[workflow]` macros |
