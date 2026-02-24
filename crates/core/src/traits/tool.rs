//! Tool and ToolExecutor traits – the Agent-Computer Interface (ACI).
//!
//! Design philosophy (from the Anthropic Agent Bible):
//! * Name tools clearly and specifically (`fetch_order_history`, not `get`).
//! * Use Rust's type system to make invalid parameters unrepresentable.
//! * Return structured errors so the model can self-correct without crashing.

use async_trait::async_trait;

use crate::message::{ContentBlock, ToolDefinition};

// ─────────────────────────────────────────────────────────────────────────────
// Tool trait
// ─────────────────────────────────────────────────────────────────────────────

/// A single callable tool exposed to an agent.
///
/// Implementations are generated automatically via the `#[tool]` macro
/// (checklist §10), but can also be written by hand for complex cases.
///
/// # Safety contract
/// The executor calls `execute` with the *raw* JSON the model produced.
/// Implementations MUST validate this input and return a structured error
/// (not a panic) on malformed data.  Assume the input is adversarial.
#[async_trait]
pub trait Tool: Send + Sync {
    /// The schema the model uses to discover and call this tool.
    fn definition(&self) -> ToolDefinition;

    /// Execute the tool with the given JSON arguments.
    ///
    /// Returns a human-readable (or JSON) string the model can read.
    /// On failure, return `Err(FrameworkError::ToolExecution { … })` –
    /// the caller converts this to a `tool_error` content block so the
    /// model can self-correct.
    async fn execute(&self, arguments: serde_json::Value) -> crate::Result<String>;
}

// ─────────────────────────────────────────────────────────────────────────────
// ToolExecutor trait
// ─────────────────────────────────────────────────────────────────────────────

/// A registry that routes tool calls to their concrete `Tool` implementations.
///
/// The `ReActAgent` holds an `Arc<dyn ToolExecutor>` – the executor abstracts
/// whether tools live in the same process, in a WASM sandbox, or behind MCP.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Return the schema for every tool currently available.
    ///
    /// Called once per agent turn to build the `tools` array sent to the LLM.
    fn tool_definitions(&self) -> Vec<ToolDefinition>;

    /// Execute a tool call by name with parsed JSON arguments.
    ///
    /// Returns a `ContentBlock::ToolResult` (or `ToolError`) that can be
    /// appended directly to the conversation messages.
    async fn execute(
        &self,
        tool_name: &str,
        tool_use_id: &str,
        arguments: serde_json::Value,
    ) -> ContentBlock;
}

// ─────────────────────────────────────────────────────────────────────────────
// Concrete: LocalToolRegistry
// ─────────────────────────────────────────────────────────────────────────────

/// An in-process registry backed by a `Vec<Box<dyn Tool>>`.
///
/// Suitable for unit tests and simple agents.  Production deployments would
/// use the WASM-backed executor from `vanswarm-runtime`.
pub struct LocalToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl LocalToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Register a tool.
    pub fn register(mut self, tool: impl Tool + 'static) -> Self {
        self.tools.push(Box::new(tool));
        self
    }
}

impl Default for LocalToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for LocalToolRegistry {
    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|t| t.definition()).collect()
    }

    async fn execute(
        &self,
        tool_name: &str,
        tool_use_id: &str,
        arguments: serde_json::Value,
    ) -> ContentBlock {
        let tool = self.tools.iter().find(|t| t.definition().name == tool_name);

        match tool {
            None => {
                tracing::warn!(tool = tool_name, "Tool not found in registry");
                ContentBlock::tool_error(
                    tool_use_id,
                    format!("Tool '{tool_name}' not found. Available tools: {}",
                        self.tools.iter().map(|t| t.definition().name.clone())
                            .collect::<Vec<_>>().join(", ")
                    ),
                )
            }
            Some(t) => match t.execute(arguments).await {
                Ok(result) => {
                    tracing::debug!(tool = tool_name, "Tool executed successfully");
                    ContentBlock::tool_result(tool_use_id, result)
                }
                Err(e) => {
                    tracing::warn!(tool = tool_name, error = %e, "Tool execution failed");
                    ContentBlock::tool_error(tool_use_id, e.to_string())
                }
            },
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FilteredToolExecutor — keyword-based tool subset (§9.10 deferred loading)
// ─────────────────────────────────────────────────────────────────────────────

/// Wraps any [`ToolExecutor`] and exposes only the tools whose names contain
/// at least one of the allow-listed keywords (§9.10 deferred / context-scoped
/// tool loading).
///
/// This is useful for multi-agent supervisors that route tasks to specialised
/// sub-agents and want to restrict the visible tool surface per agent.
///
/// # Example
///
/// ```rust,no_run
/// use vanswarm_core::{LocalToolRegistry, traits::tool::FilteredToolExecutor};
/// use std::sync::Arc;
///
/// let registry = LocalToolRegistry::new();
/// // Only expose tools whose names contain "file" or "read".
/// let filtered = FilteredToolExecutor::new(Arc::new(registry), vec!["file", "read"]);
/// ```
pub struct FilteredToolExecutor<E: ToolExecutor> {
    inner: std::sync::Arc<E>,
    /// Only tools whose names contain at least one of these keywords are visible.
    allow_keywords: Vec<String>,
}

impl<E: ToolExecutor + 'static> FilteredToolExecutor<E> {
    /// Create a new filtered executor.
    ///
    /// * `inner` — the underlying executor holding all tools.
    /// * `allow_keywords` — case-insensitive substrings; a tool is included if
    ///   its name contains *any* of the keywords.
    pub fn new(inner: std::sync::Arc<E>, allow_keywords: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            inner,
            allow_keywords: allow_keywords.into_iter().map(Into::into).collect(),
        }
    }

    fn is_allowed(&self, tool_name: &str) -> bool {
        let name_lower = tool_name.to_lowercase();
        self.allow_keywords.iter().any(|kw| name_lower.contains(kw.to_lowercase().as_str()))
    }
}

