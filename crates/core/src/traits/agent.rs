//! `Agent` trait – probabilistic, ReAct-style, model-driven components.
//!
//! An Agent is the *probabilistic* counterpart to the deterministic `Workflow`.
//! It maintains a live message history, calls an LLM at each step to decide
//! which tool to invoke (or when to return a final answer), and continues
//! until convergence or the iteration cap is hit.
//!
//! The separation from `Workflow` at the type-system level (checklist §2.4)
//! ensures callers always know whether they're dealing with deterministic or
//! probabilistic behaviour – critical for audits and safety reviews.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    config::AgentConfig,
    message::{ContentBlock, Message, TokenUsage},
    traits::tool::ToolExecutor,
};

// ─────────────────────────────────────────────────────────────────────────────
// AgentContext – the mutable state threaded through the ReAct loop
// ─────────────────────────────────────────────────────────────────────────────

/// The running context of a single agent execution.
///
/// This is deliberately **not** `Clone` – there should be exactly one live
/// context per agent run, owned by the loop driver.  If you need checkpointing
/// use the durable journal (§3) instead.
#[derive(Debug)]
pub struct AgentContext {
    /// Full conversation history (system + user + assistant + tool turns).
    pub messages: Vec<Message>,

    /// Accumulated token usage across all LLM calls in this run.
    pub token_usage: TokenUsage,

    /// How many ReAct iterations have been completed so far.
    pub iteration: usize,

    /// The maximum number of iterations before the agent is forcibly stopped.
    pub max_iterations: usize,
}

impl AgentContext {
    /// Initialise a fresh context for a new agent run.
    ///
    /// If `system_prompt` is provided it is prepended as a `Role::System`
    /// message; downstream providers extract it appropriately.
    pub fn new(user_input: impl Into<String>, system_prompt: Option<String>) -> Self {
        let mut messages = Vec::new();

        if let Some(sp) = system_prompt {
            messages.push(Message::system(sp));
        }
        messages.push(Message::user(user_input));

        Self {
            messages,
            token_usage: TokenUsage::default(),
            iteration: 0,
            max_iterations: 10,
        }
    }

    /// Append an assistant message (reasoning text or tool call blocks).
    pub fn push_assistant(&mut self, msg: Message) {
        debug_assert!(
            matches!(msg.role, crate::message::Role::Assistant),
            "push_assistant called with non-assistant message"
        );
        self.messages.push(msg);
    }

    /// Append a tool result (or error) as a new message turn.
    pub fn push_tool_result(&mut self, block: ContentBlock) {
        // Anthropic and OpenAI both want tool results as their own turn.
        self.messages.push(Message {
            role: crate::message::Role::Tool,
            content: vec![block],
        });
    }

    /// Accumulate token usage from a completed LLM call.
    pub fn record_usage(&mut self, usage: TokenUsage) {
        self.token_usage = self.token_usage.clone() + usage;
    }

    /// True if we've hit the iteration limit.
    pub fn is_exhausted(&self) -> bool {
        self.iteration >= self.max_iterations
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AgentAction – what the model decided to do next
// ─────────────────────────────────────────────────────────────────────────────

/// The outcome of a single `Agent::step` call.
#[derive(Debug)]
pub enum AgentAction {
    /// The model produced one or more tool calls.
    /// The loop driver must execute each and feed results back.
    CallTools {
        /// The full assistant message (may contain both text and tool-use blocks).
        assistant_message: Message,
        /// Extracted tool calls ready for dispatch (name, id, args).
        calls: Vec<ToolCall>,
    },

    /// The model has finished reasoning and produced a final answer.
    FinalAnswer {
        /// The natural-language response to return to the caller.
        content: String,
    },

    /// The model needs additional context from the user before proceeding.
    /// Used for interactive / HITL scenarios (checklist §16).
    NeedsClarification { question: String },
}

/// A single tool invocation extracted from an assistant message.
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// Provider-assigned unique ID (used to match results back to calls).
    pub id: String,
    /// Snake-case tool name.
    pub name: String,
    /// Parsed JSON arguments.  Never assumed to be valid – the executor
    /// validates them before calling the tool implementation.
    pub arguments: serde_json::Value,
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent trait
// ─────────────────────────────────────────────────────────────────────────────

/// A probabilistic AI agent driven by an LLM.
///
/// The minimal contract: given a mutable `AgentContext`, call the model and
/// return an `AgentAction`.  The full ReAct loop lives in `crate::react` and
/// calls `step` repeatedly.
///
/// # Object safety
/// `Agent` is object-safe only if the associated-type constraints are
/// satisfied.  For concrete polymorphism prefer generics; for runtime
/// polymorphism use `Arc<dyn Agent<…>>`.
#[async_trait]
pub trait Agent: Send + Sync {
    /// Perform one step of the ReAct loop.
    ///
    /// Implementations should:
    /// 1. Build a `CompletionRequest` from `ctx.messages` + available tools.
    /// 2. Call the provider.
    /// 3. Parse the response into an `AgentAction`.
    ///
    /// The loop in `crate::react` handles executing tool calls, appending
    /// results, and calling `step` again.
    async fn step(&self, ctx: &mut AgentContext) -> crate::Result<AgentAction>;

    /// Return the agent's display name (used in traces).
    fn name(&self) -> &str;

    /// Return the agent's configuration.
    fn config(&self) -> &AgentConfig;

    /// Return a reference to the tool executor this agent uses.
    fn tool_executor(&self) -> Option<&Arc<dyn ToolExecutor>>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper: extract ToolCall list from an assistant Message
// ─────────────────────────────────────────────────────────────────────────────

/// Parse all `ToolUse` content blocks from an assistant message into
/// `ToolCall` values that the loop can dispatch.
pub fn extract_tool_calls(msg: &Message) -> Vec<ToolCall> {
    msg.content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::ToolUse { id, name, input } => Some(ToolCall {
                id: id.clone(),
                name: name.clone(),
                arguments: input.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// Check whether an `AgentAction::CallTools` response actually contains any
/// tool calls (guards against malformed model output that sets `finish_reason`
/// to `tool_use` but provides zero tool-use blocks).
pub fn has_pending_tool_calls(action: &AgentAction) -> bool {
    matches!(action, AgentAction::CallTools { calls, .. } if !calls.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_tool_calls_from_message() {
        let msg = Message::assistant_with_blocks(vec![
            ContentBlock::text("Let me search for that."),
            ContentBlock::tool_use(
                "call_01",
                "web_search",
                serde_json::json!({"query": "Rust ownership"}),
            ),
            ContentBlock::tool_use(
                "call_02",
                "read_file",
                serde_json::json!({"path": "/README.md"}),
            ),
        ]);

        let calls = extract_tool_calls(&msg);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "web_search");
        assert_eq!(calls[1].id, "call_02");
    }

    #[test]
    fn context_iteration_cap() {
        let mut ctx = AgentContext::new("hello", None);
        ctx.max_iterations = 2;
        ctx.iteration = 2;
        assert!(ctx.is_exhausted());
    }
}
