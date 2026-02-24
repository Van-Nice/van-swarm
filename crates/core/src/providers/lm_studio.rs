//! LM Studio local inference provider.
//!
//! LM Studio exposes an **OpenAI-compatible** API at `http://127.0.0.1:1234/v1`
//! (see [LM Studio OpenAI Compatibility](https://lmstudio.ai/docs/api/openai-api)):
//!
//! * **Chat:** `POST /v1/chat/completions` — same request/response as OpenAI.
//! * **Tools:** Supported via the same tool_calls schema as OpenAI.
//!
//! The native LM Studio API also offers `GET /api/v1/models`, `POST /api/v1/chat`,
//! `POST /api/v1/models/load`, etc.; this provider uses the OpenAI-compatible
//! endpoint only so the rest of the framework works unchanged.
//!
//! No API key required; LM Studio accepts an optional placeholder (e.g. `lm-studio`).

use async_trait::async_trait;

use crate::config::ProviderCredentials;
use crate::message::{CompletionRequest, CompletionResponse, ResponseStream};

use super::{ModelProvider, OpenAiProvider};

// ─────────────────────────────────────────────────────────────────────────────
// LM Studio default (OpenAI-compatible endpoint)
// ─────────────────────────────────────────────────────────────────────────────

/// Default base URL for LM Studio's OpenAI-compatible API.
pub const LM_STUDIO_DEFAULT_BASE_URL: &str = "http://127.0.0.1:1234/v1";

/// Placeholder API key; LM Studio typically ignores it for local requests.
const LM_STUDIO_API_KEY_PLACEHOLDER: &str = "lm-studio";

// ─────────────────────────────────────────────────────────────────────────────
// Provider struct
// ─────────────────────────────────────────────────────────────────────────────

/// Local model provider for [LM Studio](https://lmstudio.ai).
///
/// Uses the OpenAI-compatible HTTP API (`/v1/chat/completions`, tool_calls).
/// Default base URL: `http://127.0.0.1:1234/v1`. No real API key required.
pub struct LmStudioProvider {
    inner: OpenAiProvider,
    base_url: String,
}

impl LmStudioProvider {
    /// Base URL for the OpenAI-compatible endpoint (e.g. `http://127.0.0.1:1234/v1`).
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Build from an explicit base URL (OpenAI-compatible path).
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        let creds = ProviderCredentials::new(LM_STUDIO_API_KEY_PLACEHOLDER).with_base_url(&base_url);
        let inner = OpenAiProvider::new(creds);
        Self { inner, base_url }
    }

    /// Build from environment or defaults.
    ///
    /// * `LM_STUDIO_BASE_URL` — optional; default `http://127.0.0.1:1234/v1`.
    pub fn from_env_or_default() -> Self {
        let base_url = std::env::var("LM_STUDIO_BASE_URL")
            .unwrap_or_else(|_| LM_STUDIO_DEFAULT_BASE_URL.to_string());
        Self::new(base_url)
    }
}

impl std::fmt::Debug for LmStudioProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LmStudioProvider")
            .field("base_url", &self.base_url)
            .finish()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ModelProvider impl (delegate to OpenAI-compatible inner)
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl ModelProvider for LmStudioProvider {
    async fn complete(&self, request: CompletionRequest) -> crate::Result<CompletionResponse> {
        self.inner.complete(request).await
    }

    async fn stream(&self, request: CompletionRequest) -> crate::Result<ResponseStream> {
        self.inner.stream(request).await
    }

    fn provider_name(&self) -> &str {
        "lm-studio"
    }
}
