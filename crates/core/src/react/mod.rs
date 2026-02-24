//! Concrete `ReActAgent` and `run_agent` loop.
//!
//! The ReAct pattern (Yao et al., 2022):
//!   Thought → Action (tool call) → Observation → repeat until convergence.
//!
//! Architecture:
//! * `ReActAgent` owns a `ModelProvider` and a `ToolExecutor`.
//! * `step()` builds the LLM request, calls the provider, and returns an
//!   `AgentAction`.
//! * `run_agent()` is the outer loop: it calls `step()`, dispatches tool
//!   calls, feeds results back, and repeats until `FinalAnswer` or error.

use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, info, instrument, warn};

use crate::{
    config::AgentConfig,
    message::{CompletionRequest, StopReason},
    providers::ModelProvider,
    traits::{
        agent::{extract_tool_calls, Agent, AgentAction, AgentContext, RunMetrics},
        tool::ToolExecutor,
    },
};

// ─────────────────────────────────────────────────────────────────────────────
// ReActAgent
// ─────────────────────────────────────────────────────────────────────────────

/// A concrete `Agent` that implements the ReAct loop.
///
/// ```rust,no_run
/// use rustmastra_core::{
///     config::{AgentConfig, ModelConfig, ProviderCredentials},
///     providers::AnthropicProvider,
///     react::ReActAgent,
///     traits::tool::LocalToolRegistry,
/// };
/// use std::sync::Arc;
///
/// let provider = AnthropicProvider::from_env().unwrap();
/// let executor = LocalToolRegistry::new(); // add tools here
/// let config = AgentConfig::new("my-agent", ModelConfig::new("claude-opus-4-6"));
/// let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(executor));
/// ```
pub struct ReActAgent {
    config: AgentConfig,
    provider: Arc<dyn ModelProvider>,
    executor: Arc<dyn ToolExecutor>,
}

impl ReActAgent {
    pub fn new(
        config: AgentConfig,
        provider: Arc<dyn ModelProvider>,
        executor: Arc<dyn ToolExecutor>,
    ) -> Self {
        Self { config, provider, executor }
    }

    /// Build the system prompt, optionally injecting a chain-of-thought prefix.
    fn effective_system_prompt(&self) -> Option<String> {
        let base = self.config.system_prompt.clone();

        if !self.config.enable_chain_of_thought {
            return base;
        }

        let cot_prefix = "\
Before each tool call, reason in <thinking>…</thinking> tags. \
Be concise. Do not over-think. Act efficiently.\n\n";

        Some(match base {
            None => cot_prefix.to_string(),
            Some(b) => format!("{cot_prefix}{b}"),
        })
    }
}

