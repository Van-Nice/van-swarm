//! Object-safe task erasure layer.
//!
//! `Task<State>` has an associated type which makes it non-object-safe.
//! We erase the type through `ErasedTask`, using `serde_json::Value` as the
//! universal state carrier.  Concrete implementations plug in via `TaskAdapter`.

use futures::future::BoxFuture;
use serde::{de::DeserializeOwned, Serialize};

use crate::{NextAction, NodeKey, Task};

// ─────────────────────────────────────────────────────────────────────────────
// ErasedTask – object-safe task
// ─────────────────────────────────────────────────────────────────────────────

/// Object-safe version of `Task` using `serde_json::Value` as the state type.
///
/// The graph engine stores `Arc<dyn ErasedTask>` so it can hold nodes with
/// different concrete state types in a single `DenseSlotMap`.
pub trait ErasedTask: Send + Sync {
    fn name(&self) -> &str;

    /// Execute the task and return `(updated_state, NextAction)`.
    ///
    /// The `state` parameter and return value are both `serde_json::Value`
    /// to allow cross-type state merging in the `FlowRunner`.
    fn run_erased(
        &self,
        key: NodeKey,
        state: serde_json::Value,
    ) -> BoxFuture<'_, openswarm_core::Result<(serde_json::Value, NextAction)>>;
}

// ─────────────────────────────────────────────────────────────────────────────
// TaskAdapter – bridges Task<S> → ErasedTask
// ─────────────────────────────────────────────────────────────────────────────

/// Wraps a concrete `Task` implementation and erases the state type.
pub struct TaskAdapter<T: Task>(pub T);

impl<T> ErasedTask for TaskAdapter<T>
where
    T: Task + Send + Sync,
    T::State: Serialize + DeserializeOwned + Send + Sync + Clone,
{
    fn name(&self) -> &str {
        self.0.name()
    }

    fn run_erased(
        &self,
        key: NodeKey,
        state: serde_json::Value,
    ) -> BoxFuture<'_, openswarm_core::Result<(serde_json::Value, NextAction)>> {
        Box::pin(async move {
            // Deserialise the generic state into T::State.
            let typed: T::State = serde_json::from_value(state).map_err(|e| {
                openswarm_core::FrameworkError::Graph(format!(
                    "Failed to deserialise state for task '{}': {e}",
                    self.0.name()
                ))
            })?;

            let (new_state, action) = self.0.run(key, typed).await?;

            let json_state = serde_json::to_value(&new_state).map_err(|e| {
                openswarm_core::FrameworkError::Graph(format!(
                    "Failed to serialise new state from task '{}': {e}",
                    self.0.name()
                ))
            })?;

            Ok((json_state, action))
        })
    }
}
