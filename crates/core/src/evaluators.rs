//! Evaluators and scorers for agent runs (§12).
//!
//! The `Scorer` trait defines a pipeline: preprocess → analyze → generateScore → generateReason.
//! Implementations can be deterministic (e.g. code compiles, API 200) or LLM-as-a-Judge.

use async_trait::async_trait;

use crate::Result;

// ─────────────────────────────────────────────────────────────────────────────
// Scorer trait (§12.1)
// ─────────────────────────────────────────────────────────────────────────────

/// Input to a scorer: the run transcript and final answer.
///
/// Extend this struct as needed for richer evals (e.g. expected output, metadata).
#[derive(Debug, Clone, Default)]
pub struct ScoreInput {
    /// Full conversation messages (system, user, assistant, tool turns).
    pub messages: Vec<crate::Message>,
    /// The final answer string returned by the agent.
    pub final_answer: String,
    /// Optional user-provided expected output or rubric (for supervised evals).
    pub expected: Option<String>,
}

/// Result of scoring: a 0–1 score and an optional reason.
#[derive(Debug, Clone)]
pub struct ScoreResult {
    /// Score in [0.0, 1.0]. Higher is better.
    pub score: f64,
    /// Human-readable reason (e.g. for LLM-as-a-Judge or heuristic explanation).
    pub reason: String,
}

/// A scorer evaluates an agent run and returns a normalized score and reason (§12.1).
///
/// Pipeline: preprocess (optional) → analyze → generateScore → generateReason.
/// Implementations can be:
/// * **Deterministic**: e.g. code compiles, API returned 200, output contains expected substring.
/// * **LLM-as-a-Judge**: call a model to rate factuality, tone, relevance.
#[async_trait]
pub trait Scorer: Send + Sync {
    /// Human-readable name for logs and dashboards.
    fn name(&self) -> &str;