#[async_trait]
impl<E: ToolExecutor + 'static> ToolExecutor for FilteredToolExecutor<E> {
    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.inner
            .tool_definitions()
            .into_iter()
            .filter(|td| self.is_allowed(&td.name))
            .collect()
    }

    async fn execute(
        &self,
        tool_name: &str,
        tool_use_id: &str,
        arguments: serde_json::Value,
    ) -> ContentBlock {
        if !self.is_allowed(tool_name) {
            return ContentBlock::tool_error(
                tool_use_id,
                format!("Tool '{tool_name}' is not available in this context"),
            );
        }
        self.inner.execute(tool_name, tool_use_id, arguments).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::ToolDefinition;

    // ── Hand-written tool (baseline) ───────────────────────────────────────

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "echo".into(),
                description: "Echoes the input back.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "text": { "type": "string" } },
                    "required": ["text"]
                }),
                examples: vec![],
            }
        }

        async fn execute(&self, args: serde_json::Value) -> crate::Result<String> {
            let text = args["text"]
                .as_str()
                .ok_or_else(|| crate::FrameworkError::tool_exec("echo", "missing 'text' field"))?;
            Ok(text.to_string())
        }
    }

    #[tokio::test]
    async fn registry_routes_to_correct_tool() {
        let reg = LocalToolRegistry::new().register(EchoTool);
        let block = reg.execute("echo", "call_01", serde_json::json!({"text": "hello"})).await;
        assert!(matches!(
            &block,
            ContentBlock::ToolResult { content, is_error: false, .. } if content == "hello"
        ));
    }

    #[tokio::test]
    async fn registry_returns_error_for_unknown_tool() {
        let reg = LocalToolRegistry::new().register(EchoTool);
        let block = reg.execute("missing_tool", "call_02", serde_json::json!({})).await;
        assert!(matches!(&block, ContentBlock::ToolResult { is_error: true, .. }));
    }

    // ── #[tool] macro: schema + type-safe wrapper (§10.2–10.5) ──────────────

    use vanswarm_macros::tool;

    #[tool]
    /// Echoes the input text back. Use for testing tool dispatch.
    async fn echo_tool(text: String) -> crate::Result<String> {
        Ok(text)
    }

    #[tool(example(description = "Echo hello", input = r#"{"text":"hello"}"#, output = r#""hello""#))]
    /// Echoes the input text with an example for the model.
    async fn echo_with_example(text: String) -> crate::Result<String> {
        Ok(text)
    }

    #[tokio::test]
    async fn tool_macro_roundtrip() {
        let reg = LocalToolRegistry::new().register(EchoToolTool);
        let defs = reg.tool_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "echo_tool");
        assert!(!defs[0].description.is_empty());
        assert!(defs[0].parameters.get("properties").is_some(), "JSON schema has properties");

        let block = reg
            .execute("echo_tool", "call_01", serde_json::json!({"text": "hello"}))
            .await;
        assert!(matches!(
            &block,
            ContentBlock::ToolResult { content, is_error: false, .. } if content == "hello"
        ));
    }

    #[tokio::test]
    async fn tool_macro_validation_error() {
        let reg = LocalToolRegistry::new().register(EchoToolTool);
        let block = reg.execute("echo_tool", "call_02", serde_json::json!({})).await;
        assert!(matches!(&block, ContentBlock::ToolResult { is_error: true, .. }));
    }

    #[tokio::test]
    async fn tool_macro_examples_in_definition() {
        let reg = LocalToolRegistry::new().register(EchoWithExampleTool);
        let defs = reg.tool_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].examples.len(), 1, "one example from #[tool(example(...))]");
        let ex = &defs[0].examples[0];
        assert_eq!(ex.description, "Echo hello");
        assert_eq!(ex.input.get("text").and_then(|v| v.as_str()), Some("hello"));
        assert_eq!(ex.output.as_str(), Some("hello"));
    }
}
