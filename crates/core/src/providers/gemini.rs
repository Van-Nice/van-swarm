//! Google Gemini API provider (Gemini 2.5 Flash / Pro).
//!
//! Gemini differs from OpenAI/Anthropic in several ways:
//! * URL: `https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent`
//! * API key is passed as a query parameter (`?key=...`).
//! * Role for the assistant is `"model"` (not `"assistant"`).
//! * Tool format uses `functionDeclarations` inside a `tools` array.
//! * System instruction is a separate top-level field.

use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, instrument, warn};

use crate::{
    config::ProviderCredentials,
    message::{
        CompletionRequest, CompletionResponse, ContentBlock, Message, ResponseStream, Role,
        StopReason, StreamChunk, TokenUsage, ToolDefinition,
    },
};

use super::ModelProvider;

// ─────────────────────────────────────────────────────────────────────────────
// Provider struct
// ─────────────────────────────────────────────────────────────────────────────

pub struct GeminiProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl GeminiProvider {
    const DEFAULT_BASE_URL: &'static str =
        "https://generativelanguage.googleapis.com/v1beta/models";

    pub fn new(creds: ProviderCredentials) -> Self {
        let base_url = creds
            .base_url
            .unwrap_or_else(|| Self::DEFAULT_BASE_URL.to_string());
        Self { client: reqwest::Client::new(), api_key: creds.api_key, base_url }
    }

    pub fn from_env() -> crate::Result<Self> {
        let creds = ProviderCredentials::from_env("GEMINI_API_KEY")?;
        Ok(Self::new(creds))
    }

    fn generate_url(&self, model: &str, stream: bool) -> String {
        let action = if stream { "streamGenerateContent" } else { "generateContent" };
        format!("{}/{}:{}?key={}", self.base_url, model, action, self.api_key)
    }
}

impl std::fmt::Debug for GeminiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeminiProvider")
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Wire types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct GemRequest {
    contents: Vec<GemContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GemSystemInstruction>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<GemTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GemGenerationConfig>,
}

#[derive(Serialize, Deserialize, Clone)]
struct GemContent {
    role: String,
    parts: Vec<GemPart>,
}

#[derive(Serialize, Deserialize, Clone)]
struct GemSystemInstruction {
    parts: Vec<GemPart>,
}

/// Gemini uses a flat object with one populated field rather than a tagged
/// enum, so we represent it as a struct with optional fields and use custom
/// serialisation via `serde_json::Value` at call sites.
#[derive(Clone, Debug)]
enum GemPart {
    Text(String),
    FunctionCall { name: String, args: serde_json::Value },
    FunctionResponse { name: String, response: GemFunctionResponse },
}

impl Serialize for GemPart {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            GemPart::Text(t) => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("text", t)?;
                m.end()
            }
            GemPart::FunctionCall { name, args } => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("functionCall", &serde_json::json!({ "name": name, "args": args }))?;
                m.end()
            }
            GemPart::FunctionResponse { name, response } => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("functionResponse", &serde_json::json!({ "name": name, "response": response }))?;
                m.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for GemPart {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        if let Some(t) = v.get("text").and_then(|t| t.as_str()) {
            return Ok(GemPart::Text(t.to_string()));
        }
        if let Some(fc) = v.get("functionCall") {
            let name = fc["name"].as_str().unwrap_or("").to_string();
            let args = fc["args"].clone();
            return Ok(GemPart::FunctionCall { name, args });
        }
        if let Some(fr) = v.get("functionResponse") {
            let name = fr["name"].as_str().unwrap_or("").to_string();
            let content = fr["response"].clone();
            return Ok(GemPart::FunctionResponse {
                name,
                response: GemFunctionResponse { content },
            });
        }
        Ok(GemPart::Text(String::new()))
    }
}


