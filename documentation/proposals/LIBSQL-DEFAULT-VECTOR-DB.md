# Proposal: libsql as default vector database for Tier 3 (semantic memory)

## Summary

Use **libsql** (Turso) as the **default** vector database for the agent framework’s Tier 3 (semantic memory), instead of only documenting Qdrant/pgvector as production options. The in-memory `SemanticMemory` remains the zero-dependency default; when a **persistent** vector store is desired, the recommended default is libsql.

---

## Why libsql?

| Criterion | libsql | Current docs (Qdrant / pgvector) |
|-----------|--------|----------------------------------|
| **Default = zero config** | Embeddable; same process, file or Turso URL | Separate server (Qdrant) or Postgres (pgvector) |
| **Vector support** | Native: `F32_BLOB`, `vector_distance_cos()`, `vector_top_k()`, optional DiskANN | Native in both |
| **Dev vs prod** | One stack: local file or Turso cloud | Often different (e.g. in-memory dev, Qdrant prod) |
| **Rust** | `libsql-client` (sync/async) | qdrant-client, sqlx/tokio-postgres |
| **Unified store** | Episodic metadata, summaries, and vectors in one SQL DB | Typically separate systems for relational vs vector |

The framework already says “SQLite for dev, pgvector/Redis for production” for the abstract `Memory` trait. libsql extends that: **one embeddable default** that does vectors natively and can back multiple tiers (episodic table, mid-term table, semantic vector table) without adding another service.

---

## What changes

1. **Documentation**  
   - In memory architecture and PLATFORM-FEATURES: state that **libsql** is the recommended default for Tier 3 when a persistent vector store is needed.  
   - Keep Qdrant and pgvector as optional “scale-out” or “existing infra” backends.

2. **Implementation (future)**  
   - Add an optional backend (e.g. feature-gated or separate crate) that implements the existing `Memory` trait (and Tier-3 semantic operations) on top of libsql:  
     - Table(s) for `MemoryEntry` and vector column (e.g. `F32_BLOB(dim)`).  
     - `store_with_embedding` → INSERT; `semantic_search` → `vector_top_k()` or cosine query.  
   - No change to the in-memory `SemanticMemory`; it stays the zero-dependency default. libsql becomes the default **persistent** vector store.

3. **No breaking changes**  
   - Existing code keeps using `SemanticMemory` (in-memory) or any future Qdrant/pgvector backend. New users who want persistence and a single default are directed to libsql.

---

## Risks and mitigations

- **Maturity** — libsql’s vector support is newer than pgvector. Mitigation: document “recommended default for most workloads”; keep pgvector/Qdrant in docs for high-scale or existing-Postgres deployments.  
- **Scale** — For very large (e.g. 100M+ vectors), dedicated vector DBs may have more tuning. Mitigation: recommend Turso or DiskANN when needed; document scale limits.  
- **Dependency** — Adding libsql-client increases dependency surface. Mitigation: make the libsql backend feature-flagged or a separate crate so the core memory crate stays minimal.

---

## Conclusion

**Yes — the agent framework should treat libsql as the default vector database** for Tier 3 when a persistent store is desired: one embeddable default, native vectors, SQL for multiple tiers, and a clear path from local file to Turso cloud. Qdrant and pgvector remain supported alternatives behind the same `Memory` trait.
