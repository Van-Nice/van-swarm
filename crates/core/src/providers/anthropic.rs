//! Anthropic Messages API provider (Claude 3.x / 4.x).
//!
//! Key differences from OpenAI:
//! * System prompt is a top-level field, not a message.
//! * Tool results live inside the `user` role as `tool_result` content blocks.
//! * Streaming uses named event types (`content_block_delta`, etc.).
//! * Required header: `anthropic-version: 2023-06-01`.

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
        StopReason, StreamChunk, TokenUsage,
    },
};

use super::ModelProvider;

// ─────────────────────────────────────────────────────────────────────────────
// Provider struct
// ─────────────────────────────────────────────────────────────────────────────

pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    const DEFAULT_BASE_URL: &'static str = "https://api.anthropic.com/v1";
    const ANTHROPIC_VERSION: &'static str = "2023-06-01";

    pub fn new(creds: ProviderCredentials) -> Self {
        let base_url = creds
            .base_url
            .unwrap_or_else(|| Self::DEFAULT_BASE_URL.to_string());
        Self { client: reqwest::Client::new(), api_key: creds.api_key, base_url }
    }

    pub fn from_env() -> crate::Result<Self> {
        let creds = ProviderCredentials::from_env("ANTHROPIC_API_KEY")?;
        Ok(Self::new(creds))
    }

    fn messages_url(&self) -> String {
        format!("{}/messages", self.base_url)
    }
}

impl std::fmt::Debug for AnthropicProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicProvider")
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Wire types
// ─────────────────────────────────────────────────────────────────────────────

/// Cache-control marker for Anthropic prompt caching (§20.2).
#[derive(Serialize)]
struct AntCacheControl {
    r#type: &'static str,
}

/// A single block in the Anthropic system-prompt field (needed for cache_control).
#[derive(Serialize)]
struct AntSystemBlock {
    r#type: &'static str,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<AntCacheControl>,
}

/// Anthropic `system` field: either a plain string or a list of typed blocks.
///
/// Plain string is used when caching is off (simpler wire format).
/// Block list is required when `cache_control` must be attached.
#[derive(Serialize)]
#[serde(untagged)]
enum AntSystemField {
    Plain(String),
    Blocks(Vec<AntSystemBlock>),
}

#[derive(Serialize)]
struct AntRequest<'a> {
    model: &'a str,
    messages: Vec<AntMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<AntSystemField>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AntTool<'a>>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct AntMessage {
    role: String,
    content: Vec<AntContentBlock>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AntContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

#[derive(Serialize)]
struct AntTool<'a> {
    name: &'a str,
    description: &'a str,
    input_schema: &'a serde_json::Value,
}

// Response

#[derive(Deserialize)]
struct AntResponse {
    id: String,
    content: Vec<AntContentBlock>,
    stop_reason: Option<String>,
    usage: Option<AntUsage>,
}

#[derive(Deserialize, Default)]
struct AntUsage {
    input_tokens: u32,
    output_tokens: u32,
    #[serde(default)]
    cache_read_input_tokens: u32,
    #[serde(default)]
    cache_creation_input_tokens: u32,
}

