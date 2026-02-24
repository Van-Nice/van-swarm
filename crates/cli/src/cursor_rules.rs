//! Default .cursor/rules content (same style as rust-mcp cursor_init_rules).
//! Written by `vanswarm init` unless `--no-cursor-rules` is set.

pub const RUST_BASICS: &str = r#"---
description: "Rust style and conventions: formatting, clippy, error handling. Apply when editing .rs files."
alwaysApply: false
---
# Rust basics

- Prefer the `?` operator for error propagation; avoid unnecessary `.unwrap()` in library code.
- Run `cargo fmt` and `cargo clippy` after changes. Fix clippy warnings unless explicitly allowed.
- If the project has `rustfmt.toml` or `.clippy.toml`, follow them; otherwise use stable defaults.
- Use snake_case for functions and variables, PascalCase for types and traits.
"#;

pub const RUST_CARGO_WORKFLOW: &str = r#"---
description: "Cargo and workspace workflows: check, test, build, features. Apply when running Cargo commands."
alwaysApply: false
---
# Cargo workflow

- Use `cargo check` before a full `cargo build` to catch errors quickly.
- Run tests for the crate you changed: `cargo test -p <crate_name>` or `cargo test` from the package directory.
- In workspaces, be aware of which package you're editing; run commands from the workspace root or with `-p`.
- When adding dependencies, prefer minimal feature flags; document why optional features are enabled.
"#;

pub const RUST_REFACTOR: &str = r#"---
description: "Refactoring Rust: renames, extract function, change signature. Apply when refactoring .rs files."
alwaysApply: false
---
# Refactoring Rust

- Use rust-analyzer (or the MCP refactor tools) for symbol renames to update all references.
- For extract function or change signature, use the IDE code actions (lightbulb) when available; they preserve semantics.
- After any refactor, run `cargo check` to confirm the project still compiles.
- Prefer small, incremental refactors; run check between steps.
"#;

pub const RUST_ARCHITECTURE: &str = r#"---
description: "Rust crate layout and entrypoints (lib.rs, main.rs)."
globs:
  - "**/lib.rs"
  - "**/main.rs"
alwaysApply: false
---
# Crate layout

- `lib.rs` is the crate root for libraries; keep re-exports and high-level module structure here.
- `main.rs` is the binary entrypoint; keep it thin and delegate to the library or modules.
- Split into modules when a file grows large or when concerns are distinct; use `mod` and `use` clearly.
- If the project uses async/await, follow existing conventions for runtime and error types.
"#;

pub const RUST_MCP_ROUTING: &str = r#"---
description: "Tool routing rules for Rust development with rust-mcp MCP server. Apply when working on .rs files or Cargo.toml."
globs:
  - "*.rs"
  - "Cargo.toml"
alwaysApply: false
---
# rust-mcp tool routing

When working on this Rust project, you have access to the rust-mcp MCP server.
Prefer its tools over native terminal/file tools for all Rust work — they return structured,
token-efficient output and keep your context window small.

**workspace_root:** The server does not infer the project path. For any tool that takes `workspace_root`, use the **opened workspace folder** (the directory that contains the root `Cargo.toml`). In Cursor, this is typically the project root you have open. Pass it as `workspace_root`; omitting it is not supported.

### When to use which tool (decision tree)

- Need workspace layout without building? → `workspace_metadata` (members, deps, features).
- Need compiler/lint diagnostics? → `inspect_codebase_structure` with `action: "diagnostics"`, or `cargo_workspace_action` with `command: "check"` / `"clippy"`.
- Need type or traits at a position? → `analyze_codebase_symbol` with `action: "type"` or `"traits"`.
- Need rustdoc at a position? → `analyze_codebase_symbol` with `action: "doc"`.
- Need "where is this used?"? → `analyze_codebase_symbol` with `action: "references"`.
- Need outline of a file? → `inspect_codebase_structure` with `action: "items"`.
- Need dependency graph? → `generate_mermaid_diagram` with `diagram_type: "crate_graph"` (default: workspace members only; use `crate_graph_scope: "full"` for all crates).
- Need "who implements this trait?"? → `find_implementations` with `symbol_name` (and optional `crate_filter`).
- Need to rename a symbol? → `refactor` with `action: "rename"`.

## 1. Cargo (CRITICAL: never use terminal for these)

Use `workspace_metadata` to get workspace members, paths, direct dependencies, and features **without** running cargo build or check.
NEVER use native terminal commands for `cargo check`, `cargo build`, `cargo test`, or `cargo clippy`.
ALWAYS use `cargo_workspace_action` with the appropriate `command`:
- `command: "check"` — compiler diagnostics (prefer over run_terminal_cmd for cargo check)
- `command: "clippy"` — lint diagnostics
- `command: "test"` — test runner (optionally add `package` or `test_filter`)
- `command: "build"` — compile artifacts (requires approval)

Check and clippy default to compact `toon` output; you can omit `output_format` or set `output_format: "toon"`.

## 2. Semantics (prefer over reading files)

For types, lifetimes, references, and traits, DO NOT rely only on reading the file.
Use `analyze_codebase_symbol` with the appropriate `action`:
- `action: "hover"` — type and documentation at position
- `action: "doc"` — rustdoc at position (from hover; at definition sites usually the doc comment)
- `action: "type"` — expression type + trait impls
- `action: "references"` — all usages across the workspace
- `action: "definition"` — go-to-definition
- `action: "traits"` — trait implementations at position
- `action: "borrow_graph"` — MIR dump (functions/consts) or closure captures

Use `inspect_codebase_structure` for file-level information:
- `action: "diagnostics"` — semantic diagnostics (prefer over terminal; use `detail: "summary"` first)
- `action: "items"` — list all symbols in a file (use instead of reading the file for an outline)
- `action: "snapshot"` / `action: "diff"` — track structural changes

## 3. Refactoring

Use `refactor` with `action: "rename"` for symbol renames instead of text find-replace.
It uses rust-analyzer and updates ALL references across the workspace accurately.
Set `apply: true` to write edits to disk (triggers approval).

## 4. Diagrams

Use `generate_mermaid_diagram` for architecture visualization:
- `diagram_type: "crate_graph"` — dependency graph (default: workspace-only; `crate_graph_scope: "workspace_plus_direct_deps"` or `"full"` for more)
- `diagram_type: "crate_structure"` — crate -> top-level modules (requires `crate_name`)
- `diagram_type: "file_structure"` — symbol outline of a .rs file
- `diagram_type: "traits"` — trait hierarchy
"#;

/// All rule files to write: (filename, content).
pub fn all_rules() -> &'static [(&'static str, &'static str)] {
    &[
        ("rust-basics.mdc", RUST_BASICS),
        ("rust-cargo-workflow.mdc", RUST_CARGO_WORKFLOW),
        ("rust-refactor.mdc", RUST_REFACTOR),
        ("rust-architecture.mdc", RUST_ARCHITECTURE),
        ("rust-mcp-routing.mdc", RUST_MCP_ROUTING),
    ]
}
