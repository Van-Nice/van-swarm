//! Basic ReAct agent: one question, no tools.
//!
//! Run from workspace root:
//!
//!   cargo run -p openswarm-core --example basic_agent
//!
//! Set one of: ANTHROPIC_API_KEY, OPENAI_API_KEY, GEMINI_API_KEY.

use std::sync::Arc;

use openswarm_core::{
    config::{AgentConfig, ModelConfig},
    providers::AnthropicProvider,
    react::{run_agent, ReActAgent},
    traits::tool::LocalToolRegistry,
};

#[tokio::main]
async fn main() -> openswarm_core::Result<()> {
    let provider = AnthropicProvider::from_env()?;
    let executor = LocalToolRegistry::new();
    let config = AgentConfig::new("basic", ModelConfig::new("claude-sonnet-4-20250514"))
        .with_max_iterations(10);
    let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(executor));

    let answer = run_agent(&agent, "What is 2 + 2? Reply in one short sentence.").await?;
    println!("{}", answer);
    Ok(())
}
