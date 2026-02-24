//! Rhai embedded scripting engine — Code Mode (checklist §7).
//!
//! `RhaiEngine` wraps a `rhai::Engine` with:
//! * `Engine::new_raw()` (no standard library packages for security)
//! * Math and string packages added back selectively (§7.2)
//! * `set_max_operations` gas limit (§7.3)
//! * Registered host functions so scripts can call framework tools (§7.4)
//! * `eval_script(code, ctx)` — "Code Mode" entry point (§7.7)
//!
//! ## Code Mode
//!
//! In Code Mode the agent outputs a Rhai script instead of plain text.  The
//! runtime executes the script, collecting tool call results internally, and
//! returns only the *final value* back to the model.  This can save many
//! round-trips when the agent knows upfront which tools to call.
//!
//! ## Security model
//!
//! * `new_raw()` disables all standard Rhai packages (no file I/O, no OS).
//! * Only explicitly registered host functions are callable.
//! * Instruction count is capped by `max_operations` (default 1 000 000).
//! * Script memory is bounded by `max_string_size` / `max_array_size` limits.
//!
//! ## Binary footprint (§7.6)
//!
//! Target: **&lt;5 MB** for the scripting component. The `rhai` crate is used with
//! minimal features (`new_raw`, selected packages only); no file I/O or OS packages.

#[cfg(feature = "scripting")]
mod inner {
    use std::sync::Arc;

    use rhai::{packages::Package, Engine, EvalAltResult, Scope};
    use rhai::packages::{BasicMathPackage, BasicStringPackage};
    use tokio::runtime::Handle;
    use tracing::{debug, instrument};

    use openswarm_core::{FrameworkError, ToolExecutor};

    // ─────────────────────────────────────────────────────────────────────────
    // ScriptConfig
    // ─────────────────────────────────────────────────────────────────────────

    /// Tuning knobs for the Rhai script sandbox.
    #[derive(Debug, Clone)]
    pub struct ScriptConfig {
        /// Maximum number of Rhai VM operations before aborting execution.
        pub max_operations: u64,
        /// Maximum length of any single string (bytes).
        pub max_string_size: usize,
        /// Maximum number of elements in any single array.
        pub max_array_size: usize,
    }

