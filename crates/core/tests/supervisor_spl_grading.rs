//! §11.11 — Supervisor SPL grading across diverse tasks.
//!
//! Grades the `KeywordRouter` on a labelled benchmark of routing tasks.
//! Each task carries the expected tier and an `optimal_path_length` of 1
//! (routing is a single classification step).
//!
//! | Metric | Description |
//! |--------|-------------|
//! | Per-task score | 1.0 if correctly routed; 0.0 otherwise |
//! | SPL | `(1/N) Σ (score_i × L_opt / max(L_exec, L_opt))` — §11.6 formula |
//! | Target | SPL ≥ 0.70 for the built-in `KeywordRouter::default_keywords()` |
//!
//! Tuning guidance printed on each run shows per-tier breakdown so the
//! keyword lists can be adjusted to close gaps.

use vanswarm_core::{
    evaluators::{spl, SplRun},
    supervisor::{KeywordRouter, Route, Router},
};

// ─────────────────────────────────────────────────────────────────────────────
// Benchmark corpus
// ─────────────────────────────────────────────────────────────────────────────

struct RoutingTask {
    input: &'static str,
    expected: Route,
}

fn routing_benchmark() -> Vec<RoutingTask> {
    vec![
        // ── Tier 1: simple tasks ─────────────────────────────────────────────
        RoutingTask { input: "summarize the following paragraph", expected: Route::Tier1 },
        RoutingTask { input: "translate this sentence to French", expected: Route::Tier1 },
        RoutingTask { input: "what is the capital of France?", expected: Route::Tier1 },
        RoutingTask { input: "format this list as bullet points", expected: Route::Tier1 },
        RoutingTask { input: "what is the current date?", expected: Route::Tier1 },
        RoutingTask { input: "classify the sentiment of this review", expected: Route::Tier1 },
        RoutingTask {
            input: "extract the key topics from this text",
            expected: Route::Tier1,
        },
        // ── Tier 2: planning / tool-use ──────────────────────────────────────
        RoutingTask {
            input: "write a Rust function to parse JSON",
            expected: Route::Tier2,
        },
        RoutingTask {
            input: "debug why this Python script fails",
            expected: Route::Tier2,
        },
        RoutingTask {
            input: "generate a SQL query to join these tables",
            expected: Route::Tier2,
        },
        RoutingTask {
            input: "refactor this code for readability",
            expected: Route::Tier2,
        },
        RoutingTask {
            input: "write unit tests for this function",
            expected: Route::Tier2,
        },
        RoutingTask {
            input: "implement a binary search tree in Rust",
            expected: Route::Tier2,
        },
        // ── Tier 3: complex reasoning / research ─────────────────────────────
        RoutingTask {
            input: "analyze the economic impact of AI on the labour market over the next decade",
            expected: Route::Tier3,
        },
        RoutingTask {
            input: "design a distributed consensus protocol for Byzantine fault tolerance",
            expected: Route::Tier3,
        },
        RoutingTask {
            input: "compare the trade-offs of microservices vs monolith for a fintech startup",
            expected: Route::Tier3,
        },
        RoutingTask {
            input: "derive the time complexity proof for quicksort in the average case",
            expected: Route::Tier3,
        },
        RoutingTask {
            input: "research the latest advances in quantum error correction",
            expected: Route::Tier3,
        },
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// §11.11 test
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn supervisor_spl_grading() {
    let router = KeywordRouter::default_keywords();
    let tasks = routing_benchmark();
    let n = tasks.len();

    let mut spl_runs: Vec<SplRun> = Vec::with_capacity(n);

    // Per-tier tracking for tuning output.
    let mut tier_correct = [0usize; 3]; // [T1, T2, T3]
    let mut tier_total = [0usize; 3];
    let mut misses: Vec<(&str, Route, Route)> = Vec::new();

    for task in &tasks {
        let got = router.route(task.input).await.expect("router should not fail");
        let correct = got == task.expected;
        let score = if correct { 1.0_f64 } else { 0.0 };

        // All routing steps have path_length = 1 and optimal = 1.
        spl_runs.push(SplRun { score, path_length: 1, optimal_path_length: 1 });

        let tier_idx = match task.expected {
            Route::Tier1 => 0,
            Route::Tier2 => 1,
            Route::Tier3 => 2,
        };
        tier_total[tier_idx] += 1;
        if correct {
            tier_correct[tier_idx] += 1;
        } else {
            misses.push((task.input, task.expected.clone(), got));
        }
    }

    let aggregate_spl = spl(&spl_runs);

    // ── Tuning report ─────────────────────────────────────────────────────────
    println!("\n── §11.11 Supervisor SPL grading report ──────────────────────────");
    println!("  Tasks: {}", n);
    println!("  Aggregate SPL: {:.3}", aggregate_spl);
    println!(
        "  Tier 1 accuracy: {}/{} ({:.0}%)",
        tier_correct[0],
        tier_total[0],
        100.0 * tier_correct[0] as f64 / tier_total[0].max(1) as f64
    );
    println!(
        "  Tier 2 accuracy: {}/{} ({:.0}%)",
        tier_correct[1],
        tier_total[1],
        100.0 * tier_correct[1] as f64 / tier_total[1].max(1) as f64
    );
    println!(
        "  Tier 3 accuracy: {}/{} ({:.0}%)",
        tier_correct[2],
        tier_total[2],
        100.0 * tier_correct[2] as f64 / tier_total[2].max(1) as f64
    );
    if !misses.is_empty() {
        println!("  Misrouted tasks:");
        for (input, expected, got) in &misses {
            println!("    [expected {:?}, got {:?}] \"{}\"", expected, got, input);
        }
    }
    println!("──────────────────────────────────────────────────────────────────\n");

    // ── Assertion ─────────────────────────────────────────────────────────────
    assert!(
        aggregate_spl >= 0.70,
        "KeywordRouter aggregate SPL {:.3} is below the 0.70 target — \
         adjust keyword lists in supervisor.rs to improve routing accuracy",
        aggregate_spl
    );
}
