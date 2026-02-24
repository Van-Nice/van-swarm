//! # vanswarm-mcp-server
//!
//! Stdio MCP server that exposes the VanSwarm agent framework to any
//! MCP-capable client (Cursor IDE, Claude Desktop, custom tooling).
//!
//! ## Provider auto-detection (checked in order)
//!
//! | Env var              | Provider     | Default model        |
//! |----------------------|--------------|----------------------|
//! | `ANTHROPIC_API_KEY`   | Anthropic    | `claude-opus-4-6`    |
//! | `OPENAI_API_KEY`     | OpenAI       | `gpt-4o`             |
//! | `GEMINI_API_KEY`     | Gemini       | `gemini-2.0-flash`   |
//! | (none)               | **LM Studio**| `local`              |
//!
//! When no cloud API key is set, the server uses **LM Studio** at
//! `http://127.0.0.1:1234/v1` (OpenAI-compatible API). Set `LM_STUDIO_BASE_URL`
//! to override. Set `RUSTMASTRA_MODEL` or `LM_STUDIO_MODEL` to the model name
//! loaded in LM Studio.
//!
//! ## Usage
//!
//! ```sh
//! ANTHROPIC_API_KEY=sk-ant-... cargo run -p vanswarm-mcp-server
//! ```
//!
//! Add to `~/.cursor/mcp.json` (or run `vanswarm init` in your project):
//! ```json
//! {
//!   "mcpServers": {
//!     "vanswarm": {
//!       "command": "/path/to/vanswarm-mcp-server",
//!       "env": { "ANTHROPIC_API_KEY": "sk-ant-...", "VANSWARM_DB_PATH": ".vanswarm/data/vanswarm.db" }
//!     }
//!   }
//! }
//! ```
//!
//! **Persistent memory:** Set `VANSWARM_DB_PATH` to a file path (e.g. `.vanswarm/data/vanswarm.db`)
//! to persist episodic memory in a local libsql database. Build with `--features libsql` to enable.
//!
//! **Documentation resources:** Set `VANSWARM_DOCS_ROOT` to the documentation directory to enable
//! `resources/list` and `resources/read` for VanSwarm docs. Falls back to `documentation/` in the
//! current working directory when the env var is not set.

use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};

use vanswarm_core::providers::{
    AnthropicProvider, GeminiProvider, LmStudioProvider, ModelProvider, OpenAiProvider,
};
use vanswarm_memory::{EpisodicMemory, Memory};

mod server;

// ─────────────────────────────────────────────────────────────────────────────
// Provider detection
// ─────────────────────────────────────────────────────────────────────────────

