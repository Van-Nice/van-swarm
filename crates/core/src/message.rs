//! Canonical message and tool-call types shared across all providers.
//!
//! The framework keeps a single internal representation.  Each provider
//! translates *to* and *from* its own wire format inside its own module –
//! nothing outside `providers/` ever touches raw OpenAI / Anthropic JSON.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// Role
// ─────────────────────────────────────────────────────────────────────────────

/// Conversation participant roles.
///
/// Provider mappings:
/// | Framework | OpenAI      | Anthropic  | Gemini  |
/// |-----------|-------------|------------|---------|
/// | System    | system      | (top-level)| system  |
/// | User      | user        | user       | user    |
/// | Assistant | assistant   | assistant  | model   |
/// | Tool      | tool        | user*      | function|
///
/// *Anthropic places tool results inside the `user` turn as content blocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    /// Tool result returned after a tool call.
    Tool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Content blocks
// ─────────────────────────────────────────────────────────────────────────────

/// A single content block inside a message.
///
/// Using an enum-of-blocks (rather than a free-form string) lets us represent
/// mixed messages (text + tool_use) in a type-safe way and avoids the
/// stringly-typed pitfall common in Python frameworks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Plain text.
    Text { text: String },

    /// A tool invocation emitted by the assistant.
    ToolUse {
        /// Provider-assigned unique ID for this call (used to match results).
        id: String,
        name: String,
        /// Arguments as a JSON value.  We store them parsed so downstream
        /// code doesn't have to re-parse a JSON string-inside-JSON.
        input: serde_json::Value,
    },

    /// The result of a tool call, sent back by the host environment.
    ToolResult {
        /// Matches the `id` from the corresponding `ToolUse` block.
        tool_use_id: String,
        /// Serialised result or error description.
        content: String,
        /// `true` when the tool failed; the model can then self-correct.
        #[serde(default)]
        is_error: bool,
    },
}

impl ContentBlock {
    /// Convenience constructor for a plain-text block.
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text { text: s.into() }
    }

    /// Convenience constructor for a tool call.
    pub fn tool_use(
        id: impl Into<String>,
        name: impl Into<String>,
        input: serde_json::Value,
    ) -> Self {
        Self::ToolUse { id: id.into(), name: name.into(), input }
    }

    /// Convenience constructor for a successful tool result.
    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::ToolResult {
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_error: false,
        }
    }

    /// Convenience constructor for a failed tool result.
    pub fn tool_error(tool_use_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self::ToolResult {
            tool_use_id: tool_use_id.into(),
            content: error.into(),
            is_error: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Message
// ─────────────────────────────────────────────────────────────────────────────

/// A single turn in a multi-turn conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    /// A message may contain multiple content blocks (e.g. reasoning text +
    /// one or more tool calls in a single assistant turn).
    pub content: Vec<ContentBlock>,
}

impl Message {
    /// Build a plain system message.
    pub fn system(text: impl Into<String>) -> Self {
        Self { role: Role::System, content: vec![ContentBlock::text(text)] }
    }

    /// Build a plain user message.
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: Role::User, content: vec![ContentBlock::text(text)] }
    }

    /// Build a plain assistant text message.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: vec![ContentBlock::text(text)] }
    }

    /// Build an assistant message that contains one or more tool calls.
    pub fn assistant_with_blocks(blocks: Vec<ContentBlock>) -> Self {
        Self { role: Role::Assistant, content: blocks }
    }

    /// Build a tool-result message (sent back to the model).
    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: vec![ContentBlock::tool_result(tool_use_id, content)],
        }
    }

    /// Build a tool-error message (sent back to the model so it can retry).
    pub fn tool_error(tool_use_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: vec![ContentBlock::tool_error(tool_use_id, error)],
        }
    }

    /// Return the concatenation of all `Text` blocks (for simple display).
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Return all `ToolUse` blocks in this message.
    pub fn tool_uses(&self) -> Vec<&ContentBlock> {
        self.content
            .iter()
            .filter(|b| matches!(b, ContentBlock::ToolUse { .. }))
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// XML-style tag parsing (§10.10 thinking time)
// ─────────────────────────────────────────────────────────────────────────────

/// Extract content of all `<tag>…</tag>` blocks from `text` (case-sensitive).
///
/// Use for parsing model output when chain-of-thought is enabled (e.g. `<thinking>…</thinking>`).
/// Returns only the inner content of each block; unclosed or nested tags are best-effort.
pub fn extract_xml_blocks(text: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&open) {
        let after_open = &rest[start + open.len()..];
        if let Some(end) = after_open.find(&close) {
            out.push(after_open[..end].trim().to_string());
            rest = &after_open[end + close.len()..];
        } else {
            break;
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Tool definition (what the model sees)
// ─────────────────────────────────────────────────────────────────────────────

/// The schema we expose to the model describing one callable tool.
///
/// Derived from Rust function signatures via `#[tool]` (checklist §10).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Short, snake_case name (e.g. `"fetch_order_history"`).
    /// Be specific – the model uses this name to invoke the tool.
    pub name: String,

    /// One or two sentences explaining WHAT the tool does, WHEN to use it,
    /// and what its output represents.  Poor descriptions are the #1 source
    /// of agent failure in production.
    pub description: String,

    /// JSON Schema object describing the parameters.
    /// Generated by `schemars` from the Rust struct; never hand-written.
    pub parameters: serde_json::Value,

    /// Optional worked examples shown in the system prompt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<ToolExample>,
}

/// A single input/output example attached to a `ToolDefinition`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExample {
    /// Natural-language description of what this example demonstrates.
    pub description: String,
    /// The input the model would pass to the tool.
    pub input: serde_json::Value,
    /// The expected output (for documentation; not enforced at runtime).
    pub output: serde_json::Value,
}

// ─────────────────────────────────────────────────────────────────────────────
// Token usage
// ─────────────────────────────────────────────────────────────────────────────

/// Token counts returned by the provider.  Used for cost attribution and
/// context-window tracking in the APM layer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_creation_tokens: u32,
}