// Streaming events

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AntStreamEvent {
    MessageStart {
        #[allow(dead_code)]
        message: AntMessageStartData,
    },
    ContentBlockStart {
        index: usize,
        content_block: AntContentBlockStart,
    },
    ContentBlockDelta {
        index: usize,
        delta: AntDelta,
    },
    ContentBlockStop {
        #[allow(dead_code)]
        index: usize,
    },
    MessageDelta {
        #[allow(dead_code)]
        delta: AntMessageDelta,
        usage: Option<AntUsageDelta>,
    },
    MessageStop,
    Ping,
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
struct AntMessageStartData {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    usage: Option<AntUsage>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AntContentBlockStart {
    Text {
        #[allow(dead_code)]
        text: String,
    },
    ToolUse { id: String, name: String },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AntDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
}

#[derive(Deserialize)]
struct AntMessageDelta {
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct AntUsageDelta {
    output_tokens: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Conversion helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Split the framework messages into (system_field, ant_messages).
///
/// Anthropic does not accept a system message in the `messages` array;
/// it must be passed as a top-level `system` field.
///
/// When `cache` is `true`, the system text is emitted as a content-block array
/// with `cache_control: {"type": "ephemeral"}` on the final block, enabling
/// Anthropic prompt caching (§20.2).  The beta header must also be set by the caller.
fn split_system(messages: &[Message], cache: bool) -> (Option<AntSystemField>, Vec<AntMessage>) {
    let mut system_text: Option<String> = None;
    let mut out = Vec::new();

    for msg in messages {
        match msg.role {
            Role::System => {
                // Concatenate all system messages (there should only be one).
                let text = msg.text_content();
                match &mut system_text {
                    None => system_text = Some(text),
                    Some(s) => {
                        s.push('\n');
                        s.push_str(&text);
                    }
                }
            }
            Role::User => {
                out.push(AntMessage {
                    role: "user".into(),
                    content: vec![AntContentBlock::Text { text: msg.text_content() }],
                });
            }
            Role::Assistant => {
                let blocks: Vec<AntContentBlock> = msg
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } if !text.is_empty() => {
                            Some(AntContentBlock::Text { text: text.clone() })
                        }
                        ContentBlock::ToolUse { id, name, input } => {
                            Some(AntContentBlock::ToolUse {
                                id: id.clone(),
                                name: name.clone(),
                                input: input.clone(),
                            })
                        }
                        _ => None,
                    })
                    .collect();
                if !blocks.is_empty() {
                    out.push(AntMessage { role: "assistant".into(), content: blocks });
                }
            }
            Role::Tool => {
                // Tool results go in a "user" turn in Anthropic's format.
                let blocks: Vec<AntContentBlock> = msg
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                            Some(AntContentBlock::ToolResult {
                                tool_use_id: tool_use_id.clone(),
                                content: content.clone(),
                                is_error: *is_error,
                            })
                        }
                        _ => None,
                    })
                    .collect();
                if !blocks.is_empty() {
                    out.push(AntMessage { role: "user".into(), content: blocks });
                }
            }
        }
    }

    // Convert the collected system text into the appropriate wire format.
    let system = system_text.map(|text| {
        if cache {
            // Emit as a content-block array so cache_control can be attached.
            AntSystemField::Blocks(vec![AntSystemBlock {
                r#type: "text",
                text,
                cache_control: Some(AntCacheControl { r#type: "ephemeral" }),
            }])
        } else {
            AntSystemField::Plain(text)
        }
    });

    (system, out)
}

fn ant_stop_reason(r: Option<&str>) -> StopReason {
    match r {
        Some("end_turn") => StopReason::EndTurn,
        Some("tool_use") => StopReason::ToolUse,
        Some("max_tokens") => StopReason::MaxTokens,
        Some("stop_sequence") => StopReason::StopSequence,
        Some(other) => StopReason::Other(other.to_string()),
        None => StopReason::EndTurn,
    }
}

fn ant_blocks_to_framework(blocks: Vec<AntContentBlock>) -> Message {
    let content: Vec<ContentBlock> = blocks
        .into_iter()
        .filter_map(|b| match b {
            AntContentBlock::Text { text } if !text.is_empty() => {
                Some(ContentBlock::Text { text })
            }
            AntContentBlock::ToolUse { id, name, input } => {
                Some(ContentBlock::ToolUse { id, name, input })
            }
            _ => None,
        })
        .collect();

    let content = if content.is_empty() {
        vec![ContentBlock::text(String::new())]
    } else {
        content
    };

    Message { role: Role::Assistant, content }
}

// ─────────────────────────────────────────────────────────────────────────────
// ModelProvider impl
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl ModelProvider for AnthropicProvider {
    #[instrument(skip(self, request), fields(provider = "anthropic", model = %request.model))]
    async fn complete(&self, request: CompletionRequest) -> crate::Result<CompletionResponse> {
        let (system, messages) = split_system(&request.messages, request.cache_system_prompt);

        let max_tokens = request.max_tokens.unwrap_or(4096);

        let tools: Vec<AntTool<'_>> = request
            .tools
            .iter()
            .map(|t| AntTool {
                name: &t.name,
                description: &t.description,
                input_schema: &t.parameters,
            })
            .collect();

        let body = AntRequest {
            model: &request.model,
            messages,
            system,
            tools,
            max_tokens,
            temperature: request.temperature,
            stream: false,
        };

        debug!(model = %request.model, "Sending completion request to Anthropic");

        let mut req_builder = self
            .client
            .post(&self.messages_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", Self::ANTHROPIC_VERSION)
            .header("content-type", "application/json");

        // Prompt caching requires the beta header (§20.2).
        if request.cache_system_prompt {
            req_builder =
                req_builder.header("anthropic-beta", "prompt-caching-2024-07-31");
        }

        let resp = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                crate::FrameworkError::provider_with_source("anthropic", e.to_string(), e)
            })?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(crate::FrameworkError::AuthenticationFailed("anthropic".into()));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(crate::FrameworkError::RateLimitExceeded("anthropic".into()));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::FrameworkError::provider(
                "anthropic",
                format!("HTTP {status}: {body}"),
            ));
        }

        let ant_resp: AntResponse = resp.json().await.map_err(|e| {
            crate::FrameworkError::InvalidResponse {
                provider: "anthropic".into(),
                message: e.to_string(),
            }
        })?;

        let stop_reason = ant_stop_reason(ant_resp.stop_reason.as_deref());
        let message = ant_blocks_to_framework(ant_resp.content);
        let usage = ant_resp
            .usage
            .map(|u| TokenUsage {
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
                cache_read_tokens: u.cache_read_input_tokens,
                cache_creation_tokens: u.cache_creation_input_tokens,
            })
            .unwrap_or_default();

        Ok(CompletionResponse { id: ant_resp.id, message, stop_reason, usage })
    }

    #[instrument(skip(self, request), fields(provider = "anthropic", model = %request.model))]
    async fn stream(&self, request: CompletionRequest) -> crate::Result<ResponseStream> {
        let (system, messages) = split_system(&request.messages, request.cache_system_prompt);
        let max_tokens = request.max_tokens.unwrap_or(4096);

        let tools: Vec<AntTool<'_>> = request
            .tools
            .iter()
            .map(|t| AntTool {
                name: &t.name,
                description: &t.description,
                input_schema: &t.parameters,
            })
            .collect();

        let body = AntRequest {
            model: &request.model,
            messages,
            system,
            tools,
            max_tokens,
            temperature: request.temperature,
            stream: true,
        };

        let mut req_builder = self
            .client
            .post(&self.messages_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", Self::ANTHROPIC_VERSION)
            .header("content-type", "application/json");

        if request.cache_system_prompt {
            req_builder =
                req_builder.header("anthropic-beta", "prompt-caching-2024-07-31");
        }

        let resp = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                crate::FrameworkError::provider_with_source("anthropic", e.to_string(), e)
            })?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(crate::FrameworkError::AuthenticationFailed("anthropic".into()));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(crate::FrameworkError::RateLimitExceeded("anthropic".into()));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::FrameworkError::provider(
                "anthropic",
                format!("HTTP {status}: {body}"),
            ));
        }

        let (tx, rx) = mpsc::channel::<crate::Result<StreamChunk>>(64);

        tokio::spawn(async move {
            let mut byte_stream = resp.bytes_stream();
            let mut buf = String::new();

            // Track tool-call builders by content-block index.
            let mut tool_builders: std::collections::HashMap<
                usize,
                (String, String, String), // (id, name, accumulated_json)
            > = std::collections::HashMap::new();

            while let Some(chunk_result) = byte_stream.next().await {
                match chunk_result {
                    Err(e) => {
                        let _ = tx
                            .send(Err(crate::FrameworkError::provider_with_source(
                                "anthropic",
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

                while let Some(pos) = buf.find("\n\n") {
                    let event_str = buf[..pos].to_string();
                    buf = buf[pos + 2..].to_string();

                    // Parse event type and data lines.
                    let mut event_type: Option<String> = None;
                    let mut data: Option<String> = None;

                    for line in event_str.lines() {
                        if let Some(et) = line.strip_prefix("event: ") {
                            event_type = Some(et.to_string());
                        } else if let Some(d) = line.strip_prefix("data: ") {
                            data = Some(d.to_string());
                        }
                    }

                    let Some(data_str) = data else { continue };

                    // Use event type to guide parsing where helpful.
                    let event: AntStreamEvent = match serde_json::from_str(&data_str) {
                        Ok(e) => e,
                        Err(err) => {
                            warn!(
                                event_type = ?event_type,
                                raw = %data_str,
                                error = %err,
                                "Failed to parse Anthropic stream event"
                            );
                            continue;
                        }
                    };

                    match event {
                        AntStreamEvent::ContentBlockStart { index, content_block } => {
                            if let AntContentBlockStart::ToolUse { id, name } = content_block {
                                tool_builders.insert(index, (id, name, String::new()));
                            }
                        }
                        AntStreamEvent::ContentBlockDelta { index, delta } => match delta {
                            AntDelta::TextDelta { text } => {
                                let _ = tx.send(Ok(StreamChunk::Text(text))).await;
                            }
                            AntDelta::InputJsonDelta { partial_json } => {
                                if let Some(builder) = tool_builders.get_mut(&index) {
                                    builder.2.push_str(&partial_json);
                                    let _ = tx
                                        .send(Ok(StreamChunk::ToolCallDelta {
                                            id: Some(builder.0.clone()),
                                            index,
                                            name: Some(builder.1.clone()),
                                            arguments_delta: partial_json,
                                        }))
                                        .await;
                                }
                            }
                        },
                        AntStreamEvent::ContentBlockStop { index } => {
                            if let Some((id, name, json)) = tool_builders.remove(&index) {
                                let arguments =
                                    serde_json::from_str(&json).unwrap_or(serde_json::json!({}));
                                let _ = tx
                                    .send(Ok(StreamChunk::ToolCallComplete { id, name, arguments }))
                                    .await;
                            }
                        }
                        AntStreamEvent::MessageDelta { usage, .. } => {
                            if let Some(u) = usage {
                                let _ = tx
                                    .send(Ok(StreamChunk::Usage(TokenUsage {
                                        output_tokens: u.output_tokens,
                                        ..Default::default()
                                    })))
                                    .await;
                            }
                        }
                        AntStreamEvent::MessageStop => {
                            let _ = tx.send(Ok(StreamChunk::Done)).await;
                            return;
                        }
                        _ => {}
                    }
                }
            }

            let _ = tx.send(Ok(StreamChunk::Done)).await;
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    fn provider_name(&self) -> &str {
        "anthropic"
    }
}
