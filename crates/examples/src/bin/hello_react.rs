//! Hello ReAct — minimal agent with one tool (current time).
//!
//! Shows: ReActAgent, one provider, LocalToolRegistry, TimeTool.
//!
//! Run from workspace root:
//!
//!   cargo run -p vanswarm-examples --bin hello_react
//!
//! Or with a prompt:
//!
//!   cargo run -p vanswarm-examples --bin hello_react -- "What time is it and what's 2+2?"
//!
//! Set one of: ANTHROPIC_API_KEY, OPENAI_API_KEY, GEMINI_API_KEY.

use std::sync::Arc;

use vanswarm_core::{
    config::{AgentConfig, ModelConfig},
    providers::AnthropicProvider,
    react::{run_agent, ReActAgent},
    traits::tool::LocalToolRegistry,
    TimeTool,
};

#[tokio::main]
async fn main() -> vanswarm_core::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let provider = AnthropicProvider::from_env()?;
    let executor = LocalToolRegistry::new().register(TimeTool);
    let config = AgentConfig::new("hello-react", ModelConfig::new("claude-sonnet-4-20250514"))
        .with_system_prompt("You are a helpful assistant. When the user asks for the time or date, use the time tool. Be concise.")
        .with_max_iterations(10);

    let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(executor));

    let prompt = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "What time is it right now? Reply in one short sentence.".to_string());

    let answer = run_agent(&agent, &prompt).await?;
    println!("{}", answer);
    Ok(())
}
