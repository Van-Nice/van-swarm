//! Compiled execution graph — topology + node data.
//!
//! `ExecutionGraph` owns:
//! * A `slotmap::DenseSlotMap<NodeKey, NodeEntry>` that stores the type-erased
//!   tasks (via `Arc<dyn ErasedTask>`).
//! * A `petgraph::stable_graph::StableGraph<NodeKey, EdgeKind>` that stores the
//!   topology.  Petgraph nodes carry the `NodeKey` so we can translate between
//!   the two representations in O(1).
//!
//! Build via `GraphBuilder`; execute via `FlowRunner`.

use std::sync::Arc;

use petgraph::stable_graph::{NodeIndex, StableGraph};
use petgraph::visit::EdgeRef as _;
use petgraph::Direction;
use slotmap::DenseSlotMap;

use crate::{task::ErasedTask, NodeKey};

// ─────────────────────────────────────────────────────────────────────────────
// EdgeKind
// ─────────────────────────────────────────────────────────────────────────────

/// A predicate evaluated against the current JSON state to decide whether an
/// edge should be traversed.
pub type Predicate = Arc<dyn Fn(&serde_json::Value) -> bool + Send + Sync>;

/// How an edge is traversed.
#[derive(Clone)]
pub enum EdgeKind {
    /// Always traverse this edge (unconditional transition).
    Always,
    /// Traverse only when the predicate returns `true` for the current state.
    Conditional(Predicate),
}

