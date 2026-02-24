//! Built-in tools for demos and testing (§10.11).
//!
//! Use with `LocalToolRegistry::new().register(TimeTool).register(ReadFileTool::new(root)).register(SearchTool)`.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::Utc;

use crate::message::ToolDefinition;
use crate::Result;
use crate::Tool;

// ─────────────────────────────────────────────────────────────────────────────
// Time
// ─────────────────────────────────────────────────────────────────────────────

/// Returns the current UTC time. Useful for demos and for agents that need
/// the current date/time (e.g. scheduling, expiry checks).
pub struct TimeTool;

#[async_trait]
impl Tool for TimeTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "time".into(),
            description: "Return the current UTC date and time in ISO 8601 format. Use when the user or task needs to know the current time.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            examples: vec![],
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<String> {
        if !arguments.as_object().map_or(true, |o| o.is_empty()) {
            return Err(crate::FrameworkError::tool_exec(
                "time",
                "time tool accepts no parameters",
            ));
        }
        Ok(Utc::now().to_rfc3339())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Read file (path under a root)
// ─────────────────────────────────────────────────────────────────────────────

/// Reads a file under a configured root directory. Path must be relative to
/// the root; path traversal is rejected (§10.6 absolute paths / safety).
pub struct ReadFileTool {
    root: PathBuf,
}

impl ReadFileTool {
    /// Create a read_file tool that allows reading only under `root`.
    /// Paths are relative to `root`; `..` and absolute paths are rejected.
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".into(),
            description: "Read the contents of a text file. Path is relative to the configured root; use forward slashes. Use when you need to read source code, config, or logs.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the file (e.g. src/main.rs)"
                    }
                },
                "required": ["path"]
            }),
            examples: vec![],
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<String> {
        let path_str = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::FrameworkError::tool_exec("read_file", "missing 'path' string"))?;

        let path = Path::new(path_str);
        if path.is_absolute() || path_str.contains("..") {
            return Err(crate::FrameworkError::tool_exec(
                "read_file",
                "path must be relative and must not contain '..'",
            ));
        }

        let root_canonical = self
            .root
            .canonicalize()
            .map_err(|e| crate::FrameworkError::tool_exec("read_file", e.to_string()))?;
        let full = self.root.join(path);
        let canonical = full
            .canonicalize()
            .map_err(|e| crate::FrameworkError::tool_exec("read_file", e.to_string()))?;
        if !canonical.starts_with(&root_canonical) {
            return Err(crate::FrameworkError::tool_exec(
                "read_file",
                "path is outside the allowed root",
            ));
        }

        let contents = tokio::fs::read_to_string(&canonical)
            .await
            .map_err(|e| crate::FrameworkError::tool_exec("read_file", e.to_string()))?;
        Ok(contents)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Search (stub)
// ─────────────────────────────────────────────────────────────────────────────

/// Stub search tool for demos. Returns a fixed message; replace with MCP
/// or a real search backend for production.
pub struct SearchTool;

#[async_trait]
impl Tool for SearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "search".into(),
            description: "Search for text (stub). In production, use an MCP search server or your own backend. Use when the user asks to search; this stub returns a short message.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    }
                },
                "required": ["query"]
            }),
            examples: vec![],
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<String> {
        let _query = arguments
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        Ok("Search is a built-in stub. For production, connect an MCP search server or implement a custom search tool.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn time_returns_iso8601() {
        let t = TimeTool;
        let out = t.execute(serde_json::json!({})).await.unwrap();
        assert!(out.contains('T'));
        assert!(chrono::DateTime::parse_from_rfc3339(&out).is_ok());
    }

    #[tokio::test]
    async fn time_rejects_params() {
        let t = TimeTool;
        let err = t
            .execute(serde_json::json!({"foo": "bar"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("time"));
    }

    #[tokio::test]
    async fn search_stub_returns_message() {
        let s = SearchTool;
        let out = s
            .execute(serde_json::json!({"query": "test"}))
            .await
            .unwrap();
        assert!(out.contains("stub"));
    }
}
