//! Configuration types for models and agents.
//!
//! Keeping config as plain data (no behaviour) means it can be cheaply
//! cloned across threads and serialised to disk / env for durable replay.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Model configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Sampling and generation parameters forwarded verbatim to the provider.
///
/// `None` values are omitted from the API request, letting the provider use
/// its defaults – this is safer than hard-coding zeros.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Provider-specific model identifier, e.g. `"gpt-4o"` or
    /// `"claude-opus-4-6"`.
    pub model_id: String,

    /// Sampling temperature.  Higher values → more random outputs.
    /// Typically 0.0–2.0 for OpenAI, 0.0–1.0 for Anthropic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Hard cap on the number of tokens the model may generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Nucleus sampling probability mass.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    /// Top-k sampling (Anthropic / Gemini only; ignored by OpenAI).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,

    /// Stop sequences that cause the model to stop generating.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,
}

impl ModelConfig {
    /// Sensible defaults for interactive agent use.
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            temperature: None,
            max_tokens: Some(4096),
            top_p: None,
            top_k: None,
            stop_sequences: Vec::new(),
        }
    }

    /// Override temperature (builder pattern).
    pub fn with_temperature(mut self, t: f32) -> Self {
        self.temperature = Some(t);
        self
    }

    /// Override max_tokens (builder pattern).
    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = Some(n);
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for a single agent instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Human-readable name used in logs and traces.
    pub name: String,

    /// Model sampling parameters.
    pub model: ModelConfig,

    /// Optional system prompt / persona injected as the first message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// Maximum number of ReAct iterations before forcibly returning an error.
    /// Prevents infinite loops in adversarial or badly-prompted scenarios.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,

    /// If `true`, the agent appends a `<thinking>…</thinking>` block to
    /// its system prompt instructing it to reason before each tool call.
    #[serde(default)]
    pub enable_chain_of_thought: bool,

    /// If `true`, request Anthropic-style prompt caching on the system prompt (§20.2).
    ///
    /// When enabled, the Anthropic provider adds `cache_control: {"type": "ephemeral"}`
    /// to the system-prompt block.  Cached tokens cost ~10 % of normal input-token price
    /// on subsequent requests that share the same prefix, reducing latency and cost for
    /// agents with long system prompts or large injected context.
    ///
    /// Has **no effect** on OpenAI or Gemini providers (ignored silently).
    #[serde(default)]
    pub cache_system_prompt: bool,
}

fn default_max_iterations() -> usize {
    10
}

impl AgentConfig {
    pub fn new(name: impl Into<String>, model: ModelConfig) -> Self {
        Self {
            name: name.into(),
            model,
            system_prompt: None,
            max_iterations: default_max_iterations(),
            enable_chain_of_thought: false,
            cache_system_prompt: false,
        }
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn with_max_iterations(mut self, n: usize) -> Self {
        self.max_iterations = n;
        self
    }

    pub fn with_chain_of_thought(mut self) -> Self {
        self.enable_chain_of_thought = true;
        self
    }

    /// Enable Anthropic prompt caching on the system prompt (§20.2).
    ///
    /// Reduces cost and latency for repeated calls with the same system prompt
    /// by caching it on Anthropic's side.  No-op for other providers.
    pub fn with_prompt_caching(mut self) -> Self {
        self.cache_system_prompt = true;
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Provider credentials (never log these!)
// ─────────────────────────────────────────────────────────────────────────────

/// API credentials for a model provider.
///
/// Loaded from environment variables at startup; never stored in plaintext
/// in config files.
#[derive(Clone)]
pub struct ProviderCredentials {
    /// The raw API key value.
    pub api_key: String,
    /// Optional override for the base URL (useful for proxies / Azure OpenAI).
    pub base_url: Option<String>,
}

impl std::fmt::Debug for ProviderCredentials {
    /// Intentionally redact the key so it never appears in logs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderCredentials")
            .field("api_key", &"[REDACTED]")
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl ProviderCredentials {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self { api_key: api_key.into(), base_url: None }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Load the API key from an environment variable.
    pub fn from_env(var: &str) -> crate::Result<Self> {
        let key = std::env::var(var)
            .map_err(|_| crate::FrameworkError::Config(format!("env var '{var}' not set")))?;
        Ok(Self::new(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_redacted_in_debug() {
        let creds = ProviderCredentials::new("sk-super-secret");
        let s = format!("{creds:?}");
        assert!(!s.contains("super-secret"), "API key must not appear in debug output");
        assert!(s.contains("REDACTED"));
    }

    #[test]
    fn agent_config_builder() {
        let cfg = AgentConfig::new("my-agent", ModelConfig::new("gpt-4o"))
            .with_system_prompt("You are a helpful assistant.")
            .with_max_iterations(5)
            .with_chain_of_thought();

        assert_eq!(cfg.name, "my-agent");
        assert_eq!(cfg.max_iterations, 5);
        assert!(cfg.enable_chain_of_thought);
        assert!(cfg.system_prompt.is_some());
    }
}
