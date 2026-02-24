//! OpenAI Chat Completions API provider.
//!
//! Handles both regular and streaming completions.  Tool-calling format
//! translates between the framework's canonical `ContentBlock` and OpenAI's
//! `tool_calls` / `tool` message schema.

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

pub struct OpenAiProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl OpenAiProvider {
    const DEFAULT_BASE_URL: &'static str = "https://api.openai.com/v1";

    pub fn new(creds: ProviderCredentials) -> Self {
        let base_url = creds
            .base_url
            .unwrap_or_else(|| Self::DEFAULT_BASE_URL.to_string());

        Self {
            client: reqwest::Client::new(),
            api_key: creds.api_key,
            base_url,
        }
    }

    /// Convenience: load credentials from `OPENAI_API_KEY` env var.
    pub fn from_env() -> crate::Result<Self> {
        let creds = ProviderCredentials::from_env("OPENAI_API_KEY")?;
        Ok(Self::new(creds))
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
}

impl std::fmt::Debug for OpenAiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiProvider")
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Wire types (private – never exposed outside this module)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct OaiRequest {
    model: String,
    messages: Vec<OaiMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OaiTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<OaiStreamOptions>,
}

#[derive(Serialize)]
struct OaiStreamOptions {
    include_usage: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct OaiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OaiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct OaiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OaiFunction,
}

#[derive(Serialize, Deserialize, Clone)]
struct OaiFunction {
    name: String,
    /// Arguments as a JSON string (OpenAI encodes them as string-inside-JSON).
    arguments: String,
}

#[derive(Serialize)]
struct OaiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OaiFunctionDef,
}

#[derive(Serialize)]
struct OaiFunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
    strict: bool,
}

// Response

#[derive(Deserialize)]
struct OaiResponse {
    id: String,
    choices: Vec<OaiChoice>,
    usage: Option<OaiUsage>,
}

#[derive(Deserialize)]
struct OaiChoice {
    message: OaiMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct OaiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
}

// Streaming chunk

#[derive(Deserialize)]
struct OaiChunk {
    #[allow(dead_code)]
    id: String,
    choices: Vec<OaiChunkChoice>,
    usage: Option<OaiUsage>,
}

#[derive(Deserialize)]
struct OaiChunkChoice {
    delta: OaiDelta,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct OaiDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OaiToolCallDelta>>,
}

#[derive(Deserialize)]
struct OaiToolCallDelta {
    index: usize,
    id: Option<String>,
    function: Option<OaiFunctionDelta>,
}

#[derive(Deserialize)]
struct OaiFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Conversion helpers
// ─────────────────────────────────────────────────────────────────────────────

