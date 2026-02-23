//! # rustmastra-runtime
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

use rustmastra_core::FrameworkError;

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
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: 5 * 1024 * 1024, // 5 MiB
            max_fuel: 10_000_000,
            allow_mcp: false,
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
    pub fn new(config: SandboxConfig) -> rustmastra_core::Result<Self> {
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
    pub fn compile(&self, wasm_bytes: &[u8]) -> rustmastra_core::Result<CompiledModule> {
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
            "rustmastra-runtime compiled without 'wasm' feature".into(),
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
    pub fn serialize_aot(&self, module: &CompiledModule) -> rustmastra_core::Result<Vec<u8>> {
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
                "rustmastra-runtime compiled without 'wasm' feature".into(),
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
    ) -> rustmastra_core::Result<CompiledModule> {
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
                "rustmastra-runtime compiled without 'wasm' feature".into(),
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
    ) -> rustmastra_core::Result<serde_json::Value> {
        let module = self.compile(wasm_bytes)?;
        self.run_compiled(&module, params).await
    }

    /// Execute a pre-compiled module with the given JSON parameters.
    ///
    /// Internally uses `tokio::task::spawn_blocking` so the Tokio thread is
    /// never blocked by WASM execution.
    pub async fn run_compiled(
        &self,
        module: &CompiledModule,
        params: serde_json::Value,
    ) -> rustmastra_core::Result<serde_json::Value> {
        #[cfg(feature = "wasm")]
        {
            let engine = self.engine.clone();
            let module = module.inner.clone();
            let config = self.config.clone();

            tokio::task::spawn_blocking(move || {
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
                "rustmastra-runtime compiled without 'wasm' feature".into(),
            ))
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Synchronous execution core (runs inside spawn_blocking)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "wasm")]
fn execute_module_sync(
    engine: &wasmtime::Engine,
    module: &wasmtime::Module,
    config: &SandboxConfig,
    params: &serde_json::Value,
) -> rustmastra_core::Result<serde_json::Value> {
    use wasmtime::{Linker, Store};

    // ── Per-invocation Store (fresh every call → no cross-tenant state) ────
    let data = StoreData {
        limiter: WasmLimiter {
            max_memory_bytes: config.max_memory_bytes,
        },
    };
    let mut store: Store<StoreData> = Store::new(engine, data);

    // ── Resource limiter — caps linear memory growth ───────────────────────
    store.limiter(|d| &mut d.limiter);

    // ── Fuel — instruction-level cap ──────────────────────────────────────
    store
        .set_fuel(config.max_fuel)
        .map_err(|e| FrameworkError::Wasm(format!("Failed to configure fuel: {e}")))?;

    // ── Linker — no host imports exposed by default ────────────────────────
    let linker: Linker<StoreData> = Linker::new(engine);

    // ── Instantiate ────────────────────────────────────────────────────────
    let instance = linker
        .instantiate(&mut store, module)
        .map_err(|e| FrameworkError::Wasm(format!("Module instantiation failed: {e}")))?;

    // ── Resolve exports ────────────────────────────────────────────────────
    let memory = instance.get_memory(&mut store, "memory").ok_or_else(|| {
        FrameworkError::Wasm("Module must export 'memory'".into())
    })?;

    let alloc_fn = instance
        .get_typed_func::<i32, i32>(&mut store, "alloc")
        .map_err(|_| FrameworkError::Wasm("Module must export 'alloc(i32) -> i32'".into()))?;

    let run_json_fn = instance
        .get_typed_func::<(i32, i32), i64>(&mut store, "run_json")
        .map_err(|_| {
            FrameworkError::Wasm("Module must export 'run_json(i32, i32) -> i64'".into())
        })?;

    // ── Write params JSON into WASM memory ────────────────────────────────
    let params_bytes = serde_json::to_vec(params)
        .map_err(|e| FrameworkError::Serialization(e.into()))?;
    let params_len = params_bytes.len() as i32;

    let params_ptr = alloc_fn
        .call(&mut store, params_len)
        .map_err(|e| FrameworkError::Wasm(format!("alloc() call failed: {e}")))?;

    memory
        .write(&mut store, params_ptr as usize, &params_bytes)
        .map_err(|e| {
            FrameworkError::Wasm(format!("Failed to write params to guest memory: {e}"))
        })?;

    // ── Call run_json ──────────────────────────────────────────────────────
    let encoding = run_json_fn
        .call(&mut store, (params_ptr, params_len))
        .map_err(|e| {
            // Wasmtime traps include fuel-exhaustion and memory-limit errors.
            FrameworkError::Wasm(format!("run_json() trapped: {e}"))
        })?;

    // The guest signals an error by returning -1 (all bits set).
    if encoding == -1_i64 {
        return Err(FrameworkError::Wasm(
            "Module signalled an error (run_json returned -1)".into(),
        ));
    }

    // ── Decode (result_ptr, result_len) from the i64 return value ─────────
    let result_ptr = ((encoding >> 32) & 0xFFFF_FFFF) as usize;
    let result_len = (encoding & 0xFFFF_FFFF) as usize;

    let result_bytes = memory
        .data(&store)
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

// ─────────────────────────────────────────────────────────────────────────────
// StoreData + ResourceLimiter
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "wasm")]
struct StoreData {
    limiter: WasmLimiter,
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
}
