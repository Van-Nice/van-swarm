//! # rustmastra-runtime
//!
//! Wasmtime-based WASM sandbox for secure tool isolation (checklist §5).
//!
//! Each tool invocation runs in its own WASM isolate with:
//! * Per-instance memory limits (§5.4)
//! * Execution fuel cap to prevent runaway loops (§5.5)
//! * Capability-gated access: no filesystem or arbitrary network (§5.12)
//!
//! Stub implementation; full build in §5 of the checklist.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Sandbox configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Security and resource limits for a single WASM isolate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Maximum WebAssembly linear memory (bytes).
    /// Default: 5MB (checklist §1.8: <5MB per agent).
    pub max_memory_bytes: usize,

    /// Maximum number of Wasmtime "fuel" units (roughly one per instruction).
    /// Prevents infinite loops / CPU exhaustion.
    pub max_fuel: u64,

    /// If true, the sandbox may call MCP tools via the host bridge (§6).
    pub allow_mcp: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: 5 * 1024 * 1024, // 5 MB
            max_fuel: 1_000_000,
            allow_mcp: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Sandbox struct (stub)
// ─────────────────────────────────────────────────────────────────────────────

/// A WASM sandbox isolate.
///
/// Full implementation (Wasmtime engine, WasiCtxBuilder, WIT interfaces,
/// MCP bridge): checklist §5 and §6.
pub struct Sandbox {
    config: SandboxConfig,
}

impl Sandbox {
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    /// Execute a WASM module with the given parameters.
    ///
    /// Returns the output as a JSON value.
    /// Target cold-start: <10ms (checklist §5.8).
    pub async fn run(
        &self,
        wasm_bytes: &[u8],
        params: serde_json::Value,
    ) -> rustmastra_core::Result<serde_json::Value> {
        // TODO: implement Wasmtime execution (§5.2–5.9)
        let _ = (wasm_bytes, params, &self.config);
        Err(rustmastra_core::FrameworkError::Wasm(
            "Sandbox not yet implemented – coming in §5".into(),
        ))
    }
}
