//! Persistent episodic memory backed by libsql (file or `:memory:`).
//!
//! Enabled by the `libsql` feature. Use when `VANSWARM_DB_PATH` is set so the
//! MCP server (or any consumer) persists memory across restarts.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use libsql::{params, Connection};
use std::sync::Arc;
use uuid::Uuid;

use crate::{Memory, MemoryEntry};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS episodic (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    heat INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_episodic_created_at ON episodic(created_at);
"#;

/// Episodic memory backed by a libsql database (file or `:memory:`).
///
/// FIFO eviction when count exceeds `max_entries`. Implements [`Memory`].
pub struct LibSqlEpisodicMemory {
    conn: Arc<Connection>,
    max_entries: usize,
}

impl LibSqlEpisodicMemory {
    /// Open or create a libsql database at `path` and initialize the episodic table.
    ///
    /// `path` can be a file path (e.g. `"./data/vanswarm.db"`) or `":memory:"` for
    /// an in-memory database. Uses `max_entries` for FIFO eviction (delete oldest when over capacity).
    pub async fn open(path: &str, max_entries: usize) -> vanswarm_core::Result<Self> {
        let db = libsql::Builder::new_local(path).build().await.map_err(|e| {
            vanswarm_core::FrameworkError::Config(format!("libsql open {}: {}", path, e))
        })?;
        let conn = db.connect().map_err(|e| {
            vanswarm_core::FrameworkError::Config(format!("libsql connect: {}", e))
        })?;
        conn.execute_batch(SCHEMA).await.map_err(|e| {
            vanswarm_core::FrameworkError::Config(format!("libsql schema: {}", e))
        })?;
        Ok(Self {
            conn: Arc::new(conn),
            max_entries,
        })
    }

    /// Current row count (for eviction check).
    async fn count(&self) -> vanswarm_core::Result<u64> {
        let mut rows = self
            .conn
            .query("SELECT COUNT(*) FROM episodic", ())
            .await
            .map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
        let row = rows
            .next()
            .await
            .map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?
            .ok_or_else(|| vanswarm_core::FrameworkError::Config("count query returned no row".into()))?;
        let c: i64 = row.get(0).map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
        Ok(c as u64)
    }

    /// Delete the oldest entry by created_at (one row).
    async fn evict_oldest(&self) -> vanswarm_core::Result<()> {
        self.conn
            .execute(
                "DELETE FROM episodic WHERE id = (SELECT id FROM episodic ORDER BY created_at ASC LIMIT 1)",
                (),
            )
            .await
            .map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl Memory for LibSqlEpisodicMemory {
    async fn store(&self, entry: MemoryEntry) -> vanswarm_core::Result<()> {
        if self.max_entries > 0 {
            while self.count().await? >= self.max_entries as u64 {
                self.evict_oldest().await?;
            }
        }
        self.conn
            .execute(
                "INSERT INTO episodic (id, content, created_at, heat) VALUES (?1, ?2, ?3, ?4)",
                params![
                    entry.id.to_string(),
                    entry.content,
                    entry.created_at.to_rfc3339(),
                    entry.heat as i64,
                ],
            )
            .await
            .map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
        Ok(())
    }

    async fn recent(&self, limit: usize) -> vanswarm_core::Result<Vec<MemoryEntry>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, content, created_at, heat FROM episodic ORDER BY created_at DESC LIMIT ?1",
                params![limit as i64],
            )
            .await
            .map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?
        {
            let id: String = row.get(0).map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
            let content: String = row.get(1).map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
            let created_at: String = row.get(2).map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
            let heat: i64 = row.get(3).map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
            let id = Uuid::parse_str(&id).map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
            let created_at = DateTime::parse_from_rfc3339(&created_at)
                .map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?
                .with_timezone(&Utc);
            out.push(MemoryEntry {
                id,
                content,
                created_at,
                heat: heat as u32,
                embedding: None,
            });
        }
        Ok(out)
    }

    async fn search(&self, query: &str, limit: usize) -> vanswarm_core::Result<Vec<MemoryEntry>> {
        let q = format!("%{}%", query.to_lowercase());
        let mut rows = self
            .conn
            .query(
                "SELECT id, content, created_at, heat FROM episodic WHERE LOWER(content) LIKE ?1 ORDER BY created_at DESC LIMIT ?2",
                params![q, limit as i64],
            )
            .await
            .map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?
        {
            let id: String = row.get(0).map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
            let content: String = row.get(1).map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
            let created_at: String = row.get(2).map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
            let heat: i64 = row.get(3).map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
            let id = Uuid::parse_str(&id).map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
            let created_at = DateTime::parse_from_rfc3339(&created_at)
                .map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?
                .with_timezone(&Utc);
            out.push(MemoryEntry {
                id,
                content,
                created_at,
                heat: heat as u32,
                embedding: None,
            });
        }
        Ok(out)
    }

    async fn delete(&self, id: Uuid) -> vanswarm_core::Result<()> {
        self.conn
            .execute("DELETE FROM episodic WHERE id = ?1", params![id.to_string()])
            .await
            .map_err(|e| vanswarm_core::FrameworkError::Config(e.to_string()))?;
        Ok(())
    }
}