    /// Score a single run. Returns a value in [0.0, 1.0] and a reason string.
    async fn score(&self, input: &ScoreInput) -> Result<ScoreResult>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Batch evals for CI (§12.10)
// ─────────────────────────────────────────────────────────────────────────────

/// Run a scorer on N test cases (runExperiment-style for CI).
///
/// Returns one `ScoreResult` per input, in order. Use with `ScoreInput { messages, final_answer, expected }`
/// from agent runs or golden test cases.
pub async fn batch_score(
    scorer: &(dyn Scorer + Send + Sync),
    inputs: Vec<ScoreInput>,
) -> Result<Vec<ScoreResult>> {
    let mut out = Vec::with_capacity(inputs.len());
    for input in inputs {
        out.push(scorer.score(&input).await?);
    }
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// SPL and benchmark tasks (§11.6, §11.7)
// ─────────────────────────────────────────────────────────────────────────────

/// One benchmark task for SPL: optional expected output and optimal path length (§11.7).
///
/// When running N benchmark tasks, provide `optimal_path_length` (L_opt) per task so SPL
/// can reward shorter successful paths.
#[derive(Debug, Clone, Default)]
pub struct BenchmarkTask {
    /// Optional expected substring or rubric for scoring this task.
    pub expected: Option<String>,
    /// Optimal number of tool calls for this task (L_opt in SPL formula).
    pub optimal_path_length: usize,
}

/// Result of one run for SPL: success score and executed path length (§11.6).
#[derive(Debug, Clone)]
pub struct SplRun {
    /// Success score in [0.0, 1.0] (e.g. from a `Scorer`).
    pub score: f64,
    /// Number of tool calls executed (L_exec).
    pub path_length: usize,
    /// Optimal path length for this task (L_opt). Must be ≥ 1 to avoid division issues.
    pub optimal_path_length: usize,
}

/// Compute Success weighted by Path Length: (1/N) * Σ (S_i × L_opt / max(L_exec, L_opt)) (§11.6).
///
/// Rewards success while penalizing excess tool use. Uses `max(L_opt, 1)` when L_opt is 0 to avoid division by zero.
pub fn spl(runs: &[SplRun]) -> f64 {
    if runs.is_empty() {
        return 0.0;
    }
    let n = runs.len() as f64;
    let sum: f64 = runs
        .iter()
        .map(|r| {
            let l_opt = r.optimal_path_length.max(1) as f64;
            let l_exec = r.path_length as f64;
            let ratio = l_opt / l_exec.max(l_opt);
            r.score * ratio
        })
        .sum();
    sum / n
}

// ─────────────────────────────────────────────────────────────────────────────
// Deterministic / heuristic scorers (§12.2)
// ─────────────────────────────────────────────────────────────────────────────

/// Scores 1.0 if the final answer is non-empty (after trim), else 0.0.
///
/// Useful as a minimal sanity check (e.g. agent produced some output).
#[derive(Debug, Default)]
pub struct NonEmptyScorer;

#[async_trait::async_trait]
impl Scorer for NonEmptyScorer {
    fn name(&self) -> &str {
        "non_empty"
    }
    async fn score(&self, input: &ScoreInput) -> Result<ScoreResult> {
        let ok = !input.final_answer.trim().is_empty();
        Ok(ScoreResult {
            score: if ok { 1.0 } else { 0.0 },
            reason: if ok {
                "output is non-empty".into()
            } else {
                "output is empty".into()
            },
        })
    }
}

/// Scores 1.0 if the final answer contains the expected string (from `ScoreInput::expected`).
///
/// Use for deterministic evals: set `expected` to a substring that must appear
/// (e.g. "200 OK", "compilation succeeded"). Case-insensitive by default.
#[derive(Debug)]
pub struct ContainsScorer {
    /// If true, comparison is case-sensitive.
    pub case_sensitive: bool,
}

impl Default for ContainsScorer {
    fn default() -> Self {
        Self { case_sensitive: false }
    }
}

impl ContainsScorer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn case_sensitive(mut self, b: bool) -> Self {
        self.case_sensitive = b;
        self
    }
}

#[async_trait::async_trait]
impl Scorer for ContainsScorer {
    fn name(&self) -> &str {
        "contains"
    }
    async fn score(&self, input: &ScoreInput) -> Result<ScoreResult> {
        let expected = match &input.expected {
            Some(s) => s.as_str(),
            None => {
                return Ok(ScoreResult {
                    score: 0.0,
                    reason: "no expected value set in ScoreInput".into(),
                });
            }
        };
        let answer = input.final_answer.as_str();
        let contains = if self.case_sensitive {
            answer.contains(expected)
        } else {
            answer.to_lowercase().contains(&expected.to_lowercase())
        };
        Ok(ScoreResult {
            score: if contains { 1.0 } else { 0.0 },
            reason: if contains {
                format!("output contains expected substring")
            } else {
                format!("output does not contain expected substring")
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ConstantScorer;

    #[async_trait::async_trait]
    impl Scorer for ConstantScorer {
        fn name(&self) -> &str {
            "constant"
        }
        async fn score(&self, _input: &ScoreInput) -> Result<ScoreResult> {
            Ok(ScoreResult {
                score: 0.5,
                reason: "constant scorer".into(),
            })
        }
    }

    #[tokio::test]
    async fn scorer_trait_works() {
        let s = ConstantScorer;
        let input = ScoreInput::default();
        let out = s.score(&input).await.unwrap();
        assert!((out.score - 0.5).abs() < 1e-9);
        assert_eq!(out.reason, "constant scorer");
    }

    #[tokio::test]
    async fn non_empty_scorer() {
        let s = super::NonEmptyScorer;
        assert!((s.score(&ScoreInput { final_answer: "hi".into(), ..Default::default() }).await.unwrap().score - 1.0).abs() < 1e-9);
        assert!((s.score(&ScoreInput { final_answer: "  ".into(), ..Default::default() }).await.unwrap().score - 0.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn contains_scorer() {
        let s = super::ContainsScorer::default();
        let out = s.score(&ScoreInput {
            final_answer: "The API returned 200 OK.".into(),
            expected: Some("200 OK".into()),
            ..Default::default()
        }).await.unwrap();
        assert!((out.score - 1.0).abs() < 1e-9);
        let out2 = s.score(&ScoreInput {
            final_answer: "The API returned 200 OK.".into(),
            expected: Some("404".into()),
            ..Default::default()
        }).await.unwrap();
        assert!((out2.score - 0.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn batch_score_ci() {
        let scorer = super::NonEmptyScorer;
        let inputs = vec![
            ScoreInput { final_answer: "a".into(), ..Default::default() },
            ScoreInput { final_answer: "".into(), ..Default::default() },
        ];
        let results = super::batch_score(&scorer, inputs).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!((results[0].score - 1.0).abs() < 1e-9);
        assert!((results[1].score - 0.0).abs() < 1e-9);
    }

    #[test]
    fn spl_formula() {
        // One run: success 1.0, path 2, optimal 2 → ratio 1.0 → SPL = 1.0
        let runs = vec![super::SplRun {
            score: 1.0,
            path_length: 2,
            optimal_path_length: 2,
        }];
        assert!((super::spl(&runs) - 1.0).abs() < 1e-9);
        // One run: success 1.0, path 4, optimal 2 → ratio 0.5 → SPL = 0.5
        let runs2 = vec![super::SplRun {
            score: 1.0,
            path_length: 4,
            optimal_path_length: 2,
        }];
        assert!((super::spl(&runs2) - 0.5).abs() < 1e-9);
        // Two runs: (1.0, 2, 2) and (0.0, 1, 1) → (1.0*1.0 + 0.0*1.0)/2 = 0.5
        let runs3 = vec![
            super::SplRun { score: 1.0, path_length: 2, optimal_path_length: 2 },
            super::SplRun { score: 0.0, path_length: 1, optimal_path_length: 1 },
        ];
        assert!((super::spl(&runs3) - 0.5).abs() < 1e-9);
    }
}
