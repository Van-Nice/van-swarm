//! Centralised error type for the entire framework.
//!
//! Every public function returns `Result<T, FrameworkError>`.  Wrapping
//! provider-specific failures in structured variants means the ReAct loop
//! can distinguish transient HTTP 429s (retry) from permanent 401s (abort).

use thiserror::Error;

/// The top-level error type propagated throughout the framework.
#[derive(Debug, Error)]
pub enum FrameworkError {
    // ── Provider / LLM ────────────────────────────────────────────────────

    /// The upstream LLM provider returned a non-successful HTTP status.
    #[error("Provider '{provider}' returned an error: {message}")]
    Provider {
        provider: String,
        message: String,
        /// HTTP status code, if the error came from an HTTP response.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// The LLM provider rejected our API credentials (HTTP 401/403).
    #[error("Authentication failed for provider '{0}' – check your API key")]
    AuthenticationFailed(String),

    /// The LLM provider throttled our requests (HTTP 429).
    #[error("Rate limit exceeded for provider '{0}'")]
    RateLimitExceeded(String),

    /// The provider response body could not be decoded or had an unexpected
    /// shape.  We never assume LLM output is well-formed.
    #[error("Invalid response format from provider '{provider}': {message}")]
    InvalidResponse { provider: String, message: String },

    // ── HTTP ──────────────────────────────────────────────────────────────

    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),

    // ── Serialisation ─────────────────────────────────────────────────────

    #[error("Serialisation error: {0}")]
    Serialization(#[from] serde_json::Error),

    // ── Tool calling ──────────────────────────────────────────────────────

    /// The model requested a tool that doesn't exist in the executor's
    /// registry.
    #[error("Tool not found: '{0}'")]
    ToolNotFound(String),

    /// The tool was found but its execution failed (e.g. external API down).
    /// The error message is returned to the model so it can self-correct.
    #[error("Tool '{tool_name}' execution failed: {message}")]
    ToolExecution { tool_name: String, message: String },

    /// The model provided parameters that failed JSON-schema validation.
    #[error("Tool '{tool_name}' received invalid parameters: {message}")]
    ToolValidation { tool_name: String, message: String },

    // ── Agent / Workflow ──────────────────────────────────────────────────

    #[error("Agent '{name}' error: {message}")]
    Agent { name: String, message: String },

    #[error("Workflow error: {0}")]
    Workflow(String),

    /// The agent has reached its iteration cap without producing a final
    /// answer.  Callers can inspect the last context to understand why.
    #[error("Agent reached maximum iteration limit ({0}) without converging")]
    MaxIterationsReached(usize),

    /// The accumulated conversation would exceed the model's context window.
    #[error("Context window exceeded: {used} tokens used (limit {limit})")]
    ContextWindowExceeded { used: usize, limit: usize },

    // ── Configuration ─────────────────────────────────────────────────────

    #[error("Configuration error: {0}")]
    Config(String),

    // ── Graph / orchestration ─────────────────────────────────────────────

    #[error("Graph error: {0}")]
    Graph(String),

    // ── Memory ────────────────────────────────────────────────────────────

    #[error("Memory store error: {0}")]
    Memory(String),

    // ── I/O ───────────────────────────────────────────────────────────────

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // ── WASM runtime ──────────────────────────────────────────────────────

    #[error("WASM runtime error: {0}")]
    Wasm(String),
}

/// Convenience alias – every public function in the framework returns this.
pub type Result<T> = std::result::Result<T, FrameworkError>;

// ─────────────────────────────────────────────────────────────────────────────
// Helper constructors
// ─────────────────────────────────────────────────────────────────────────────

impl FrameworkError {
    /// Build a `Provider` error without a root cause chain.
    pub fn provider(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Provider {
            provider: provider.into(),
            message: message.into(),
            source: None,
        }
    }

    /// Build a `Provider` error with a root cause.
    pub fn provider_with_source(
        provider: impl Into<String>,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Provider {
            provider: provider.into(),
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Build a `ToolExecution` error.
    pub fn tool_exec(tool_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ToolExecution {
            tool_name: tool_name.into(),
            message: message.into(),
        }
    }

    /// Build an `Agent` error.
    pub fn agent(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Agent { name: name.into(), message: message.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_provider_error() {
        let e = FrameworkError::provider("openai", "context_length_exceeded");
        assert!(e.to_string().contains("openai"));
        assert!(e.to_string().contains("context_length_exceeded"));
    }

    #[test]
    fn display_tool_exec_error() {
        let e = FrameworkError::tool_exec("web_search", "timeout after 30s");
        assert!(e.to_string().contains("web_search"));
    }
}
