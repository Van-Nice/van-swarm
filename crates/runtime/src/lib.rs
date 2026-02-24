//! # vanswarm-runtime
//!
//! Wasmtime-based WASM sandbox for secure tool isolation.
//!
//! Each tool invocation runs in its own WASM isolate with:
//! * **Per-invocation `Store`** — no cross-tenant memory leakage (§5.6).
//! * **Fuel cap** — prevents runaway loops / CPU exhaustion (§5.5).
//! * **`ResourceLimiter`** — caps linear memory growth (§5.4).
//! * **No default capabilities** — no host filesystem, no network, no env
//!   vars injected into the guest (§5.3, §5.12).
//!
//! ## Module calling convention  (`run-json`)
//!
//! A WASM module that wants to participate in the `run_json` protocol must
//! export the following:
//!
//! | Export | Signature | Description |
//! |--------|-----------|-------------|
//! | `memory` | `Memory` | Guest linear memory |
//! | `alloc` | `(i32) -> i32` | Allocate `len` bytes; return guest pointer |
//! | `run_json` | `(i32, i32) -> i64` | Execute with JSON at `(ptr, len)`; return `(result_ptr << 32) \| result_len` |
//!
//! On error, `run_json` should return `-1` (i.e. `0xFFFF_FFFF_FFFF_FFFF` as `i64`).
//!
//! ## Security model
//!
//! The host linker exposes **no** imports by default.  Future revisions may
//! optionally add a restricted MCP bridge import (§5.10) when
//! `SandboxConfig::allow_mcp` is `true`.  The guest cannot perform arbitrary
//! I/O, filesystem access, or network calls.

use serde::{Deserialize, Serialize};

use vanswarm_core::FrameworkError;

// ─────────────────────────────────────────────────────────────────────────────
// SandboxConfig
// ─────────────────────────────────────────────────────────────────────────────

/// Security and resource limits for a single WASM isolate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Maximum WebAssembly linear memory in bytes.
    ///
    /// Default: 5 MiB — matches the framework target of <5 MB per agent.
    pub max_memory_bytes: usize,

    /// Maximum Wasmtime *fuel* units.
    ///
    /// Wasmtime consumes roughly one unit per WASM instruction.  The default
    /// of 10M units is sufficient for typical tool implementations while
    /// preventing runaway loops.
    pub max_fuel: u64,

    /// Whether the sandbox may invoke MCP tools via the host bridge (§6).
    ///
    /// When `false` the linker exposes zero host imports.
    pub allow_mcp: bool,

    /// MCP client for the WASM-to-MCP bridge (§6.2–6.4). When set with `allow_mcp`,
    /// guest imports `mcp/call_tool` are bound to this client. Requires `mcp-bridge` feature.
    #[cfg(feature = "mcp-bridge")]
    #[serde(skip)]
    pub mcp_client: Option<McpClientRef>,
}

/// Wrapper for [`McpClient`] so [`SandboxConfig`] can derive `Debug`. Use with `mcp-bridge` feature.
#[cfg(feature = "mcp-bridge")]
#[derive(Clone)]
pub struct McpClientRef(pub std::sync::Arc<vanswarm_mcp::McpClient>);

#[cfg(feature = "mcp-bridge")]
impl std::fmt::Debug for McpClientRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "McpClient(..)")
    }
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: 5 * 1024 * 1024, // 5 MiB
            max_fuel: 10_000_000,
            allow_mcp: false,
            #[cfg(feature = "mcp-bridge")]
            mcp_client: None::<McpClientRef>,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CompiledModule — cheap-to-clone handle for pre-compiled WASM
// ─────────────────────────────────────────────────────────────────────────────

