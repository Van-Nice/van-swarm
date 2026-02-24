//! Property-based tests for journal replay determinism (§22.10).
//!
//! These tests verify that the durable-execution journal satisfies its
//! core invariants regardless of the shape of the input:
//!
//! | Property                                      | Checklist |
//! |-----------------------------------------------|-----------|
//! | `replay_determinism`                          | §22.10    |
//! | `idempotent_put`                              | §22.10    |
//! | `load_all_sorted`                             | §22.10    |
//! | `resume_returns_journaled_values_not_live`    | §22.10    |
//! | `multiple_restarts_produce_identical_output`  | §22.10    |

use std::sync::Arc;

use proptest::prelude::*;
use vanswarm_core::durable::{
    DurableContext, InMemoryJournal, JournalBackend, JournalEntry, JournalKind,
};

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Run `proptest!` with an async body executed on a single-threaded Tokio runtime.
///
/// The macro forwards the `Result<(), TestCaseError>` returned by the async
/// block back to `proptest!` so `prop_assert!` / `prop_assume!` work correctly.
macro_rules! async_proptest {
    ($(#[$attr:meta])* fn $name:ident($($arg:pat in $strategy:expr),+) $body:block) => {
        proptest! {
            $(#[$attr])*
            #[test]
            fn $name($($arg in $strategy),+) {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                // `?` propagates TestCaseError; proptest! appends Ok(()) for us.
                rt.block_on(async move { $body })?;
            }
        }
    };
}

/// Build a pre-populated `InMemoryJournal` from a slice of result strings.
async fn build_journal(workflow_id: &str, results: &[String]) -> Arc<InMemoryJournal> {
    let journal = InMemoryJournal::new();
    for (seq, result) in results.iter().enumerate() {
        let entry = JournalEntry {
            seq: seq as u64,
            kind: JournalKind::Custom { label: format!("step-{}", seq) },
            result: serde_json::json!(result),
            recorded_at: chrono::Utc::now(),
            duration_ms: 1,
        };
        journal.put(workflow_id, entry).await.unwrap();
    }
    journal
}

/// Run `n` `ctx.run_once` calls and collect the results.
async fn run_n(ctx: &DurableContext, n: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let label = format!("step-{}", i);
        let live_val = format!("live-{}", i);
        let v: String = ctx
            .run_once(label, || {
                let v = live_val.clone();
                async move { Ok(v) }
            })
            .await
            .unwrap();
        out.push(v);
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// P1 — Replay determinism
// ─────────────────────────────────────────────────────────────────────────────

async_proptest! {
    /// §22.10 P1: Replaying a pre-populated journal always returns the same
    /// sequence of values, on every replay pass.
    fn replay_determinism(
        results in prop::collection::vec("[a-zA-Z0-9 _-]{1,24}", 1usize..=8)
    ) {
        let wid = "wf-replay";
        let journal = build_journal(wid, &results).await;
        let n = results.len();

        for _pass in 0..2 {
            let ctx = DurableContext::resume(
                wid,
                Arc::clone(&journal) as Arc<dyn JournalBackend>,
                None,
            )
            .await
            .unwrap();
            let got = run_n(&ctx, n).await;
            prop_assert_eq!(&got, &results);
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// P2 — Idempotent put
// ─────────────────────────────────────────────────────────────────────────────

async_proptest! {
    /// §22.10 P2: Writing the same journal entry multiple times is a no-op —
    /// `load_all` returns the entry exactly once.
    fn idempotent_put(
        result in "[a-zA-Z0-9]{1,32}"
    ) {
        let wid = "wf-idem";
        let journal = InMemoryJournal::new();
        let entry = JournalEntry {
            seq: 0,
            kind: JournalKind::Custom { label: "step".into() },
            result: serde_json::json!(result),
            recorded_at: chrono::Utc::now(),
            duration_ms: 0,
        };
        // Write the same entry three times.
        journal.put(wid, entry.clone()).await.unwrap();
        journal.put(wid, entry.clone()).await.unwrap();
        journal.put(wid, entry).await.unwrap();

        let all = journal.load_all(wid).await.unwrap();
        prop_assert_eq!(all.len(), 1usize);
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// P3 — load_all sorted
// ─────────────────────────────────────────────────────────────────────────────

async_proptest! {
    /// §22.10 P3: `load_all` always returns entries sorted by `seq` regardless
    /// of the order they were inserted.
    fn load_all_sorted(
        n in 1usize..=10,
        seed in any::<u64>()
    ) {
        let wid = "wf-sorted";
        let journal = InMemoryJournal::new();

        // Build seq 0..n and shuffle deterministically with an LCG.
        let mut seqs: Vec<u64> = (0..n as u64).collect();
        let mut state = seed;
        for i in (1..seqs.len()).rev() {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let j = (state >> 33) as usize % (i + 1);
            seqs.swap(i, j);
        }

        for seq in seqs {
            let entry = JournalEntry {
                seq,
                kind: JournalKind::Custom { label: format!("step-{}", seq) },
                result: serde_json::json!(seq),
                recorded_at: chrono::Utc::now(),
                duration_ms: 0,
            };
            journal.put(wid, entry).await.unwrap();
        }

        let all = journal.load_all(wid).await.unwrap();
        prop_assert_eq!(all.len(), n);
        let is_sorted = all.windows(2).all(|w| w[0].seq < w[1].seq);
        prop_assert!(is_sorted, "load_all returned entries out of order");
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// P4 — Resume returns journaled values, not live fallbacks
// ─────────────────────────────────────────────────────────────────────────────

async_proptest! {
    /// §22.10 P4: When a `DurableContext` is resumed from a complete journal,
    /// it returns the stored values — NOT the live computation fallback.
    fn resume_returns_journaled_values_not_live(
        journaled in prop::collection::vec("[a-z]{4,12}", 1usize..=6)
    ) {
        let wid = "wf-resume-vs-live";
        let journal = build_journal(wid, &journaled).await;
        let n = journaled.len();

        let ctx = DurableContext::resume(
            wid,
            Arc::clone(&journal) as Arc<dyn JournalBackend>,
            None,
        )
        .await
        .unwrap();

        for i in 0..n {
            let journaled_value = journaled[i].clone();
            let live_value = format!("LIVE-SHOULD-NOT-APPEAR-{}", i);
            let result: String = ctx
                .run_once(format!("step-{}", i), || {
                    let v = live_value.clone();
                    async move { Ok(v) }
                })
                .await
                .unwrap();
            prop_assert_eq!(
                &result,
                &journaled_value,
                "step {}: got '{}' but expected journaled '{}'",
                i,
                result,
                journaled_value
            );
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// P5 — Multiple restarts produce identical outputs
// ─────────────────────────────────────────────────────────────────────────────

async_proptest! {
    /// §22.10 P5: Replaying a journal 5 times always yields the same values.
    fn multiple_restarts_produce_identical_output(
        results in prop::collection::vec("[a-z0-9]{2,16}", 1usize..=5)
    ) {
        let wid = "wf-multi-restart";
        let journal = build_journal(wid, &results).await;
        let n = results.len();

        let first_run = {
            let ctx = DurableContext::resume(wid, Arc::clone(&journal) as Arc<dyn JournalBackend>, None)
                .await.unwrap();
            run_n(&ctx, n).await
        };

        for restart in 1usize..5 {
            let ctx = DurableContext::resume(wid, Arc::clone(&journal) as Arc<dyn JournalBackend>, None)
                .await.unwrap();
            let this_run = run_n(&ctx, n).await;
            prop_assert_eq!(
                &this_run,
                &first_run,
                "restart {} produced different output",
                restart
            );
        }
        Ok(())
    }
}
