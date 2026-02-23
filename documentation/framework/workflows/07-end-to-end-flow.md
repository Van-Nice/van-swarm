# End-to-End Request Flow

This document ties together the main components: from an incoming request through orchestration, tools, memory, and durable execution to a final response.

## High-Level Request Path

```mermaid
flowchart TB
    Request[Incoming request] --> Supervisor[SupervisorAgent]
    Supervisor --> Route[Route by complexity]
    Route --> Graph[Graph orchestrator]
    Graph --> Nodes[Agent nodes]
    Nodes --> LLM[LLM calls]
    Nodes --> Tools[Tool calls via MCP]
    Tools --> WASM[WASM sandbox]
    WASM --> MCP[MCP bridge → host]
    Nodes --> Memory[Three-tier memory]
    Graph --> Durable[Durable journal]
    Nodes --> Response[Final response]
```

## Detailed Sequence: One Agent Turn

```mermaid
sequenceDiagram
    participant User
    participant Supervisor
    participant Orchestrator
    participant Node
    participant Context
    participant Journal
    participant WASM
    participant MCP
    participant Memory
    participant LLM

    User->>Supervisor: Query
    Supervisor->>Orchestrator: Route + start graph
    Orchestrator->>Journal: Load / init state
    loop For each ready node
        Orchestrator->>Node: Run with Context
        Node->>Context: ctx.call_tool(...)
        Context->>Journal: Check journal
        alt Replay
            Journal-->>Context: Cached result
        else Execute
            Context->>WASM: Invoke in sandbox
            WASM->>MCP: Tool call (proxied)
            MCP-->>WASM: Result
            WASM-->>Context: Result
            Context->>Journal: Append result
        end
        Context-->>Node: Tool result
        Node->>Memory: Read/write episodic/semantic
        Node->>LLM: Reason / next step
        LLM-->>Node: Decision
        Node->>Orchestrator: TaskResult (Continue/End/...)
        Orchestrator->>Journal: Checkpoint state
    end
    Orchestrator->>Supervisor: Done or converged
    Supervisor->>User: Response
```

## Component Interaction Map

```mermaid
flowchart TB
    subgraph Entry["Entry"]
        Req[Request]
        Sup[SupervisorAgent]
    end

    subgraph Execution["Execution"]
        Orch[Graph orchestrator]
        Ctx[Durable context]
        Journal[(Journal)]
    end

    subgraph Tools["Tools & sandbox"]
        WASM[Wasmtime]
        MCP[MCP bridge]
    end

    subgraph State["State & memory"]
        Mem[Tier memory]
        GraphState[Graph state]
    end

    Req --> Sup
    Sup --> Orch
    Orch --> Ctx
    Ctx --> Journal
    Orch --> WASM
    WASM --> MCP
    Orch --> Mem
    Orch --> GraphState
```

## Failure and Resume

If the process dies mid-run, the next run replays from the journal and continues.

```mermaid
flowchart LR
    A[Request] --> B[Run workflow]
    B --> C[Steps 1..k complete]
    C --> Crash[Process crash]
    Crash --> Restart[Restart]
    Restart --> Replay[Replay from start]
    Replay --> Inject[Inject steps 1..k from journal]
    Inject --> D[Execute step k+1...]
    D --> E[Response]
```

## Code Mode (Programmatic Tool Calling)

For heavy tool use, the agent can emit a **script** (e.g. Rhai) that runs in the sandbox and calls MCP tools internally; only the final result is returned to the LLM. This reduces tokens and keeps large data out of context.

```mermaid
flowchart TB
    LLM[LLM] --> Decide{Complex task?}
    Decide -->|Yes| Script[Generate script]
    Decide -->|No| Single[Single tool call]
    Script --> Sandbox[Rhai in WASM sandbox]
    Sandbox --> Multi[Many MCP calls inside]
    Multi --> Result[Single result to LLM]
    Single --> Result
```

## References

- All prior workflow docs: durable execution, graph orchestration, WASM–MCP, memory, supervisor.
- Technical Specification & PRD: end-to-end architecture, crate roles, platform vs framework.
