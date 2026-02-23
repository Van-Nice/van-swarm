//! `Runnable` – the base trait for every executable component.
//!
//! Both `Agent` and `Workflow` extend `Runnable`.  Keeping the base generic
//! lets the graph engine (§4) schedule heterogeneous nodes via a common
//! interface without dynamic dispatch overhead on the hot path.

use async_trait::async_trait;

/// A component that can be asynchronously executed with typed I/O.
///
/// # Design note
/// We intentionally keep `Input` and `Output` as associated types rather
/// than generic parameters.  A type can only implement `Runnable` once,
/// preventing accidental ambiguity when scheduling nodes in the graph.
///
/// For object-safe polymorphism use the erased wrapper in
/// `crates/orchestrator` instead of `dyn Runnable<Input = …>`.
#[async_trait]
pub trait Runnable: Send + Sync {
    /// The value consumed when this component starts execution.
    type Input: Send + Sync + 'static;
    /// The value produced when this component finishes successfully.
    type Output: Send + Sync + 'static;

    /// Execute the component.
    ///
    /// Implementations must be **cancellation-safe**: if the returned future
    /// is dropped at any `.await` point, no irrecoverable side-effects should
    /// remain (e.g. half-written journal entries, orphaned handles).
    async fn run(&self, input: Self::Input) -> crate::Result<Self::Output>;

    /// Human-readable component identifier – used in traces and error messages.
    fn name(&self) -> &str;
}