impl TokenUsage {
    pub fn total(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }

    /// Compute the incremental usage between `self` (current) and `prev` (before a step).
    ///
    /// Used by `run_agent_traced` to attribute tokens to individual spans.
    pub fn delta(&self, prev: &TokenUsage) -> TokenUsage {
        TokenUsage {
            input_tokens: self.input_tokens.saturating_sub(prev.input_tokens),
            output_tokens: self.output_tokens.saturating_sub(prev.output_tokens),
            cache_read_tokens: self.cache_read_tokens.saturating_sub(prev.cache_read_tokens),
            cache_creation_tokens: self
                .cache_creation_tokens
                .saturating_sub(prev.cache_creation_tokens),
        }
    }
}

impl std::ops::Add for TokenUsage {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            input_tokens: self.input_tokens + rhs.input_tokens,
            output_tokens: self.output_tokens + rhs.output_tokens,
            cache_read_tokens: self.cache_read_tokens + rhs.cache_read_tokens,
            cache_creation_tokens: self.cache_creation_tokens + rhs.cache_creation_tokens,
        }
    }
}

impl std::ops::AddAssign for TokenUsage {
    fn add_assign(&mut self, rhs: Self) {
        self.input_tokens += rhs.input_tokens;
        self.output_tokens += rhs.output_tokens;
        self.cache_read_tokens += rhs.cache_read_tokens;
        self.cache_creation_tokens += rhs.cache_creation_tokens;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Stop reason
// ─────────────────────────────────────────────────────────────────────────────

/// Why the model stopped generating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Model reached a natural stopping point (`\n\nHuman:` etc.).
    EndTurn,
    /// Model requested one or more tool calls.
    ToolUse,
    /// `max_tokens` was reached before a natural stop.
    MaxTokens,
    /// A stop sequence was hit.
    StopSequence,
    /// Output filtered by the provider's safety system.
    ContentFilter,
    /// Any other stop reason returned by the provider.
    Other(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Streaming
// ─────────────────────────────────────────────────────────────────────────────

/// A single chunk emitted during a streaming completion.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// A fragment of text content.
    Text(String),

    /// Partial tool call data (name / arguments being built up incrementally).
    ToolCallDelta {
        /// Provider-assigned call ID (may be absent in early deltas).
        id: Option<String>,
        /// Index in the tool-calls array (for multiplexing several calls).
        index: usize,
        /// Tool name fragment (only present once, in the first delta).
        name: Option<String>,
        /// Incremental arguments JSON fragment.
        arguments_delta: String,
    },

    /// A fully assembled tool call emitted after all its deltas have been
    /// received.  Convenient for callers that don't want to buffer manually.
    ToolCallComplete {
        id: String,
        name: String,
        /// Fully parsed arguments (the framework does the JSON re-parse).
        arguments: serde_json::Value,
    },

    /// Usage statistics appended at the end of the stream by some providers.
    Usage(TokenUsage),

    /// Sentinel value – the stream has finished.
    Done,
}

/// Type alias for a boxed streaming response.
///
/// Using `Pin<Box<dyn Stream<...>>>` rather than `impl Stream` allows the
/// trait `ModelProvider` to be object-safe (`dyn ModelProvider`).
pub type ResponseStream =
    std::pin::Pin<Box<dyn futures::Stream<Item = crate::Result<StreamChunk>> + Send>>;

// ─────────────────────────────────────────────────────────────────────────────
// CompletionRequest / CompletionResponse
// ─────────────────────────────────────────────────────────────────────────────

/// The canonical request shape passed to every `ModelProvider`.
///
/// Provider implementations translate this into their own wire format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// The model to use (e.g. `"gpt-4o"`, `"claude-opus-4-6"`).
    pub model: String,

    /// The full conversation history including the latest user turn.
    pub messages: Vec<Message>,

    /// Tools the model is allowed to call.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDefinition>,

    /// Override temperature for this specific call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Override max_tokens for this specific call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Whether to use streaming mode.
    #[serde(default)]
    pub stream: bool,

    /// Whether to cache the system prompt (Anthropic prompt caching, §20.2).
    ///
    /// When `true`, the Anthropic provider adds `cache_control: {"type": "ephemeral"}`
    /// to the system-prompt block.  Ignored by other providers.
    #[serde(default)]
    pub cache_system_prompt: bool,

    /// Provider-specific extra parameters (passed through as-is).
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl CompletionRequest {
    /// Build a minimal request from messages + model name.
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
            stream: false,
            cache_system_prompt: false,
            extra: serde_json::Map::new(),
        }
    }

    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_temperature(mut self, t: f32) -> Self {
        self.temperature = Some(t);
        self
    }

    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = Some(n);
        self
    }

    pub fn streaming(mut self) -> Self {
        self.stream = true;
        self
    }

    /// Enable Anthropic prompt caching on the system-prompt block (§20.2).
    pub fn with_cache_system_prompt(mut self, enabled: bool) -> Self {
        self.cache_system_prompt = enabled;
        self
    }
}

