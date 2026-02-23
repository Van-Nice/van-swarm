//! § 3 — Log-centric Durable Execution (Restate-inspired Replay pattern).
//!
//! ## Design
//!
//! Every non-deterministic operation (tool calls, sleep, timestamps) is
//! intercepted by `DurableContext`, which maintains a monotonically-ordered
//! journal:
//!
//! ```text
//! ctx.call_tool("fetch", args)
//!   ├── check journal[seq]
//!   │     hit  → return cached result   (REPLAY path, zero I/O)
//!   │     miss → execute → append to journal atomically
//!   └── return result
//! ```
//!
//! On crash-restart the workflow function is re-run from the **beginning**
//! (no coroutine serialisation required) and the journal fills in every
//! already-completed side-effect in O(1) per step.
//!
//! ## Replay vs Snapshot
//! We chose **Replay** (event-sourced journal) over **Snapshot** (WASM
//! memory serialisation) because it:
//! * requires no special runtime support,
//! * keeps storage portable (NDJSON → S3 / any KV store),
//! * allows reconstructing any historical state for debugging.
//!
//! ## Storage tiers  (checklist §3.15 / §18.7)
//! | Tier | Implementation          | Use case             |
//! |------|-------------------------|----------------------|
//! | 1    | `InMemoryJournal`       | unit tests           |
//! | 2    | `FileJournal`           | local dev / CI       |
//! | 3    | *(future) RocksDB / S3* | production platform  |

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument};

use crate::{traits::tool::ToolExecutor, Result};

// ─────────────────────────────────────────────────────────────────────────────
// Journal entry (§3.2)
// ─────────────────────────────────────────────────────────────────────────────

/// Kind of operation recorded in the journal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JournalKind {
    /// A tool call (name + args → result string).
    ToolCall { tool_name: String },
    /// A `ctx.sleep()` call (duration stored for audit; wall-clock is skipped on replay).
    Sleep { duration_ms: u64 },
    /// A deterministic timestamp snapshot.
    Timestamp,
    /// Any arbitrary side-effect wrapped with `ctx.run_once`.
    Custom { label: String },
}

/// A single, immutable record in the workflow journal.
///
/// `seq` is the primary key; it is assigned by `DurableContext` in strict
/// ascending order and must never be reused within a workflow run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Monotonically increasing sequence number (0-based).
    pub seq: u64,
    /// What kind of operation was recorded.
    pub kind: JournalKind,
    /// The serialised result of the operation.
    /// `Null` for operations that have no meaningful output (e.g. sleep).
    pub result: serde_json::Value,
    /// Wall-clock time when the entry was first written.
    pub recorded_at: DateTime<Utc>,
    /// Actual execution time on the first run (ms); 0 on replay.
    pub duration_ms: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// JournalBackend trait (§3.15)
// ─────────────────────────────────────────────────────────────────────────────

/// Persistent storage for journal entries.
///
/// Implementations swap out the underlying store without changing any
/// workflow code.
#[async_trait]
pub trait JournalBackend: Send + Sync {
    /// Retrieve the entry at `seq`, if it exists.
    async fn get(&self, workflow_id: &str, seq: u64) -> Result<Option<JournalEntry>>;

    /// Atomically append an entry (§3.7).
    /// Must be idempotent: writing the same `(workflow_id, seq)` twice is a
    /// no-op (or returns the existing entry).
    async fn put(&self, workflow_id: &str, entry: JournalEntry) -> Result<()>;

    /// Return all entries for `workflow_id` sorted by `seq`.
    /// Used during recovery (§3.9) to pre-populate the replay cache.
    async fn load_all(&self, workflow_id: &str) -> Result<Vec<JournalEntry>>;

