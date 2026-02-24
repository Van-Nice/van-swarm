# Runtime (WASM sandbox)

This guide covers **vanswarm-runtime**: the **Wasmtime**-based sandbox for running WASM modules as tools, and the optional **MCP bridge**.

---

## 1. When to use the runtime

- Run **untrusted** or third-party tool logic in isolation.
- Enforce **resource limits** (memory, fuel) per invocation.
- Optional: let sandboxed WASM call **host MCP tools** via the MCP bridge (no raw host I/O).

The runtime is **optional**; many agents use only **LocalToolRegistry** and **McpToolExecutor**.

---

## 2. Sandbox and SandboxConfig

**SandboxConfig** controls limits:

- **max_memory_bytes** — cap on linear memory (e.g. 5 MiB).
- **max_fuel** — instruction fuel; exhaustion traps (stops runaway loops).
- **allow_mcp** — when true, the linker can expose the MCP bridge so the guest can call host MCP tools.

**Sandbox** holds the engine and config. You **compile** once, then **run_compiled** many times:

```rust
use vanswarm_runtime::{Sandbox, SandboxConfig};

let config = SandboxConfig {
    max_memory_bytes: 5 * 1024 * 1024, // 5 MiB
    max_fuel: 10_000_000,
    allow_mcp: false,
};
let sandbox = Sandbox::new(config);
let module = sandbox.compile(wasm_bytes)?;
// Each call gets a fresh Store
let result = sandbox.run_compiled(&module, params).await?;
```

---

## 3. run_json convention

WASM modules that participate in the framework must export:

- **memory** — linear memory.
- **alloc(len)** — allocate `len` bytes; return guest pointer.
- **run_json(ptr, len)** — input JSON at (ptr, len); return `(result_ptr << 32) | result_len`, or **-1** on error.

The host writes the input JSON into guest memory (via alloc + memory write), calls **run_json**, then reads the result from guest memory and deserializes JSON. This keeps the host/guest contract simple and safe.

---

## 4. Example: run a compiled module

```rust
use vanswarm_runtime::{Sandbox, SandboxConfig};

let config = SandboxConfig {
    max_memory_bytes: 2 * 1024 * 1024,
    max_fuel: 1_000_000,
    allow_mcp: false,
};
let sandbox = Sandbox::new(config);
let wasm_bytes = std::fs::read("path/to/tool.wasm")?;
let module = sandbox.compile(&wasm_bytes)?;
let params = serde_json::json!({ "query": "hello" });
let value = sandbox.run_compiled(&module, params).await?;
println!("{:?}", value);
```

---

## 5. MCP bridge (allow_mcp)

When **allow_mcp** is true, the Wasmtime **linker** can expose an import that the guest uses to call MCP tools. The host proxies those calls to an MCP client. So the guest cannot access the host filesystem or network directly; it can only invoke tools the host has authorized.

See [documentation/framework/workflows/04-wasm-mcp-bridge.md](../framework/workflows/04-wasm-mcp-bridge.md) for the full workflow.

---

## 6. Feature flags

The **vanswarm-runtime** crate may have optional features (e.g. **mcp-bridge**, **scripting** for Rhai). Check `crates/runtime/Cargo.toml` and enable what you need:

```toml
vanswarm-runtime = { path = "../runtime", features = ["wasm", "mcp-bridge"] }
```

---

## 7. Next steps

- Tools in the main process: [03-tools](03-tools.md).
- MCP from the host: [04-mcp](04-mcp.md).
- Architecture: [documentation/architecture/05-runtime.md](../architecture/05-runtime.md).
