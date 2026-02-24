//! # rustmastra-orchestrator
//!
//! Graph-based workflow orchestration engine for the RustMastra agent framework.
//!
//! ## Alignment Principle (checklist §4.16)
//!
//! The **centralized orchestrator** acts as the **validation bottleneck** for
//! all workflow execution. Every node runs only when the `FlowRunner` schedules
//! it; state flows through a single accumulated JSON value; and human-in-the-loop
//! (`WaitForInput`) is the only way to pause. This gives you:
//!
//! - **Single point of control:** No node can run unless the orchestrator
//!   enqueues it, so you can enforce policies (rate limits, auth, audit) in one place.
//! - **Observability:** The graph topology and `RunResult` (state + status) describe
//!   exactly what happened and where the flow stopped.
//! - **Recoverability:** When integrated with a durable journal (§4.14), the
//!   orchestrator can persist state at pause and resume later from the same point.
//!
//! Keep business logic inside `Task` implementations and use the orchestrator
//! to sequence and validate, not to duplicate domain rules.
//!
//! ## Key types
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`GraphBuilder`] | Fluent API for composing nodes and edges |
//! | [`ExecutionGraph`] | Compiled, immutable graph (petgraph + slotmap) |
//! | [`FlowRunner`] | Executes a graph; manages the ready queue |
//! | [`Task`] | Trait for a single node's logic |
//! | [`NextAction`] | What a node instructs the runner to do next |
//! | [`NodeKey`] | Generational index into the node arena |
//!
//! ## Quick example
//!
//! ```rust,no_run
//! use rustmastra_orchestrator::{FlowRunner, GraphBuilder, NextAction, NodeKey, Task};
//! use async_trait::async_trait;
//! use serde::{Deserialize, Serialize};
//! use std::sync::Arc;
//!
//! #[derive(Clone, Default, Serialize, Deserialize)]
//! struct PipelineState { result: String }
//!
//! struct ExtractNode;
//!
//! #[async_trait]
//! impl Task for ExtractNode {
//!     type State = PipelineState;
//!     async fn run(&self, _key: NodeKey, mut s: PipelineState)
//!         -> rustmastra_core::Result<(PipelineState, NextAction)>
//!     {
//!         s.result = "extracted".into();
//!         Ok((s, NextAction::Continue))
//!     }
//!     fn name(&self) -> &str { "extract" }
//! }
//!
//! #[tokio::main]
//! async fn main() -> rustmastra_core::Result<()> {
//!     let mut b = GraphBuilder::new();
//!     let n = b.add_node(ExtractNode);
//!     b.start(n);
//!     let graph = Arc::new(b.build());
//!     let result = FlowRunner::new(graph)
//!         .run(serde_json::json!({}))
//!         .await?;
//!     println!("{:?}", result.state);
//!     Ok(())
//! }
//! ```

pub mod graph;
pub mod runner;
pub mod task;

// ── Re-exports ────────────────────────────────────────────────────────────────

pub use graph::{EdgeKind, ExecutionGraph, GraphBuilder, Predicate};
pub use runner::{FlowRunner, GraphCheckpoint, RunResult, RunStatus, RunnerConfig};
pub use task::{ErasedTask, TaskAdapter};

// ─────────────────────────────────────────────────────────────────────────────
// NodeKey
// ─────────────────────────────────────────────────────────────────────────────

use slotmap::new_key_type;

new_key_type! {
    /// Stable, generational handle into the `DenseSlotMap<NodeKey, _>`.
    ///
    /// A `NodeKey` remains valid even after other nodes are removed (ABA-safe).
    pub struct NodeKey;
}

// ─────────────────────────────────────────────────────────────────────────────
// NextAction
// ─────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

/// What a task instructs the `FlowRunner` to do after it finishes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NextAction {
    /// Follow all outgoing edges whose conditions pass; schedule eligible
    /// successors in the next round.
    Continue,

    /// Immediately schedule the listed nodes, bypassing edge conditions.
    ///
    /// Used to implement explicit fan-out or cyclic evaluator-optimizer loops.
    Parallelize {
        node_keys: Vec<NodeKey>,
    },

    /// Pause the workflow and surface a prompt to a human operator.
    ///
    /// `FlowRunner::run()` returns `RunStatus::WaitingForInput` so the caller
    /// can collect the response and resume the workflow.
    WaitForInput {
        /// Human-readable description of what input is required.
        prompt: String,
    },

    /// Terminate traversal from this node; do not follow its successors.
    End,
}

// ─────────────────────────────────────────────────────────────────────────────
// Task trait
// ─────────────────────────────────────────────────────────────────────────────

use async_trait::async_trait;

/// A single, executable node in the `ExecutionGraph`.
///
/// `State` is the data structure shared across all nodes in the graph.
/// Each call receives the current state and must return an updated state plus
/// a `NextAction` that tells the runner what to do next.
///
/// ## State merging
///
/// When multiple nodes run in parallel, their output states are JSON-merged
/// (right-wins on scalar conflicts) into the single accumulated state before
/// the next round of scheduling.
#[async_trait]
pub trait Task: Send + Sync {
    /// The shared state type that flows through the graph.
    ///
    /// Must be `Serialize + DeserializeOwned` so it can be erased to
    /// `serde_json::Value` and restored for each node.
    type State: Send + Sync + Clone + Serialize + serde::de::DeserializeOwned + 'static;

    /// Execute this node.
    ///
    /// # Errors
    ///
    /// Return `Err` to propagate a fatal error that aborts the entire run.
    async fn run(
        &self,
        key: NodeKey,
        state: Self::State,
    ) -> rustmastra_core::Result<(Self::State, NextAction)>;

    /// Human-readable name used in logs and error messages.
    fn name(&self) -> &str;
}
