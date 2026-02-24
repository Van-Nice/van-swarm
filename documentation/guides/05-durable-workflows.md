# Durable workflows

This guide covers **DurableContext**, **JournalBackend**, and **#[workflow]** for replay-based durable execution.

---

## 1. Idea: log-centric replay

Durable workflows **log every side effect** (tool call, sleep, timestamp). On restart, the workflow function is **re-run from the beginning**; when it reaches a step that was already logged, the journal returns the cached result instead of re-executing. So you get durability without persisting full process state.

- **First run:** execute side effect → append to journal → return result.  
- **Replay:** read from journal → return cached result (no execution).

---

## 2. JournalBackend and backends

**JournalBackend** (async trait) stores entries keyed by `workflow_id` and sequence number:

- **get(workflow_id, seq)** — return cached entry if present.  
- **put(workflow_id, entry)** — append an entry.  
- **load_all(workflow_id)** — load full journal for a run (used by `resume`).

Implementations:

- **InMemoryJournal** — for tests; lost on process exit.  
- **FileJournal** — NDJSON file per workflow (or one file with workflow_id in each line); survives restart.

```rust
use std::sync::Arc;
use rustmastra_core::durable::{FileJournal, InMemoryJournal, JournalBackend};

// Development: persist under a directory
let journal: Arc<dyn JournalBackend> = Arc::new(FileJournal::new("/tmp/my_workflow_journals"));

// Tests
let journal = Arc::new(InMemoryJournal::new());
```

---

## 3. DurableContext

**DurableContext** is the handle your workflow code uses. Every non-deterministic operation goes through it so it can journal and replay.

### Creating a context

- **New run:** `DurableContext::new(workflow_id, journal, executor?)`.  
- **Resume after crash:** `DurableContext::resume(workflow_id, journal, executor?).await` — loads the journal so the first `get` for each seq is a cache hit.

`executor` is an optional **ToolExecutor** for **call_tool**; you can pass `None` if the workflow only uses **run_once** / **sleep** / **timestamp**.

```rust
use std::sync::Arc;
use rustmastra_core::durable::{DurableContext, FileJournal, JournalBackend};

let journal: Arc<dyn JournalBackend> = Arc::new(FileJournal::new("/tmp/journals"));
let ctx = DurableContext::new("run-1", Arc::clone(&journal), None);
```

### call_tool(name, args)

Execute a tool and journal the result. On replay, the executor is not called.

```rust
let result = ctx.call_tool("read_file", serde_json::json!({ "path": "config.json" })).await?;
```

### sleep(duration)

Deterministic sleep: first run waits and journals the end time; replay returns immediately using the cached time.

```rust
ctx.sleep(std::time::Duration::from_secs(1)).await?;
```

### timestamp()

Non-deterministic “now” becomes deterministic: first run records the time, replay returns the same time.

```rust
let t = ctx.timestamp().await?;
```

### run_once(label, async_block)

Generic “run this once and journal the result.” Use for any custom side effect (e.g. HTTP call, DB write).

```rust
let value = ctx.run_once("fetch_config", async {
    let body = reqwest::get("https://api.example.com/config").await?.text().await?;
    Ok::<_, rustmastra_core::FrameworkError>(body)
}).await?;
```

---

## 4. Resume after crash

1. Use the same **workflow_id** and **JournalBackend** (e.g. same file path).  
2. Build the context with **DurableContext::resume(workflow_id, journal, executor?).await**.  
3. Run your workflow function again from the start. It will replay from the journal until it reaches the next step that wasn’t completed; then it runs that step and continues.

---

## 5. #[workflow] macro

The **#[workflow]** procedural macro lives in **rustmastra-macros**. It **validates** that the first parameter of the function is `Arc<DurableContext>` (or equivalent). It does **not** transform the body; checkpointing is done by calling `ctx.call_tool`, `ctx.sleep`, `ctx.run_once`, etc.

```rust
use std::sync::Arc;
use rustmastra_core::durable::DurableContext;
use rustmastra_macros::workflow;

#[workflow]
async fn my_workflow(ctx: Arc<DurableContext>) -> rustmastra_core::Result<String> {
    let x = ctx.run_once("step1", async { Ok::<_, rustmastra_core::FrameworkError>("done") }).await?;
    ctx.sleep(std::time::Duration::from_millis(100)).await?;
    let t = ctx.timestamp().await?;
    Ok(format!("{} at {:?}", x, t))
}
```

If the first parameter is not `Arc<DurableContext>`, the macro emits a compile error.

---

## 6. Full example: workflow with tool call and sleep

```rust
use std::sync::Arc;
use rustmastra_core::{
    durable::{DurableContext, FileJournal, InMemoryJournal, JournalBackend},
    traits::tool::LocalToolRegistry,
};
use rustmastra_macros::workflow;

#[workflow]
async fn demo_workflow(ctx: Arc<DurableContext>) -> rustmastra_core::Result<String> {
    let step1 = ctx.run_once("compute", async {
        Ok::<_, rustmastra_core::FrameworkError>("computed".to_string())
    }).await?;
    ctx.sleep(std::time::Duration::from_secs(1)).await?;
    let result = ctx.call_tool("time", serde_json::json!({})).await?;
    Ok(format!("{} -> {}", step1, result))
}

#[tokio::main]
async fn main() -> rustmastra_core::Result<()> {
    let journal: Arc<dyn JournalBackend> = Arc::new(InMemoryJournal::new());
    let executor = Arc::new(LocalToolRegistry::new().register(rustmastra_core::TimeTool));
    let ctx = DurableContext::new("demo-1", journal, Some(executor));
    let out = demo_workflow(ctx).await?;
    println!("{}", out);
    Ok(())
}
```

---

## 7. Next steps

- Graph orchestration with human-in-the-loop: [06-orchestrator](06-orchestrator.md).  
- Architecture details: [documentation/architecture/01-core.md](../architecture/01-core.md) (durable section).
