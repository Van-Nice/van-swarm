# Graph Orchestration Workflow

The framework uses a **generational arena** (slotmap + petgraph) for graph-based orchestration: DAGs and cycles, parallel execution, and clear ownership in Rust.

## Arena vs Actor Model

Orchestration is centralized in an arena; nodes are referenced by stable indices, not raw pointers.

```mermaid
flowchart TB
    subgraph Arena["Arena Allocation (Recommended)"]
        direction TB
        A1[SlotMap: NodeKey → AgentNode]
        A2[StableGraph: topology]
        A3[Single owner, Send+Sync]
        A4[Ready Queue → parallel spawn]
    end

    subgraph Actor["Actor Model (Alternative)"]
        direction TB
        B1[One tokio::task per node]
        B2[Channels: mpsc, oneshot]
        B3[Decentralized state]
    end

    Choice[Orchestration strategy] --> Arena
    Choice --> Actor
```

## Graph Structure

Nodes are agent steps; edges are conditional transitions. The orchestrator walks the graph and runs ready nodes in parallel.

```mermaid
flowchart LR
    subgraph Graph["Workflow Graph"]
        Start([START])
        N1[Plan]
        N2[Research]
        N3[Write]
        N4[Evaluate]
        End([END])

        Start --> N1
        N1 --> N2
        N1 --> N3
        N2 --> N4
        N3 --> N4
        N4 -->|retry| N3
        N4 --> End
    end

    Orch[Orchestrator] --> Graph
```

## Ready Queue and Parallel Execution

Nodes whose dependencies are satisfied enter a ready queue and can be executed in parallel.

```mermaid
flowchart TB
    subgraph Orchestrator["Orchestrator"]
        RQ[Ready Queue]
        Exec[Tokio / Rayon executor]
    end

    subgraph Graph["Graph State"]
        D1[Node A done]
        D2[Node B done]
        D3[Node C ready]
        D4[Node D ready]
    end

    D1 --> RQ
    D2 --> RQ
    RQ --> D3
    RQ --> D4
    D3 --> Exec
    D4 --> Exec
    Exec --> State[Shared state S += results]
```

## State Flow Through Graph

State is a shared container that accumulates results as the graph runs. Each node reads and updates it.

```mermaid
flowchart LR
    S0["S₀ (initial)"] --> N1[Node 1]
    N1 --> S1["S₁ = S₀ + result₁"]
    S1 --> N2[Node 2]
    N2 --> S2["S₂ = S₁ + result₂"]
    S2 --> N3[Node 3]
    N3 --> S3["S₃ = S₂ + result₃"]
```

Formula: **S_{n+1} = S_n + f(node_n)** — state is persistent across nodes and supports pause/resume.

## Task Trait and NextAction

Each node implements a **Task** trait and returns a **TaskResult** that drives the next step.

```mermaid
stateDiagram-v2
    [*] --> Continue
    [*] --> Parallelize
    [*] --> WaitForInput
    [*] --> End

    Continue: Spawn next node(s)
    Parallelize: Fork multiple branches
    WaitForInput: Human-in-the-loop
    End: Workflow complete
```

## Alignment Principle

Centralized orchestration improves outcomes on parallelizable work (e.g. ~80.9% in studies) and acts as a validation bottleneck (e.g. 4.4x error containment vs 17.2x for independent agents).

```mermaid
flowchart TB
    Query[Incoming query] --> Orch[Orchestrator]
    Orch --> W1[Worker 1]
    Orch --> W2[Worker 2]
    Orch --> W3[Worker 3]
    W1 --> Validate[Validate / aggregate]
    W2 --> Validate
    W3 --> Validate
    Validate --> Response[Final response]
```

## References

- Technical Specification: Arena allocation, petgraph, slotmap, cycles, ready queue.
- Product Strategy: GraphFlow, Task trait, FlowRunner, Orchestrator–Worker pattern.
