//! Cold-start benchmark for a single WASM tool call (§5.8).
//!
//! Target: <10 ms for cold (compile + run). AOT path (load_aot + run) is
//! intended for sub-millisecond invocation after load.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use openswarm_runtime::{Sandbox, SandboxConfig};

/// Minimal WAT that exports memory, alloc, run_json and returns a fixed JSON result.
/// Matches the ECHO_WAT in lib.rs tests.
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

fn cold_start_compile_and_run(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    let sandbox = Sandbox::new(SandboxConfig::default()).expect("Sandbox::new");
    let wasm_bytes = ECHO_WAT.as_bytes();
    let params = serde_json::json!({"n": 1});

    c.bench_function("cold_start_compile_and_run", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = sandbox.run(black_box(wasm_bytes), params.clone()).await;
            });
        });
    });
}

fn aot_load_and_run(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    let sandbox = Sandbox::new(SandboxConfig::default()).expect("Sandbox::new");
    let wasm_bytes = ECHO_WAT.as_bytes();
    let module = sandbox.compile(wasm_bytes).expect("compile");
    let aot_bytes = sandbox.serialize_aot(&module).expect("serialize_aot");
    let params = serde_json::json!({"n": 1});

    c.bench_function("aot_load_and_run", |b| {
        b.iter(|| {
            rt.block_on(async {
                let module = unsafe { sandbox.load_aot(&aot_bytes).expect("load_aot") };
                let _ = sandbox.run_compiled(&module, params.clone()).await;
            });
        });
    });
}

criterion_group!(benches, cold_start_compile_and_run, aot_load_and_run);
criterion_main!(benches);
