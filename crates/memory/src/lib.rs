//! # vanswarm-memory
//!
//! Three-tier cognitive memory subsystem (checklist §8).
//!
//! ## Tiers
//!
//! | Tier | Name       | Storage            | Consolidation           |
//! |------|------------|--------------------|-------------------------|
//! | 1    | Episodic   | Redis / in-memory  | Sliding window / FIFO   |
//! | 2    | Mid-term   | Disk / summaries   | Heat-based promotion    |
//! | 3    | Semantic   | Qdrant / pgvector  | Embedding-based RAG     |

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// Memory entry
// ─────────────────────────────────────────────────────────────────────────────

/// A single fact or event stored in memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: Uuid,
    pub content: String,
    pub created_at: DateTime<Utc>,
    /// How many times this entry has been retrieved (heat score for §8.6).
    pub heat: u32,
    /// Optional vector embedding (Tier 3).
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,
}

impl MemoryEntry {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            content: content.into(),
            created_at: Utc::now(),
            heat: 0,
            embedding: None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Memory trait
// ─────────────────────────────────────────────────────────────────────────────

/// Common interface for all three memory tiers.
///
/// Implementing this trait allows hot-swapping backends between
/// development (SQLite) and production (Redis / Qdrant / pgvector).
#[async_trait]
pub trait Memory: Send + Sync {
    /// Store a new entry.
    async fn store(&self, entry: MemoryEntry) -> vanswarm_core::Result<()>;

    /// Retrieve the most recent `limit` entries.
    async fn recent(&self, limit: usize) -> vanswarm_core::Result<Vec<MemoryEntry>>;

    /// Full-text or semantic search.
    async fn search(&self, query: &str, limit: usize)
        -> vanswarm_core::Result<Vec<MemoryEntry>>;

    /// Delete an entry by ID.
    async fn delete(&self, id: Uuid) -> vanswarm_core::Result<()>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Tier 1: EpisodicMemory (in-process stub)
// ─────────────────────────────────────────────────────────────────────────────

/// In-memory episodic store – FIFO sliding window.
///
/// Production replacement: Redis-backed (checklist §8.2 / §8.12).
pub struct EpisodicMemory {
    max_entries: usize,
    entries: tokio::sync::RwLock<std::collections::VecDeque<MemoryEntry>>,
}

impl EpisodicMemory {
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            entries: tokio::sync::RwLock::new(std::collections::VecDeque::new()),
        }
    }

    /// Time-travel query (§8.3): return entries in chronological order as they were *before*
    /// the given entry. Use to reconstruct state at a past decision point.
    ///
    /// Returns all entries that appear before `id` in the buffer (oldest first). If `id` is not
    /// found, returns an empty vec.
    pub async fn entries_before(&self, id: Uuid) -> vanswarm_core::Result<Vec<MemoryEntry>> {
        let entries = self.entries.read().await;
        let before: Vec<MemoryEntry> = entries
            .iter()
            .take_while(|e| e.id != id)
            .cloned()
            .collect();
        Ok(before)
    }

    /// Return the most recent `limit` entries in **chronological order** (oldest first).
    /// Useful for reconstructing a linear timeline up to "now".
    pub async fn recent_ordered(&self, limit: usize) -> vanswarm_core::Result<Vec<MemoryEntry>> {
        let entries = self.entries.read().await;
        let n = entries.len().saturating_sub(limit);
        Ok(entries.range(n..).cloned().collect())
    }
}

#[async_trait]
impl Memory for EpisodicMemory {
    async fn store(&self, entry: MemoryEntry) -> vanswarm_core::Result<()> {
        let mut entries = self.entries.write().await;
        if entries.len() >= self.max_entries {
            entries.pop_front();
        }
        entries.push_back(entry);
        Ok(())
    }

    async fn recent(&self, limit: usize) -> vanswarm_core::Result<Vec<MemoryEntry>> {
        let entries = self.entries.read().await;
        Ok(entries.iter().rev().take(limit).cloned().collect())
    }

