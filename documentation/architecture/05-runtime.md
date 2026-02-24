# Runtime crate (rustmastra-runtime)

Wasmtime-based WASM sandbox for secure, isolated tool execution. Each invocation gets a fresh Store; fuel and memory are capped.

## Sandbox model

```mermaid
flowchart TB
    subgraph host["Host"]
        Sandbox[Sandbox]
        Config[SandboxConfig]
        Compiled[CompiledModule]
    end

    subgraph per_invocation["Per invocation"]
        Store[Store - fresh each call]
        Limiter[ResourceLimiter]
        Fuel[Fuel cap]
        Linker[Linker - no host imports by default]
    end

    Sandbox --> Config
    Sandbox --> compile["compile() / load_aot()"]
    compile --> Compiled
    Sandbox --> run_compiled["run_compiled(module, params)"]

    run_compiled --> Store
    Store --> Limiter
    Store --> Fuel
    Store --> Linker
```

- **Sandbox** — holds engine and config; `compile(wasm_bytes)` or `load_aot(aot_bytes)` produces a **CompiledModule** (cheap to clone).
- **run_compiled(module, params)** — for each call: new Store, attach resource limiter and fuel, linker with no host imports (unless `allow_mcp`), instantiate, call `run_json`.

## SandboxConfig

```mermaid
classDiagram
    class SandboxConfig {
        +max_memory_bytes: usize
        +max_fuel: u64
        +allow_mcp: bool
    }
```

| Field | Purpose |
|-------|---------|
| `max_memory_bytes` | Cap on linear memory (e.g. 5 MiB). |
| `max_fuel` | Instruction fuel; exhaustion traps (stops runaway loops). |
| `allow_mcp` | When true, linker may expose MCP bridge (future); default false = no host imports. |

## run_json calling convention

WASM modules that participate in the framework must export:

```mermaid
flowchart LR
    subgraph exports["Module exports"]
        memory["memory: Memory"]
        alloc["alloc(len) -> ptr"]
        run_json["run_json(ptr, len) -> i64"]
    end
```

| Export | Signature | Description |
|--------|------------|-------------|
| `memory` | `Memory` | Guest linear memory |
| `alloc` | `(i32) -> i32` | Allocate `len` bytes; return guest pointer |
| `run_json` | `(i32, i32) -> i64` | Input JSON at `(ptr, len)`; return `(result_ptr << 32) \| result_len`, or `-1` on error |

Host writes params JSON into guest memory via `alloc` + memory write; calls `run_json`; reads result from guest memory and deserializes JSON.

## Execution path

```mermaid
sequenceDiagram
    participant Caller
    participant Sandbox
    participant spawn_blocking
    participant Store
    participant Module

    Caller->>Sandbox: run_compiled(module, params)
    Sandbox->>spawn_blocking: execute_module_sync(...)
    spawn_blocking->>Store: new Store + limiter + fuel
    spawn_blocking->>Store: Linker (no imports)
    spawn_blocking->>Module: instantiate
    spawn_blocking->>Module: alloc, write params
    spawn_blocking->>Module: run_json(ptr, len)
    Module-->>spawn_blocking: i64 (ptr|len or -1)
    spawn_blocking->>Store: read result from memory
    spawn_blocking-->>Sandbox: Result~Value~
    Sandbox-->>Caller: Result~Value~
```

- Execution runs inside `tokio::task::spawn_blocking` so the async runtime is not blocked by WASM.
- **ResourceLimiter** — `memory_growing` enforces `max_memory_bytes`; table growth can be allowed.
- **Fuel** — set once per Store; when exhausted, `run_json` traps and the host gets an error.

## AOT (ahead-of-time) compilation

```mermaid
flowchart LR
    A[compile(wasm_bytes)]
    B[CompiledModule]
    C[serialize_aot]
    D[aot_bytes]
    E[load_aot]
    F[CompiledModule]

    A --> B
    B --> C
    C --> D
    D --> E
    E --> F
```

- **serialize_aot(module)** — engine-specific bytes; store to disk for fast subsequent load.
- **load_aot(aot_bytes)** — unsafe: bytes must come from the same engine config; yields a `CompiledModule` without re-parsing WASM.