impl std::fmt::Debug for EdgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EdgeKind::Always => write!(f, "Always"),
            EdgeKind::Conditional(_) => write!(f, "Conditional(<fn>)"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// NodeEntry — per-node storage
// ─────────────────────────────────────────────────────────────────────────────

/// Internal per-node record kept in the `DenseSlotMap`.
pub(crate) struct NodeEntry {
    /// Type-erased task.
    pub task: Arc<dyn ErasedTask>,
    /// This node's index in the petgraph topology (kept in sync).
    pub pg_index: NodeIndex,
}

// ─────────────────────────────────────────────────────────────────────────────
// ExecutionGraph
// ─────────────────────────────────────────────────────────────────────────────

/// A compiled, immutable execution graph.
///
/// Build with `GraphBuilder`; pass to `FlowRunner::new()`.
pub struct ExecutionGraph {
    pub(crate) nodes: DenseSlotMap<NodeKey, NodeEntry>,
    /// Petgraph topology: each node weight is the `NodeKey` of that node.
    pub(crate) topology: StableGraph<NodeKey, EdgeKind>,
    pub(crate) start: Option<NodeKey>,
}

impl ExecutionGraph {
    /// The designated start node, if one was set via `GraphBuilder::start()`.
    pub fn start_node(&self) -> Option<NodeKey> {
        self.start
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Iterate over all `NodeKey`s in the graph.
    pub fn node_keys(&self) -> impl Iterator<Item = NodeKey> + '_ {
        self.nodes.keys()
    }

    /// Task handle for a given node key.
    pub(crate) fn task_of(&self, key: NodeKey) -> Option<Arc<dyn ErasedTask>> {
        self.nodes.get(key).map(|e| Arc::clone(&e.task))
    }

    /// Petgraph `NodeIndex` for a `NodeKey`.
    pub(crate) fn pg_index_of(&self, key: NodeKey) -> Option<NodeIndex> {
        self.nodes.get(key).map(|e| e.pg_index)
    }

    /// `NodeKey` for a petgraph `NodeIndex`.
    pub(crate) fn key_of_pg(&self, idx: NodeIndex) -> Option<NodeKey> {
        self.topology.node_weight(idx).copied()
    }

    /// All successors of `key` with their associated edge kinds.
    pub(crate) fn successors(&self, key: NodeKey) -> Vec<(NodeKey, EdgeKind)> {
        let Some(pg_idx) = self.pg_index_of(key) else {
            return Vec::new();
        };
        self.topology
            .edges_directed(pg_idx, Direction::Outgoing)
            .filter_map(|edge| {
                let target_key = self.key_of_pg(edge.target())?;
                Some((target_key, edge.weight().clone()))
            })
            .collect()
    }

    /// All predecessors of `key`.
    pub(crate) fn predecessors(&self, key: NodeKey) -> Vec<NodeKey> {
        let Some(pg_idx) = self.pg_index_of(key) else {
            return Vec::new();
        };
        self.topology
            .neighbors_directed(pg_idx, Direction::Incoming)
            .filter_map(|src_idx| self.key_of_pg(src_idx))
            .collect()
    }

    /// In-degree of a node (number of distinct incoming edges).
    pub(crate) fn in_degree(&self, key: NodeKey) -> usize {
        self.predecessors(key).len()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GraphBuilder
// ─────────────────────────────────────────────────────────────────────────────

/// Fluent builder for `ExecutionGraph`.
///
/// ## Example
///
/// ```rust,no_run
/// use rustmastra_orchestrator::{GraphBuilder, NextAction, NodeKey, Task};
/// use async_trait::async_trait;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Clone, Serialize, Deserialize, Default)]
/// struct State { value: i64 }
///
/// struct AddOne;
///
/// #[async_trait]
/// impl Task for AddOne {
///     type State = State;
///     async fn run(&self, _key: NodeKey, mut s: State)
///         -> rustmastra_core::Result<(State, NextAction)>
///     {
///         s.value += 1;
///         Ok((s, NextAction::Continue))
///     }
///     fn name(&self) -> &str { "add_one" }
/// }
///
/// let mut b = GraphBuilder::new();
/// let a = b.add_node(AddOne);
/// let c = b.add_node(AddOne);
/// b.edge(a, c).start(a);
/// let graph = b.build();
/// ```
pub struct GraphBuilder {
    nodes: DenseSlotMap<NodeKey, NodeEntry>,
    topology: StableGraph<NodeKey, EdgeKind>,
    start: Option<NodeKey>,
}

impl GraphBuilder {
    pub fn new() -> Self {
        Self {
            nodes: DenseSlotMap::with_key(),
            topology: StableGraph::new(),
            start: None,
        }
    }

    /// Add a task node.  Returns the `NodeKey` used in subsequent `.edge()` calls.
    ///
    /// The task's concrete `State` type is erased through `TaskAdapter<T>`.
    pub fn add_node<T>(&mut self, task: T) -> NodeKey
    where
        T: crate::Task + Send + Sync + 'static,
        T::State: serde::Serialize
            + serde::de::DeserializeOwned
            + Send
            + Sync
            + Clone
            + 'static,
    {
        use crate::task::TaskAdapter;
        let arc_task: Arc<dyn ErasedTask> = Arc::new(TaskAdapter(task));

        // Insert a placeholder first so the slotmap gives us the key, then
        // add the key to petgraph and immediately fix up the pg_index.
        let key = self.nodes.insert(NodeEntry {
            task: arc_task,
            pg_index: NodeIndex::new(0), // patched below
        });
        let pg_idx = self.topology.add_node(key);
        self.nodes[key].pg_index = pg_idx;
        key
    }

    /// Add an unconditional (always-traversed) edge from `from` → `to`.
    pub fn edge(&mut self, from: NodeKey, to: NodeKey) -> &mut Self {
        self.add_edge_inner(from, to, EdgeKind::Always);
        self
    }

    /// Alias for `edge()` — reads more naturally in chains.
    pub fn then(&mut self, from: NodeKey, to: NodeKey) -> &mut Self {
        self.edge(from, to)
    }

    /// Add a conditional edge from `from` → `to`.
    ///
    /// `predicate` receives the current JSON state; return `true` to traverse.
    pub fn conditional_edge(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        predicate: impl Fn(&serde_json::Value) -> bool + Send + Sync + 'static,
    ) -> &mut Self {
        self.add_edge_inner(from, to, EdgeKind::Conditional(Arc::new(predicate)));
        self
    }

    /// Add parallel fan-out edges: `from` → each node in `targets`.
    pub fn parallel(&mut self, from: NodeKey, targets: &[NodeKey]) -> &mut Self {
        for &to in targets {
            self.add_edge_inner(from, to, EdgeKind::Always);
        }
        self
    }

    /// Designate the start node (the first node `FlowRunner` will execute).
    pub fn start(&mut self, key: NodeKey) -> &mut Self {
        self.start = Some(key);
        self
    }

    /// Consume the builder and return the compiled `ExecutionGraph`.
    pub fn build(self) -> ExecutionGraph {
        ExecutionGraph {
            nodes: self.nodes,
            topology: self.topology,
            start: self.start,
        }
    }

    fn add_edge_inner(&mut self, from: NodeKey, to: NodeKey, kind: EdgeKind) {
        let from_pg = self.nodes[from].pg_index;
        let to_pg = self.nodes[to].pg_index;
        self.topology.add_edge(from_pg, to_pg, kind);
    }
}

impl Default for GraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}
