//! Model provider abstraction and concrete implementations.
//!
//! All communication with LLM APIs flows through `ModelProvider`.  The trait
//! normalises request/response formats so the rest of the framework never
//! has to care which provider is in use.
//!
//! Concrete providers:
//! * [`openai`]     – OpenAI Chat Completions API (GPT-4o, o1, …)
//! * [`anthropic`]  – Anthropic Messages API  (Claude 3.x / 4.x)
//! * [`gemini`]     – Google Gemini API (Gemini 2.5 Flash/Pro)

use async_trait::async_trait;

use crate::message::{CompletionRequest, CompletionResponse, ResponseStream};

pub mod anthropic;
pub mod gemini;
pub mod openai;

pub use anthropic::AnthropicProvider;
pub use gemini::GeminiProvider;
pub use openai::OpenAiProvider;

// ─────────────────────────────────────────────────────────────────────────────
// CompletionRequest / CompletionResponse (re-export from message)
// ─────────────────────────────────────────────────────────────────────────────
// They live in `message` so that the message module is self-contained, but
// users typically import them via `crate::providers::*`.

// ─────────────────────────────────────────────────────────────────────────────
// ModelProvider trait
// ─────────────────────────────────────────────────────────────────────────────

/// Abstraction over an LLM provider's chat-completion endpoint.
///
/// # Object safety
/// Using `async_trait` means the returned futures are boxed, making this
/// trait object-safe (`Arc<dyn ModelProvider>`).  The heap allocation per
/// call is acceptable given the far larger network latency.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Perform a blocking (non-streaming) completion.
    async fn complete(&self, request: CompletionRequest) -> crate::Result<CompletionResponse>;

    /// Begin a streaming completion; yields `StreamChunk`s until `Done`.
    async fn stream(&self, request: CompletionRequest) -> crate::Result<ResponseStream>;

    /// Short identifier used in traces and error messages, e.g. `"openai"`.
    fn provider_name(&self) -> &str;
}
