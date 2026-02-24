# Memory

This guide covers the **vanswarm-memory** crate: **Memory** trait, **EpisodicMemory**, **MidTermMemory**, **SemanticMemory**, and **MemoryManager** for conflict resolution.

---

## 1. Three-tier design

| Tier | Name     | Storage (current)      | Use case                |
| ---- | -------- | ---------------------- | ----------------------- |
| 1    | Episodic | In-memory VecDeque     | Recent events, FIFO     |
| 2    | Mid-term | NDJSON file (optional) | Summaries, heat-based   |
| 3    | Semantic | In-memory vectors      | Similarity search (RAG) |

Implementations can be swapped (e.g. Redis for Tier 1, Qdrant for Tier 3) by providing different **Memory** impls.

---

## 2. Memory trait

Common interface for all tiers:

```rust
#[async_trait]
pub trait Memory: Send + Sync {
    async fn store(&self, entry: MemoryEntry) -> Result<()>;
    async fn recent(&self, limit: usize) -> Result<Vec<MemoryEntry>>;
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>>;
    async fn delete(&self, id: Uuid) -> Result<()>;
}
```

**MemoryEntry** has: `id`, `content`, `created_at`, `heat`, optional `embedding`.

---

## 3. EpisodicMemory (Tier 1)

In-memory FIFO; when capacity is reached, the oldest entry is dropped. Search is substring match.

```rust
use vanswarm_memory::{EpisodicMemory, MemoryEntry};

let mem = EpisodicMemory::new(1000);
mem.store(MemoryEntry::new("User asked about Rust ownership")).await?;
mem.store(MemoryEntry::new("Agent explained borrow checker")).await?;

let recent = mem.recent(10).await?;
let found = mem.search("ownership", 5).await?;
```

**Time-travel helpers:**

- **entries_before(id)** — entries in chronological order as they were before the given entry.
- **recent_ordered(limit)** — most recent `limit` entries in chronological order (oldest first).

```rust
let before = mem.entries_before(some_entry_id).await?;
let timeline = mem.recent_ordered(20).await?;
```

---

## 4. MidTermMemory (Tier 2)

Disk-backed (optional NDJSON) summaries with **heat** (retrieval count). Used to consolidate episodic entries and promote hot items toward Tier 3.

```rust
use vanswarm_memory::{MidTermMemory, MemoryEntry, SummaryEntry};

let path = std::path::PathBuf::from("/tmp/midterm.json");
let mem = MidTermMemory::new(Some(path), 5); // heat_threshold = 5
mem.load().await?;

// Consolidate episodic entries into a summary
let episodes = vec![
    MemoryEntry::new("Event A"),
    MemoryEntry::new("Event B"),
];
let summary = mem.consolidate(&episodes, Some("Compressed summary".to_string())).await?;

// Retrieve recent summaries (bumps heat)
let recent = mem.recent_summaries(10).await?;

// Entries with heat >= threshold (candidates for Tier 3)
let hot = mem.hot_entries().await?;
```

---

## 5. SemanticMemory (Tier 3)

In-memory vector store; similarity search via **cosine_similarity**. Production would use Qdrant or pgvector.

```rust
use vanswarm_memory::{MemoryEntry, SemanticMemory};

let mem = SemanticMemory::new();
let mut entry = MemoryEntry::new("Rust has ownership");
entry.embedding = Some(vec![0.1, 0.9, 0.2]); // from your embedding model
mem.store_with_embedding(entry, vec![0.1, 0.9, 0.2]).await?;

let query_embedding = vec![0.1, 0.85, 0.2];
let results = mem.semantic_search(&query_embedding, 5).await?;
// results: Vec<(MemoryEntry, f32)> sorted by similarity descending
```

**cosine_similarity** is also exported for custom use:

```rust
use vanswarm_memory::cosine_similarity;
let sim = cosine_similarity(&[1.0, 0.0], &[0.9, 0.1]);
```

---

## 6. MemoryManager (conflict resolution)

**MemoryManager** applies a heuristic (Memory-R1 style) to decide what to do when adding a new fact:

- **Add** — novel fact.
- **Noop** — duplicate (high word-overlap similarity).
- **Update { target_id }** — new fact supersedes an existing entry (update keywords + overlap).
- **Delete { target_id }** — new fact negates an existing entry (negation keywords + overlap).

```rust
use vanswarm_memory::{MemoryEntry, MemoryManager, MemoryAction};

let manager = MemoryManager::new(0.85);
let existing = mem.recent(100).await?;
let action = manager.evaluate("The CEO is now Bob", &existing);
match action {
    MemoryAction::Add => { mem.store(MemoryEntry::new("The CEO is now Bob")).await?; }
    MemoryAction::Update { target_id } => { /* update entry target_id */ }
    MemoryAction::Delete { target_id } => { mem.delete(target_id).await?; }
    MemoryAction::Noop => {}
}
```

---

## 7. Full example: episodic + search

```rust
use vanswarm_memory::{EpisodicMemory, Memory, MemoryEntry};

#[tokio::main]
async fn main() -> vanswarm_core::Result<()> {
    let mem = EpisodicMemory::new(100);
    mem.store(MemoryEntry::new("User: What is Rust?")).await?;
    mem.store(MemoryEntry::new("Agent: Rust is a systems language.")).await?;
    mem.store(MemoryEntry::new("User: Tell me about ownership.")).await?;

    let hits = mem.search("ownership", 5).await?;
    println!("Found {} entries", hits.len());
    for e in &hits {
        println!("  {}: {}", e.id, e.content);
    }
    Ok(())
}
```

---

## 8. Next steps

- Use memory inside an agent loop (e.g. store each turn, search before answering): [02-building-an-agent](02-building-an-agent.md).
- Architecture: [documentation/architecture/03-memory.md](../architecture/03-memory.md).
