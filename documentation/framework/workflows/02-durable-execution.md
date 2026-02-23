# Durable Execution Workflow

The framework uses **log-centric durable execution** (Replay approach) so workflows can pause, resume, and recover after crashes without losing progress.

## Replay vs Snapshot

Two main strategies for persisting agent execution:

```mermaid
flowchart LR
    subgraph Replay["Replay (Temporal/Restate)"]
        R1[Event-sourced history]
        R2[Re-run from start]
        R3[Inject recorded results at side-effect points]
    end

    subgraph Snapshot["Snapshot (Golem/Flawless)"]
        S1[Serialize WASM memory + stack]
        S2[Freeze at instruction pointer]
        S3[Load blob to resume]
    end

    Choice[Persistence strategy] --> Replay
    Choice --> Snapshot
```

The framework **recommends Replay** for low storage overhead and good recovery behavior at scale.

## Log-Centric Durable RPC Flow

Execution is driven by a **Context** that intercepts non-deterministic operations and consults a durable journal.

```mermaid
sequenceDiagram
    participant Handler as Agent Handler
    participant Context as Durable Context
    participant Journal as Durable Log (Bifrost/RocksDB)
    participant Tool as External Tool

    Handler->>Context: ctx.call_tool("fetch_data")
    Context->>Journal: Check journal for this call
    alt Result already in journal (replay)
        Journal-->>Context: Return recorded result
        Context-->>Handler: Cached result (no re-execution)
    else First execution
        Context->>Tool: Execute tool
        Tool-->>Context: Result
        Context->>Journal: Append result atomically
        Context-->>Handler: Result
    end
```

## Deterministic Wrapping

All non-deterministic operations go through the context so replay stays deterministic.

```mermaid
flowchart TB
    subgraph Deterministic["Deterministic (no journal)"]
        D1[Pure computation]
        D2[Same inputs → same outputs]
    end

    subgraph NonDeterministic["Wrapped via Context"]
        N1["ctx.sleep()"]
        N2["ctx.call_tool()"]
        N3["ctx.timestamp()"]
    end

    Code[Async workflow code] --> Deterministic
    Code --> NonDeterministic
    NonDeterministic --> Journal[(Durable Journal)]
```

## Procedural Macro: #[workflow]

A `#[workflow]` macro injects yield points at every `.await`, so progress is persisted with each async step.

```mermaid
flowchart LR
    subgraph Source["Developer writes"]
        A["async fn my_workflow(ctx) {\n  ctx.call_tool(a).await;\n  ctx.call_tool(b).await;\n}"]
    end

    subgraph Transformed["Macro expands"]
        B["State machine with yield points"]
        C["Each .await → journal checkpoint"]
    end

    A --> B
    B --> C
```

## Recovery After Crash

Resume is done by re-running from the beginning and replaying the journal.

```mermaid
flowchart TB
    Start([Workflow starts / restarts])
    Start --> Run[Run from beginning]
    Run --> Step[Reach next .await / side-effect]
    Step --> Check{Result in journal?}
    Check -->|Yes| Inject[Inject recorded result]
    Check -->|No| Execute[Execute and append to journal]
    Inject --> Step
    Execute --> Step
    Step --> Done([Continue until end])
```

## References

- Technical Specification: Replay vs Snapshot, Durable RPC, Bifrost, deterministic wrapping, `#[workflow]`.
- PRD: Managed state persistence, Restate-inspired engine.
