//! Integration tests for three-tier memory progression (§22.6).
//!
//! Exercises the full Tier 1 → Tier 2 → Tier 3 pipeline:
//!
//! | Test                                    | Checklist |
//! |-----------------------------------------|-----------|
//! | `tier1_to_tier2_consolidation`          | §22.6     |
//! | `tier2_heat_promotes_to_tier3`          | §22.6     |
//! | `tier3_semantic_search_returns_closest` | §22.6     |
//! | `memory_manager_deduplication`          | §22.6     |
//! | `full_pipeline_tier1_tier2_tier3`       | §22.6     |

use rustmastra_memory::{
    EpisodicMemory, Memory, MemoryAction, MemoryEntry, MemoryManager, MidTermMemory,
    SemanticMemory,
};

// ─────────────────────────────────────────────────────────────────────────────
// §22.6 Tier 1 → Tier 2: consolidation
// ─────────────────────────────────────────────────────────────────────────────

/// §22.6: Episodic entries are consolidated into a Tier-2 summary.
///
/// Verifies that `MidTermMemory::consolidate()` correctly folds a slice of
/// Tier-1 entries into a single `SummaryEntry` whose content contains all
/// source entry texts.
#[tokio::test]
async fn tier1_to_tier2_consolidation() {
    let tier1 = EpisodicMemory::new(100);

    // Populate Tier 1.
    tier1.store(MemoryEntry::new("Rust ownership rules prevent data races")).await.unwrap();
    tier1.store(MemoryEntry::new("The borrow checker runs at compile time")).await.unwrap();
    tier1.store(MemoryEntry::new("Lifetimes annotate reference validity")).await.unwrap();

    let recent = tier1.recent(10).await.unwrap();
    assert_eq!(recent.len(), 3);

    // Consolidate into Tier 2 (no LLM, raw concatenation fallback).
    let tier2 = MidTermMemory::new(None, /*heat_threshold=*/ 2);
    let summary = tier2.consolidate(&recent, None).await.unwrap();

    // All source content appears in the summary.
    assert!(summary.content.contains("ownership"));
    assert!(summary.content.contains("borrow checker"));
    assert!(summary.content.contains("Lifetimes"));
    assert_eq!(summary.source_ids.len(), 3);
    assert_eq!(summary.heat, 0, "fresh summary should have heat=0");
}