/// A pre-compiled WASM module that can be cheaply cloned and run many times.
///
/// Obtain via [`Sandbox::compile`] or [`Sandbox::compile_aot`].
#[derive(Clone)]
pub struct CompiledModule {
    #[cfg(feature = "wasm")]
    inner: std::sync::Arc<wasmtime::Module>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Sandbox
// ─────────────────────────────────────────────────────────────────────────────

/// A WASM sandbox isolate factory.
///
/// One `Sandbox` instance holds a shared, configured `Engine`.  Each call to
/// `run()` or `run_compiled()` creates a *fresh* `Store`, guaranteeing
/// complete isolation between invocations.
pub struct Sandbox {
    config: SandboxConfig,
    #[cfg(feature = "wasm")]
    engine: wasmtime::Engine,
}

impl Sandbox {
    /// Create a new sandbox with the given config.
    ///
    /// # Errors
    ///
    /// Returns an error if Wasmtime engine initialisation fails (unlikely
    /// under normal conditions).
    pub fn new(config: SandboxConfig) -> vanswarm_core::Result<Self> {
        #[cfg(feature = "wasm")]
        {
            let mut wt_config = wasmtime::Config::new();
            // Enable instruction-level fuel consumption for CPU-time limits.
            wt_config.consume_fuel(true);
            // Single-threaded; disable WASM threads extension.
            wt_config.wasm_threads(false);
            // Use Cranelift (fastest compiled tier).
            wt_config
                .strategy(wasmtime::Strategy::Cranelift);

            let engine = wasmtime::Engine::new(&wt_config)
                .map_err(|e| FrameworkError::Wasm(format!("Engine init failed: {e}")))?;
            return Ok(Self { config, engine });
        }
        #[cfg(not(feature = "wasm"))]
        Ok(Self { config })
    }

    /// Compile a WASM module from raw bytes (or WAT text).
    ///
    /// Compilation is CPU-intensive; prefer calling this once and caching the
    /// returned `CompiledModule`.
    pub fn compile(&self, wasm_bytes: &[u8]) -> vanswarm_core::Result<CompiledModule> {
        #[cfg(feature = "wasm")]
        {
            let module = wasmtime::Module::new(&self.engine, wasm_bytes)
                .map_err(|e| FrameworkError::Wasm(format!("Module compilation failed: {e}")))?;
            return Ok(CompiledModule {
                inner: std::sync::Arc::new(module),
            });
        }
        #[cfg(not(feature = "wasm"))]
        Err(FrameworkError::Wasm(
            "vanswarm-runtime compiled without 'wasm' feature".into(),
        ))
    }

    /// Serialize a compiled module to AOT bytes for fast subsequent loads.
    ///
    /// The returned bytes can be stored to disk and later deserialized via
    /// [`Sandbox::load_aot`], achieving sub-millisecond instantiation on
    /// subsequent calls (§5.7).
    ///
    /// # Safety
    ///
    /// AOT bytes are engine-version- and platform-specific.  They must only
    /// be deserialized by an engine with the **same** configuration.
    pub fn serialize_aot(&self, module: &CompiledModule) -> vanswarm_core::Result<Vec<u8>> {
        #[cfg(feature = "wasm")]
        {
            module
                .inner
                .serialize()
                .map_err(|e| FrameworkError::Wasm(format!("AOT serialization failed: {e}")))
        }
        #[cfg(not(feature = "wasm"))]
        {
            let _ = module;
            Err(FrameworkError::Wasm(
                "vanswarm-runtime compiled without 'wasm' feature".into(),
            ))
        }
    }

    /// Load a module from AOT-serialized bytes produced by [`serialize_aot`].
    ///
    /// # Safety
    ///
    /// The bytes must have been produced by an engine with the same
    /// configuration as this `Sandbox`.  Passing arbitrary bytes is undefined
    /// behaviour.
    pub unsafe fn load_aot(
        &self,
        aot_bytes: &[u8],
    ) -> vanswarm_core::Result<CompiledModule> {
        #[cfg(feature = "wasm")]
        {
            // SAFETY: caller guarantees bytes came from a compatible engine.
            let module = wasmtime::Module::deserialize(&self.engine, aot_bytes)
                .map_err(|e| FrameworkError::Wasm(format!("AOT load failed: {e}")))?;
            return Ok(CompiledModule {
                inner: std::sync::Arc::new(module),
            });
        }
        #[cfg(not(feature = "wasm"))]
        {
            let _ = aot_bytes;
            Err(FrameworkError::Wasm(
                "vanswarm-runtime compiled without 'wasm' feature".into(),
            ))
        }
    }

