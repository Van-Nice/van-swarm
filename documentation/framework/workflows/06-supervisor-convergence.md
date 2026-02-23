# Supervisor Agent and Convergence Metrics

The **SupervisorAgent** routes work and decides when a workflow is done. **SPL** and **TQGR** quantify trajectory efficiency and convergence so the system can stop or backtrack when the agent is stuck.

## Supervisor Role

The supervisor classifies incoming work and routes it to the right tier; it also decides when to terminate or escalate.

```mermaid
flowchart TB
    Query[Incoming query] --> Classify[Classify complexity]
    Classify --> T1[Tier 1: Fast/cheap model\nSimple tasks]
    Classify --> T2[Tier 2: Mid-tier\nPlanning, tools]
    Classify --> T3[Tier 3: Frontier\nHard reasoning]
    T1 --> Execute[Execute]
    T2 --> Execute
    T3 --> Execute
    Execute --> Converge{Converged?}
    Converge -->|Yes| End[Return final answer]
    Converge -->|No| Patience{Patience exhausted?}
    Patience -->|Yes| End
    Patience -->|No| Execute
```

## Success weighted by Path Length (SPL)

SPL measures how close the agent was to the **shortest** successful path.

```mermaid
flowchart LR
    subgraph SPL["SPL = (1/N) Σ S_i × (L_opt / max(L_exec, L_opt))"]
        N[N = episodes]
        S[S_i = 1 if success, 0 else]
        Lopt[L_opt = ideal path length]
        Lexec[L_exec = actual path length]
    end
```

- **Ideal path length** L_opt: minimum steps needed for a correct solution.
- **Executed path length** L_exec: steps the agent actually took.
- SPL is 1 when the agent succeeds in the minimal number of steps; detours or failure reduce it.

## Trajectory-Quality Growth Rate (TQGR)

TQGR relates **improvement in solution quality** to **cost (steps)**. If TQGR stays near or below zero for several steps, the agent is no longer improving (converged or stuck).

```mermaid
flowchart TB
    Steps[Each step] --> Quality[Solution quality]
    Steps --> Cost[Computational cost]
    Quality --> TQGR[TQGR = Δ quality / Δ cost]
    TQGR --> Low{TQGR < ε for 2–3 turns?}
    Low -->|Yes| Act[Terminate or backtrack]
    Low -->|No| Steps
```

## Patience and Termination

The supervisor uses TQGR to implement a **patience** parameter: if more steps don’t improve (or worsen) the answer, it forces a final answer or failure.

```mermaid
stateDiagram-v2
    [*] --> Running
    Running --> Check: After each step
    Check --> Running: TQGR > ε
    Check --> ForceEnd: TQGR ≤ ε for 2–3 turns
    ForceEnd --> [*]
```

## Model Tiering (FinOps)

Routing by complexity keeps cost and latency in check.

```mermaid
flowchart LR
    subgraph Tier1["Tier 1: Low cost"]
        Flash[Gemini Flash / Flash-Lite]
        Use1[Intent, formatting, summarization]
    end

    subgraph Tier2["Tier 2: Reasoning"]
        Mid[Gemini 2.5 Flash]
        Use2[Planning, tool use]
    end

    subgraph Tier3["Tier 3: Frontier"]
        Pro[Gemini 2.5 Pro / O1-Pro]
        Use3[Research, complex coding]
    end

    Input[Query] --> Tier1
    Input --> Tier2
    Input --> Tier3
```

## Convergence Score in APM

The platform tracks **convergence score** (e.g. ratio of optimal to actual path length) so you can tune routing and prompts.

```mermaid
flowchart TB
    Runs[Workflow runs] --> Metric[Convergence score]
    Metric --> Dashboard[APM dashboard]
    Dashboard --> Tune[Refine router + prompts]
    Tune --> Runs
```

## References

- Technical Specification: SPL, TQGR, SupervisorAgent, patience, trajectory efficiency.
- Product Strategy: SupervisorAgent, model tiering, FinOps, convergence score.
