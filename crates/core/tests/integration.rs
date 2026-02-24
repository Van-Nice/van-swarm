//! Integration tests for vanswarm-core (§22.1–22.4).
//!
//! These tests exercise the full public API with a scripted `MockModelProvider`
//! so no real API keys or network access are needed.
//!
//! | Test                                       | Checklist |
//! |--------------------------------------------|-----------|
//! | `react_loop_tool_then_answer`              | §22.2     |
//! | `react_loop_max_iterations_guard`          | §22.2     |
//! | `evaluator_optimizer_integration`          | §22.3     |
//! | `plan_and_execute_integration`             | §22.3     |
//! | `filtered_executor_blocks_disallowed`      | §22.4     |
//! | `filtered_executor_passes_allowed`         | §22.4     |
//! | `filtered_executor_hides_definitions`      | §22.4     |

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vanswarm_core::{
    config::{AgentConfig, ModelConfig},
    message::{
        CompletionRequest, CompletionResponse, ContentBlock, Message, ResponseStream,
        StopReason, StreamChunk, TokenUsage, ToolDefinition,
    },
    providers::ModelProvider,
    react::{run_agent, ReActAgent},
    traits::{LocalToolRegistry, Tool, ToolExecutor},
    EvaluatorOptimizerLoop, FilteredToolExecutor, PlanAndExecute, Result,
    evaluators::{ScoreInput, ScoreResult, Scorer},
};

// ─────────────────────────────────────────────────────────────────────────────
// MockModelProvider — scripted response queue
// ─────────────────────────────────────────────────────────────────────────────

/// One scripted response returned by `MockModelProvider::complete`.
enum MockResponse {
    /// Plain assistant text (stop_reason = EndTurn).
    Text(String),
    /// Tool invocation (stop_reason = ToolUse).
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

/// A deterministic, queue-backed `ModelProvider` for testing.
///
/// Responses are returned in order; the queue is reversed at construction so
/// `pop()` is O(1).  Panics when the queue is exhausted to catch script errors.
struct MockModelProvider {
    queue: Mutex<Vec<MockResponse>>,
}

impl MockModelProvider {
    fn new(mut responses: Vec<MockResponse>) -> Self {
        responses.reverse(); // pop() from back = front-to-back consumption
        Self { queue: Mutex::new(responses) }
    }
}

#[async_trait]
impl ModelProvider for MockModelProvider {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse> {
        let mock = self
            .queue
            .lock()
            .unwrap()
            .pop()
            .expect("MockModelProvider: response queue exhausted");

        let (message, stop_reason) = match mock {
            MockResponse::Text(text) => (Message::assistant(text), StopReason::EndTurn),
            MockResponse::ToolUse { id, name, input } => (
                Message::assistant_with_blocks(vec![ContentBlock::tool_use(id, name, input)]),
                StopReason::ToolUse,
            ),
        };

        Ok(CompletionResponse {
            id: "mock-response".into(),
            message,
            stop_reason,
            usage: TokenUsage::default(),
        })
    }

    async fn stream(&self, _req: CompletionRequest) -> Result<ResponseStream> {
        Ok(Box::pin(futures::stream::once(async { Ok(StreamChunk::Done) })))
    }

    fn provider_name(&self) -> &str {
        "mock"
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// EchoTool — echoes the "text" argument back as a string
// ─────────────────────────────────────────────────────────────────────────────

struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "echo".into(),
            description: "Echoes the text argument back.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            }),
            examples: vec![],
        }
    }

    async fn execute(&self, args: serde_json::Value) -> Result<String> {
        let text = args["text"].as_str().unwrap_or("(empty)");
        Ok(format!("echo: {text}"))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// §22.2 – Full ReAct loop with mock LLM and one tool
// ─────────────────────────────────────────────────────────────────────────────

/// §22.2: Agent calls one tool then produces a final answer.
///
/// Script:
///   1. Provider returns ToolUse(echo, {text: "hello"})
///   2. Provider returns Text("Done!")
/// Expected: run completes with "Done!" after 1 tool call.
#[tokio::test]
async fn react_loop_tool_then_answer() {
    let provider = MockModelProvider::new(vec![
        MockResponse::ToolUse {
            id: "call_01".into(),
            name: "echo".into(),
            input: serde_json::json!({ "text": "hello" }),
        },
        MockResponse::Text("Done!".into()),
    ]);

    let registry = LocalToolRegistry::new().register(EchoTool);

    let config = AgentConfig::new("test-agent", ModelConfig::new("mock"));
    let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(registry));

    let answer = run_agent(&agent, "echo hello").await.unwrap();
    assert_eq!(answer, "Done!");
}

/// §22.2: Agent skips tools entirely and returns a direct text answer.
///
/// Script: provider immediately returns Text("Paris") with no tool call.
#[tokio::test]
async fn react_loop_direct_answer_no_tool() {
    let provider = MockModelProvider::new(vec![MockResponse::Text("Paris".into())]);
    let registry = LocalToolRegistry::new();
    let config = AgentConfig::new("test-agent", ModelConfig::new("mock"));
    let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(registry));

    let answer = run_agent(&agent, "What is the capital of France?").await.unwrap();
    assert_eq!(answer, "Paris");
}