    /// Compile from raw bytes and run immediately.
    ///
    /// For repeated invocations prefer `compile()` + `run_compiled()` to
    /// avoid recompiling on every call.
    pub async fn run(
        &self,
        wasm_bytes: &[u8],
        params: serde_json::Value,
    ) -> vanswarm_core::Result<serde_json::Value> {
        let module = self.compile(wasm_bytes)?;
        self.run_compiled(&module, params).await
    }

    /// Safe API: run a WASM script with JSON params and return JSON result (§5.9).
    ///
    /// Alias for `run()`; use when the semantic is "script invocation".
    pub async fn run_script(
        &self,
        wasm_bytes: &[u8],
        params: serde_json::Value,
    ) -> vanswarm_core::Result<serde_json::Value> {
        self.run(wasm_bytes, params).await
    }

    /// Execute a pre-compiled module with the given JSON parameters.
    ///
    /// Internally uses `tokio::task::spawn_blocking` so the Tokio thread is
    /// never blocked by WASM execution.
    pub async fn run_compiled(
        &self,
        module: &CompiledModule,
        params: serde_json::Value,
    ) -> vanswarm_core::Result<serde_json::Value> {
        #[cfg(feature = "wasm")]
        {
            let engine = self.engine.clone();
            let module = module.inner.clone();
            let config = self.config.clone();

            #[cfg(feature = "mcp-bridge")]
            let (tokio_handle, mcp_client) = (
                tokio::runtime::Handle::current(),
                config.mcp_client.clone(),
            );
            tokio::task::spawn_blocking(move || {
                #[cfg(feature = "mcp-bridge")]
                return execute_module_sync_with_mcp(&engine, &module, &config, &params, tokio_handle, mcp_client);
                #[cfg(not(feature = "mcp-bridge"))]
                execute_module_sync(&engine, &module, &config, &params)
            })
            .await
            .map_err(|e| {
                FrameworkError::Wasm(format!("Sandbox task panicked or was cancelled: {e}"))
            })?
        }
        #[cfg(not(feature = "wasm"))]
        {
            let _ = (module, params);
            Err(FrameworkError::Wasm(
                "vanswarm-runtime compiled without 'wasm' feature".into(),
            ))
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WasiCtx policy (§5.3): default null/empty; add only required capabilities
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "wasm")]
mod wasi_ctx {
    //! WASI context configuration: default to null/empty (§5.3).
    //!
    //! We use [WasiCtxBuilder] with no args, env, preopens, or stdio inheritance,
    //! so the guest gets no host filesystem, no environment variables, and no
    //! network. When MCP or other capabilities are added (§5.10, §6), they are
    //! added explicitly to the linker only; the WASI context remains minimal.

    use wasmtime_wasi::preview1::WasiP1Ctx;
    use wasmtime_wasi::WasiCtxBuilder;

    /// Build an empty WASI preview1 context: no preopens, no env, no args, no stdio.
    ///
    /// Use this when instantiating modules that may import WASI; they get no
    /// host capabilities by default. Add only required capabilities (e.g. MCP
    /// bridge) via the linker when `SandboxConfig::allow_mcp` is true.
    pub fn empty_wasi_p1_ctx() -> WasiP1Ctx {
        let mut b = WasiCtxBuilder::new();
        b.build_p1()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Synchronous execution core (runs inside spawn_blocking)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(all(feature = "wasm", not(feature = "mcp-bridge")))]
fn execute_module_sync(
    engine: &wasmtime::Engine,
    module: &wasmtime::Module,
    config: &SandboxConfig,
    params: &serde_json::Value,
) -> vanswarm_core::Result<serde_json::Value> {
    use wasmtime::{Linker, Store};

    let data = StoreData {
        limiter: WasmLimiter { max_memory_bytes: config.max_memory_bytes },
        wasi: wasi_ctx::empty_wasi_p1_ctx(),
    };
    let mut store: Store<StoreData> = Store::new(engine, data);
    store.limiter(|d| &mut d.limiter);
    store
        .set_fuel(config.max_fuel)
        .map_err(|e| FrameworkError::Wasm(format!("Failed to configure fuel: {e}")))?;

    let mut linker: Linker<StoreData> = Linker::new(engine);
    wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |d| &mut d.wasi)
        .map_err(|e| FrameworkError::Wasm(format!("WASI add_to_linker failed: {e}")))?;