    async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> vanswarm_core::Result<Vec<MemoryEntry>> {
        let entries = self.entries.read().await;
        let q = query.to_lowercase();
        Ok(entries
            .iter()
            .filter(|e| e.content.to_lowercase().contains(&q))
            .take(limit)
            .cloned()
            .collect())
    }

    async fn delete(&self, id: Uuid) -> vanswarm_core::Result<()> {
        let mut entries = self.entries.write().await;
        entries.retain(|e| e.id != id);
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tier 2: MidTermMemory (disk-backed NDJSON, heat-based promotion §8.4-8.7)
// ─────────────────────────────────────────────────────────────────────────────

/// A consolidated summary entry stored in Tier 2.
///
/// Unlike the raw [`MemoryEntry`], summaries are durable (NDJSON on disk)
/// and carry a heat score used to decide promotion to Tier 3.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryEntry {
    pub id: Uuid,
    pub content: String,
    /// IDs of the Tier-1 episodic entries that were folded into this summary.
    pub source_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    /// Access frequency: incremented on every retrieval (§8.6 heat score).
    pub heat: u32,
    /// Optional embedding for Tier-3 promotion (skipped in NDJSON).
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,
}

impl SummaryEntry {
    pub fn new(content: impl Into<String>, source_ids: Vec<Uuid>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            content: content.into(),
            source_ids,
            created_at: now,
            last_accessed: now,
            heat: 0,
            embedding: None,
        }
    }
}

/// Mid-term memory: disk-backed NDJSON with heat-based retrieval (§8.4–§8.7).
///
/// On every read the matched entry's `heat` counter is incremented and
/// the file is rewritten. Entries whose heat exceeds `heat_threshold`
/// are surfaced via [`MidTermMemory::hot_entries`] for Tier-3 promotion.
///
/// # Persistence
///
/// Pass `Some(path)` to persist to an NDJSON file (one JSON object per line).
/// Pass `None` to keep everything in memory (useful for tests).
pub struct MidTermMemory {
    summaries: tokio::sync::RwLock<Vec<SummaryEntry>>,
    path: Option<std::path::PathBuf>,
    /// Minimum heat score to be considered a Tier-3 promotion candidate (§8.7).
    pub heat_threshold: u32,
}

impl MidTermMemory {
    /// Create a new mid-term memory store.
    ///
    /// * `path` — NDJSON file for durability; `None` = in-memory only.
    /// * `heat_threshold` — entries with `heat >= threshold` are returned by
    ///   [`hot_entries`][Self::hot_entries] as Tier-3 promotion candidates.
    pub fn new(path: Option<std::path::PathBuf>, heat_threshold: u32) -> Self {
        Self {
            summaries: tokio::sync::RwLock::new(Vec::new()),
            path,
            heat_threshold,
        }
    }

    /// Load existing summaries from the NDJSON file (if a path was given).
    ///
    /// Call once during startup. Silently succeeds if the file does not exist yet.
    pub async fn load(&self) -> vanswarm_core::Result<()> {
        let Some(ref path) = self.path else { return Ok(()); };
        let text = match tokio::fs::read_to_string(path).await {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let mut summaries = self.summaries.write().await;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let entry: SummaryEntry = serde_json::from_str(line)?;
            summaries.push(entry);
        }
        Ok(())
    }

    /// Flush the current in-memory state back to the NDJSON file.
    async fn flush(&self, summaries: &[SummaryEntry]) -> vanswarm_core::Result<()> {
        let Some(ref path) = self.path else { return Ok(()); };
        let mut lines = String::new();
        for s in summaries {
            let line = serde_json::to_string(s)?;
            lines.push_str(&line);
            lines.push('\n');
        }
        tokio::fs::write(path, lines).await?;
        Ok(())
    }

    /// Store a pre-built summary entry.
    pub async fn store_summary(&self, entry: SummaryEntry) -> vanswarm_core::Result<()> {
        let mut summaries = self.summaries.write().await;
        summaries.push(entry);
        self.flush(&summaries).await
    }

