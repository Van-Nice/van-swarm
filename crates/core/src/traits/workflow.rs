//! `Workflow` trait – deterministic, fixed-code-path orchestration.
//!
//! **Workflows ≠ Agents.**  A workflow follows a predefined sequence of
//! steps whose transitions are hard-coded in Rust.  No LLM decides which
//! branch to take; the code does.  This is the right tool for:
//!   * Prompt chaining (extract → transform → format)
//!   * Evaluator-Optimizer loops
//!   * Anything where auditability and reproducibility are paramount
//!
//! The graph engine in `vanswarm-orchestrator` composes `Workflow`
//! implementations into directed graphs that may include cycles.

use async_trait::async_trait;

// ─────────────────────────────────────────────────────────────────────────────
// Workflow trait
// ─────────────────────────────────────────────────────────────────────────────

/// A deterministic execution unit with typed state.
///
/// Unlike `Agent`, the control flow is fully defined at compile time.
/// `State` is the data structure that flows through the workflow, being
/// accumulated and transformed at each step:
///
/// ```text
/// S_{n+1} = S_n + f(step_n)
/// ```
///
/// # Durable execution
/// When the `#[workflow]` macro (§3) is applied, each `.await` in the
/// implementation becomes a journal checkpoint.  On restart, the runtime
/// replays the journal and re-hydrates the `State` without re-executing
/// already-completed steps.
#[async_trait]
pub trait Workflow: Send + Sync {
    /// The state type threaded through all steps.
    ///
    /// Must be `Clone` for journal checkpointing, and `Serialize`/
    /// `Deserialize` for durable persistence.
    type State: Send + Sync + Clone + serde::Serialize + serde::de::DeserializeOwned + 'static;

    /// Run the workflow to completion, returning the final state.
    async fn execute(&self, initial: Self::State) -> crate::Result<Self::State>;

    /// Human-readable workflow name.
    fn name(&self) -> &str;
}

// ─────────────────────────────────────────────────────────────────────────────
// WorkflowStatus – for long-running / HITL workflows
// ─────────────────────────────────────────────────────────────────────────────

/// The current status of a long-running workflow instance.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum WorkflowStatus<S> {
    /// The workflow is actively running.
    Running,

    /// The workflow paused and is waiting for external input (HITL, §16).
    WaitingForInput {
        /// Context about what input is expected.
        prompt: String,
        /// The state snapshot at the pause point.
        state_snapshot: S,
    },

    /// The workflow completed successfully.
    Completed { final_state: S },

    /// The workflow failed with an error.
    Failed { error: String },
}

// ─────────────────────────────────────────────────────────────────────────────
// WorkflowStep – a single named step inside a workflow
// ─────────────────────────────────────────────────────────────────────────────

/// A single named, traceable step inside a workflow.
///
/// Composing workflows from typed steps (rather than inline closures) makes
/// the execution graph explicit and enables the graph engine to visualise
/// the execution path.
#[async_trait]
pub trait WorkflowStep: Send + Sync {
    type State: Send + Sync + Clone + 'static;

    /// Execute this step and return the updated state.
    async fn run(&self, state: Self::State) -> crate::Result<Self::State>;

    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trivial counter workflow for testing.
    struct IncrementWorkflow {
        amount: u32,
    }

    #[async_trait]
    impl Workflow for IncrementWorkflow {
        type State = u32;

        async fn execute(&self, initial: u32) -> crate::Result<u32> {
            Ok(initial + self.amount)
        }

        fn name(&self) -> &str {
            "increment"
        }
    }

    #[tokio::test]
    async fn workflow_increments_state() {
        let wf = IncrementWorkflow { amount: 5 };
        let result = wf.execute(10).await.unwrap();
        assert_eq!(result, 15);
    }
}