    /// Delete all entries for `workflow_id` (used by tests / cleanup).
    async fn clear(&self, workflow_id: &str) -> Result<()>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Tier 1: InMemoryJournal
// ─────────────────────────────────────────────────────────────────────────────

/// Ephemeral journal backed by a `HashMap` protected by a `RwLock`.
///
/// Ideal for unit tests; entries are lost when the process exits.
#[derive(Default)]
pub struct InMemoryJournal {
    // key: (workflow_id, seq) → entry
    store: tokio::sync::RwLock<std::collections::HashMap<(String, u64), JournalEntry>>,
}

impl InMemoryJournal {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

#[async_trait]
impl JournalBackend for InMemoryJournal {
    async fn get(&self, workflow_id: &str, seq: u64) -> Result<Option<JournalEntry>> {
        let store = self.store.read().await;
        Ok(store.get(&(workflow_id.to_owned(), seq)).cloned())
    }

    async fn put(&self, workflow_id: &str, entry: JournalEntry) -> Result<()> {
        let mut store = self.store.write().await;
        // Idempotent: skip if already present.
        store.entry((workflow_id.to_owned(), entry.seq)).or_insert(entry);
        Ok(())
    }

    async fn load_all(&self, workflow_id: &str) -> Result<Vec<JournalEntry>> {
        let store = self.store.read().await;
        let mut entries: Vec<JournalEntry> = store
            .iter()
            .filter(|((wid, _), _)| wid == workflow_id)
            .map(|(_, e)| e.clone())
            .collect();
        entries.sort_by_key(|e| e.seq);
        Ok(entries)
    }

    async fn clear(&self, workflow_id: &str) -> Result<()> {
        let mut store = self.store.write().await;
        store.retain(|(wid, _), _| wid != workflow_id);
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tier 2: FileJournal (NDJSON WAL)
// ─────────────────────────────────────────────────────────────────────────────

/// Append-only NDJSON journal stored in `{dir}/{workflow_id}.journal`.
///
/// Each line is a `JournalEntry` serialised to JSON.
/// On recovery, the file is scanned top-to-bottom to rebuild the in-memory
/// cache; subsequent operations use the in-memory layer for fast reads.
pub struct FileJournal {
    dir: std::path::PathBuf,
    /// In-memory write-through cache populated on first access.
    cache: tokio::sync::RwLock<std::collections::HashMap<String, Vec<JournalEntry>>>,
}

impl FileJournal {
    pub fn new(dir: impl Into<std::path::PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            dir: dir.into(),
            cache: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        })
    }

    fn path_for(&self, workflow_id: &str) -> std::path::PathBuf {
        // Sanitise the workflow ID so it's a safe filename.
        let safe: String = workflow_id.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        self.dir.join(format!("{safe}.journal"))
    }

    /// Ensure the directory exists.
    async fn ensure_dir(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.dir).await?;
        Ok(())
    }

    /// Load the journal file into cache (idempotent).
    async fn warm_cache(&self, workflow_id: &str) -> Result<()> {
        {
            let cache = self.cache.read().await;
            if cache.contains_key(workflow_id) {
                return Ok(());
            }
        }

        let path = self.path_for(workflow_id);
        let mut entries = Vec::new();

        if path.exists() {
            let content = tokio::fs::read_to_string(&path).await?;
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_str::<JournalEntry>(line) {
                    Ok(e) => entries.push(e),
                    Err(e) => {
                        tracing::warn!(error = %e, line, "Skipping corrupt journal line");
                    }
                }
            }
            entries.sort_by_key(|e| e.seq);
        }

        let mut cache = self.cache.write().await;
        cache.insert(workflow_id.to_owned(), entries);
        Ok(())
    }
}

#[async_trait]
impl JournalBackend for FileJournal {
    async fn get(&self, workflow_id: &str, seq: u64) -> Result<Option<JournalEntry>> {
        self.warm_cache(workflow_id).await?;
        let cache = self.cache.read().await;
        Ok(cache
            .get(workflow_id)
            .and_then(|v| v.iter().find(|e| e.seq == seq))
            .cloned())
    }

