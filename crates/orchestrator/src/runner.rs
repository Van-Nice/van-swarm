//! `FlowRunner` — executes an `ExecutionGraph` from start to finish.
//!
//! ## Scheduling algorithm
//!
//! 1. Maintain `pending_preds: HashMap<NodeKey, usize>` — decremented as
//!    predecessors complete.  A node is **ready** when its count reaches 0.
//! 2. At each round, drain the entire ready queue and spawn all ready nodes
//!    **in parallel** via `tokio::task::JoinSet`.
//! 3. When a node finishes:
//!    * Its output state is JSON-merged into the shared accumulated state.
//!    * Its `NextAction` drives subsequent scheduling:
//!      - `Continue`       → decrement each eligible successor's pending count.
//!      - `Parallelize`    → force-queue the specified nodes directly.
//!      - `WaitForInput`   → checkpoint & return `RunStatus::WaitingForInput`.
//!      - `End`            → do not follow successors.
//! 4. Cycles are handled naturally: when a `Continue` edge points to an
//!    already-completed node, it is removed from `completed` and re-queued.
//!    A per-node cycle counter prevents infinite loops.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use tokio::task::JoinSet;
use tracing::{debug, info, instrument, warn};

use crate::{graph::EdgeKind, ExecutionGraph, NextAction, NodeKey};

// ─────────────────────────────────────────────────────────────────────────────
// Public result types
// ─────────────────────────────────────────────────────────────────────────────

/// Why the flow stopped.
#[derive(Debug)]
pub enum RunStatus {
    /// All reachable nodes finished without requesting a pause.
    Completed,
    /// A node returned `NextAction::WaitForInput`.  The caller should collect
    /// external input and call `FlowRunner::resume()`.
    WaitingForInput {
        prompt: String,
        /// The node that issued the wait — useful for resume logic.
        paused_at: NodeKey,
    },
}

/// The result of a completed (or paused) flow run.
#[derive(Debug)]
pub struct RunResult {
    /// Accumulated JSON state at the time the run stopped.
    pub state: serde_json::Value,
    pub status: RunStatus,
}

// ─────────────────────────────────────────────────────────────────────────────
// RunnerConfig
// ─────────────────────────────────────────────────────────────────────────────

/// Tuning knobs for `FlowRunner`.
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    /// Maximum number of times *any single node* may execute within one run.
    /// Guards against accidental infinite loops in cyclic graphs.
    pub max_cycles_per_node: usize,
    /// Hard cap on the total number of node executions across the whole run.
    pub max_total_steps: usize,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            max_cycles_per_node: 32,
            max_total_steps: 4_096,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FlowRunner
// ─────────────────────────────────────────────────────────────────────────────

/// Executes a compiled `ExecutionGraph` from its designated start node.
pub struct FlowRunner {
    graph: Arc<ExecutionGraph>,
    config: RunnerConfig,
}

impl FlowRunner {
    /// Create a runner with default config.
    pub fn new(graph: Arc<ExecutionGraph>) -> Self {
        Self { graph, config: RunnerConfig::default() }
    }

    /// Create a runner with a custom config.
    pub fn with_config(graph: Arc<ExecutionGraph>, config: RunnerConfig) -> Self {
        Self { graph, config }
    }

