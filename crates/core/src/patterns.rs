//! High-level agent reasoning patterns (checklist §15.7–§15.8).
//!
//! These are composable building blocks that sit *above* the raw ReAct loop but
//! *below* a full multi-agent workflow.  They accept generic async closures so
//! callers can plug in any LLM call, `run_agent`, or custom logic.
//!
//! | Pattern              | Checklist | Summary                                       |
//! |----------------------|-----------|-----------------------------------------------|
//! | `EvaluatorOptimizer` | §15.7     | Critic scores output; generator refines        |
//! | `PlanAndExecute`     | §15.8     | Planner produces steps; executor runs them     |

use std::future::Future;

use crate::{evaluators::ScoreInput, Result};

// ─────────────────────────────────────────────────────────────────────────────
// EvaluatorOptimizerLoop (§15.7)
// ─────────────────────────────────────────────────────────────────────────────

/// Result from a single [`EvaluatorOptimizerLoop::run`] invocation.
#[derive(Debug, Clone)]
pub struct EvalOptResult {
    /// The best answer found (highest score, or the final iteration's output).
    pub answer: String,
    /// Score of the returned answer in `[0.0, 1.0]`.
    pub score: f64,
    /// How many generate-evaluate cycles were performed (≥ 1).
    pub iterations: usize,
    /// Full history: `(answer, score)` for each iteration, oldest first.
    pub history: Vec<(String, f64)>,
}

/// Evaluator-Optimizer loop: **generate → evaluate → refine → repeat** (§15.7).
///
/// The loop calls a `generator` to produce an answer, scores it with a `scorer`,
/// and if the score is below `threshold`, feeds the critique back to the generator
/// as a "refinement prompt" for the next iteration.
///
/// # Usage
///
/// ```rust,no_run
/// use vanswarm_core::patterns::{EvaluatorOptimizerLoop, EvalOptResult};
/// use vanswarm_core::evaluators::NonEmptyScorer;
///
/// async fn example() {
///     let eval_opt = EvaluatorOptimizerLoop::new(0.8, 3);
///     let result = eval_opt
///         .run(
///             "Explain Rust's ownership model",
///             |input, feedback| async move {
///                 Ok(format!("Answer about: {input}. Feedback: {feedback:?}"))
///             },
///             &NonEmptyScorer,
///         )
///         .await
///         .unwrap();
///     println!("Score: {}, iterations: {}", result.score, result.iterations);
/// }
/// ```
pub struct EvaluatorOptimizerLoop {
    /// Minimum score to accept an answer without further refinement.
    pub threshold: f64,
    /// Maximum number of generate-evaluate cycles (≥ 1).
    pub max_iterations: usize,
}

impl EvaluatorOptimizerLoop {
    pub fn new(threshold: f64, max_iterations: usize) -> Self {
        Self {
            threshold: threshold.clamp(0.0, 1.0),
            max_iterations: max_iterations.max(1),
        }
    }

