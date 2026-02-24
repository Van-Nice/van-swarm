# Orchestrator

This guide covers **graph-based workflows**: **GraphBuilder**, **ExecutionGraph**, **FlowRunner**, **Task**, and **NextAction** (including **WaitForInput** for human-in-the-loop).

---

## 1. Overview

The **openswarm-orchestrator** crate runs workflows defined as a **graph** of nodes. Each node implements **Task**: it receives the current **state**, runs, and returns updated state plus a **NextAction** that tells the runner which nodes to run next (or to pause/end).

- **GraphBuilder** — add nodes, edges, conditional edges, set start node, then **build()** → **ExecutionGraph**.
- **ExecutionGraph** — immutable graph (petgraph + slotmap); holds type-erased tasks.
- **FlowRunner** — executes from the start node; maintains a ready queue and runs ready nodes in parallel; merges state; respects **NextAction**.
- **Task** — trait: `State` type, `run(key, state) -> (State, NextAction)`, `name()`.

---

## 2. Defining a Task

Each node has a **State** type (shared across the graph) and must return that state plus a **NextAction**:

```rust
use async_trait::async_trait;
use openswarm_orchestrator::{FlowRunner, GraphBuilder, NextAction, NodeKey, Task};
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
struct PipelineState {
    message: String,
}

struct ExtractNode;

#[async_trait]
impl Task for ExtractNode {
    type State = PipelineState;

    async fn run(&self, _key: NodeKey, mut state: PipelineState)
        -> openswarm_core::Result<(PipelineState, NextAction)>
    {
        state.message = "extracted".to_string();
        Ok((state, NextAction::Continue))
    }

    fn name(&self) -> &str {
        "extract"
    }
}
```

---

## 3. NextAction

| Variant                       | Effect                                                                                                                           |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| **Continue**                  | Follow outgoing edges; successors that pass conditions become ready.                                                             |
| **Parallelize { node_keys }** | Schedule the listed nodes in the next round (explicit fan-out or cycles).                                                        |
| **WaitForInput { prompt }**   | Pause; `FlowRunner::run()` returns `RunStatus::WaitingForInput`. Caller can resume with `FlowRunner::resume(checkpoint, input)`. |
| **End**                       | Do not follow successors; traversal from this node stops.                                                                        |

---

## 4. Building the graph

```rust
use std::sync::Arc;
use openswarm_orchestrator::{ExecutionGraph, GraphBuilder, NextAction, Task};

let mut builder = GraphBuilder::new();
let extract_key = builder.add_node(ExtractNode);
let transform_key = builder.add_node(TransformNode);
let load_key = builder.add_node(LoadNode);

builder.edge(extract_key, transform_key);
builder.edge(transform_key, load_key);
builder.start(extract_key);

let graph: Arc<ExecutionGraph> = Arc::new(builder.build());
```

- **add_node(task)** — add a node; returns **NodeKey**.
- **edge(from, to)** — always follow from → to when from completes with **Continue**.
- **conditional_edge(from, to, predicate)** — follow edge only if `predicate(&state)` is true.
- **start(key)** — set the entry node.
- **build()** — produce the immutable **ExecutionGraph**.

---

## 5. Running the graph

```rust
use openswarm_orchestrator::FlowRunner;

let runner = FlowRunner::new(Arc::clone(&graph));
let initial_state = serde_json::json!({ "message": "" });
let result = runner.run(initial_state).await?;

println!("Final state: {:?}", result.state);
match result.status {
    openswarm_orchestrator::RunStatus::Completed => {}
    openswarm_orchestrator::RunStatus::WaitingForInput { prompt, paused_at } => {
        println!("Waiting for input: {} (paused at {:?})", prompt, paused_at);
        if let Some(checkpoint) = result.checkpoint {
            let user_input = get_input_from_user();
            let resumed = runner.resume(checkpoint, user_input).await?;
            // ...
        }
    }
}
```

**RunResult** contains:

- **state** — accumulated JSON state after the run.
- **status** — **Completed** or **WaitingForInput { prompt, paused_at }**.
- **checkpoint** — optional **GraphCheckpoint** to pass to **resume()** when status is **WaitingForInput**.

---

## 6. Conditional edges

Schedule a successor only when a condition on the current state holds:

```rust
builder.conditional_edge(
    transform_key,
    load_key,
    |state: &serde_json::Value| state.get("skip_load").and_then(|v| v.as_bool()).unwrap_or(false) == false,
);
```

The predicate receives the **merged** state (after the node runs).

---

## 7. Parallel branches

Add multiple edges from one node; when that node returns **Continue**, all successors that pass their conditions are scheduled and run **in parallel** in the next round.

```rust
builder.edge(extract_key, transform_a_key);
builder.edge(extract_key, transform_b_key);
builder.edge(transform_a_key, merge_key);
builder.edge(transform_b_key, merge_key);
```

State from parallel nodes is **merged** (e.g. JSON merge, right-wins) before the next round.

---

## 8. Human-in-the-loop (WaitForInput)

A task can pause and ask for external input:

```rust
Ok((state, NextAction::WaitForInput {
    prompt: "Please approve the draft (yes/no):".to_string(),
}))
```

The runner returns **RunStatus::WaitingForInput** and an optional **GraphCheckpoint**. Your application collects the user’s response, then:

```rust
let resumed = runner.resume(checkpoint, user_response).await?;
```

---

## 9. Full example: linear pipeline with state

```rust
use async_trait::async_trait;
use openswarm_core::Result;
use openswarm_orchestrator::{FlowRunner, GraphBuilder, NextAction, NodeKey, Task};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Default, Serialize, Deserialize)]
struct PipelineState {
    step: String,
}

struct StepA;
struct StepB;

#[async_trait]
impl Task for StepA {
    type State = PipelineState;
    async fn run(&self, _: NodeKey, mut s: PipelineState) -> Result<(PipelineState, NextAction)> {
        s.step = "A".into();
        Ok((s, NextAction::Continue))
    }
    fn name(&self) -> &str { "step_a" }
}

#[async_trait]
impl Task for StepB {
    type State = PipelineState;
    async fn run(&self, _: NodeKey, mut s: PipelineState) -> Result<(PipelineState, NextAction)> {
        s.step = format!("{} -> B", s.step);
        Ok((s, NextAction::End))
    }
    fn name(&self) -> &str { "step_b" }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut b = GraphBuilder::new();
    let a = b.add_node(StepA);
    let b_node = b.add_node(StepB);
    b.edge(a, b_node);
    b.start(a);
    let graph = Arc::new(b.build());
    let result = FlowRunner::new(graph).run(serde_json::json!({})).await?;
    println!("{:?}", result.state);
    Ok(())
}
```

---

## 10. Next steps

- Combine with durable execution (tasks that use **DurableContext**): [05-durable-workflows](05-durable-workflows.md).
- Architecture: [documentation/architecture/02-orchestrator.md](../architecture/02-orchestrator.md).
