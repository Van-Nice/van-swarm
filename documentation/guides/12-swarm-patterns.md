# Swarm & multi-agent patterns (§24)

Multi-agent systems decompose a complex task into parallel or hierarchical sub-tasks, each handled by a specialized agent. OpenSwarm provides the primitives to build every standard swarm pattern.

> **Before using multi-agent:** Be aware of the **Sequential Penalty** (§24.6) — multi-agent coordination has overhead. If the task is strictly sequential, a single well-prompted agent is often cheaper and faster.

---

## 1. Orchestrator-Worker (§24.1)

The simplest and most common pattern. A central **Orchestrator** decomposes a task, dispatches subtasks to **Worker** agents, and aggregates results.

```
                ┌─────────────────┐
  user input ─→ │  Orchestrator   │ ←── dispatches / aggregates
                └────────┬────────┘
               ┌─────────┼─────────┐
               ↓         ↓         ↓
          ┌────────┐ ┌────────┐ ┌────────┐
          │Worker A│ │Worker B│ │Worker C│
          └────────┘ └────────┘ └────────┘
```

**Implementation:** Use `FlowRunner` with `NextAction::Parallel` to fan out to workers. Each worker runs as a `Task` node. The `FlowRunner` merges their JSON state outputs via additive merge.

```rust
// Orchestrator node dispatches to three workers
struct OrchestratorNode;

#[async_trait]
impl Task for OrchestratorNode {
    type State = SwarmState;

    async fn run(&self, _key: NodeKey, state: SwarmState)
        -> Result<(SwarmState, NextAction)>
    {
        // Return the keys of the three workers to run in parallel.
        Ok((state, NextAction::Parallel))
    }
}
```

**When to use:** Research pipelines, report generation, data enrichment where subtasks are independent.

---

## 2. Hierarchical Swarm (§24.2)

A **Director** agent manages a team of **Manager** agents, each of which manages **Worker** agents. The hierarchy allows complex decomposition without overwhelming any single agent's context window.

```
     ┌──────────┐
     │ Director │
     └────┬─────┘
    ┌─────┴──────┐
    ↓            ↓
┌────────┐  ┌────────┐
│Manager1│  │Manager2│
└───┬────┘  └────────┘
  ┌─┴──┐
  ↓    ↓
 W1   W2
```

**Guideline:** Keep each agent's tool count below 16 (§24.5). Hierarchical swarms solve the tool-count problem — each agent sees only the tools relevant to its level of abstraction.

**Implementation:** Model each level as a `ReActAgent` node inside a `Workflow`. The Director node calls `run_agent` with the Director agent; the returned plan is parsed and dispatched to Manager nodes.

---

## 3. Blackboard pattern (§24.3)

Agents read from and write to a **shared knowledge repository** (the "blackboard"). No agent has complete knowledge; each contributes partial results. A **Controller** agent decides which specialist to invoke next based on the current blackboard state.

```
         ┌─────────────────────┐
         │     Blackboard      │
         │  (shared JSON state)│
         └───────┬─────────────┘
          ↑      │       ↑
    write │      │read   │ write
          │      ↓       │
     ┌────┴──┐ ┌──────┐ ┌┴──────┐
     │ Search│ │Parser│ │Summary│
     └───────┘ └──────┘ └───────┘
```

**Implementation in OpenSwarm:** The `FlowRunner`'s shared state (`S`) is the blackboard. Nodes read from `state`, append results, and pass the enriched state forward. `conditional_edge` allows the controller to route based on what's already on the blackboard.

```rust
// Conditional routing: if blackboard has summary, go to store; else go to summarizer
graph.conditional_edge(
    controller,
    move |state: &BlackboardState| {
        if state.summary.is_some() {
            NodePredicate::Target(store_node)
        } else {
            NodePredicate::Target(summarizer_node)
        }
    },
);
```

---

## 4. Forest Swarm (§24.4)

Multiple independent agent trees run in parallel; a **Root Router** selects the best tree for the input, or fans out to all and uses consensus.

```
  user input
      ↓
 ┌──────────┐
 │  Router  │
 └─┬──┬──┬─┘
   ↓  ↓  ↓
  T1 T2  T3   ← three specialised agent trees
   ↓  ↓  ↓
 ┌──────────┐
 │  Voting  │  ← majority or similarity consensus
 └──────────┘
      ↓
   answer
```