/// The normalised response from a completed (non-streaming) call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Provider-assigned response ID (useful for support tickets).
    pub id: String,

    /// The assistant message produced by the model.
    pub message: Message,

    /// Why the model stopped generating.
    pub stop_reason: StopReason,

    /// Token counts for cost tracking.
    pub usage: TokenUsage,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Generate a unique tool-call ID (used when the provider doesn't supply one).
pub fn new_tool_call_id() -> String {
    format!("call_{}", Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_text_content_concat() {
        let m = Message::assistant("Hello, world!");
        assert_eq!(m.text_content(), "Hello, world!");
    }

    #[test]
    fn extract_xml_blocks_thinking() {
        let text = "First <thinking>reason one</thinking> then <thinking>reason two</thinking> end";
        let blocks = extract_xml_blocks(text, "thinking");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], "reason one");
        assert_eq!(blocks[1], "reason two");
    }

    #[test]
    fn token_usage_add() {
        let a = TokenUsage { input_tokens: 10, output_tokens: 5, ..Default::default() };
        let b = TokenUsage { input_tokens: 20, output_tokens: 8, ..Default::default() };
        let c = a + b;
        assert_eq!(c.input_tokens, 30);
        assert_eq!(c.output_tokens, 13);
        assert_eq!(c.total(), 43);
    }

    #[test]
    fn message_tool_uses_filter() {
        let blocks = vec![
            ContentBlock::text("I'll call a tool now."),
            ContentBlock::tool_use("call_01", "search", serde_json::json!({"q": "Rust"})),
        ];
        let m = Message::assistant_with_blocks(blocks);
        assert_eq!(m.tool_uses().len(), 1);
    }
}
