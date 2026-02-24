# Memory crate (vanswarm-memory)

Three-tier cognitive memory subsystem. Currently only **Tier 1 (Episodic)** is implemented as an in-process stub.

## Tier overview (target design)

```mermaid
flowchart TB
    subgraph tier1["Tier 1 — Episodic"]
        E1[EpisodicMemory]
        E2["Storage: Redis / in-memory"]
        E3["Consolidation: sliding window / FIFO"]
    end

    subgraph tier2["Tier 2 — Mid-term"]
        M1["(Future)"]
        M2["Storage: disk / summaries"]
        M3["Consolidation: heat-based promotion"]
    end

    subgraph tier3["Tier 3 — Semantic"]
        S1["(Future)"]
        S2["Storage: Qdrant / pgvector"]
        S3["Consolidation: embedding-based RAG"]
    end

    E1 --> E2
    E1 --> E3
    M1 --> M2
    M1 --> M3
    S1 --> S2
    S1 --> S3
```

| Tier | Name     | Storage (target)  | Consolidation (target) |
| ---- | -------- | ----------------- | ---------------------- |
| 1    | Episodic | Redis / in-memory | Sliding window / FIFO  |
| 2    | Mid-term | Disk / summaries  | Heat-based promotion   |
| 3    | Semantic | Qdrant / pgvector | Embedding-based RAG    |

## Implemented: Memory trait and EpisodicMemory

```mermaid
classDiagram
    class Memory {
        <<async trait>>
        +store(entry)
        +recent(limit)
        +search(query, limit)
        +delete(id)
    }
    class MemoryEntry {
        +id: Uuid
        +content: String
        +created_at: DateTime
        +heat: u32
        +embedding: Option~Vec~f32~~
    }
    class EpisodicMemory {
        -max_entries: usize
        -entries: RwLock~VecDeque~
        +new(max_entries)
    }

    Memory <|.. EpisodicMemory
    Memory --> MemoryEntry
    EpisodicMemory --> MemoryEntry
```

- **Memory** — common interface for all tiers: store, recent, search, delete. Allows swapping backends (e.g. in-memory for dev, Redis for production).
- **MemoryEntry** — id, content, created_at, heat (retrieval count for future heat-based promotion), optional embedding (for Tier 3).
- **EpisodicMemory** — in-memory `VecDeque`: FIFO, max capacity; when full, oldest entry is dropped. Search is simple substring match (full-text/semantic reserved for future tiers). **Time-travel** (§8.3): `entries_before(id)` returns entries in order as they were before the given entry; `recent_ordered(limit)` returns the most recent `limit` entries in chronological order (oldest first).

## Usage (conceptual)

```mermaid
flowchart LR
    App[Application / Agent]
    Memory[Memory trait]
    Episodic[EpisodicMemory]

    App --> Memory
    Memory <|.. Episodic
```

- Depend on `vanswarm-memory`; construct `EpisodicMemory::new(max_entries)` and use it as `dyn Memory` where the three-tier design will later plug in Tier 2 and Tier 3 implementations.