fn messages_to_oai(messages: &[Message]) -> Vec<OaiMessage> {
    let mut out = Vec::with_capacity(messages.len());

    for msg in messages {
        match msg.role {
            Role::System => {
                let text = msg.text_content();
                out.push(OaiMessage {
                    role: "system".into(),
                    content: Some(serde_json::Value::String(text)),
                    ..Default::default()
                });
            }
            Role::User => {
                let text = msg.text_content();
                out.push(OaiMessage {
                    role: "user".into(),
                    content: Some(serde_json::Value::String(text)),
                    ..Default::default()
                });
            }
            Role::Assistant => {
                // An assistant message may have text AND tool calls.
                let text = msg.text_content();
                let tool_calls: Vec<OaiToolCall> = msg
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::ToolUse { id, name, input } => Some(OaiToolCall {
                            id: id.clone(),
                            call_type: "function".into(),
                            function: OaiFunction {
                                name: name.clone(),
                                // Re-serialise parsed args to a JSON string.
                                arguments: serde_json::to_string(input)
                                    .unwrap_or_else(|_| "{}".into()),
                            },
                        }),
                        _ => None,
                    })
                    .collect();

                let content = if text.is_empty() { None } else { Some(serde_json::json!(text)) };
                let tool_calls_opt = if tool_calls.is_empty() { None } else { Some(tool_calls) };

                out.push(OaiMessage {
                    role: "assistant".into(),
                    content,
                    tool_calls: tool_calls_opt,
                    ..Default::default()
                });
            }
            Role::Tool => {
                // Each tool result becomes a separate "tool" role message.
                for block in &msg.content {
                    if let ContentBlock::ToolResult { tool_use_id, content, .. } = block {
                        out.push(OaiMessage {
                            role: "tool".into(),
                            content: Some(serde_json::json!(content)),
                            tool_call_id: Some(tool_use_id.clone()),
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }

    out
}

fn tools_to_oai(tools: &[ToolDefinition]) -> Vec<OaiTool> {
    tools
        .iter()
        .map(|t| OaiTool {
            tool_type: "function".into(),
            function: OaiFunctionDef {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
                strict: true,
            },
        })
        .collect()
}

fn finish_reason_to_stop(fr: Option<&str>) -> StopReason {
    match fr {
        Some("stop") => StopReason::EndTurn,
        Some("tool_calls") => StopReason::ToolUse,
        Some("length") => StopReason::MaxTokens,
        Some("content_filter") => StopReason::ContentFilter,
        Some(other) => StopReason::Other(other.to_string()),
        None => StopReason::EndTurn,
    }
}

fn oai_message_to_framework(msg: &OaiMessage, finish_reason: Option<&str>) -> Message {
    let mut blocks = Vec::new();

    // Text content
    if let Some(content) = &msg.content {
        let text = match content {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if !text.is_empty() {
            blocks.push(ContentBlock::text(text));
        }
    }

    // Tool calls
    if let Some(calls) = &msg.tool_calls {
        for call in calls {
            // Arguments come back as a JSON string; parse them.
            let input = serde_json::from_str::<serde_json::Value>(&call.function.arguments)
                .unwrap_or_else(|_| {
                    warn!(
                        tool = call.function.name,
                        raw = call.function.arguments,
                        "Failed to parse tool arguments as JSON"
                    );
                    serde_json::json!({})
                });

            blocks.push(ContentBlock::tool_use(call.id.clone(), call.function.name.clone(), input));
        }
    }

    if blocks.is_empty() {
        blocks.push(ContentBlock::text(String::new()));
    }

    let _ = finish_reason; // used by caller for stop_reason
    Message { role: Role::Assistant, content: blocks }
}

// ─────────────────────────────────────────────────────────────────────────────
// SSE parser helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a single SSE line and return the data payload if it has `data: ` prefix.
fn parse_sse_line(line: &str) -> Option<&str> {
    line.strip_prefix("data: ")
}

// ─────────────────────────────────────────────────────────────────────────────
// ModelProvider impl
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl ModelProvider for OpenAiProvider {
    #[instrument(skip(self, request), fields(provider = "openai", model = %request.model))]
    async fn complete(&self, request: CompletionRequest) -> crate::Result<CompletionResponse> {
        let oai_req = OaiRequest {
            model: request.model.clone(),
            messages: messages_to_oai(&request.messages),
            tools: tools_to_oai(&request.tools),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: false,
            stream_options: None,
        };

        debug!(model = %request.model, "Sending completion request to OpenAI");

        let resp = self
            .client
            .post(&self.chat_url())
            .bearer_auth(&self.api_key)
            .json(&oai_req)
            .send()
            .await
            .map_err(|e| crate::FrameworkError::provider_with_source("openai", e.to_string(), e))?;

        let status = resp.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(crate::FrameworkError::AuthenticationFailed("openai".into()));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(crate::FrameworkError::RateLimitExceeded("openai".into()));
        }

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::FrameworkError::provider(
                "openai",
                format!("HTTP {status}: {body}"),
            ));
        }

        let body: OaiResponse = resp.json().await.map_err(|e| {
            crate::FrameworkError::InvalidResponse {
                provider: "openai".into(),
                message: e.to_string(),
            }
        })?;

        let choice = body.choices.into_iter().next().ok_or_else(|| {
            crate::FrameworkError::InvalidResponse {
                provider: "openai".into(),
                message: "empty choices array".into(),
            }
        })?;

        let stop_reason = finish_reason_to_stop(choice.finish_reason.as_deref());
        let message = oai_message_to_framework(&choice.message, choice.finish_reason.as_deref());

        let usage = body
            .usage
            .map(|u| TokenUsage {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                ..Default::default()
            })
            .unwrap_or_default();

        Ok(CompletionResponse { id: body.id, message, stop_reason, usage })
    }

    #[instrument(skip(self, request), fields(provider = "openai", model = %request.model))]
    async fn stream(&self, request: CompletionRequest) -> crate::Result<ResponseStream> {
        let oai_req = OaiRequest {
            model: request.model.clone(),
            messages: messages_to_oai(&request.messages),
            tools: tools_to_oai(&request.tools),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: true,
            stream_options: Some(OaiStreamOptions { include_usage: true }),
        };

        let resp = self
            .client
            .post(&self.chat_url())
            .bearer_auth(&self.api_key)
            .json(&oai_req)
            .send()
            .await
            .map_err(|e| crate::FrameworkError::provider_with_source("openai", e.to_string(), e))?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(crate::FrameworkError::AuthenticationFailed("openai".into()));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(crate::FrameworkError::RateLimitExceeded("openai".into()));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::FrameworkError::provider("openai", format!("HTTP {status}: {body}")));
        }

        // Spawn a task that processes the SSE byte stream and sends
        // StreamChunks over an mpsc channel.  This decouples the stream
        // consumer from the HTTP layer's lifetime constraints.
        let (tx, rx) = mpsc::channel::<crate::Result<StreamChunk>>(64);

        tokio::spawn(async move {
            let mut byte_stream = resp.bytes_stream();
            let mut buf = String::new();

            // State for assembling tool-call deltas.
            // Key: index → (id, name, accumulated_args)
            let mut tool_builders: std::collections::HashMap<
                usize,
                (Option<String>, Option<String>, String),
            > = std::collections::HashMap::new();

            while let Some(chunk_result) = byte_stream.next().await {
                match chunk_result {
                    Err(e) => {
                        let _ = tx
                            .send(Err(crate::FrameworkError::provider_with_source(
                                "openai",
                                "stream read error",
                                e,
                            )))
                            .await;
                        return;
                    }
                    Ok(bytes) => {
                        // Bytes may contain partial SSE events; buffer them.
                        buf.push_str(&String::from_utf8_lossy(&bytes));
                    }
                }

                // Process complete events (separated by "\n\n").
                while let Some(pos) = buf.find("\n\n") {
                    let event = buf[..pos].to_string();
                    buf = buf[pos + 2..].to_string();

                    for line in event.lines() {
                        let Some(data) = parse_sse_line(line) else { continue };

                        if data == "[DONE]" {
                            // Emit completed tool calls before Done.
                            for (index, (id, name, args)) in tool_builders.drain() {
                                let id = id.unwrap_or_else(|| format!("call_{index}"));
                                let name = name.unwrap_or_default();
                                let arguments =
                                    serde_json::from_str(&args).unwrap_or(serde_json::json!({}));
                                let _ = tx
                                    .send(Ok(StreamChunk::ToolCallComplete {
                                        id,
                                        name,
                                        arguments,
                                    }))
                                    .await;
                            }
                            let _ = tx.send(Ok(StreamChunk::Done)).await;
                            return;
                        }

                        let chunk: OaiChunk = match serde_json::from_str(data) {
                            Ok(c) => c,
                            Err(e) => {
                                warn!("Failed to parse SSE chunk: {e}");
                                continue;
                            }
                        };

                        for choice in &chunk.choices {
                            // Text delta
                            if let Some(text) = &choice.delta.content {
                                if !text.is_empty() {
                                    let _ = tx.send(Ok(StreamChunk::Text(text.clone()))).await;
                                }
                            }

                            // Tool call deltas
                            if let Some(tc_deltas) = &choice.delta.tool_calls {
                                for delta in tc_deltas {
                                    let entry =
                                        tool_builders.entry(delta.index).or_insert_with(|| {
                                            (None, None, String::new())
                                        });
                                    if let Some(id) = &delta.id {
                                        entry.0 = Some(id.clone());
                                    }
                                    if let Some(func) = &delta.function {
                                        if let Some(name) = &func.name {
                                            entry.1 = Some(name.clone());
                                        }
                                        if let Some(args) = &func.arguments {
                                            entry.2.push_str(args);
                                            let _ = tx
                                                .send(Ok(StreamChunk::ToolCallDelta {
                                                    id: entry.0.clone(),
                                                    index: delta.index,
                                                    name: entry.1.clone(),
                                                    arguments_delta: args.clone(),
                                                }))
                                                .await;
                                        }
                                    }
                                }
                            }

                            // Usage (injected when stream_options.include_usage = true)
                            if let Some(usage) = &chunk.usage {
                                let _ = tx
                                    .send(Ok(StreamChunk::Usage(TokenUsage {
                                        input_tokens: usage.prompt_tokens,
                                        output_tokens: usage.completion_tokens,
                                        ..Default::default()
                                    })))
                                    .await;
                            }
                        }
                    }
                }
            }

            // Stream ended without [DONE] – emit any buffered tool calls and Done.
            for (index, (id, name, args)) in tool_builders.drain() {
                let id = id.unwrap_or_else(|| format!("call_{index}"));
                let name = name.unwrap_or_default();
                let arguments = serde_json::from_str(&args).unwrap_or(serde_json::json!({}));
                let _ =
                    tx.send(Ok(StreamChunk::ToolCallComplete { id, name, arguments })).await;
            }
            let _ = tx.send(Ok(StreamChunk::Done)).await;
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    fn provider_name(&self) -> &str {
        "openai"
    }
}