**Implementation:** Use `KeywordRouter` or `LlmRouter` for the root routing decision. For consensus, use `majority_vote` or `similarity_vote` from `openswarm_orchestrator::patterns`:

```rust
use openswarm_orchestrator::patterns::{majority_vote, similarity_vote};

// Collect answers from each tree
let answers = vec![answer_t1, answer_t2, answer_t3];

// Pick the most common answer
let best = majority_vote(&answers).unwrap_or(answers[0].clone());

// Or: pick the answer closest to the centroid (requires embeddings)
let best = similarity_vote(&answers).unwrap_or(answers[0].clone());
```

---

## 5. Tool-count trade-off (§24.5)

Research shows that agents with more than **~16 tools** in context suffer degraded accuracy — the model struggles to select the right tool among too many options.

**Mitigations:**

- Use `FilteredToolExecutor` to expose only the tools relevant to a specific node.
- Organise tools into categories; give each agent access to one category.
- Use hierarchical routing: Director selects which agent tree (and its tools) to activate.

```rust
use openswarm_core::tools::FilteredToolExecutor;

// Worker A only sees search and read_file
let executor_a = FilteredToolExecutor::new(
    full_registry.clone(),
    |name| matches!(name, "search" | "read_file"),
);
```

---

## 6. Sequential penalty (§24.6)

Multi-agent coordination has real costs:

- **Latency:** Each agent handoff requires a new LLM call (TTFT × N agents).
- **Token cost:** Each agent re-reads the problem, sometimes duplicating context.
- **Coordination errors:** The orchestrator may misinterpret a worker's output.

**Rule of thumb:** If the task can be solved with a single, well-prompted agent, do not add multi-agent overhead. Reserve multi-agent for tasks where:

- Parallel specialization provides a measurable speedup, or
- Context windows would overflow with a single agent, or
- Different agents need different tool access.

---

## 7. Consensus patterns (§24.7)

For safety-critical decisions, run the same task across multiple agents and require agreement before acting.

OpenSwarm provides three utilities in `openswarm_orchestrator::patterns`:

```rust
/// Exact string match: return the value held by the majority.
pub fn majority_vote<T: Eq + Hash + Clone>(answers: &[T]) -> Option<T>;

/// Majority vote over owned strings.
pub fn majority_vote_owned(answers: &[String]) -> Option<String>;

/// Similarity vote: pick the answer whose embedding is closest to the centroid.
/// Requires pre-computed embeddings for each answer.
pub fn similarity_vote(answers: &[String]) -> Option<String>;
```

**Example: three-way consensus with SPL accounting**

```rust
// Run the same prompt with three models
let (a1, m1) = run_agent_with_metrics(&agent1, prompt).await?;
let (a2, m2) = run_agent_with_metrics(&agent2, prompt).await?;
let (a3, m3) = run_agent_with_metrics(&agent3, prompt).await?;

let answer = majority_vote_owned(&[a1, a2, a3])
    .unwrap_or_else(|| "no consensus".to_string());

// Aggregate SPL: each run contributes one SplRun
let runs = vec![
    SplRun { score: 1.0, path_length: m1.tool_call_count, optimal_path_length: 2 },
    SplRun { score: 1.0, path_length: m2.tool_call_count, optimal_path_length: 2 },
    SplRun { score: 1.0, path_length: m3.tool_call_count, optimal_path_length: 2 },
];
let aggregate = spl(&runs);
```

---

## Summary

| Pattern             | Best for                               | OpenSwarm primitive                   |
| ------------------- | -------------------------------------- | ------------------------------------- |
| Orchestrator-Worker | Decomposable parallel tasks            | `NextAction::Parallel` in FlowRunner  |
| Hierarchical Swarm  | Deep decomposition, many tools         | Nested `ReActAgent` nodes in Workflow |
| Blackboard          | Emergent assembly from partial results | Shared `State` + `conditional_edge`   |
| Forest Swarm        | Multiple specialist trees              | `Router` + `majority_vote`            |
| Consensus           | Safety-critical decisions              | `majority_vote` / `similarity_vote`   |
