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
/// use the WASM-backed executor from `rustmastra-runtime`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::ToolDefinition;

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
}