    if config.allow_mcp {
        linker
            .func_wrap(
                "mcp",
                "call_tool",
                |_name_ptr: i32, _name_len: i32, _params_ptr: i32, _params_len: i32| -> i64 {
                    -1i64
                },
            )
            .map_err(|e| FrameworkError::Wasm(format!("MCP add_to_linker failed: {e}")))?;
    }

    let instance = linker
        .instantiate(&mut store, module)
        .map_err(|e| FrameworkError::Wasm(format!("Module instantiation failed: {e}")))?;

    run_json_and_decode(&mut store, &instance, params)
}

#[cfg(feature = "wasm")]
fn run_json_and_decode(
    store: &mut wasmtime::Store<StoreData>,
    instance: &wasmtime::Instance,
    params: &serde_json::Value,
) -> vanswarm_core::Result<serde_json::Value> {
    let memory = instance.get_memory(&mut *store, "memory").ok_or_else(|| {
        FrameworkError::Wasm("Module must export 'memory'".into())
    })?;

    let alloc_fn = instance
        .get_typed_func::<i32, i32>(&mut *store, "alloc")
        .map_err(|_| FrameworkError::Wasm("Module must export 'alloc(i32) -> i32'".into()))?;

    let run_json_fn = instance
        .get_typed_func::<(i32, i32), i64>(&mut *store, "run_json")
        .map_err(|_| {
            FrameworkError::Wasm("Module must export 'run_json(i32, i32) -> i64'".into())
        })?;

    let params_bytes = serde_json::to_vec(params)
        .map_err(|e| FrameworkError::Serialization(e.into()))?;
    let params_len = params_bytes.len() as i32;

    let params_ptr = alloc_fn
        .call(&mut *store, params_len)
        .map_err(|e| FrameworkError::Wasm(format!("alloc() call failed: {e}")))?;

    memory
        .write(&mut *store, params_ptr as usize, &params_bytes)
        .map_err(|e| {
            FrameworkError::Wasm(format!("Failed to write params to guest memory: {e}"))
        })?;

    let encoding = run_json_fn
        .call(&mut *store, (params_ptr, params_len))
        .map_err(|e| FrameworkError::Wasm(format!("run_json() trapped: {e}")))?;

    if encoding == -1_i64 {
        return Err(FrameworkError::Wasm(
            "Module signalled an error (run_json returned -1)".into(),
        ));
    }

    let result_ptr = ((encoding >> 32) & 0xFFFF_FFFF) as usize;
    let result_len = (encoding & 0xFFFF_FFFF) as usize;

    let result_bytes = memory
        .data(&*store)
        .get(result_ptr..result_ptr + result_len)
        .ok_or_else(|| {
            FrameworkError::Wasm(format!(
                "Result region [{result_ptr}..{end}] is out of guest memory bounds",
                end = result_ptr + result_len
            ))
        })?
        .to_vec();

    serde_json::from_slice(&result_bytes).map_err(|e| {
        FrameworkError::Wasm(format!("Failed to deserialise WASM result JSON: {e}"))
    })
}

#[cfg(all(feature = "wasm", feature = "mcp-bridge"))]
fn execute_module_sync(
    engine: &wasmtime::Engine,
    module: &wasmtime::Module,
    config: &SandboxConfig,
    params: &serde_json::Value,
) -> vanswarm_core::Result<serde_json::Value> {
    execute_module_sync_with_mcp(
        engine,
        module,
        config,
        params,
        tokio::runtime::Handle::current(),
        config.mcp_client.clone(),
    )
}

#[cfg(all(feature = "wasm", feature = "mcp-bridge"))]
fn execute_module_sync_with_mcp(
    engine: &wasmtime::Engine,
    module: &wasmtime::Module,
    config: &SandboxConfig,
    params: &serde_json::Value,
    tokio_handle: tokio::runtime::Handle,
    mcp_client: Option<McpClientRef>,
) -> vanswarm_core::Result<serde_json::Value> {
    use wasmtime::{Caller, Linker, Store};

    let data = StoreData {
        limiter: WasmLimiter { max_memory_bytes: config.max_memory_bytes },
        wasi: wasi_ctx::empty_wasi_p1_ctx(),
        mcp_client: mcp_client.map(|r| r.0),
        tokio_handle: Some(tokio_handle),
    };
    let mut store: Store<StoreData> = Store::new(engine, data);
    store.limiter(|d| &mut d.limiter);
    store
        .set_fuel(config.max_fuel)
        .map_err(|e| FrameworkError::Wasm(format!("Failed to configure fuel: {e}")))?;

    let mut linker: Linker<StoreData> = Linker::new(engine);
    wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |d| &mut d.wasi)
        .map_err(|e| FrameworkError::Wasm(format!("WASI add_to_linker failed: {e}")))?;

