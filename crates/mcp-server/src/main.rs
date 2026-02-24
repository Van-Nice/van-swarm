//! # vanswarm-mcp-server
//!
//! Stdio MCP server that exposes the VanSwarm agent framework to any
//! MCP-capable client (Cursor IDE, Claude Desktop, custom tooling).
//!
//! ## Provider auto-detection (checked in order)
//!
//! | Env var             | Provider     | Default model        |
//! |---------------------|--------------|----------------------|
//! | `ANTHROPIC_API_KEY` | Anthropic    | `claude-opus-4-6`    |
//! | `OPENAI_API_KEY`    | OpenAI       | `gpt-4o`             |
//! | `GEMINI_API_KEY`    | Gemini       | `gemini-2.0-flash`   |
//!
//! Set `RUSTMASTRA_MODEL` to override the default model for any provider.
//!
//! ## Usage
//!
//! ```sh
//! ANTHROPIC_API_KEY=sk-ant-... cargo run -p vanswarm-mcp-server
//! ```
//!
//! Add to `~/.cursor/mcp.json`:
//! ```json
//! {
//!   "mcpServers": {
//!     "vanswarm": {
//!       "command": "/path/to/vanswarm-mcp-server",
//!       "env": { "ANTHROPIC_API_KEY": "sk-ant-..." }
//!     }
//!   }
//! }
//! ```

use std::sync::Arc;

use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};

use vanswarm_core::providers::{AnthropicProvider, GeminiProvider, ModelProvider, OpenAiProvider};

mod server;

// ─────────────────────────────────────────────────────────────────────────────
// Provider detection
// ─────────────────────────────────────────────────────────────────────────────

/// Returns `(provider, default_model, provider_name)` if any API key is set.
///
/// Priority: Anthropic → OpenAI → Gemini.
/// The model can be overridden globally with `RUSTMASTRA_MODEL`.
fn detect_provider() -> Option<(Arc<dyn ModelProvider>, String, String)> {
    let model_override = std::env::var("RUSTMASTRA_MODEL").ok();

    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        match AnthropicProvider::from_env() {
            Ok(p) => {
                let model = model_override.unwrap_or_else(|| "claude-opus-4-6".to_string());
                tracing::info!(provider = "anthropic", model, "LLM provider ready");
                return Some((Arc::new(p), model, "anthropic".to_string()));
            }
            Err(e) => tracing::warn!("ANTHROPIC_API_KEY set but provider init failed: {}", e),
        }
    }

    if std::env::var("OPENAI_API_KEY").is_ok() {
        match OpenAiProvider::from_env() {
            Ok(p) => {
                let model = model_override.unwrap_or_else(|| "gpt-4o".to_string());
                tracing::info!(provider = "openai", model, "LLM provider ready");
                return Some((Arc::new(p), model, "openai".to_string()));
            }
            Err(e) => tracing::warn!("OPENAI_API_KEY set but provider init failed: {}", e),
        }
    }

    if std::env::var("GEMINI_API_KEY").is_ok() {
        match GeminiProvider::from_env() {
            Ok(p) => {
                let model = model_override.unwrap_or_else(|| "gemini-2.0-flash".to_string());
                tracing::info!(provider = "gemini", model, "LLM provider ready");
                return Some((Arc::new(p), model, "gemini".to_string()));
            }
            Err(e) => tracing::warn!("GEMINI_API_KEY set but provider init failed: {}", e),
        }
    }

    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    // Logs go to stderr — stdout is exclusively the JSON-RPC MCP transport.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("vanswarm_mcp_server=info".parse()?)
                .add_directive("warn".parse()?),
        )
        .with_writer(std::io::stderr)
        .compact()
        .init();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "vanswarm-mcp-server starting"
    );

    let (provider, default_model, provider_name) = match detect_provider() {
        Some((p, m, n)) => (Some(p), m, n),
        None => {
            tracing::warn!(
                "No LLM provider configured. Memory and info tools will work, \
                 but run_agent will fail. Set ANTHROPIC_API_KEY, OPENAI_API_KEY, \
                 or GEMINI_API_KEY to enable agent execution."
            );
            (None, "none".to_string(), "none".to_string())
        }
    };

    let tools = server::FrameworkTools::new(provider, default_model, provider_name);

    let service = tools
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!("MCP transport error: {}", e))?;

    tracing::info!("vanswarm-mcp-server ready");
    service.waiting().await?;
    Ok(())
}
