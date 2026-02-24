//! All public framework traits.
//!
//! | Trait          | Behaviour      | Dispatch           |
//! |----------------|----------------|--------------------|
//! | `Runnable`     | base           | generic or dyn     |
//! | `Agent`        | probabilistic  | generic or dyn     |
//! | `Workflow`     | deterministic  | generic preferred  |
//! | `Tool`         | single tool    | boxed in registry  |
//! | `ToolExecutor` | tool registry  | Arc<dyn ...>       |

pub mod agent;
pub mod runnable;
pub mod tool;
pub mod workflow;

pub use agent::{Agent, AgentAction, AgentContext, RunMetrics, ToolCall};
pub use runnable::Runnable;
pub use tool::{FilteredToolExecutor, LocalToolRegistry, Tool, ToolExecutor};
pub use workflow::{Workflow, WorkflowStatus, WorkflowStep};