/// §22.6: Tier-2 summary with LLM-supplied text (simulated compression).
#[tokio::test]
async fn tier2_custom_summary_text() {
    let tier2 = MidTermMemory::new(None, 2);
    let entries = vec![
        MemoryEntry::new("raw fact A"),
        MemoryEntry::new("raw fact B"),
    ];
    let compressed = "Compressed: A and B are stored.".to_string();
    let summary = tier2.consolidate(&entries, Some(compressed.clone())).await.unwrap();

    assert_eq!(summary.content, compressed);
    assert_eq!(summary.source_ids.len(), 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// §22.6 Tier 2 → Tier 3: heat-based promotion
// ─────────────────────────────────────────────────────────────────────────────

/// §22.6: After repeated retrieval a Tier-2 summary exceeds the heat threshold
/// and appears in `hot_entries()` — signalling readiness for Tier-3 promotion.
#[tokio::test]
async fn tier2_heat_promotes_to_tier3_candidate() {
    let heat_threshold = 3u32;
    let tier2 = MidTermMemory::new(None, heat_threshold);

    let entries = vec![MemoryEntry::new("Rust async uses poll-based futures")];
    tier2.consolidate(&entries, None).await.unwrap();

    // Before threshold — no hot entries.
    let hot = tier2.hot_entries().await.unwrap();
    assert!(hot.is_empty(), "heat=0, should not be hot yet");

    // Retrieve (bumps heat) until threshold is met.
    for _ in 0..heat_threshold {
        let found = tier2.search_summaries("async", 5).await.unwrap();
        assert!(!found.is_empty());
    }

    let hot = tier2.hot_entries().await.unwrap();
    assert_eq!(hot.len(), 1, "entry should be hot after {} retrievals", heat_threshold);
    assert!(hot[0].heat >= heat_threshold, "heat={}", hot[0].heat);

    // Simulate Tier-3 promotion: store as SemanticMemory entry.
    let tier3 = SemanticMemory::new();
    let mut promoted = MemoryEntry::new(&hot[0].content);
    // Assign a synthetic embedding (dimension 3 for test speed).
    let embedding = vec![1.0_f32, 0.0, 0.0];
    promoted.embedding = Some(embedding.clone());
    tier3.store_with_embedding(promoted, embedding).await.unwrap();

    assert_eq!(tier3.len().await, 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// §22.6 Tier 3: semantic search
// ─────────────────────────────────────────────────────────────────────────────

/// §22.6: Tier-3 cosine similarity search returns the closest entry first.
#[tokio::test]
async fn tier3_semantic_search_returns_closest() {
    let tier3 = SemanticMemory::new();

    // Two entries with orthogonal embeddings.
    let mut e_rust = MemoryEntry::new("Rust borrow checker");
    e_rust.embedding = Some(vec![1.0_f32, 0.0, 0.0]);
    tier3.store_with_embedding(e_rust, vec![1.0, 0.0, 0.0]).await.unwrap();

    let mut e_python = MemoryEntry::new("Python garbage collector");
    e_python.embedding = Some(vec![0.0_f32, 1.0, 0.0]);
    tier3.store_with_embedding(e_python, vec![0.0, 1.0, 0.0]).await.unwrap();

    // Query aligned with "Rust" entry.
    let query = vec![0.9_f32, 0.1, 0.0];
    let results = tier3.semantic_search(&query, 2).await.unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0.content, "Rust borrow checker",
        "Rust entry should rank highest: similarity={}", results[0].1);
    assert!(results[0].1 > results[1].1, "similarity should be strictly decreasing");
}

/// §22.6: Tier-3 search on empty store returns empty result (no panic).
#[tokio::test]
async fn tier3_empty_search_is_safe() {
    let tier3 = SemanticMemory::new();
    let results = tier3.semantic_search(&[1.0, 0.0], 5).await.unwrap();
    assert!(results.is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// §22.6 MemoryManager deduplication
// ─────────────────────────────────────────────────────────────────────────────

/// §22.6: MemoryManager returns Noop for a near-duplicate entry.
#[tokio::test]
async fn memory_manager_blocks_duplicate() {
    let mgr = MemoryManager::new(0.85);
    let existing = vec![MemoryEntry::new("Rust is a systems programming language")];
    let action = mgr.evaluate("Rust is a systems programming language", &existing);
    assert_eq!(action, MemoryAction::Noop, "identical content should be Noop");
}

/// §22.6: MemoryManager returns Add for a genuinely novel entry.
#[tokio::test]
async fn memory_manager_adds_novel_entry() {
    let mgr = MemoryManager::new(0.85);
    let existing = vec![MemoryEntry::new("Tokio is an async runtime")];
    let action = mgr.evaluate("Rayon is a data-parallelism library", &existing);
    assert_eq!(action, MemoryAction::Add);
}

/// §22.6: MemoryManager returns Update when an update keyword appears with overlap.
#[tokio::test]
async fn memory_manager_detects_update() {
    let mgr = MemoryManager::new(0.85);
    let existing = vec![MemoryEntry::new("The default Tokio runtime has 4 threads")];
    let action = mgr.evaluate("The default Tokio runtime now has 8 threads", &existing);
    assert!(
        matches!(action, MemoryAction::Update { .. }),
        "expected Update, got {:?}",
        action
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// §22.6 Full pipeline: Tier 1 → 2 consolidation → 3 semantic search
// ─────────────────────────────────────────────────────────────────────────────

/// §22.6: Full three-tier pipeline in a single test.
///
/// 1. Store facts in Tier 1.
/// 2. Retrieve and consolidate into Tier 2.
/// 3. Bump heat past threshold.
/// 4. Promote hot summary to Tier 3 with a synthetic embedding.
/// 5. Semantic search retrieves the promoted entry.
#[tokio::test]
async fn full_pipeline_tier1_tier2_tier3() {
    // ── Tier 1: store facts ────────────────────────────────────────────────
    let tier1 = EpisodicMemory::new(50);
    for fact in &[
        "Async Rust uses `async`/`await` syntax",
        "Tokio is the de-facto async runtime",
        "Futures in Rust are lazy — they must be polled",
    ] {
        tier1.store(MemoryEntry::new(*fact)).await.unwrap();
    }
    let recent = tier1.recent(10).await.unwrap();
    assert_eq!(recent.len(), 3);

    // ── Tier 2: consolidate ───────────────────────────────────────────────
    let heat_threshold = 2;
    let tier2 = MidTermMemory::new(None, heat_threshold);
    let summary = tier2
        .consolidate(&recent, Some("Async Rust summary: await/tokio/futures".to_string()))
        .await
        .unwrap();
    assert_eq!(summary.source_ids.len(), 3);

    // Bump heat to threshold.
    for _ in 0..heat_threshold {
        tier2.search_summaries("async", 5).await.unwrap();
    }

    let hot = tier2.hot_entries().await.unwrap();
    assert_eq!(hot.len(), 1);
    assert!(hot[0].heat >= heat_threshold as u32);

    // ── Tier 3: promote and search ────────────────────────────────────────
    let tier3 = SemanticMemory::new();

    // Embed "async Rust" concept as unit vector dimension 0.
    let promoted = MemoryEntry::new(&hot[0].content);
    let emb = vec![1.0_f32, 0.0, 0.0];
    tier3.store_with_embedding(promoted, emb.clone()).await.unwrap();

    // A "decoy" — orthogonal to the query.
    let decoy = MemoryEntry::new("Diesel is a synchronous ORM");
    tier3.store_with_embedding(decoy, vec![0.0, 1.0, 0.0]).await.unwrap();

    // Query: aligned with "async Rust" embedding.
    let results = tier3.semantic_search(&emb, 2).await.unwrap();
    assert_eq!(results.len(), 2);
    assert!(
        results[0].0.content.contains("Async Rust"),
        "promoted Async Rust summary should rank first"
    );
    assert!((results[0].1 - 1.0).abs() < 1e-5, "perfect cosine similarity expected");
}