    /// Execute the graph starting from the designated start node.
    ///
    /// `initial_state` is a `serde_json::Value` — typically `serde_json::json!({})`
    /// or a serialized instance of your concrete state struct.
    #[instrument(skip(self, initial_state), fields(nodes = self.graph.node_count()))]
    pub async fn run(
        &self,
        initial_state: serde_json::Value,
    ) -> rustmastra_core::Result<RunResult> {
        let start = self.graph.start_node().ok_or_else(|| {
            rustmastra_core::FrameworkError::Graph(
                "ExecutionGraph has no start node — call GraphBuilder::start()".into(),
            )
        })?;

        let mut state = initial_state;
        let mut completed: HashSet<NodeKey> = HashSet::new();
        let mut visit_count: HashMap<NodeKey, usize> = HashMap::new();
        let mut total_steps: usize = 0;

        // ── Initialise pending predecessor counts ──────────────────────────
        // For each node, track how many predecessors still need to finish
        // before it is eligible to run.
        let mut pending_preds: HashMap<NodeKey, usize> = self
            .graph
            .node_keys()
            .map(|k| (k, self.graph.in_degree(k)))
            .collect();

        // Start node is force-queued regardless of its in-degree.
        *pending_preds.entry(start).or_insert(0) = 0;
        let mut ready: VecDeque<NodeKey> = VecDeque::from([start]);

        // ── Main scheduling loop ───────────────────────────────────────────
        while !ready.is_empty() {
            // Collect *all* currently-ready nodes so they run in parallel.
            let batch: Vec<NodeKey> = ready.drain(..).collect();
            debug!(batch_size = batch.len(), "Dispatching ready batch");

            // ── Guard checks before spawning ───────────────────────────────
            for &key in &batch {
                let visits = visit_count.entry(key).or_insert(0);
                *visits += 1;
                if *visits > self.config.max_cycles_per_node {
                    let name = self
                        .graph
                        .task_of(key)
                        .map(|t| t.name().to_owned())
                        .unwrap_or_else(|| format!("{key:?}"));
                    return Err(rustmastra_core::FrameworkError::Graph(format!(
                        "Node '{name}' exceeded max cycle limit ({})",
                        self.config.max_cycles_per_node
                    )));
                }

                total_steps += 1;
                if total_steps > self.config.max_total_steps {
                    return Err(rustmastra_core::FrameworkError::Graph(format!(
                        "FlowRunner exceeded max total steps ({})",
                        self.config.max_total_steps
                    )));
                }
            }

            // ── Spawn all nodes in the batch concurrently ──────────────────
            let mut join_set: JoinSet<(
                NodeKey,
                rustmastra_core::Result<(serde_json::Value, NextAction)>,
            )> = JoinSet::new();

            for &key in &batch {
                let task = self.graph.task_of(key).ok_or_else(|| {
                    rustmastra_core::FrameworkError::Graph(format!(
                        "Node {key:?} has no associated task"
                    ))
                })?;
                // Each task runs with a snapshot of the current state.
                // State deltas are merged after all tasks in the batch finish.
                let state_snapshot = state.clone();
                join_set.spawn(async move {
                    let result = task.run_erased(key, state_snapshot).await;
                    (key, result)
                });
            }

            // ── Collect results ────────────────────────────────────────────
            // Snapshot state *before* the batch so we can compute per-node
            // deltas and apply them additively (numeric fields accumulate,
            // arrays grow, objects merge field-by-field).
            let base_state = state.clone();
            let mut raw_results: Vec<(NodeKey, serde_json::Value, NextAction)> =
                Vec::with_capacity(batch.len());

            while let Some(join_result) = join_set.join_next().await {
                let (key, task_result) = join_result.map_err(|e| {
                    rustmastra_core::FrameworkError::Graph(format!(
                        "Task panicked or was cancelled: {e}"
                    ))
                })?;
                let (new_state, action) = task_result?;
                completed.insert(key);
                raw_results.push((key, new_state, action));
            }

            // Apply each node's delta additively against the pre-batch snapshot.
            // This ensures that parallel branches that independently modify the
            // same numeric field both have their changes reflected.
            let mut batch_actions: Vec<(NodeKey, NextAction)> =
                Vec::with_capacity(raw_results.len());
            for (key, new_state, action) in raw_results {
                json_additive_merge(&mut state, &base_state, new_state);
                batch_actions.push((key, action));
            }

            // ── Process actions → update ready queue ───────────────────────
            for (key, action) in batch_actions {
                match action {
                    NextAction::Continue => {
                        self.enqueue_successors(
                            key,
                            &state,
                            &mut completed,
                            &mut pending_preds,
                            &mut ready,
                        );
                    }
                    NextAction::Parallelize { node_keys } => {
                        // Directly schedule the nominated nodes.
                        for nk in node_keys {
                            if !ready.contains(&nk) {
                                debug!(?nk, "Parallelize: force-queuing node");
                                pending_preds.insert(nk, 0);
                                ready.push_back(nk);
                            }
                        }
                    }
                    NextAction::WaitForInput { prompt } => {
                        info!(%prompt, "Workflow paused — waiting for human input");
                        return Ok(RunResult {
                            state,
                            status: RunStatus::WaitingForInput {
                                prompt,
                                paused_at: key,
                            },
                        });
                    }
                    NextAction::End => {
                        debug!(?key, "Node signalled End — branch terminated");
                        // Do not follow successors; just leave them unscheduled.
                    }
                }
            }
        }

        Ok(RunResult {
            state,
            status: RunStatus::Completed,
        })
    }