/// §22.2: Agent with max_iterations=1 hits the iteration cap.
///
/// Script: provider always returns ToolUse (never a final answer).
/// Expected: `MaxIterationsReached` error after 1 iteration.
#[tokio::test]
async fn react_loop_max_iterations_guard() {
    let provider = MockModelProvider::new(vec![
        MockResponse::ToolUse {
            id: "c1".into(),
            name: "echo".into(),
            input: serde_json::json!({ "text": "a" }),
        },
        MockResponse::ToolUse {
            id: "c2".into(),
            name: "echo".into(),
            input: serde_json::json!({ "text": "b" }),
        },
    ]);

    let registry = LocalToolRegistry::new().register(EchoTool);
    let config = AgentConfig::new("capped", ModelConfig::new("mock")).with_max_iterations(1);
    let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(registry));

    let result = run_agent(&agent, "loop forever").await;
    assert!(result.is_err(), "expected MaxIterationsReached error");
}

// ─────────────────────────────────────────────────────────────────────────────
// §22.3 – EvaluatorOptimizerLoop and PlanAndExecute integration
// ─────────────────────────────────────────────────────────────────────────────

/// §22.3: EvaluatorOptimizerLoop exits when the threshold is met.
///
/// Two-phase scorer: first call → 0.4, second call → 1.0.
/// Threshold = 0.9, max_iterations = 5.
/// Expected: loop terminates after exactly 2 iterations.
#[tokio::test]
async fn evaluator_optimizer_integration() {
    struct TwoPhaseScorer {
        call_count: Mutex<usize>,
    }

    #[async_trait]
    impl Scorer for TwoPhaseScorer {
        fn name(&self) -> &str {
            "two_phase"
        }
        async fn score(&self, _: &ScoreInput) -> Result<ScoreResult> {
            let mut c = self.call_count.lock().unwrap();
            *c += 1;
            if *c == 1 {
                Ok(ScoreResult { score: 0.4, reason: "needs work".into() })
            } else {
                Ok(ScoreResult { score: 1.0, reason: "perfect".into() })
            }
        }
    }

    let scorer = TwoPhaseScorer { call_count: Mutex::new(0) };
    let eval_opt = EvaluatorOptimizerLoop::new(0.9, 5);

    let result = eval_opt
        .run(
            "test task",
            |_prompt, _feedback| async { Ok("attempt".to_string()) },
            &scorer,
        )
        .await
        .unwrap();

    assert_eq!(result.iterations, 2);
    assert!((result.score - 1.0).abs() < 1e-9);
    assert_eq!(result.history.len(), 2);
}

/// §22.3: PlanAndExecute runs all planned steps and synthesizes a final answer.
#[tokio::test]
async fn plan_and_execute_integration() {
    let result = PlanAndExecute::new(10)
        .run(
            "research rust",
            |_task| async { Ok(vec!["gather data".into(), "summarize".into()]) },
            |step, _ctx| async move { Ok(format!("completed: {step}")) },
            |task, results| async move { Ok(format!("answer for {task}: {results}")) },
        )
        .await
        .unwrap();

    assert_eq!(result.steps.len(), 2);
    assert!(result.steps[0].result.as_deref().unwrap().contains("gather data"));
    assert!(result.steps[1].result.as_deref().unwrap().contains("summarize"));
    assert!(result.final_answer.contains("research rust"));
}

// ─────────────────────────────────────────────────────────────────────────────
// §22.4 – FilteredToolExecutor — allow-list enforcement
// ─────────────────────────────────────────────────────────────────────────────

/// §22.4: FilteredToolExecutor blocks tools not in the allow-list.
///
/// Registry has "echo"; allow-list contains only "safe_tool".
/// Calling "echo" must return a ToolResult with `is_error = true`.
#[tokio::test]
async fn filtered_executor_blocks_disallowed() {
    let registry = LocalToolRegistry::new().register(EchoTool);
    let filtered =
        FilteredToolExecutor::new(Arc::new(registry), vec!["safe_tool".to_string()]);

    let block = filtered
        .execute("echo", "call_01", serde_json::json!({ "text": "hi" }))
        .await;

    assert!(
        matches!(block, ContentBlock::ToolResult { is_error: true, .. }),
        "echo should be blocked, got {block:?}"
    );
}

/// §22.4: FilteredToolExecutor passes tools in the allow-list.
///
/// Registry has "echo"; allow-list contains "echo".
/// Calling "echo" must succeed and return the echoed text.
#[tokio::test]
async fn filtered_executor_passes_allowed() {
    let registry = LocalToolRegistry::new().register(EchoTool);
    let filtered = FilteredToolExecutor::new(Arc::new(registry), vec!["echo".to_string()]);

    let block = filtered
        .execute("echo", "call_01", serde_json::json!({ "text": "hi" }))
        .await;

    let ok = match &block {
        ContentBlock::ToolResult { is_error, content, .. } => {
            !is_error && content.contains("hi")
        }
        _ => false,
    };
    assert!(ok, "Expected successful echo result, got {block:?}");
}

/// §22.4: `tool_definitions()` only exposes allowed tools.
#[test]
fn filtered_executor_hides_disallowed_definitions() {
    let registry = LocalToolRegistry::new().register(EchoTool);
    let filtered =
        FilteredToolExecutor::new(Arc::new(registry), vec!["other_tool".to_string()]);

    let defs = filtered.tool_definitions();
    assert!(
        defs.is_empty(),
        "echo should not appear in definitions when allow-list is ['other_tool']"
    );
}

/// §22.4: `tool_definitions()` exposes matching tools.
#[test]
fn filtered_executor_exposes_allowed_definitions() {
    let registry = LocalToolRegistry::new().register(EchoTool);
    let filtered = FilteredToolExecutor::new(Arc::new(registry), vec!["echo".to_string()]);

    let defs = filtered.tool_definitions();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "echo");
}