    /// Consolidate a slice of episodic entries into a single summary (§8.4 / §8.5).
    ///
    /// If `summary_text` is provided it is used as the content directly (e.g. the
    /// output of an LLM compression call). Otherwise the contents of all `entries`
    /// are concatenated with newlines as a minimal fallback.
    ///
    /// Returns the [`SummaryEntry`] that was stored.
    pub async fn consolidate(
        &self,
        entries: &[MemoryEntry],
        summary_text: Option<String>,
    ) -> vanswarm_core::Result<SummaryEntry> {
        let content = summary_text.unwrap_or_else(|| {
            entries.iter().map(|e| e.content.as_str()).collect::<Vec<_>>().join("\n")
        });
        let source_ids: Vec<Uuid> = entries.iter().map(|e| e.id).collect();
        let summary = SummaryEntry::new(content, source_ids);
        self.store_summary(summary.clone()).await?;
        Ok(summary)
    }

    /// Return entries whose heat score is at or above `heat_threshold` (§8.7).
    ///
    /// These are candidates for promotion to Tier 3 (semantic memory).
    pub async fn hot_entries(&self) -> vanswarm_core::Result<Vec<SummaryEntry>> {
        let summaries = self.summaries.read().await;
        Ok(summaries.iter().filter(|s| s.heat >= self.heat_threshold).cloned().collect())
    }

    /// Retrieve the most recent `limit` summaries and bump their heat counters.
    pub async fn recent_summaries(
        &self,
        limit: usize,
    ) -> vanswarm_core::Result<Vec<SummaryEntry>> {
        let mut summaries = self.summaries.write().await;
        let n = summaries.len().saturating_sub(limit);
        for s in summaries[n..].iter_mut() {
            s.heat = s.heat.saturating_add(1);
            s.last_accessed = Utc::now();
        }
        let result: Vec<SummaryEntry> = summaries[n..].to_vec();
        self.flush(&summaries).await?;
        Ok(result)
    }

    /// Full-text search across summaries; bumps heat on matches.
    pub async fn search_summaries(
        &self,
        query: &str,
        limit: usize,
    ) -> vanswarm_core::Result<Vec<SummaryEntry>> {
        let mut summaries = self.summaries.write().await;
        let q = query.to_lowercase();
        let matched_indices: Vec<usize> = summaries
            .iter()
            .enumerate()
            .filter(|(_, s)| s.content.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .take(limit)
            .collect();

        let mut result = Vec::with_capacity(matched_indices.len());
        for &i in &matched_indices {
            summaries[i].heat = summaries[i].heat.saturating_add(1);
            summaries[i].last_accessed = Utc::now();
            result.push(summaries[i].clone());
        }
        self.flush(&summaries).await?;
        Ok(result)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tier 3: SemanticMemory (cosine-similarity vector store §8.8-§8.11)
// ─────────────────────────────────────────────────────────────────────────────

/// Compute the cosine similarity between two equal-length f32 slices.
///
/// Returns `None` if either vector is empty or the lengths differ.
///
/// ```
/// use vanswarm_memory::cosine_similarity;
/// let a = [1.0_f32, 0.0, 0.0];
/// let b = [0.0_f32, 1.0, 0.0];
/// assert_eq!(cosine_similarity(&a, &b), Some(0.0));
/// let same = [1.0_f32, 1.0];
/// assert!((cosine_similarity(&same, &same).unwrap() - 1.0).abs() < 1e-6);
/// ```
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return None;
    }
    Some(dot / (norm_a * norm_b))
}

/// Tier-3 semantic memory: in-memory cosine-similarity vector store (§8.8–§8.11).
///
/// Production replacement: Qdrant or pgvector.
///
/// Entries are stored together with their embedding vectors. Similarity
/// search is O(n) over the stored vectors — adequate for up to ~100k
/// entries before a dedicated ANNS index is needed.
pub struct SemanticMemory {
    store: tokio::sync::RwLock<Vec<(MemoryEntry, Vec<f32>)>>,
}

impl SemanticMemory {
    pub fn new() -> Self {
        Self { store: tokio::sync::RwLock::new(Vec::new()) }
    }

