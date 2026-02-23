# Three-Tier Memory Workflow

The framework uses a **three-tier memory** model (Episodic → Mid-term → Semantic) with **heat-based consolidation** so important information is promoted and retained without blowing the context window.

## Tier Overview

```mermaid
flowchart TB
    subgraph Tier1["Tier 1: Episodic (Working)"]
        T1[(Redis / In-memory)]
        T1Desc["Raw events, current session\nSliding window / FIFO"]
    end

    subgraph Tier2["Tier 2: Mid-term (Summarized)"]
        T2[(Local disk / Summary pages)]
        T2Desc["Summarized segments\nHeat-based promotion"]
    end

    subgraph Tier3["Tier 3: Semantic (Long-term)"]
        T3[(Qdrant / Vector DB)]
        T3Desc["Embeddings, RAG\nStable knowledge"]
    end

    Tier1 --> Tier2
    Tier2 --> Tier3
```

## Consolidation: Tier 1 → Tier 2

After N turns or when the context window approaches a token limit, a summarization step runs. A small LLM scores “state-altering” information (e.g. user preferences) and writes summaries to Tier 2.

```mermaid
flowchart LR
    E[Episodic buffer] --> Trigger{Every N turns\nor token limit?}
    Trigger -->|Yes| Summarize[Significance scoring\n+ summarization]
    Summarize --> T2[Mid-term summary pages]
```

## Consolidation: Tier 2 → Tier 3 (Heat-Based)

Each summary segment has a **heat** value. When a segment is retrieved to answer a query, its heat increases. When heat exceeds a threshold, the segment is embedded and stored in the vector DB (Tier 3).

```mermaid
flowchart TB
    subgraph Heat["Heat-based promotion"]
        H1[Segment in Tier 2]
        H2[Retrieved for query → heat++]
        H3{Heat > threshold?}
        H4[Embed & store in Tier 3]
    end

    H1 --> H2
    H2 --> H3
    H3 -->|Yes| H4
    H3 -->|No| H1
```

## Memory-R1 Style Conflict Resolution

When consolidating, a **Memory Manager** can apply CRUD-style operations (ADD, UPDATE, DELETE, NOOP) so the memory bank evolves instead of only appending.

```mermaid
flowchart LR
    New[New information] --> MM[Memory Manager]
    MM --> ADD[ADD]
    MM --> UPDATE[UPDATE]
    MM --> DELETE[DELETE]
    MM --> NOOP[NOOP]
    ADD --> Bank[(Memory bank)]
    UPDATE --> Bank
    DELETE --> Bank
```

## Storage and Consolidation Summary

| Tier | Storage | Consolidation |
|------|---------|----------------|
| **Tier 1: Episodic** | Redis / in-memory | Sliding window / FIFO |
| **Tier 2: Mid-term** | Local disk / summary pages | Heat-based promotion |
| **Tier 3: Semantic** | Qdrant / vector DB | Embedding-based RAG |

## Trait-Based Abstraction

A common **Memory** trait lets you swap backends (e.g. SQLite for dev, pgvector/Redis for production) without changing agent logic.

```mermaid
flowchart TB
    Agent[Agent logic]
    Agent --> Trait[Memory trait]
    Trait --> Redis[Redis]
    Trait --> Qdrant[Qdrant]
    Trait --> PgVector[pgvector]
```

## References

- Technical Specification: Three-tier memory, heat-based consolidation, Memory-R1, Qdrant, Redis.
- PRD & Product Strategy: Episodic / Semantic / Procedural, trait-based API.
