//! # rustmastra-orchestrator
//!
//! Graph-based workflow orchestration engine.
//!
//! Implements checklist §4: directed-graph execution with arena allocation,
//! parallel node scheduling, conditional edges, and HITL pause/resume.
//!
//! ## Key types (§4 — to be implemented)
//!
//! * `GraphBuilder` – fluent API: `.then()`, `.branch()`, `.parallel()`
//! * `FlowRunner`   – executes a graph; manages Ready Queue
//! * `AgentNode`    – one step (agent or pure function) in the graph
//! * `NextAction`   – `Continue | Parallelize | WaitForInput | End`

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use slotmap::{new_key_type, DenseSlotMap};

// ─────────────────────────────────────────────────────────────────────────────
// Node key (generational index — prevents ABA problem)
// ─────────────────────────────────────────────────────────────────────────────

new_key_type! {
    /// Stable handle into the `DenseSlotMap<NodeKey, AgentNode>`.
    pub struct NodeKey;
}

// ─────────────────────────────────────────────────────────────────────────────
// NextAction / TaskResult (checklist §4.10)
// ─────────────────────────────────────────────────────────────────────────────

/// What a node instructs the `FlowRunner` to do after it completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NextAction {
    /// Proceed to the next node(s) in the graph.
    Continue,

    /// Spawn several independent branches in parallel.
    Parallelize { node_keys: Vec<NodeKey> },

    /// Pause and wait for external input before resuming.
    /// The workflow state is checkpointed to the durable journal.
    WaitForInput {
        /// Describes what input is required (shown to the human approver).
        prompt: String,
    },

    /// The workflow has finished successfully.
    End,
}

// ─────────────────────────────────────────────────────────────────────────────
// Task trait (checklist §4.9)
// ─────────────────────────────────────────────────────────────────────────────

/// A single node in the execution graph.
///
/// `State` is the data structure accumulated across nodes:
/// `S_{n+1} = S_n + result(node_n)`.
#[async_trait]
pub trait Task: Send + Sync {
    /// The shared state type flowing through the graph.
    type State: Send + Sync + Clone + Serialize + serde::de::DeserializeOwned + 'static;

    /// Execute this task and return the updated state + what to do next.
    async fn run(
        &self,
        key: NodeKey,
        state: Self::State,
    ) -> rustmastra_core::Result<(Self::State, NextAction)>;

    fn name(&self) -> &str;
}

// ─────────────────────────────────────────────────────────────────────────────
// Graph topology (§4.1–4.4) — stubs; full implementation in next phase
// ─────────────────────────────────────────────────────────────────────────────

/// A directed, possibly-cyclic execution graph.
///
/// Internal representation: `petgraph::stable_graph::StableGraph` for
/// topology + `slotmap::DenseSlotMap` for node data.
/// Full implementation: checklist §4.
pub struct ExecutionGraph {
    _topology: petgraph::stable_graph::StableGraph<NodeKey, ()>,
    _nodes: DenseSlotMap<NodeKey, String>, // placeholder: will hold Box<dyn Task>
}

impl ExecutionGraph {
    pub fn new() -> Self {
        Self {
            _topology: petgraph::stable_graph::StableGraph::new(),
            _nodes: DenseSlotMap::with_key(),
        }
    }
}

impl Default for ExecutionGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Fluent builder for `ExecutionGraph`.
pub struct GraphBuilder {
    graph: ExecutionGraph,
}

impl GraphBuilder {
    pub fn new() -> Self {
        Self { graph: ExecutionGraph::new() }
    }

    pub fn build(self) -> ExecutionGraph {
        self.graph
    }
}

impl Default for GraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}