    /// Store an entry with its pre-computed embedding.
    pub async fn store_with_embedding(
        &self,
        mut entry: MemoryEntry,
        embedding: Vec<f32>,
    ) -> vanswarm_core::Result<()> {
        entry.embedding = Some(embedding.clone());
        self.store.write().await.push((entry, embedding));
        Ok(())
    }

    /// Return the top-`limit` entries sorted by cosine similarity to `query_embedding`.
    ///
    /// Returns `(entry, similarity_score)` pairs in descending order.
    pub async fn semantic_search(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> vanswarm_core::Result<Vec<(MemoryEntry, f32)>> {
        let store = self.store.read().await;
        let mut scored: Vec<(f32, usize)> = store
            .iter()
            .enumerate()
            .filter_map(|(i, (_, emb))| cosine_similarity(query_embedding, emb).map(|s| (s, i)))
            .collect();
        // sort descending by similarity
        scored.sort_unstable_by(|(a, _), (b, _)| {
            b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
        });
        let results = scored
            .into_iter()
            .take(limit)
            .map(|(score, i)| (store[i].0.clone(), score))
            .collect();
        Ok(results)
    }

    /// Total number of stored entries.
    pub async fn len(&self) -> usize {
        self.store.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.store.read().await.is_empty()
    }
}

impl Default for SemanticMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Memory for SemanticMemory {
    async fn store(&self, entry: MemoryEntry) -> vanswarm_core::Result<()> {
        let embedding = entry.embedding.clone().unwrap_or_default();
        self.store_with_embedding(entry, embedding).await
    }

    async fn recent(&self, limit: usize) -> vanswarm_core::Result<Vec<MemoryEntry>> {
        let store = self.store.read().await;
        Ok(store.iter().rev().take(limit).map(|(e, _)| e.clone()).collect())
    }

    async fn search(&self, query: &str, limit: usize) -> vanswarm_core::Result<Vec<MemoryEntry>> {
        let store = self.store.read().await;
        let q = query.to_lowercase();
        Ok(store
            .iter()
            .filter(|(e, _)| e.content.to_lowercase().contains(&q))
            .take(limit)
            .map(|(e, _)| e.clone())
            .collect())
    }

