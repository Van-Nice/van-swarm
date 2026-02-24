# Evaluation

This guide covers **evaluators** in **openswarm-core**: **Scorer**, **ScoreInput** / **ScoreResult**, **batch_score**, **SPL** (Success weighted by Path Length), and **RunMetrics**.

---

## 1. RunMetrics (from the agent)

Use **run_agent_with_metrics** to get **RunMetrics** for each run:

```rust
use openswarm_core::react::run_agent_with_metrics;

let (answer, metrics) = run_agent_with_metrics(&agent, "What is the capital of France?").await?;
// metrics.iterations   — number of ReAct steps
// metrics.tool_call_count — total tool calls (path length L_exec for SPL)
```

---

## 2. Scorer trait

A **Scorer** assigns a **score** in [0, 1] and a **reason** to a single run:

```rust
#[async_trait]
pub trait Scorer: Send + Sync {
    fn name(&self) -> &str;
    async fn score(&self, input: &ScoreInput) -> Result<ScoreResult>;
}
```

**ScoreInput** contains:

- **messages** — full conversation.
- **final_answer** — the agent’s final text.
- **expected** — optional ground truth (for supervised evals).

**ScoreResult** has **score: f64** and **reason: String**.

---

## 3. Built-in scorers

| Scorer                                                                                                      | Use                                                                         |
| ----------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| **NonEmptyScorer**                                                                                          | Passes if `final_answer` is non-empty.                                      |
| **ContainsScorer**                                                                                          | Passes if `final_answer` contains `expected` (case-insensitive by default). |
| **LlmJudgeScorer**                                                                                          | Uses an LLM to judge quality (e.g. relevance, correctness).                 |
| **CompletenessScorer**, **RelevancyScorer**, **FaithfulnessScorer**, **BiasScorer**, **ToolAccuracyScorer** | Domain-specific LLM or rule-based.                                          |

Example:

```rust
use openswarm_core::evaluators::{ContainsScorer, ScoreInput, Scorer};

let scorer = ContainsScorer::default();
let input = ScoreInput {
    messages: vec![],
    final_answer: "The capital of France is Paris.".to_string(),
    expected: Some("Paris".to_string()),
};
let result = scorer.score(&input).await?;
println!("Score: {} Reason: {}", result.score, result.reason);
```

---

## 4. batch_score — run scorer on many cases

For CI or benchmarks, run a scorer on a list of inputs:

```rust
use openswarm_core::evaluators::{batch_score, ContainsScorer, ScoreInput};

let scorer = ContainsScorer::default();
let inputs = vec![
    ScoreInput {
        messages: vec![],
        final_answer: "Paris".into(),
        expected: Some("Paris".into()),
    },
    ScoreInput {
        messages: vec![],
        final_answer: "Lyon".into(),
        expected: Some("Paris".into()),
    },
];
let results = batch_score(&scorer, &inputs).await?;
// results: Vec<ScoreResult>; aggregate (e.g. mean score) as needed
```

---

## 5. SPL (Success weighted by Path Length)

SPL rewards correct answers with **shorter** tool-use paths. You need:

- **score** S_i ∈ [0, 1] for each run (e.g. from a **Scorer**).
- **path_length** L_exec = `RunMetrics.tool_call_count`.
- **optimal_path_length** L_opt (from your benchmark definition).

```rust
use openswarm_core::evaluators::{BenchmarkTask, SplRun, spl};

let runs = vec![
    SplRun {
        score: 1.0,
        path_length: 3,
        optimal_path_length: 2,
    },
    SplRun {
        score: 1.0,
        path_length: 2,
        optimal_path_length: 2,
    },
];
let spl_value = spl(&runs);
// spl_value = (1/N) * Σ (S_i * L_opt / max(L_exec, L_opt))
```

**BenchmarkTask** (optional) holds **expected** and **optimal_path_length** for a single task; you can build **SplRun** from your agent’s **RunMetrics** and the task’s optimal path.

---

## 6. Full example: eval loop with RunMetrics and ContainsScorer

```rust
use openswarm_core::{
    evaluators::{ContainsScorer, ScoreInput, Scorer, SplRun, spl},
    react::run_agent_with_metrics,
};

let agent = /* ... */;
let scorer = ContainsScorer::default();
let tasks = vec![
    ("What is 2+2?", "4"),
    ("What is the capital of France?", "Paris"),
];
let mut runs = Vec::new();

for (question, expected) in tasks {
    let (answer, metrics) = run_agent_with_metrics(&agent, question).await?;
    let input = ScoreInput {
        messages: vec![],
        final_answer: answer,
        expected: Some(expected.to_string()),
    };
    let result = scorer.score(&input).await?;
    runs.push(SplRun {
        score: result.score,
        path_length: metrics.tool_call_count,
        optimal_path_length: 1, // example
    });
}

let spl_value = spl(&runs);
println!("SPL: {}", spl_value);
```

---

## 7. Next steps

- More scorers and golden datasets: see `openswarm_core::evaluators` and [documentation/architecture/01-core.md](../architecture/01-core.md).
- Traces and cost: [02-building-an-agent](02-building-an-agent.md) (run_agent_traced).