#[async_trait]
impl Agent for ReActAgent {
    #[instrument(skip(self, ctx), fields(agent = %self.config.name, iteration = ctx.iteration))]
    async fn step(&self, ctx: &mut AgentContext) -> crate::Result<AgentAction> {
        let tools = self.executor.tool_definitions();

        let request = CompletionRequest::new(self.config.model.model_id.clone(), ctx.messages.clone())
            .with_tools(tools)
            .with_temperature(self.config.model.temperature.unwrap_or(0.7))
            .with_max_tokens(self.config.model.max_tokens.unwrap_or(4096));

        debug!(
            agent = %self.config.name,
            messages = ctx.messages.len(),
            "Calling model"
        );

        let response = self.provider.complete(request).await?;
        ctx.record_usage(response.usage);

        match response.stop_reason {
            StopReason::ToolUse => {
                let calls = extract_tool_calls(&response.message);
                if calls.is_empty() {
                    // Model said tool_use but provided no tool calls – treat as end.
                    warn!(
                        agent = %self.config.name,
                        "stop_reason=tool_use but no tool calls found; treating as FinalAnswer"
                    );
                    return Ok(AgentAction::FinalAnswer {
                        content: response.message.text_content(),
                    });
                }
                Ok(AgentAction::CallTools { assistant_message: response.message, calls })
            }
            StopReason::EndTurn | StopReason::StopSequence => {
                Ok(AgentAction::FinalAnswer { content: response.message.text_content() })
            }
            StopReason::MaxTokens => {
                warn!(agent = %self.config.name, "Model hit max_tokens; returning partial answer");
                Ok(AgentAction::FinalAnswer { content: response.message.text_content() })
            }
            StopReason::ContentFilter => Err(crate::FrameworkError::provider(
                self.provider.provider_name(),
                "output blocked by content filter",
            )),
            StopReason::Other(ref reason) => {
                warn!(agent = %self.config.name, reason, "Unexpected stop reason");
                Ok(AgentAction::FinalAnswer { content: response.message.text_content() })
            }
        }
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn config(&self) -> &AgentConfig {
        &self.config
    }

    fn tool_executor(&self) -> Option<&Arc<dyn ToolExecutor>> {
        Some(&self.executor)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// run_agent – the outer ReAct loop
// ─────────────────────────────────────────────────────────────────────────────

/// Drive the full ReAct loop for an `Agent`.
///
/// This is the primary entry point for running an agent.  It:
/// 1. Creates a fresh `AgentContext` with the user's input.
/// 2. Calls `agent.step()` repeatedly.
/// 3. Dispatches tool calls via the agent's `ToolExecutor`.
/// 4. Appends tool results to the context.
/// 5. Returns on `FinalAnswer` or when the iteration cap is reached.
///
/// # Example
/// ```rust,no_run
/// use rustmastra_core::react::{run_agent, ReActAgent};
/// # async fn example(agent: ReActAgent) -> rustmastra_core::Result<()> {
/// let answer = run_agent(&agent, "What is the capital of France?").await?;
/// println!("{answer}");
/// # Ok(())
/// # }
/// ```
#[instrument(skip(agent, user_input), fields(agent = %agent.name()))]
pub async fn run_agent(agent: &impl Agent, user_input: impl Into<String>) -> crate::Result<String> {
    let (answer, _) = run_agent_with_metrics(agent, user_input).await?;
    Ok(answer)
}

/// Run the agent and return the final answer plus per-run metrics (§11.5).
///
/// Use for SPL, APM, and tuning: `metrics.tool_call_count` is the executed path length.
#[instrument(skip(agent, user_input), fields(agent = %agent.name()))]
pub async fn run_agent_with_metrics(
    agent: &impl Agent,
    user_input: impl Into<String>,
) -> crate::Result<(String, RunMetrics)> {
    let system_prompt = if let Some(ra) = agent_as_react(agent) {
        ra.effective_system_prompt()
    } else {
        agent.config().system_prompt.clone()
    };

    let mut ctx = AgentContext::new(user_input.into(), system_prompt);
    ctx.max_iterations = agent.config().max_iterations;

    info!(agent = %agent.name(), "Starting agent run");

    loop {
        if ctx.is_exhausted() {
            return Err(crate::FrameworkError::MaxIterationsReached(ctx.max_iterations));
        }

        let action = agent.step(&mut ctx).await?;
        ctx.iteration += 1;

        match action {
            AgentAction::FinalAnswer { content } => {
                let metrics = RunMetrics {
                    iterations: ctx.iteration,
                    tool_call_count: ctx.tool_call_count,
                };
                info!(
                    agent = %agent.name(),
                    iterations = metrics.iterations,
                    tool_call_count = metrics.tool_call_count,
                    input_tokens = ctx.token_usage.input_tokens,
                    output_tokens = ctx.token_usage.output_tokens,
                    "Agent run complete"
                );
                return Ok((content, metrics));
            }

            AgentAction::CallTools { assistant_message, calls } => {
                // 1. Add the assistant's message (with tool-use blocks) to history.
                ctx.push_assistant(assistant_message);
                ctx.tool_call_count += calls.len();

                // 2. Execute every tool call concurrently.
                //    We use join_all so parallel tools don't block each other.
                let executor = agent
                    .tool_executor()
                    .ok_or_else(|| crate::FrameworkError::agent(agent.name(), "no tool executor"))?;

                let tool_futures: Vec<_> = calls
                    .iter()
                    .map(|call| {
                        let exec = Arc::clone(executor);
                        let name = call.name.clone();
                        let id = call.id.clone();
                        let args = call.arguments.clone();
                        async move { exec.execute(&name, &id, args).await }
                    })
                    .collect();

                let results = futures::future::join_all(tool_futures).await;

                // 3. Append all tool results as a single tool-role message.
                //    Most providers want one result per turn; for providers
                //    that want multi-result turns we batch them here.
                for result_block in results {
                    ctx.push_tool_result(result_block);
                }

                debug!(
                    agent = %agent.name(),
                    tools_called = calls.len(),
                    "Tool calls complete, continuing loop"
                );
            }

            AgentAction::NeedsClarification { question } => {
                // HITL pause point.  For now return the question as the answer;
                // the durable execution layer (§3) will handle resumption.
                info!(agent = %agent.name(), "Agent requested human clarification");
                let metrics = RunMetrics {
                    iterations: ctx.iteration,
                    tool_call_count: ctx.tool_call_count,
                };
                return Ok((question, metrics));
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal: downcast helper for system prompt injection
// ─────────────────────────────────────────────────────────────────────────────

/// Try to get a reference to the concrete `ReActAgent` for accessing
/// its `effective_system_prompt()`.  Falls back gracefully if the
/// runtime agent is not a `ReActAgent`.
fn agent_as_react<A: Agent>(_agent: &A) -> Option<&ReActAgent> {
    // Use `Any` downcasting only if we add `+ Any` to the `Agent` bound,
    // which we deliberately avoid to keep object safety.
    // For now, system-prompt injection is done in `ReActAgent::step()` itself
    // by reading `config.system_prompt`.
    None
}