    async fn put(&self, workflow_id: &str, entry: JournalEntry) -> Result<()> {
        self.ensure_dir().await?;
        self.warm_cache(workflow_id).await?;

        // Idempotency check.
        {
            let cache = self.cache.read().await;
            if cache.get(workflow_id).map(|v| v.iter().any(|e| e.seq == entry.seq)).unwrap_or(false) {
                return Ok(());
            }
        }

        // Append to file (WAL semantics).
        let line = serde_json::to_string(&entry)? + "\n";
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path_for(workflow_id))
            .await?;
        tokio::fs::write(self.path_for(workflow_id), {
            // We re-read + append to keep it simple; a production impl
            // would use O_APPEND to avoid the race.
            let existing = tokio::fs::read_to_string(self.path_for(workflow_id))
                .await
                .unwrap_or_default();
            format!("{existing}{line}")
        })
        .await?;

        // Update cache.
        let mut cache = self.cache.write().await;
        cache.entry(workflow_id.to_owned()).or_default().push(entry);

        Ok(())
    }

    async fn load_all(&self, workflow_id: &str) -> Result<Vec<JournalEntry>> {
        self.warm_cache(workflow_id).await?;
        let cache = self.cache.read().await;
        Ok(cache.get(workflow_id).cloned().unwrap_or_default())
    }

    async fn clear(&self, workflow_id: &str) -> Result<()> {
        let path = self.path_for(workflow_id);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }
        let mut cache = self.cache.write().await;
        cache.remove(workflow_id);
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DurableContext (§3.3–3.7)
// ─────────────────────────────────────────────────────────────────────────────

/// The context passed into every `#[workflow]`-annotated function.
///
/// All operations that cross the determinism boundary go through this type.
/// Pure computation (sorting, parsing) needs no journaling.
///
/// # Sequence numbers
/// The `seq` counter increments atomically for each journaled call.  The
/// workflow function **must** call context methods in the same order on
/// every run for replay to succeed.  Using non-deterministic logic (e.g.
/// `if rand::random() { ctx.call_tool(...) }`) breaks replay and is
/// explicitly disallowed.
pub struct DurableContext {
    /// Unique identifier for this workflow run (used as journal partition key).
    pub workflow_id: String,
    journal: Arc<dyn JournalBackend>,
    executor: Option<Arc<dyn ToolExecutor>>,
    /// Monotonically increasing sequence counter.
    /// `AtomicU64` keeps `DurableContext: Sync` without a `Mutex`.
    seq: AtomicU64,
}