    impl Default for ScriptConfig {
        fn default() -> Self {
            Self {
                max_operations: 1_000_000,
                max_string_size: 1024 * 1024, // 1 MiB
                max_array_size: 10_000,
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // ScriptContext — values injected into each script run
    // ─────────────────────────────────────────────────────────────────────────

    /// Context passed into each script invocation.
    ///
    /// Variables in this map are injected as top-level Rhai variables.
    #[derive(Debug, Clone, Default)]
    pub struct ScriptContext {
        /// Named variables to inject (must be JSON-serialisable).
        pub variables: std::collections::HashMap<String, serde_json::Value>,
    }

    impl ScriptContext {
        pub fn new() -> Self {
            Self::default()
        }

        /// Set a variable that will be available in the script.
        pub fn set(mut self, name: impl Into<String>, value: serde_json::Value) -> Self {
            self.variables.insert(name.into(), value);
            self
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // RhaiEngine
    // ─────────────────────────────────────────────────────────────────────────

    /// Sandboxed Rhai scripting runtime.
    pub struct RhaiEngine {
        executor: Arc<dyn ToolExecutor>,
        config: ScriptConfig,
    }

    impl RhaiEngine {
        /// Create a new scripting engine backed by the given tool executor.
        pub fn new(executor: Arc<dyn ToolExecutor>, config: ScriptConfig) -> Self {
            Self { executor, config }
        }

        /// Execute a Rhai script and return the final value as JSON.
        ///
        /// The script can call any registered tool via `call_tool(name, json_args)`.
        ///
        /// # Code Mode
        ///
        /// ```text
        /// let fetch_result = call_tool("fetch", `{"url": "https://example.com"}`);
        /// let parse_result = call_tool("parse_html", `{"html": fetch_result}`);
        /// parse_result
        /// ```
        #[instrument(skip(self, script_code))]
        pub async fn eval_script(
            &self,
            script_code: &str,
            context: ScriptContext,
        ) -> openswarm_core::Result<serde_json::Value> {
            let executor = Arc::clone(&self.executor);
            let config = self.config.clone();
            let script_code = script_code.to_owned();
            let tokio_handle = Handle::current();

            tokio::task::spawn_blocking(move || {
                run_script_sync(&script_code, &context, executor, &config, tokio_handle)
            })
            .await
            .map_err(|e| FrameworkError::Config(format!("Rhai task panicked: {e}")))?
        }

        /// Return the definitions of all registered tools (for agent discovery, §7.9).
        pub fn tool_definitions(&self) -> Vec<openswarm_core::ToolDefinition> {
            self.executor.tool_definitions()
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Synchronous execution core (runs inside spawn_blocking)
    // ─────────────────────────────────────────────────────────────────────────

    fn run_script_sync(
        script_code: &str,
        context: &ScriptContext,
        executor: Arc<dyn ToolExecutor>,
        config: &ScriptConfig,
        tokio_handle: Handle,
    ) -> openswarm_core::Result<serde_json::Value> {
        // ── Build engine ───────────────────────────────────────────────────
        let mut engine = Engine::new_raw();

        // Add only safe, sandboxed packages.
        engine.register_global_module(BasicMathPackage::new().as_shared_module());
        engine.register_global_module(BasicStringPackage::new().as_shared_module());

        // ── Apply limits ────────────────────────────────────────────────────
        engine.set_max_operations(config.max_operations as usize);
        engine.set_max_string_size(config.max_string_size);
        engine.set_max_array_size(config.max_array_size);

        // ── Register `call_tool(name, json_args_string) -> String` ─────────
        let exec_clone = Arc::clone(&executor);
        let handle_clone = tokio_handle.clone();

        engine.register_fn(
            "call_tool",
            move |tool_name: &str, args_json: &str| -> Result<rhai::Dynamic, Box<EvalAltResult>> {
                let args: serde_json::Value =
                    serde_json::from_str(args_json).map_err(|e| {
                        Box::new(EvalAltResult::ErrorRuntime(
                            format!("call_tool: invalid JSON args — {e}").into(),
                            rhai::Position::NONE,
                        )) as Box<EvalAltResult>
                    })?;

                let tool_use_id = uuid::Uuid::new_v4().to_string();
                let exec = Arc::clone(&exec_clone);
                let name = tool_name.to_owned();

                // Block the current thread (inside spawn_blocking) on the
                // async tool execution.
                let result = handle_clone.block_on(exec.execute(&name, &tool_use_id, args));

                let text = match result {
                    openswarm_core::ContentBlock::ToolResult { content, .. } => content,
                    openswarm_core::ContentBlock::Text { text } => text,
                    other => format!("{other:?}"),
                };

                Ok(rhai::Dynamic::from(text))
            },
        );

        // ── `print_tool(name, args)` alias ─────────────────────────────────
        let exec_print = Arc::clone(&executor);
        let handle_print = tokio_handle;

        engine.register_fn(
            "invoke",
            move |tool_name: &str, args: rhai::Dynamic| -> Result<rhai::Dynamic, Box<EvalAltResult>> {
                let args_json: serde_json::Value = dynamic_to_json(args)?;
                let tool_use_id = uuid::Uuid::new_v4().to_string();
                let exec = Arc::clone(&exec_print);
                let name = tool_name.to_owned();

                let result =
                    handle_print.block_on(exec.execute(&name, &tool_use_id, args_json));

                let text = match result {
                    openswarm_core::ContentBlock::ToolResult { content, .. } => content,
                    openswarm_core::ContentBlock::Text { text } => text,
                    other => format!("{other:?}"),
                };

                Ok(rhai::Dynamic::from(text))
            },
        );

        // ── Build scope with context variables ─────────────────────────────
        let mut scope = Scope::new();
        for (name, value) in &context.variables {
            scope.push_dynamic(name.clone(), json_to_dynamic(value.clone()));
        }

        // ── Execute script ─────────────────────────────────────────────────
        debug!(max_ops = config.max_operations, "Executing Rhai script");
        let result = engine
            .eval_with_scope::<rhai::Dynamic>(&mut scope, script_code)
            .map_err(|e| FrameworkError::Config(format!("Rhai execution error: {e}")))?;

        // Convert the result Dynamic to JSON.
        Ok(dynamic_to_json_value(result))
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Dynamic ↔ JSON helpers
    // ─────────────────────────────────────────────────────────────────────────

    fn json_to_dynamic(v: serde_json::Value) -> rhai::Dynamic {
        match v {
            serde_json::Value::Null => rhai::Dynamic::UNIT,
            serde_json::Value::Bool(b) => rhai::Dynamic::from(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    rhai::Dynamic::from(i)
                } else {
                    rhai::Dynamic::from(n.as_f64().unwrap_or(0.0))
                }
            }
            serde_json::Value::String(s) => rhai::Dynamic::from(s),
            serde_json::Value::Array(arr) => {
                let v: rhai::Array = arr.into_iter().map(json_to_dynamic).collect();
                rhai::Dynamic::from(v)
            }
            serde_json::Value::Object(map) => {
                let mut m = rhai::Map::new();
                for (k, v) in map {
                    m.insert(k.into(), json_to_dynamic(v));
                }
                rhai::Dynamic::from(m)
            }
        }
    }

    fn dynamic_to_json_value(d: rhai::Dynamic) -> serde_json::Value {
        if d.is_unit() {
            serde_json::Value::Null
        } else if let Ok(b) = d.clone().try_cast::<bool>() {
            serde_json::Value::Bool(b)
        } else if let Ok(i) = d.clone().try_cast::<i64>() {
            serde_json::json!(i)
        } else if let Ok(f) = d.clone().try_cast::<f64>() {
            serde_json::json!(f)
        } else if let Ok(s) = d.clone().try_cast::<String>() {
            serde_json::Value::String(s)
        } else if let Ok(arr) = d.clone().try_cast::<rhai::Array>() {
            serde_json::Value::Array(arr.into_iter().map(dynamic_to_json_value).collect())
        } else if let Ok(map) = d.try_cast::<rhai::Map>() {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .map(|(k, v)| (k.to_string(), dynamic_to_json_value(v)))
                .collect();
            serde_json::Value::Object(obj)
        } else {
            serde_json::Value::Null
        }
    }

    fn dynamic_to_json(
        d: rhai::Dynamic,
    ) -> Result<serde_json::Value, Box<EvalAltResult>> {
        Ok(dynamic_to_json_value(d))
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[cfg(test)]
    mod tests {
        use super::*;
        use async_trait::async_trait;
        use openswarm_core::{ContentBlock, ToolDefinition};

        struct CountingTool;

        #[async_trait]
        impl ToolExecutor for CountingTool {
            fn tool_definitions(&self) -> Vec<ToolDefinition> {
                vec![ToolDefinition {
                    name: "add".into(),
                    description: "Add two numbers".into(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "a": { "type": "number" },
                            "b": { "type": "number" }
                        }
                    }),
                    examples: vec![],
                }]
            }

            async fn execute(
                &self,
                _name: &str,
                id: &str,
                args: serde_json::Value,
            ) -> ContentBlock {
                let a = args["a"].as_f64().unwrap_or(0.0);
                let b = args["b"].as_f64().unwrap_or(0.0);
                ContentBlock::ToolResult {
                    tool_use_id: id.into(),
                    content: (a + b).to_string(),
                    is_error: false,
                }
            }
        }

        fn engine() -> RhaiEngine {
            RhaiEngine::new(Arc::new(CountingTool), ScriptConfig::default())
        }

        #[tokio::test]
        async fn test_basic_eval() {
            let e = engine();
            let result = e
                .eval_script("1 + 2 * 3", ScriptContext::new())
                .await
                .unwrap();
            assert_eq!(result, serde_json::json!(7));
        }

        #[tokio::test]
        async fn test_call_tool_from_script() {
            let e = engine();
            // The script calls `add` twice and sums the string results.
            let script = r#"
                let r1 = call_tool("add", `{"a": 3, "b": 4}`);
                let r2 = call_tool("add", `{"a": 10, "b": 20}`);
                // Results are strings — convert to int for arithmetic.
                let sum = r1.parse_int() + r2.parse_int();
                sum
            "#;
            let result = e.eval_script(script, ScriptContext::new()).await.unwrap();
            // 3+4=7, 10+20=30, sum=37
            assert_eq!(result, serde_json::json!(37));
        }

        #[tokio::test]
        async fn test_context_variables() {
            let e = engine();
            let ctx = ScriptContext::new()
                .set("x", serde_json::json!(10))
                .set("y", serde_json::json!(5));
            let result = e.eval_script("x * y", ctx).await.unwrap();
            assert_eq!(result, serde_json::json!(50));
        }

        #[tokio::test]
        async fn test_operation_limit() {
            let e = RhaiEngine::new(
                Arc::new(CountingTool),
                ScriptConfig { max_operations: 100, ..Default::default() },
            );
            let script = r#"
                let i = 0;
                loop { i += 1; }
            "#;
            let result = e.eval_script(script, ScriptContext::new()).await;
            assert!(result.is_err(), "expected operation-limit error");
            let msg = result.unwrap_err().to_string();
            assert!(
                msg.contains("operation") || msg.contains("Rhai"),
                "unexpected error: {msg}"
            );
        }

        #[tokio::test]
        async fn test_string_return() {
            let e = engine();
            let result = e
                .eval_script(r#""hello " + "world""#, ScriptContext::new())
                .await
                .unwrap();
            assert_eq!(result, serde_json::json!("hello world"));
        }
    }
}

// ── Public re-exports (feature-gated) ────────────────────────────────────────

#[cfg(feature = "scripting")]
pub use inner::{RhaiEngine, ScriptConfig, ScriptContext};