    if config.allow_mcp {
        let mcp_bridge = move |mut caller: Caller<'_, StoreData>,
                              name_ptr: i32,
                              name_len: i32,
                              params_ptr: i32,
                              params_len: i32|
              -> Result<i64, wasmtime::Error> {
            let client = caller.data().mcp_client.clone();
            let handle = caller.data().tokio_handle.clone();
            let (client, handle) = match (client, handle) {
                (Some(c), Some(h)) => (c, h),
                _ => return Ok(-1i64),
            };
            let memory = caller.get_export("memory").and_then(|e| e.into_memory()).ok_or_else(|| {
                wasmtime::Error::msg("guest did not export 'memory'")
            })?;
            let name_bytes = memory
                .data(&caller)
                .get(name_ptr as usize..(name_ptr as usize).saturating_add(name_len as usize))
                .ok_or_else(|| wasmtime::Error::msg("name region out of bounds"))?
                .to_vec();
            let name = String::from_utf8(name_bytes)
                .map_err(|e| wasmtime::Error::msg(format!("tool name not UTF-8: {e}")))?;
            let params_bytes = memory
                .data(&caller)
                .get(params_ptr as usize..(params_ptr as usize).saturating_add(params_len as usize))
                .ok_or_else(|| wasmtime::Error::msg("params region out of bounds"))?
                .to_vec();
            let arguments: serde_json::Value = serde_json::from_slice(&params_bytes)
                .map_err(|e| wasmtime::Error::msg(format!("params not JSON: {e}")))?;

            let result = handle.block_on(client.call_tool(&name, arguments));
            let (result_ptr, result_len) = match result {
                Ok(ctr) => {
                    let text = ctr
                        .content
                        .first()
                        .and_then(|c| c.as_text())
                        .unwrap_or("")
                        .to_string();
                    let result_json = serde_json::json!({ "content": text, "is_error": ctr.is_error.unwrap_or(false) });
                    let result_bytes = serde_json::to_vec(&result_json)
                        .map_err(|e| wasmtime::Error::msg(format!("serialize result: {e}")))?;
                    let alloc = caller.get_export("alloc").and_then(|e| e.into_func()).ok_or_else(|| {
                        wasmtime::Error::msg("guest did not export 'alloc'")
                    })?;
                    let alloc_typed = alloc.typed::<i32, i32>(&caller)?;
                    let ptr = alloc_typed.call(&mut caller, result_bytes.len() as i32)?;
                    memory.write(&mut caller, ptr as usize, &result_bytes)?;
                    (ptr as u64, result_bytes.len() as u64)
                }
                Err(e) => {
                    let err_json = serde_json::json!({ "content": e.to_string(), "is_error": true });
                    let result_bytes = serde_json::to_vec(&err_json).unwrap_or_default();
                    let alloc = caller.get_export("alloc").and_then(|e| e.into_func()).ok_or_else(|| {
                        wasmtime::Error::msg("guest did not export 'alloc'")
                    })?;
                    let alloc_typed = alloc.typed::<i32, i32>(&caller)?;
                    let ptr = alloc_typed.call(&mut caller, result_bytes.len() as i32)?;
                    memory.write(&mut caller, ptr as usize, &result_bytes)?;
                    (ptr as u64, result_bytes.len() as u64)
                }
            };
            Ok((result_ptr << 32 | result_len) as i64)
        };

        linker
            .func_wrap("mcp", "call_tool", mcp_bridge)
            .map_err(|e| FrameworkError::Wasm(format!("MCP add_to_linker failed: {e}")))?;
    }

    let instance = linker
        .instantiate(&mut store, module)
        .map_err(|e| FrameworkError::Wasm(format!("Module instantiation failed: {e}")))?;