    /// Enqueue eligible successors of `from_key` after it completes.
    fn enqueue_successors(
        &self,
        from_key: NodeKey,
        state: &serde_json::Value,
        completed: &mut HashSet<NodeKey>,
        pending_preds: &mut HashMap<NodeKey, usize>,
        ready: &mut VecDeque<NodeKey>,
    ) {
        for (successor, edge_kind) in self.graph.successors(from_key) {
            if !passes_condition(&edge_kind, state) {
                continue;
            }

            if completed.contains(&successor) {
                // Back-edge in a cycle: allow re-execution by resetting the
                // node's completed status and pending count.
                warn!(
                    ?successor,
                    "Back-edge detected — re-queuing completed node (cycle)"
                );
                completed.remove(&successor);
                pending_preds.insert(successor, 0);
                ready.push_back(successor);
            } else if !ready.contains(&successor) {
                // Forward edge: decrement pending predecessor count.
                let count = pending_preds.entry(successor).or_insert(0);
                if *count > 0 {
                    *count -= 1;
                }
                if *count == 0 {
                    ready.push_back(successor);
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Additively merge `new` into `base` using `original` as the diff reference.
///
/// This is used when multiple parallel branches each return an updated copy of
/// the same starting state.  Instead of last-wins, we compute each branch's
/// *delta* relative to `original` and accumulate it into `base`:
///
/// * **Numbers** — `base += (new - original)` (additive).
/// * **Arrays**  — items appended in `new` beyond `original.len()` are pushed
///                 onto `base` (append-only delta).
/// * **Objects** — merged field-by-field recursively.
/// * **Everything else** — last-wins (string, bool, null).
fn json_additive_merge(
    base: &mut serde_json::Value,
    original: &serde_json::Value,
    new: serde_json::Value,
) {
    use serde_json::Value;
    match (base, original, new) {
        (Value::Object(b), Value::Object(o), Value::Object(n)) => {
            for (k, nv) in n {
                let orig = o.get(&k).cloned().unwrap_or(Value::Null);
                json_additive_merge(b.entry(k).or_insert(Value::Null), &orig, nv);
            }
        }
        // Numeric: apply the delta (new - original) to the accumulated base.
        (base_v, Value::Number(o_n), Value::Number(n_n)) => {
            let base_f = base_v.as_f64().unwrap_or(0.0);
            let orig_f = o_n.as_f64().unwrap_or(0.0);
            let new_f = n_n.as_f64().unwrap_or(0.0);
            let diff = new_f - orig_f;
            if diff != 0.0 {
                // Preserve integer representation when possible.
                let result = base_f + diff;
                *base_v = if result.fract() == 0.0 && result.abs() < i64::MAX as f64 {
                    Value::Number((result as i64).into())
                } else {
                    serde_json::Number::from_f64(result)
                        .map(Value::Number)
                        .unwrap_or(Value::Null)
                };
            }
        }
        // Array: append items that were added beyond the original length.
        (Value::Array(b), Value::Array(o), Value::Array(mut n)) => {
            let orig_len = o.len();
            if n.len() > orig_len {
                b.extend(n.drain(orig_len..));
            }
        }
        // Scalar fallback: last-wins.
        (base_v, _, new_v) => *base_v = new_v,
    }
}

/// Returns `true` when the edge's condition passes for the given state.
fn passes_condition(edge: &EdgeKind, state: &serde_json::Value) -> bool {
    match edge {
        EdgeKind::Always => true,
        EdgeKind::Conditional(pred) => pred(state),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GraphBuilder, Task};
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};

    // ── Shared test state ──────────────────────────────────────────────────

    #[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
    struct TestState {
        visited: Vec<String>,
        counter: i64,
    }

    // ── Minimal task: appends its name to `visited` ────────────────────────

    struct AppendTask {
        name: String,
        action: NextAction,
    }

    impl AppendTask {
        fn new(name: &str, action: NextAction) -> Self {
            Self { name: name.to_owned(), action }
        }
    }

    #[async_trait]
    impl Task for AppendTask {
        type State = TestState;

        async fn run(
            &self,
            _key: NodeKey,
            mut state: Self::State,
        ) -> rustmastra_core::Result<(Self::State, NextAction)> {
            state.visited.push(self.name.clone());
            Ok((state, self.action.clone()))
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    // ── Incrementing task ──────────────────────────────────────────────────

    struct IncrTask {
        name: String,
        by: i64,
    }

    impl IncrTask {
        fn new(name: &str, by: i64) -> Self {
            Self { name: name.to_owned(), by }
        }
    }

    #[async_trait]
    impl Task for IncrTask {
        type State = TestState;

        async fn run(
            &self,
            _key: NodeKey,
            mut state: Self::State,
        ) -> rustmastra_core::Result<(Self::State, NextAction)> {
            state.counter += self.by;
            state.visited.push(self.name.clone());
            Ok((state, NextAction::Continue))
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    // ── Helper: make JSON from TestState ──────────────────────────────────

    fn initial() -> serde_json::Value {
        serde_json::to_value(TestState::default()).unwrap()
    }

    fn decode(v: serde_json::Value) -> TestState {
        serde_json::from_value(v).unwrap()
    }

    // ── Test 1: simple linear DAG  A → B → C ──────────────────────────────

    #[tokio::test]
    async fn test_linear_dag() {
        let mut b = GraphBuilder::new();
        let a = b.add_node(AppendTask::new("A", NextAction::Continue));
        let bk = b.add_node(AppendTask::new("B", NextAction::Continue));
        let c = b.add_node(AppendTask::new("C", NextAction::End));
        b.edge(a, bk).edge(bk, c).start(a);
        let graph = Arc::new(b.build());

        let runner = FlowRunner::new(graph);
        let result = runner.run(initial()).await.expect("run failed");
        assert!(matches!(result.status, RunStatus::Completed));

        let state = decode(result.state);
        assert_eq!(state.visited, vec!["A", "B", "C"]);
    }

    // ── Test 2: parallel branches  A → [B, C] → D ─────────────────────────

    #[tokio::test]
    async fn test_parallel_branches() {
        let mut b = GraphBuilder::new();
        let a = b.add_node(IncrTask::new("A", 1));
        let bk = b.add_node(IncrTask::new("B", 10));
        let c = b.add_node(IncrTask::new("C", 100));
        let d = b.add_node(IncrTask::new("D", 1000));
        b.parallel(a, &[bk, c]);
        b.edge(bk, d).edge(c, d).start(a);
        let graph = Arc::new(b.build());

        let runner = FlowRunner::new(graph);
        let result = runner.run(initial()).await.expect("run failed");
        assert!(matches!(result.status, RunStatus::Completed));

        let state = decode(result.state);
        // Counter: A(+1) + B(+10) + C(+100) + D(+1000) = 1111
        assert_eq!(state.counter, 1111);
        // All four nodes visited (order of B and C is non-deterministic).
        assert!(state.visited.contains(&"A".to_owned()));
        assert!(state.visited.contains(&"B".to_owned()));
        assert!(state.visited.contains(&"C".to_owned()));
        assert!(state.visited.contains(&"D".to_owned()));
    }

    // ── Test 3: conditional edge — only the passing branch runs ───────────

    #[tokio::test]
    async fn test_conditional_edge() {
        // A → (counter>0) → B,  A → (counter<=0) → C
        // Initial counter == 0 so only C should run.
        let mut b = GraphBuilder::new();
        let a = b.add_node(IncrTask::new("A", 0)); // no change
        let bk = b.add_node(AppendTask::new("B", NextAction::End));
        let c = b.add_node(AppendTask::new("C", NextAction::End));
        b.conditional_edge(a, bk, |s| s["counter"].as_i64().unwrap_or(0) > 0);
        b.conditional_edge(a, c, |s| s["counter"].as_i64().unwrap_or(0) <= 0);
        b.start(a);
        let graph = Arc::new(b.build());

        let runner = FlowRunner::new(graph);
        let result = runner.run(initial()).await.expect("run failed");
        assert!(matches!(result.status, RunStatus::Completed));

        let state = decode(result.state);
        assert!(state.visited.contains(&"A".to_owned()));
        assert!(state.visited.contains(&"C".to_owned()));
        assert!(!state.visited.contains(&"B".to_owned()));
    }

    // ── Test 4: WaitForInput pauses and returns mid-run ───────────────────

    #[tokio::test]
    async fn test_wait_for_input() {
        let mut b = GraphBuilder::new();
        let a = b.add_node(AppendTask::new(
            "A",
            NextAction::WaitForInput { prompt: "approve?".into() },
        ));
        let bk = b.add_node(AppendTask::new("B", NextAction::End));
        b.edge(a, bk).start(a);
        let graph = Arc::new(b.build());

        let runner = FlowRunner::new(graph);
        let result = runner.run(initial()).await.expect("run failed");
        assert!(matches!(
            result.status,
            RunStatus::WaitingForInput { prompt, .. } if prompt == "approve?"
        ));

        // B must NOT have run yet.
        let state = decode(result.state);
        assert!(!state.visited.contains(&"B".to_owned()));
    }

    // ── Test 5: cycle guard — A → B → A hits max cycle limit ──────────────

    #[tokio::test]
    async fn test_cycle_guard() {
        let mut b = GraphBuilder::new();
        let a = b.add_node(AppendTask::new("A", NextAction::Continue));
        let bk = b.add_node(AppendTask::new("B", NextAction::Continue));
        b.edge(a, bk).edge(bk, a).start(a);
        let graph = Arc::new(b.build());

        let runner = FlowRunner::with_config(
            graph,
            RunnerConfig { max_cycles_per_node: 3, max_total_steps: 4_096 },
        );
        let result = runner.run(initial()).await;
        assert!(
            result.is_err(),
            "expected cycle-guard error, got {:?}",
            result
        );
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cycle"), "error should mention 'cycle': {err}");
    }

    // ── Test 6: NextAction::End stops branch without following successors ──

    #[tokio::test]
    async fn test_end_stops_branch() {
        let mut b = GraphBuilder::new();
        let a = b.add_node(AppendTask::new("A", NextAction::End));
        let bk = b.add_node(AppendTask::new("B", NextAction::End)); // should NOT run
        b.edge(a, bk).start(a);
        let graph = Arc::new(b.build());

        let runner = FlowRunner::new(graph);
        let result = runner.run(initial()).await.expect("run failed");
        assert!(matches!(result.status, RunStatus::Completed));

        let state = decode(result.state);
        assert_eq!(state.visited, vec!["A".to_owned()]);
    }
}