    async fn delete(&self, id: Uuid) -> vanswarm_core::Result<()> {
        self.store.write().await.retain(|(e, _)| e.id != id);
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MemoryManager — conflict resolution (Memory-R1 §8.9)
// ─────────────────────────────────────────────────────────────────────────────

/// The action the [`MemoryManager`] recommends after evaluating a new fact
/// against existing memories (Memory-R1 conflict resolution, §8.9).
#[derive(Debug, Clone, PartialEq)]
pub enum MemoryAction {
    /// The new fact is novel — add it to memory.
    Add,
    /// The new fact supersedes an existing entry — update `target_id`.
    Update { target_id: Uuid },
    /// The new fact contradicts/negates an existing entry — delete `target_id`.
    Delete { target_id: Uuid },
    /// The new fact is redundant — do nothing.
    Noop,
}

/// Heuristic conflict resolver for new facts (§8.9 / Memory-R1).
///
/// The resolver inspects existing [`MemoryEntry`] items and returns the most
/// appropriate [`MemoryAction`]:
///
/// * **Noop** — if the new fact is highly similar (≥ `similarity_threshold`)
///   to an existing entry (deduplication).
/// * **Update** — if the new fact contains an update keyword ("updated",
///   "now", "changed", "new") *and* shares significant keyword overlap with
///   an existing entry.
/// * **Delete** — if the new fact contains a negation keyword ("no longer",
///   "not anymore", "removed", "deleted") with keyword overlap.
/// * **Add** — no match: the fact is genuinely novel.
///
/// Pass `similarity_threshold` in `[0.0, 1.0]`. A value of `0.85` is a
/// good starting point; lower it to be more aggressive about deduplication.
pub struct MemoryManager {
    /// Minimum Jaccard word-overlap to treat two entries as duplicates.
    pub similarity_threshold: f64,
}

impl MemoryManager {
    pub fn new(similarity_threshold: f64) -> Self {
        Self { similarity_threshold: similarity_threshold.clamp(0.0, 1.0) }
    }

    /// Evaluate a new fact against `existing` entries and return the recommended action.
    ///
    /// This is a pure, synchronous heuristic — no I/O or LLM calls.
    pub fn evaluate(&self, new_fact: &str, existing: &[MemoryEntry]) -> MemoryAction {
        let new_lower = new_fact.to_lowercase();
        let new_words: std::collections::HashSet<&str> = new_lower.split_whitespace().collect();

        // Duplicate check: near-identical content → Noop.
        for entry in existing {
            let sim = self.word_overlap_similarity(&new_lower, &entry.content.to_lowercase());
            if sim >= self.similarity_threshold {
                return MemoryAction::Noop;
            }
        }

        const DELETE_KEYWORDS: &[&str] =
            &["no longer", "not anymore", "removed", "deleted", "false"];
        const UPDATE_KEYWORDS: &[&str] =
            &["updated", "now ", "changed to", "new ", "corrected"];

        for entry in existing {
            let entry_lower = entry.content.to_lowercase();
            let entry_words: std::collections::HashSet<&str> =
                entry_lower.split_whitespace().collect();
            let overlap = new_words.intersection(&entry_words).count();
            let min_words = new_words.len().min(entry_words.len()).max(1);
            // Require ≥40 % keyword overlap before considering Update/Delete.
            if overlap as f64 / min_words as f64 >= 0.4 {
                if DELETE_KEYWORDS.iter().any(|k| new_lower.contains(k)) {
                    return MemoryAction::Delete { target_id: entry.id };
                }
                if UPDATE_KEYWORDS.iter().any(|k| new_lower.contains(k)) {
                    return MemoryAction::Update { target_id: entry.id };
                }
            }
        }

        MemoryAction::Add
    }

    /// Simple Jaccard word-overlap similarity ∈ [0, 1].
    fn word_overlap_similarity(&self, a: &str, b: &str) -> f64 {
        let a_words: std::collections::HashSet<&str> = a.split_whitespace().collect();
        let b_words: std::collections::HashSet<&str> = b.split_whitespace().collect();
        let intersection = a_words.intersection(&b_words).count();
        let union = a_words.union(&b_words).count();
        if union == 0 {
            return 1.0;
        }
        intersection as f64 / union as f64
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Tier 1 ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn episodic_fifo_eviction() {
        let mem = EpisodicMemory::new(2);
        mem.store(MemoryEntry::new("a")).await.unwrap();
        mem.store(MemoryEntry::new("b")).await.unwrap();
        mem.store(MemoryEntry::new("c")).await.unwrap(); // evicts "a"
        let recent = mem.recent(10).await.unwrap();
        assert_eq!(recent.len(), 2);
        // most recent first
        assert_eq!(recent[0].content, "c");
        assert_eq!(recent[1].content, "b");
    }

    #[tokio::test]
    async fn episodic_search() {
        let mem = EpisodicMemory::new(100);
        mem.store(MemoryEntry::new("Rust is fast")).await.unwrap();
        mem.store(MemoryEntry::new("Python is easy")).await.unwrap();
        let results = mem.search("rust", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "Rust is fast");
    }

    // ── Tier 2 ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn midterm_consolidate_no_path() {
        let mem = MidTermMemory::new(None, 3);
        let entries = vec![MemoryEntry::new("fact A"), MemoryEntry::new("fact B")];
        let summary = mem.consolidate(&entries, None).await.unwrap();
        assert!(summary.content.contains("fact A"));
        assert!(summary.content.contains("fact B"));
        assert_eq!(summary.source_ids.len(), 2);
    }

    #[tokio::test]
    async fn midterm_consolidate_custom_text() {
        let mem = MidTermMemory::new(None, 3);
        let entries = vec![MemoryEntry::new("raw")];
        let summary = mem
            .consolidate(&entries, Some("LLM compressed".to_string()))
            .await
            .unwrap();
        assert_eq!(summary.content, "LLM compressed");
    }

    #[tokio::test]
    async fn midterm_heat_threshold() {
        let mem = MidTermMemory::new(None, 2);
        // Store one entry with heat = 0, then bump it via recent_summaries twice.
        let entry = SummaryEntry::new("hot fact", vec![]);
        mem.store_summary(entry).await.unwrap();
        mem.recent_summaries(1).await.unwrap(); // heat → 1
        mem.recent_summaries(1).await.unwrap(); // heat → 2
        let hot = mem.hot_entries().await.unwrap();
        assert_eq!(hot.len(), 1);
        assert_eq!(hot[0].heat, 2);
    }

    #[tokio::test]
    async fn midterm_search_bumps_heat() {
        let mem = MidTermMemory::new(None, 10);
        mem.store_summary(SummaryEntry::new("vector databases", vec![]))
            .await
            .unwrap();
        let results = mem.search_summaries("vector", 5).await.unwrap();
        assert_eq!(results.len(), 1);
        // Heat was bumped during the search.
        let all = mem.recent_summaries(10).await.unwrap();
        assert_eq!(all[0].heat, 2); // 1 from search + 1 from recent_summaries call
    }

    // ── Tier 3 ────────────────────────────────────────────────────────────────

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = [1.0_f32, 0.0];
        let b = [0.0_f32, 1.0];
        assert_eq!(cosine_similarity(&a, &b), Some(0.0));
    }

    #[test]
    fn cosine_similarity_identical() {
        let v = [0.6_f32, 0.8];
        let s = cosine_similarity(&v, &v).unwrap();
        assert!((s - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_length_mismatch() {
        assert!(cosine_similarity(&[1.0_f32], &[1.0, 2.0]).is_none());
    }

    #[tokio::test]
    async fn semantic_search_returns_closest() {
        let mem = SemanticMemory::new();
        // entry A aligned with query
        let mut a = MemoryEntry::new("topic A");
        a.embedding = Some(vec![1.0, 0.0]);
        // entry B orthogonal to query
        let mut b = MemoryEntry::new("topic B");
        b.embedding = Some(vec![0.0, 1.0]);

        mem.store(a).await.unwrap();
        mem.store(b).await.unwrap();

        let query = [1.0_f32, 0.0];
        let results = mem.semantic_search(&query, 2).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0.content, "topic A"); // highest similarity first
        assert!((results[0].1 - 1.0).abs() < 1e-6);
    }

    // ── MemoryManager ─────────────────────────────────────────────────────────

    #[test]
    fn manager_add_novel_fact() {
        let mgr = MemoryManager::new(0.85);
        let action = mgr.evaluate("Rust has a borrow checker", &[]);
        assert_eq!(action, MemoryAction::Add);
    }

    #[test]
    fn manager_noop_duplicate() {
        let mgr = MemoryManager::new(0.85);
        let existing = vec![MemoryEntry::new("The capital of France is Paris")];
        let action = mgr.evaluate("The capital of France is Paris", &existing);
        assert_eq!(action, MemoryAction::Noop);
    }

    #[test]
    fn manager_update_keyword() {
        let mgr = MemoryManager::new(0.85);
        let existing = vec![MemoryEntry::new("The CEO of Acme is Alice")];
        let action = mgr.evaluate("The CEO of Acme is now Bob", &existing);
        assert!(matches!(action, MemoryAction::Update { .. }));
    }

    #[test]
    fn manager_delete_keyword() {
        let mgr = MemoryManager::new(0.85);
        let existing = vec![MemoryEntry::new("The office in Berlin is open")];
        let action = mgr.evaluate("The office in Berlin is no longer open", &existing);
        assert!(matches!(action, MemoryAction::Delete { .. }));
    }
}