    run_json_and_decode(&mut store, &instance, params)
}

// ─────────────────────────────────────────────────────────────────────────────
// StoreData + ResourceLimiter
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "wasm")]
struct StoreData {
    limiter: WasmLimiter,
    wasi: wasmtime_wasi::preview1::WasiP1Ctx,
    #[cfg(feature = "mcp-bridge")]
    mcp_client: Option<std::sync::Arc<vanswarm_mcp::McpClient>>,
    #[cfg(feature = "mcp-bridge")]
    tokio_handle: Option<tokio::runtime::Handle>,
}

#[cfg(feature = "wasm")]
struct WasmLimiter {
    max_memory_bytes: usize,
}

#[cfg(feature = "wasm")]
impl wasmtime::ResourceLimiter for WasmLimiter {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool, wasmtime::Error> {
        Ok(desired <= self.max_memory_bytes)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        _desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool, wasmtime::Error> {
        // Permit all table growth; only memory is capped by this limiter.
        Ok(true)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "wasm")]
    mod wasm_tests {
        use super::*;

        /// Minimal WAT module that:
        ///   - Exposes `memory`
        ///   - `alloc` always returns 0 (params written at offset 0)
        ///   - `run_json` returns a hardcoded `{"result":42}` at offset 512
        ///     (pre-loaded via a data segment)
        const ECHO_WAT: &str = r#"
(module
  (memory (export "memory") 1)
  (data (i32.const 512) "{\"result\":42}")

  (func (export "alloc") (param i32) (result i32)
    i32.const 0)

  ;; Return ptr=512, len=13  =>  (512i64 << 32) | 13i64
  (func (export "run_json") (param i32) (param i32) (result i64)
    i64.const 512
    i64.const 32
    i64.shl
    i64.const 13
    i64.or)
)
"#;

        /// WAT module that loops forever — should be killed by fuel exhaustion.
        const INFINITE_LOOP_WAT: &str = r#"
(module
  (memory (export "memory") 1)

  (func (export "alloc") (param i32) (result i32)
    i32.const 0)

  (func (export "run_json") (param i32) (param i32) (result i64)
    block $b
      loop $l
        br $l
      end
    end
    i64.const 0)
)
"#;

        /// WAT module that signals an error by returning -1.
        const ERROR_WAT: &str = r#"
(module
  (memory (export "memory") 1)

  (func (export "alloc") (param i32) (result i32)
    i32.const 0)

  (func (export "run_json") (param i32) (param i32) (result i64)
    i64.const -1)
)
"#;

        fn sandbox() -> Sandbox {
            Sandbox::new(SandboxConfig::default()).expect("Sandbox::new")
        }

        // ── Test 1: happy path ─────────────────────────────────────────────

        #[tokio::test]
        async fn test_run_returns_result() {
            let sb = sandbox();
            let result = sb
                .run(ECHO_WAT.as_bytes(), serde_json::json!({"n": 1}))
                .await
                .expect("run failed");
            assert_eq!(result["result"], 42);
        }

        // ── Test 2: compile once, run multiple times ───────────────────────

        #[tokio::test]
        async fn test_precompile_and_reuse() {
            let sb = sandbox();
            let module = sb.compile(ECHO_WAT.as_bytes()).expect("compile");
            for _ in 0..3 {
                let r = sb
                    .run_compiled(&module, serde_json::json!({}))
                    .await
                    .expect("run_compiled failed");
                assert_eq!(r["result"], 42);
            }
        }

        // ── Test 3: AOT round-trip ─────────────────────────────────────────

        #[tokio::test]
        async fn test_aot_round_trip() {
            let sb = sandbox();
            let module = sb.compile(ECHO_WAT.as_bytes()).expect("compile");
            let aot_bytes = sb.serialize_aot(&module).expect("serialize_aot");
            let aot_module = unsafe { sb.load_aot(&aot_bytes).expect("load_aot") };
            let r = sb
                .run_compiled(&aot_module, serde_json::json!({}))
                .await
                .expect("run_compiled aot failed");
            assert_eq!(r["result"], 42);
        }

        // ── Test 4: fuel exhaustion ────────────────────────────────────────