    /// Run the evaluator-optimizer loop.
    ///
    /// * `input` — the original task string passed to the generator.
    /// * `generate` — async closure: `(task_with_feedback: String, iteration: usize) -> Result<String>`.
    ///   On iteration 0 `task_with_feedback == input`. On subsequent iterations it includes
    ///   the scorer's reason as a refinement hint.
    /// * `scorer` — any [`crate::evaluators::Scorer`] implementation.
    pub async fn run<Gen, GenFut>(
        &self,
        input: &str,
        generate: Gen,
        scorer: &(dyn crate::evaluators::Scorer + Send + Sync),
    ) -> Result<EvalOptResult>
    where
        Gen: Fn(String, Option<String>) -> GenFut,
        GenFut: Future<Output = Result<String>>,
    {
        let mut history: Vec<(String, f64)> = Vec::with_capacity(self.max_iterations);
        let mut best_answer = String::new();
        let mut best_score = f64::NEG_INFINITY;
        let mut feedback: Option<String> = None;

        for iteration in 0..self.max_iterations {
            // Build prompt: inject feedback from previous scorer reason (if any).
            let prompt = if let Some(ref fb) = feedback {
                format!("{input}\n\n[Feedback from previous attempt]: {fb}")
            } else {
                input.to_string()
            };

            let answer = generate(prompt, feedback.clone()).await?;

            let score_input = ScoreInput {
                messages: vec![],
                final_answer: answer.clone(),
                expected: None,
            };
            let score_result = scorer.score(&score_input).await?;

            tracing::debug!(
                iteration,
                score = score_result.score,
                reason = %score_result.reason,
                "EvaluatorOptimizerLoop scored answer"
            );

            history.push((answer.clone(), score_result.score));

            if score_result.score > best_score {
                best_score = score_result.score;
                best_answer = answer;
            }

            if score_result.score >= self.threshold {
                return Ok(EvalOptResult {
                    answer: best_answer,
                    score: best_score,
                    iterations: iteration + 1,
                    history,
                });
            }

            // Pass the critique as feedback for the next generator call.
            feedback = Some(score_result.reason);
        }

        Ok(EvalOptResult {
            answer: best_answer,
            score: best_score,
            iterations: self.max_iterations,
            history,
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PlanAndExecute (§15.8)
// ─────────────────────────────────────────────────────────────────────────────

/// A single step in a [`PlanAndExecuteResult`].
#[derive(Debug, Clone)]
pub struct PlanStep {
    /// 0-based step index.
    pub index: usize,
    /// Natural-language description of what this step does.
    pub description: String,
    /// The output produced by executing this step (`None` if not yet executed).
    pub result: Option<String>,
}

/// Result from a [`PlanAndExecute::run`] invocation.
#[derive(Debug, Clone)]
pub struct PlanAndExecuteResult {
    /// The original task string.
    pub task: String,
    /// All plan steps with their descriptions and results.
    pub steps: Vec<PlanStep>,
    /// Final synthesized answer (returned by the `synthesizer` closure).
    pub final_answer: String,
}

/// Plan-and-Execute agent pattern (§15.8).
///
/// A **planner** produces a list of step descriptions; an **executor** runs
/// each step sequentially, feeding the accumulated context; a **synthesizer**
/// produces the final answer from all step results.
///
/// This is more deterministic than a pure ReAct loop: the plan is fixed upfront,
/// which makes it easier to audit and parallelize later.
///
/// # Usage
///
/// ```rust,no_run
/// use vanswarm_core::patterns::PlanAndExecute;
///
/// async fn example() {
///     let result = PlanAndExecute::new(5)
///         .run(
///             "Research and summarize the Rust 2024 edition features",
///             |task| async move {
///                 Ok(vec![
///                     "Identify key Rust 2024 edition features".to_string(),
///                     "Summarize each feature".to_string(),
///                 ])
///             },
///             |step, ctx| async move {
///                 Ok(format!("Result of [{step}] with context: {ctx}"))
///             },
///             |task, results| async move {
///                 Ok(format!("Final answer for {task}: {results}"))
///             },
///         )
///         .await
///         .unwrap();
///     println!("{}", result.final_answer);
/// }
/// ```
pub struct PlanAndExecute {
    /// Maximum number of steps the planner may produce.
    pub max_steps: usize,
}

impl PlanAndExecute {
    pub fn new(max_steps: usize) -> Self {
        Self { max_steps: max_steps.max(1) }
    }

    /// Run the plan-and-execute pattern.
    ///
    /// # Arguments
    ///
    /// * `task` — the original user task.
    /// * `planner` — async closure: `task: String → Result<Vec<String>>` (list of step descriptions).
    /// * `executor` — async closure: `(step_description: String, context_so_far: String) → Result<String>`.
    ///   `context_so_far` is a newline-joined summary of all previous step results.
    /// * `synthesizer` — async closure: `(task: String, all_step_results: String) → Result<String>`.
    ///   Receives newline-joined step results and returns the final answer.
    pub async fn run<PlanFn, PlanFut, ExecFn, ExecFut, SynthFn, SynthFut>(
        &self,
        task: &str,
        planner: PlanFn,
        executor: ExecFn,
        synthesizer: SynthFn,
    ) -> Result<PlanAndExecuteResult>
    where
        PlanFn: FnOnce(String) -> PlanFut,
        PlanFut: Future<Output = Result<Vec<String>>>,
        ExecFn: Fn(String, String) -> ExecFut,
        ExecFut: Future<Output = Result<String>>,
        SynthFn: FnOnce(String, String) -> SynthFut,
        SynthFut: Future<Output = Result<String>>,
    {
        // 1. Plan
        let plan_descriptions = planner(task.to_string()).await?;
        let plan_descriptions: Vec<String> =
            plan_descriptions.into_iter().take(self.max_steps).collect();

        tracing::debug!(
            task,
            steps = plan_descriptions.len(),
            "PlanAndExecute: plan produced"
        );

        // 2. Execute steps sequentially, accumulating context.
        let mut steps: Vec<PlanStep> = Vec::with_capacity(plan_descriptions.len());
        let mut context_parts: Vec<String> = Vec::new();

        for (i, description) in plan_descriptions.into_iter().enumerate() {
            let context_so_far = context_parts.join("\n");
            let result = executor(description.clone(), context_so_far).await?;

            tracing::debug!(step = i, %description, result_len = result.len(), "PlanAndExecute: step executed");

            context_parts.push(format!("Step {}: {result}", i + 1));
            steps.push(PlanStep { index: i, description, result: Some(result) });
        }

        // 3. Synthesize final answer.
        let all_results = context_parts.join("\n");
        let final_answer = synthesizer(task.to_string(), all_results).await?;

        Ok(PlanAndExecuteResult { task: task.to_string(), steps, final_answer })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluators::{NonEmptyScorer, ScoreResult};

    // ── EvaluatorOptimizerLoop ────────────────────────────────────────────────

    #[tokio::test]
    async fn eval_opt_stops_at_threshold() {
        let eval_opt = EvaluatorOptimizerLoop::new(0.5, 5);
        let result = eval_opt
            .run(
                "test task",
                |_prompt, _feedback| async { Ok("non-empty answer".to_string()) },
                &NonEmptyScorer,
            )
            .await
            .unwrap();
        // NonEmptyScorer returns 1.0 for non-empty → should stop after 1 iteration.
        assert_eq!(result.iterations, 1);
        assert!((result.score - 1.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn eval_opt_exhausts_max_iterations() {
        struct AlwaysZeroScorer;

        #[async_trait::async_trait]
        impl crate::evaluators::Scorer for AlwaysZeroScorer {
            fn name(&self) -> &str {
                "zero"
            }
            async fn score(&self, _: &ScoreInput) -> Result<ScoreResult> {
                Ok(ScoreResult { score: 0.0, reason: "always zero".to_string() })
            }
        }

        let eval_opt = EvaluatorOptimizerLoop::new(0.9, 3);
        let result = eval_opt
            .run(
                "impossible task",
                |_prompt, _feedback| async { Ok("attempt".to_string()) },
                &AlwaysZeroScorer,
            )
            .await
            .unwrap();
        assert_eq!(result.iterations, 3);
        assert_eq!(result.history.len(), 3);
    }

    #[tokio::test]
    async fn eval_opt_passes_feedback_to_generator() {
        // Check that the feedback from the scorer is passed to the second generator call.
        struct FixedFeedbackScorer;

        #[async_trait::async_trait]
        impl crate::evaluators::Scorer for FixedFeedbackScorer {
            fn name(&self) -> &str {
                "fixed_feedback"
            }
            async fn score(&self, input: &ScoreInput) -> Result<ScoreResult> {
                if input.final_answer.contains("improved") {
                    Ok(ScoreResult { score: 1.0, reason: "great".to_string() })
                } else {
                    Ok(ScoreResult { score: 0.1, reason: "needs improvement".to_string() })
                }
            }
        }

        let eval_opt = EvaluatorOptimizerLoop::new(0.9, 5);
        let result = eval_opt
            .run(
                "task",
                |_prompt, feedback| async move {
                    if feedback.as_deref().map_or(false, |f| f.contains("needs improvement")) {
                        Ok("improved answer".to_string())
                    } else {
                        Ok("initial answer".to_string())
                    }
                },
                &FixedFeedbackScorer,
            )
            .await
            .unwrap();
        assert_eq!(result.iterations, 2);
        assert!(result.answer.contains("improved"));
    }

    // ── PlanAndExecute ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn plan_and_execute_runs_steps() {
        let result = PlanAndExecute::new(5)
            .run(
                "count to three",
                |_task| async { Ok(vec!["step one".to_string(), "step two".to_string()]) },
                |step, _ctx| async move { Ok(format!("done: {step}")) },
                |_task, results| async move { Ok(format!("summary: {results}")) },
            )
            .await
            .unwrap();

        assert_eq!(result.steps.len(), 2);
        assert!(result.steps[0].result.as_deref().unwrap().contains("step one"));
        assert!(result.final_answer.contains("summary"));
    }

    #[tokio::test]
    async fn plan_and_execute_respects_max_steps() {
        let result = PlanAndExecute::new(2) // cap at 2
            .run(
                "task",
                |_| async { Ok(vec!["a".into(), "b".into(), "c".into(), "d".into()]) },
                |s, _| async move { Ok(s) },
                |_, r| async move { Ok(r) },
            )
            .await
            .unwrap();
        assert_eq!(result.steps.len(), 2);
    }

    #[tokio::test]
    async fn plan_and_execute_accumulates_context() {
        // Each step should see results from all previous steps in context.
        let seen_contexts: std::sync::Arc<tokio::sync::Mutex<Vec<String>>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(vec![]));
        let seen_clone = seen_contexts.clone();

        PlanAndExecute::new(3)
            .run(
                "task",
                |_| async { Ok(vec!["s1".into(), "s2".into(), "s3".into()]) },
                move |step, ctx| {
                    let seen = seen_clone.clone();
                    async move {
                        seen.lock().await.push(ctx.clone());
                        Ok(format!("r:{step}"))
                    }
                },
                |_, r| async move { Ok(r) },
            )
            .await
            .unwrap();

        let contexts = seen_contexts.lock().await;
        assert!(contexts[0].is_empty()); // first step has no prior context
        assert!(contexts[1].contains("r:s1")); // second step sees first result
        assert!(contexts[2].contains("r:s1") && contexts[2].contains("r:s2"));
    }
}