#[derive(Debug, Serialize, Deserialize, Clone)]
struct GemFunctionResponse {
    content: serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GemTool {
    function_declarations: Vec<GemFunctionDecl>,
}

#[derive(Serialize)]
struct GemFunctionDecl {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GemGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

// Response

#[derive(Deserialize)]
struct GemResponse {
    candidates: Vec<GemCandidate>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GemUsageMetadata>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GemCandidate {
    content: GemContent,
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct GemUsageMetadata {
    #[serde(default)]
    prompt_token_count: u32,
    #[serde(default)]
    candidates_token_count: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Conversion helpers
// ─────────────────────────────────────────────────────────────────────────────

fn messages_to_gemini(messages: &[Message]) -> (Option<GemSystemInstruction>, Vec<GemContent>) {
    let mut system: Option<GemSystemInstruction> = None;
    let mut contents = Vec::new();

    for msg in messages {
        match msg.role {
            Role::System => {
                let text = msg.text_content();
                system = Some(GemSystemInstruction {
                    parts: vec![GemPart::Text(text)],
                });
            }
            Role::User => {
                // Check for tool results.
                let has_tool_results =
                    msg.content.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }));

                if has_tool_results {
                    let parts: Vec<GemPart> = msg
                        .content
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::ToolResult { tool_use_id: _, content, .. } => {
                                // Gemini FunctionResponse needs the function name;
                                // we store it in the tool_use_id as a workaround.
                                // A proper solution uses a lookup map.
                                Some(GemPart::FunctionResponse {
                                    name: "unknown".into(),
                                    response: GemFunctionResponse {
                                        content: serde_json::json!({ "result": content }),
                                    },
                                })
                            }
                            _ => None,
                        })
                        .collect();
                    contents.push(GemContent { role: "user".into(), parts });
                } else {
                    let text = msg.text_content();
                    contents.push(GemContent {
                        role: "user".into(),
                        parts: vec![GemPart::Text(text)],
                    });
                }
            }
            Role::Assistant => {
                let parts: Vec<GemPart> = msg
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } if !text.is_empty() => {
                            Some(GemPart::Text(text.clone()))
                        }
                        ContentBlock::ToolUse { id: _, name, input } => {
                            Some(GemPart::FunctionCall {
                                name: name.clone(),
                                args: input.clone(),
                            })
                        }
                        _ => None,
                    })
                    .collect();
                if !parts.is_empty() {
                    contents.push(GemContent { role: "model".into(), parts });
                }
            }
            Role::Tool => {}
        }
    }

    (system, contents)
}