        #[tokio::test]
        async fn test_fuel_exhaustion() {
            let sb =
                Sandbox::new(SandboxConfig { max_fuel: 100, ..Default::default() }).unwrap();
            let err = sb
                .run(INFINITE_LOOP_WAT.as_bytes(), serde_json::json!({}))
                .await
                .expect_err("expected fuel-exhaustion error");
            let msg = err.to_string();
            assert!(
                msg.contains("trapped") || msg.contains("fuel"),
                "unexpected error: {msg}"
            );
        }

        // ── Test 5: module signals error (-1) ─────────────────────────────

        #[tokio::test]
        async fn test_module_error_signal() {
            let sb = sandbox();
            let err = sb
                .run(ERROR_WAT.as_bytes(), serde_json::json!({}))
                .await
                .expect_err("expected module error");
            assert!(err.to_string().contains("error"), "{err}");
        }

        // ── Test 6: missing export ─────────────────────────────────────────

        #[tokio::test]
        async fn test_missing_export() {
            // A module that exports only `memory`, missing `alloc` + `run_json`.
            let wat = r#"(module (memory (export "memory") 1))"#;
            let sb = sandbox();
            let err = sb
                .run(wat.as_bytes(), serde_json::json!({}))
                .await
                .expect_err("expected missing-export error");
            assert!(
                err.to_string().contains("alloc") || err.to_string().contains("export"),
                "{err}"
            );
        }
    }

    // ── §6.10: guest WASM calls mcp/call_tool; host proxies to MCP server ─────

    #[cfg(all(feature = "wasm", feature = "mcp-bridge"))]
    mod mcp_bridge_tests {
        use std::sync::Arc;

        use async_trait::async_trait;
        use vanswarm_core::{ContentBlock, ToolDefinition, ToolExecutor};
        use vanswarm_mcp::{McpClient, McpServer};

        use super::super::*;

        /// WAT that imports mcp/call_tool and calls it with name "echo" and params "{}".
        /// Name at 0..4, params at 4..6. Returns the host's (ptr|len) as run_json result.
        const MCP_CALL_WAT: &str = r#"
(module
  (import "mcp" "call_tool" (func $call_tool (param i32 i32 i32 i32) (result i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "echo")
  (data (i32.const 4) "{}")
  (func (export "alloc") (param i32) (result i32) i32.const 0)
  (func (export "run_json") (param i32 i32) (result i64)
    (call $call_tool (i32.const 0) (i32.const 4) (i32.const 4) (i32.const 2)))
)
"#;

        struct EchoTool;

        #[async_trait]
        impl ToolExecutor for EchoTool {
            fn tool_definitions(&self) -> Vec<ToolDefinition> {
                vec![ToolDefinition {
                    name: "echo".into(),
                    description: "Echo the message.".into(),
                    parameters: serde_json::json!({"type": "object", "properties": {}}),
                    examples: vec![],
                }]
            }

            async fn execute(
                &self,
                _name: &str,
                id: &str,
                args: serde_json::Value,
            ) -> ContentBlock {
                let msg = args.get("message").and_then(|v| v.as_str()).unwrap_or("(empty)");
                ContentBlock::ToolResult {
                    tool_use_id: id.into(),
                    content: format!("echo: {msg}"),
                    is_error: false,
                }
            }
        }

        #[tokio::test]
        async fn test_guest_calls_mcp_tool_via_bridge() {
            let server = McpServer::new("test", "0.1.0", Arc::new(EchoTool));
            let (transport, _handle) = server.serve_channel();
            let client = McpClient::channel(transport);
            client.initialize().await.expect("initialize");

            let config = SandboxConfig {
                allow_mcp: true,
                mcp_client: Some(McpClientRef(Arc::new(client))),
                ..SandboxConfig::default()
            };
            let sandbox = Sandbox::new(config).expect("Sandbox::new");

            let result = sandbox
                .run(MCP_CALL_WAT.as_bytes(), serde_json::json!({}))
                .await
                .expect("run with MCP bridge");

            assert_eq!(result.get("is_error").and_then(|v| v.as_bool()), Some(false));
            let content = result.get("content").and_then(|v| v.as_str()).unwrap_or("");
            assert!(content.starts_with("echo:"), "expected echo result, got {content}");
        }
    }
}
