//! # rustmastra-memory
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
//!
//! Stub implementation; full build in §8 of the checklist.

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
    async fn store(&self, entry: MemoryEntry) -> rustmastra_core::Result<()>;

    /// Retrieve the most recent `limit` entries.
    async fn recent(&self, limit: usize) -> rustmastra_core::Result<Vec<MemoryEntry>>;

    /// Full-text or semantic search.
    async fn search(&self, query: &str, limit: usize)
        -> rustmastra_core::Result<Vec<MemoryEntry>>;

    /// Delete an entry by ID.
    async fn delete(&self, id: Uuid) -> rustmastra_core::Result<()>;
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
    pub async fn entries_before(&self, id: Uuid) -> rustmastra_core::Result<Vec<MemoryEntry>> {
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
    pub async fn recent_ordered(&self, limit: usize) -> rustmastra_core::Result<Vec<MemoryEntry>> {
        let entries = self.entries.read().await;
        let n = entries.len().saturating_sub(limit);
        Ok(entries.range(n..).cloned().collect())
    }
}

#[async_trait]
impl Memory for EpisodicMemory {
    async fn store(&self, entry: MemoryEntry) -> rustmastra_core::Result<()> {
        let mut entries = self.entries.write().await;
        if entries.len() >= self.max_entries {
            entries.pop_front();
        }
        entries.push_back(entry);
        Ok(())
    }

    async fn recent(&self, limit: usize) -> rustmastra_core::Result<Vec<MemoryEntry>> {
        let entries = self.entries.read().await;
        Ok(entries.iter().rev().take(limit).cloned().collect())
    }

    async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> rustmastra_core::Result<Vec<MemoryEntry>> {
        let entries = self.entries.read().await;
        let q = query.to_lowercase();
        Ok(entries
            .iter()
            .filter(|e| e.content.to_lowercase().contains(&q))
            .take(limit)
            .cloned()
            .collect())
    }

    async fn delete(&self, id: Uuid) -> rustmastra_core::Result<()> {
        let mut entries = self.entries.write().await;
        entries.retain(|e| e.id != id);
        Ok(())
    }
}
