# Workflow vs Agent — when to use which (§15.9)

Two fundamental execution models exist in RustMastra: **Workflows** (deterministic) and **Agents** (probabilistic). Choosing the right one for a task is one of the most impactful architectural decisions you will make.

---

## 1. The key distinction

| Property | Workflow (`Task` + `FlowRunner`) | Agent (`ReActAgent`) |
|---|---|---|
| **Execution path** | Fixed, coded in the graph | Chosen at runtime by the model |
| **Determinism** | Yes — given the same inputs the same code runs | No — model may pick different tools or stop earlier |
| **Debuggability** | High — you can inspect every edge and state transition | Low — you must inspect the full trace to understand decisions |
| **Cost** | Lower — one LLM call per node that needs it | Higher — multiple rounds of inference |
| **Flexibility** | Lower — you must anticipate all branches | Higher — model adapts to novel inputs |
| **Auditability** | Excellent — every step is a journal entry | Good — full transcript stored, but reasoning is implicit |
| **Durable replay** | Full support via `DurableContext` | Supported but replay only replays tool calls, not model choices |

---

## 2. Use a Workflow when

- **The steps are known in advance.** If you can draw a flowchart before writing any code, use a workflow.
- **Compliance or auditability matters.** Every decision must be traceable to a line of code, not a model inference.
- **The input domain is narrow and well-defined.** Processing invoices, transforming data, extracting structured fields from a fixed schema.
- **Parallelism is explicit.** You know which steps can run concurrently and want to control them.
- **You need human-in-the-loop at specific checkpoints.** Use `NextAction::WaitForInput` at known pause points.
- **Durable execution matters.** Long-running jobs that must survive restarts; replay the journal to recover.
- **Cost is a hard constraint.** Each node can call an LLM exactly once (or not at all).

```
┌────────┐    ┌──────────┐    ┌──────────┐    ┌───────┐
│ ingest │ →  │ validate │ →  │ enrich   │ →  │ store │
└────────┘    └──────────┘    └──────────┘    └───────┘
                    ↓ (invalid)
               ┌──────────┐
               │  notify  │
               └──────────┘
```

Workflows excel at pipelines like this — every arrow is an `edge()` or `conditional_edge()` in `GraphBuilder`.

---

## 3. Use an Agent when

- **The steps are not known in advance.** The model must decide which tool to call next based on what it observes.
- **Open-ended tasks.** Research, code generation with unknown sub-tasks, natural language Q&A that requires multiple retrieval steps.
- **Tool usage is exploratory.** The agent may call a search tool, look at the result, then decide to call a code execution tool.
- **Requirements change at runtime.** The user's request may have ambiguities the model resolves on the fly.
- **Tolerance for variability.** Occasional sub-optimal paths are acceptable in exchange for flexibility.

```
model ─── calls ──→ search_tool
  ↑                      ↓
  └─── observes ─── result
  ↓
model ─── calls ──→ read_file_tool
  ↑                      ↓
  └─── observes ─── result
  ↓
model ──→ final answer
```

The path length is not fixed. The agent decides to stop when it has enough information.

---

## 4. Combining both

The most powerful patterns combine Workflows for the outer structure with Agents at individual nodes:

```
Workflow graph:
  ┌───────────┐    ┌────────────────────────┐    ┌───────────┐
  │   ingest  │ →  │ ReActAgent (enrich)     │ →  │   store   │
  └───────────┘    │  - search              │    └───────────┘
                   │  - read_knowledge_base │
                   │  - generate_summary    │
                   └────────────────────────┘
```

The outer Workflow guarantees the sequence (ingest → enrich → store) and handles durability. The inner Agent handles the open-ended enrichment step using whatever tools it needs.

RustMastra's `Task` trait makes this pattern natural — any node can internally call `run_agent`:

```rust
struct EnrichNode {
    agent: ReActAgent,
}

#[async_trait]
impl Task for EnrichNode {
    type State = PipelineState;

    async fn run(&self, _key: NodeKey, mut state: PipelineState)
        -> Result<(PipelineState, NextAction)>
    {
        let answer = run_agent(&self.agent, &state.raw_text).await?;
        state.enriched = answer;
        Ok((state, NextAction::Continue))
    }
}
```

---

## 5. Decision checklist

```
Can you enumerate all required steps before running?
  ├─ YES → Workflow
  └─ NO  → Agent

Do compliance or audit requirements demand exact step traceability?
  ├─ YES → Workflow (or Workflow with Agent nodes)
  └─ NO  → Either

Is the input domain well-defined and narrow?
  ├─ YES → Workflow
  └─ NO  → Agent

Does cost matter a lot (e.g. high-volume API calls)?
  ├─ YES → Workflow (deterministic = predictable cost)
  └─ NO  → Agent acceptable

Is there a known, fixed optimal path length?
  ├─ YES → Workflow
  └─ NO  → Agent (let the model decide)
```

---

## 6. SPL note for agentic evaluation

When evaluating agents with SPL (§11.6), the **optimal path length** (`L_opt`) serves as a target. A Workflow always achieves `L_exec == L_opt` by construction. An Agent's SPL depends on how efficiently the model reaches the answer. If you find SPL consistently below 0.7, consider converting the agentic step to a Workflow node.

---

## Summary

| Use Workflow when... | Use Agent when... |
|---|---|
| Steps are predetermined | Steps are unknown upfront |
| Compliance/audit is critical | Flexibility is more important |
| Cost is a hard constraint | Open-ended exploration is needed |
| Durable replay required | Novel inputs expected |
| Parallelism is explicit | Tool order depends on observations |