/// Returns `(provider, default_model, provider_name)`.
///
/// Priority: Anthropic → OpenAI → Gemini. If no cloud API key is set, falls back
/// to **LM Studio** at http://127.0.0.1:1234/v1 (OpenAI-compatible).
/// The model can be overridden globally with `RUSTMASTRA_MODEL` (or `LM_STUDIO_MODEL` for local).
///
/// This function always succeeds — LM Studio is the unconditional fallback.
fn detect_provider() -> (Arc<dyn ModelProvider>, String, String) {
    let model_override = std::env::var("RUSTMASTRA_MODEL").ok();
    let lm_studio_model = std::env::var("LM_STUDIO_MODEL").ok();

    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        match AnthropicProvider::from_env() {
            Ok(p) => {
                let model = model_override.unwrap_or_else(|| "claude-opus-4-6".to_string());
                tracing::info!(provider = "anthropic", model, "LLM provider ready");
                return (Arc::new(p), model, "anthropic".to_string());
            }
            Err(e) => tracing::warn!("ANTHROPIC_API_KEY set but provider init failed: {}", e),
        }
    }

    if std::env::var("OPENAI_API_KEY").is_ok() {
        match OpenAiProvider::from_env() {
            Ok(p) => {
                let model = model_override.unwrap_or_else(|| "gpt-4o".to_string());
                tracing::info!(provider = "openai", model, "LLM provider ready");
                return (Arc::new(p), model, "openai".to_string());
            }
            Err(e) => tracing::warn!("OPENAI_API_KEY set but provider init failed: {}", e),
        }
    }

    if std::env::var("GEMINI_API_KEY").is_ok() {
        match GeminiProvider::from_env() {
            Ok(p) => {
                let model = model_override.unwrap_or_else(|| "gemini-2.0-flash".to_string());
                tracing::info!(provider = "gemini", model, "LLM provider ready");
                return (Arc::new(p), model, "gemini".to_string());
            }
            Err(e) => tracing::warn!("GEMINI_API_KEY set but provider init failed: {}", e),
        }
    }

    // Default: LM Studio at 127.0.0.1:1234 (OpenAI-compatible API)
    let provider = LmStudioProvider::from_env_or_default();
    let model = model_override
        .or(lm_studio_model)
        .unwrap_or_else(|| "local".to_string());
    tracing::info!(
        provider = "lm-studio",
        model,
        base_url = %provider.base_url(),
        "LLM provider ready (local)"
    );
    (Arc::new(provider), model, "lm-studio".to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Docs root detection (for resources/list and resources/read)
// ─────────────────────────────────────────────────────────────────────────────

/// Detect the docs root directory.
///
/// Priority:
/// 1. `VANSWARM_DOCS_ROOT` env var (explicit absolute or relative path).
/// 2. `documentation/` relative to the current working directory (dev default).
///
/// Returns `None` with a warning if neither path exists.
fn detect_docs_root() -> Option<PathBuf> {
    // Explicit override always wins.
    if let Ok(path) = std::env::var("VANSWARM_DOCS_ROOT") {
        let p = PathBuf::from(&path);
        if p.is_dir() {
            tracing::info!(path = %p.display(), "docs: using VANSWARM_DOCS_ROOT");
            return Some(p);
        }
        tracing::warn!(
            path = %p.display(),
            "VANSWARM_DOCS_ROOT set but directory does not exist; docs disabled"
        );
        return None;
    }

    // Dev fallback: look for documentation/ in cwd.
    let fallback = PathBuf::from("documentation");
    if fallback.is_dir() {
        tracing::info!(path = %fallback.display(), "docs: using ./documentation (fallback)");
        return Some(fallback);
    }

    tracing::debug!("docs: not configured (set VANSWARM_DOCS_ROOT to enable resources)");
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Memory backend (in-memory or libsql when VANSWARM_DB_PATH set)
// ─────────────────────────────────────────────────────────────────────────────

const DEFAULT_MAX_MEMORY_ENTRIES: usize = 1_000;

async fn create_memory() -> Arc<dyn Memory> {
    #[cfg(feature = "libsql")]
    {
        if let Ok(path) = std::env::var("VANSWARM_DB_PATH") {
            match vanswarm_memory::LibSqlEpisodicMemory::open(&path, DEFAULT_MAX_MEMORY_ENTRIES).await
            {
                Ok(store) => {
                    tracing::info!(path = %path, "memory: libsql (persistent)");
                    return Arc::new(store);
                }
                Err(e) => {
                    tracing::warn!(path = %path, error = %e, "VANSWARM_DB_PATH set but libsql open failed; using in-memory");
                }
            }
        }
    }
    tracing::debug!("memory: in-process (FIFO, {} entries)", DEFAULT_MAX_MEMORY_ENTRIES);
    Arc::new(EpisodicMemory::new(DEFAULT_MAX_MEMORY_ENTRIES))
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

    let (provider, default_model, provider_name) = detect_provider();
    let docs_root = detect_docs_root();

    let memory = create_memory().await;
    let tools = server::FrameworkTools::new(
        Some(provider),
        default_model,
        provider_name,
        memory,
        docs_root,
    );

    let service = tools
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!("MCP transport error: {}", e))?;

    tracing::info!("vanswarm-mcp-server ready");
    service.waiting().await?;
    Ok(())
}