fn tools_to_gemini(tools: &[ToolDefinition]) -> Vec<GemTool> {
    if tools.is_empty() {
        return Vec::new();
    }
    vec![GemTool {
        function_declarations: tools
            .iter()
            .map(|t| GemFunctionDecl {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect(),
    }]
}

fn gemini_stop_reason(r: Option<&str>) -> StopReason {
    match r {
        Some("STOP") => StopReason::EndTurn,
        Some("MAX_TOKENS") => StopReason::MaxTokens,
        Some("SAFETY") => StopReason::ContentFilter,
        Some(other) => StopReason::Other(other.to_string()),
        None => StopReason::EndTurn,
    }
}

fn gemini_content_to_framework(content: GemContent) -> Message {
    let blocks: Vec<ContentBlock> = content
        .parts
        .into_iter()
        .filter_map(|part| match part {
            GemPart::Text(text) if !text.is_empty() => Some(ContentBlock::Text { text }),
            GemPart::FunctionCall { name, args } => Some(ContentBlock::ToolUse {
                id: crate::message::new_tool_call_id(),
                name,
                input: args,
            }),
            _ => None,
        })
        .collect();

    let blocks = if blocks.is_empty() {
        vec![ContentBlock::text(String::new())]
    } else {
        blocks
    };

    Message { role: Role::Assistant, content: blocks }
}

// ─────────────────────────────────────────────────────────────────────────────
// ModelProvider impl
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl ModelProvider for GeminiProvider {
    #[instrument(skip(self, request), fields(provider = "gemini", model = %request.model))]
    async fn complete(&self, request: CompletionRequest) -> crate::Result<CompletionResponse> {
        let (system_instruction, contents) = messages_to_gemini(&request.messages);
        let tools = tools_to_gemini(&request.tools);

        let body = GemRequest {
            contents,
            system_instruction,
            tools,
            generation_config: Some(GemGenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            }),
        };

        debug!(model = %request.model, "Sending completion request to Gemini");

        let resp = self
            .client
            .post(&self.generate_url(&request.model, false))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                crate::FrameworkError::provider_with_source("gemini", e.to_string(), e)
            })?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            return Err(crate::FrameworkError::AuthenticationFailed("gemini".into()));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(crate::FrameworkError::RateLimitExceeded("gemini".into()));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::FrameworkError::provider(
                "gemini",
                format!("HTTP {status}: {body}"),
            ));
        }

        let gem_resp: GemResponse = resp.json().await.map_err(|e| {
            crate::FrameworkError::InvalidResponse {
                provider: "gemini".into(),
                message: e.to_string(),
            }
        })?;

        let candidate = gem_resp.candidates.into_iter().next().ok_or_else(|| {
            crate::FrameworkError::InvalidResponse {
                provider: "gemini".into(),
                message: "empty candidates array".into(),
            }
        })?;

        let stop_reason = gemini_stop_reason(candidate.finish_reason.as_deref());
        let message = gemini_content_to_framework(candidate.content);
        let usage = gem_resp
            .usage_metadata
            .map(|u| TokenUsage {
                input_tokens: u.prompt_token_count,
                output_tokens: u.candidates_token_count,
                ..Default::default()
            })
            .unwrap_or_default();

        // Gemini doesn't return a response ID; fabricate one.
        let id = format!("gemini-{}", uuid::Uuid::new_v4().simple());

        Ok(CompletionResponse { id, message, stop_reason, usage })
    }

    #[instrument(skip(self, request), fields(provider = "gemini", model = %request.model))]
    async fn stream(&self, request: CompletionRequest) -> crate::Result<ResponseStream> {
        // Gemini streaming returns newline-delimited JSON objects (not SSE).
        let (system_instruction, contents) = messages_to_gemini(&request.messages);
        let tools = tools_to_gemini(&request.tools);

        let body = GemRequest {
            contents,
            system_instruction,
            tools,
            generation_config: Some(GemGenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            }),
        };

        let resp = self
            .client
            .post(&self.generate_url(&request.model, true))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                crate::FrameworkError::provider_with_source("gemini", e.to_string(), e)
            })?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::FrameworkError::provider(
                "gemini",
                format!("HTTP {status}: {body}"),
            ));
        }

        let (tx, rx) = mpsc::channel::<crate::Result<StreamChunk>>(64);

        tokio::spawn(async move {
            let mut byte_stream = resp.bytes_stream();
            let mut buf = String::new();

            while let Some(chunk_result) = byte_stream.next().await {
                match chunk_result {
                    Err(e) => {
                        let _ = tx
                            .send(Err(crate::FrameworkError::provider_with_source(
                                "gemini",
                                "stream error",
                                e,
                            )))
                            .await;
                        return;
                    }
                    Ok(bytes) => {
                        buf.push_str(&String::from_utf8_lossy(&bytes));
                    }
                }

                // Gemini streaming returns a JSON array piecemeal.
                // Attempt to parse complete objects line by line.
                while let Some(nl) = buf.find('\n') {
                    let line = buf[..nl].trim().to_string();
                    buf = buf[nl + 1..].to_string();

                    // Skip array delimiters and empty lines.
                    let line = line.trim_matches(|c| c == '[' || c == ',' || c == ']');
                    if line.is_empty() {
                        continue;
                    }

                    let chunk: GemResponse = match serde_json::from_str(line) {
                        Ok(c) => c,
                        Err(e) => {
                            warn!("Failed to parse Gemini stream chunk: {e}");
                            continue;
                        }
                    };

                    for candidate in chunk.candidates {
                        for part in candidate.content.parts {
                            match part {
                                GemPart::Text(text) if !text.is_empty() => {
                                    let _ = tx.send(Ok(StreamChunk::Text(text))).await;
                                }
                                GemPart::FunctionCall { name, args } => {
                                    let id = crate::message::new_tool_call_id();
                                    let _ = tx
                                        .send(Ok(StreamChunk::ToolCallComplete {
                                            id,
                                            name,
                                            arguments: args,
                                        }))
                                        .await;
                                }
                                _ => {}
                            }
                        }

                        if let Some(u) = chunk.usage_metadata.as_ref() {
                            let _ = tx
                                .send(Ok(StreamChunk::Usage(TokenUsage {
                                    input_tokens: u.prompt_token_count,
                                    output_tokens: u.candidates_token_count,
                                    ..Default::default()
                                })))
                                .await;
                        }
                    }
                }
            }

            let _ = tx.send(Ok(StreamChunk::Done)).await;
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    fn provider_name(&self) -> &str {
        "gemini"
    }
}
