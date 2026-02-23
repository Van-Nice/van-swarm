# Rust AI Agent Framework — Overview Architecture

This document describes the high-level architecture of the Rust AI Agent Framework: layers, crate structure, and how the framework relates to the managed platform.

## System Layers

The system is split into the **Framework** (local development) and the **Platform** (cloud execution).

```mermaid
flowchart TB
    subgraph Framework["Framework Layer (Local)"]
        core["core-runtime\nTokio + Wasmtime"]
        graphEngine["graph-engine\nDAGs, cycles, state"]
        memory["tier-memory\nEpisodic / Semantic / Procedural"]
        protocol["protocol-bridge\nMCP + Redis Streams"]
    end

    subgraph Platform["Platform Layer (Cloud)"]
        mesh["Isolate Mesh\nHigh-density agent nodes"]
        store["Durable State Store\nBifrost / RocksDB journal"]
        apm["APM Hub\nReasoning traces, cost attribution"]
        cli["CLI Gateway\ndeploy, config"]
    end

    Framework --> Platform
    cli --> mesh
    mesh --> store
    mesh --> apm
```

## Crate Structure (Framework)

Modular crates keep binary size small and compilation fast.

```mermaid
graph LR
    subgraph RustCrates["Rust Crate Structure"]
        core["rustmastra-core\nRunnable, Agent, Workflow traits\nModel providers"]
        orch["rustmastra-orchestrator\nGraphBuilder, FlowRunner\nTask trait, TaskResult"]
        mem["rustmastra-memory\nThree-tier memory API\nQdrant, Redis, pgvector"]
        mcp["rustmastra-mcp\nMCP client/server\nProgrammatic tool calling"]
        rt["rustmastra-runtime\nWasmtime sandbox\nFuel + memory limits"]
        macros["rustmastra-macros\n#\[tool\], #\[workflow\]"]
    end

    core --> orch
    core --> mem
    core --> mcp
    core --> rt
    macros --> core
```

## Workflow vs Agent (Type-System Separation)

Workflows are deterministic; agents are probabilistic. The framework enforces this in the type system.

```mermaid
flowchart LR
    subgraph Workflow["Workflow (Deterministic)"]
        W1[Fixed code paths]
        W2[Predictable steps]
        W3[Evaluator-Optimizer loops]
    end

    subgraph Agent["Agent (Probabilistic)"]
        A1[ReAct loop]
        A2[Model chooses tools]
        A3[Open-ended reasoning]
    end

    Input[User Input] --> Workflow
    Input --> Agent
    Workflow --> Output[Structured Output]
    Agent --> Output
```

## Performance Targets

| Metric | Target | Rationale |
|--------|--------|-----------|
| Cold start | < 10 ms | Enable thousands of isolates per host |
| Memory per agent | < 5 MB | High density (e.g. 1,500+ per 8GB VPS) |
| Tool isolation | WASM (Wasmtime) | Capability-gated, no host access |
| State persistence | Durable execution (Replay) | Resume after crash/migration |

## References

- *Rust Agent Framework Technical Specification* — Durable execution, graph orchestration, WASM–MCP, memory, convergence.
- *AI Agent Framework Strategy & PRD* — Architecture diagram, crate layout, platform vs framework.
- *Rust AI Framework Product Strategy* — RustMastra crate design, Workflow vs Agent, ACI.