impl DurableContext {
    /// Create a fresh context for a new workflow run.
    pub fn new(
        workflow_id: impl Into<String>,
        journal: Arc<dyn JournalBackend>,
        executor: Option<Arc<dyn ToolExecutor>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            workflow_id: workflow_id.into(),
            journal,
            executor,
            seq: AtomicU64::new(0),
        })
    }

    /// Create a context that replays from an existing journal.
    ///
    /// The sequence counter starts at 0; cached entries are served in order.
    /// Call this on restart to recover an in-progress workflow.
    pub async fn resume(
        workflow_id: impl Into<String>,
        journal: Arc<dyn JournalBackend>,
        executor: Option<Arc<dyn ToolExecutor>>,
    ) -> Result<Arc<Self>> {
        let wid = workflow_id.into();
        // Eagerly load the journal so the first `get` is a cache hit.
        let entries = journal.load_all(&wid).await?;
        info!(workflow_id = %wid, replaying = entries.len(), "Resuming workflow from journal");
        Ok(Arc::new(Self {
            workflow_id: wid,
            journal,
            executor,
            seq: AtomicU64::new(0),
        }))
    }

    // ── Core journaling primitive ─────────────────────────────────────────

    /// Execute `f` exactly once per `seq` position, journaling the result.
    ///
    /// On replay: returns the cached value without calling `f`.
    /// On first run: calls `f`, records the result, then returns it.
    ///
    /// `T` must be `Serialize + DeserializeOwned` so results survive the
    /// journal round-trip.
    #[instrument(skip(self, f, label), fields(workflow_id = %self.workflow_id))]
    pub async fn run_once<F, Fut, T>(
        &self,
        label: impl Into<String>,
        f: F,
    ) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
        T: Serialize + serde::de::DeserializeOwned,
    {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let label = label.into();

        // §3.6 – Check journal before executing any side effect.
        if let Some(entry) = self.journal.get(&self.workflow_id, seq).await? {
            debug!(seq, label, "Replaying from journal");
            let value: T = serde_json::from_value(entry.result)?;
            return Ok(value);
        }

        // First run – execute and persist.
        let start = std::time::Instant::now();
        let result = f().await?;
        let duration_ms = start.elapsed().as_millis() as u64;

        // §3.7 – Append result atomically.
        let entry = JournalEntry {
            seq,
            kind: JournalKind::Custom { label },
            result: serde_json::to_value(&result)?,
            recorded_at: Utc::now(),
            duration_ms,
        };
        self.journal.put(&self.workflow_id, entry).await?;

        debug!(seq, duration_ms, "Journaled new entry");
        Ok(result)
    }

    // ── High-level helpers ────────────────────────────────────────────────

    /// Execute a named tool call through the registered `ToolExecutor`.
    ///
    /// The result is journaled; on replay the executor is bypassed.
    #[instrument(skip(self, args), fields(workflow_id = %self.workflow_id, tool = %name))]
    pub async fn call_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<String> {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);

        // Replay path.
        if let Some(entry) = self.journal.get(&self.workflow_id, seq).await? {
            debug!(seq, tool = name, "Replaying tool call from journal");
            return Ok(serde_json::from_value(entry.result)?);
        }

        // Live path.
        let executor = self.executor.as_ref().ok_or_else(|| {
            crate::FrameworkError::agent("workflow", "no ToolExecutor registered on DurableContext")
        })?;

        let call_id = crate::message::new_tool_call_id();
        let start = std::time::Instant::now();
        let block = executor.execute(name, &call_id, args).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        let result_str = match &block {
            crate::message::ContentBlock::ToolResult { content, is_error: false, .. } => {
                content.clone()
            }
            crate::message::ContentBlock::ToolResult { content, is_error: true, .. } => {
                return Err(crate::FrameworkError::tool_exec(name, content.clone()));
            }
            _ => String::new(),
        };

        self.journal
            .put(
                &self.workflow_id,
                JournalEntry {
                    seq,
                    kind: JournalKind::ToolCall { tool_name: name.to_owned() },
                    result: serde_json::json!(result_str),
                    recorded_at: Utc::now(),
                    duration_ms,
                },
            )
            .await?;

        Ok(result_str)
    }

    /// Sleep for `duration`, but skip the wait on replay.
    ///
    /// The original wall-clock duration is stored in the journal for audit
    /// purposes, but the sleep is **not** re-executed during replay.
    #[instrument(skip(self), fields(workflow_id = %self.workflow_id))]
    pub async fn sleep(&self, duration: std::time::Duration) -> Result<()> {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);

        // §3.6 – On replay skip the actual sleep entirely.
        if self.journal.get(&self.workflow_id, seq).await?.is_some() {
            debug!(seq, "Skipping sleep on replay");
            return Ok(());
        }

        // Live path.
        tokio::time::sleep(duration).await;

        self.journal
            .put(
                &self.workflow_id,
                JournalEntry {
                    seq,
                    kind: JournalKind::Sleep { duration_ms: duration.as_millis() as u64 },
                    result: serde_json::Value::Null,
                    recorded_at: Utc::now(),
                    duration_ms: duration.as_millis() as u64,
                },
            )
            .await?;

        Ok(())
    }

    /// Return a deterministic timestamp.
    ///
    /// On first run: captures `Utc::now()` and journals it.
    /// On replay: returns the original captured value.
    ///
    /// This prevents the classic durable-execution bug where `now()` diverges
    /// between runs (§3.5 / §2.14).
    #[instrument(skip(self), fields(workflow_id = %self.workflow_id))]
    pub async fn timestamp(&self) -> Result<DateTime<Utc>> {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);

        if let Some(entry) = self.journal.get(&self.workflow_id, seq).await? {
            debug!(seq, "Replaying timestamp from journal");
            let ts: DateTime<Utc> = serde_json::from_value(entry.result)?;
            return Ok(ts);
        }

        let now = Utc::now();
        self.journal
            .put(
                &self.workflow_id,
                JournalEntry {
                    seq,
                    kind: JournalKind::Timestamp,
                    result: serde_json::to_value(&now)?,
                    recorded_at: now,
                    duration_ms: 0,
                },
            )
            .await?;

        Ok(now)
    }

    /// Return the current sequence number (useful for diagnostics).
    pub fn current_seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (§3.14)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulates running a workflow to completion, killing the process, then
    /// restarting and verifying that the same results are returned without
    /// re-executing side effects.
    #[tokio::test]
    async fn replay_produces_same_result() {
        let journal = InMemoryJournal::new();
        let wid = "wf-replay-test";

        // ── First run ──────────────────────────────────────────────────────
        let mut call_count = 0usize;
        {
            let ctx = DurableContext::new(wid, journal.clone(), None);
            let result: String = ctx
                .run_once("step-1", || async {
                    call_count += 1;
                    Ok("hello from step 1".to_string())
                })
                .await
                .unwrap();
            assert_eq!(result, "hello from step 1");
            assert_eq!(call_count, 1);
        }

        // ── Simulated restart (same journal, new context) ──────────────────
        {
            let ctx = DurableContext::resume(wid, journal.clone(), None).await.unwrap();
            let mut replay_call_count = 0usize;
            let result: String = ctx
                .run_once("step-1", || async {
                    replay_call_count += 1; // must NOT be called on replay
                    Ok("different value that should be ignored".to_string())
                })
                .await
                .unwrap();

            // The journal value is returned; the closure is never called.
            assert_eq!(result, "hello from step 1");
            assert_eq!(replay_call_count, 0, "side effect must not re-execute on replay");
        }
    }

    #[tokio::test]
    async fn timestamp_is_deterministic_across_runs() {
        let journal = InMemoryJournal::new();
        let wid = "wf-ts-test";

        let ctx1 = DurableContext::new(wid, journal.clone(), None);
        let ts1 = ctx1.timestamp().await.unwrap();

        // Small delay to ensure real time has advanced.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        let ctx2 = DurableContext::resume(wid, journal.clone(), None).await.unwrap();
        let ts2 = ctx2.timestamp().await.unwrap();

        assert_eq!(ts1, ts2, "replayed timestamp must equal original");
    }

    #[tokio::test]
    async fn sleep_is_skipped_on_replay() {
        let journal = InMemoryJournal::new();
        let wid = "wf-sleep-test";

        // First run: sleep 100ms.
        let ctx1 = DurableContext::new(wid, journal.clone(), None);
        ctx1.sleep(std::time::Duration::from_millis(100)).await.unwrap();

        // Replay: should complete almost instantly.
        let ctx2 = DurableContext::resume(wid, journal.clone(), None).await.unwrap();
        let start = std::time::Instant::now();
        ctx2.sleep(std::time::Duration::from_millis(100)).await.unwrap();
        let elapsed = start.elapsed();

        assert!(elapsed.as_millis() < 50, "sleep must be skipped on replay, took {}ms", elapsed.as_millis());
    }

    #[tokio::test]
    async fn file_journal_persists_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let journal = FileJournal::new(dir.path());
        let wid = "wf-file-test";

        // Write two entries.
        let ctx = DurableContext::new(wid, journal.clone(), None);
        let _: u32 = ctx.run_once("a", || async { Ok(42u32) }).await.unwrap();
        let _: u32 = ctx.run_once("b", || async { Ok(99u32) }).await.unwrap();

        // Create a new journal pointed at the same directory.
        let journal2 = FileJournal::new(dir.path());
        let entries = journal2.load_all(wid).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].seq, 0);
        assert_eq!(entries[1].seq, 1);
    }
}
