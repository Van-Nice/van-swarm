# Rust-MCP server — feedback, wishlist & suggestions

Feedback from using the rust-mcp server (Cursor MCP for Rust) while building this framework. See also [rust-mcp-wishlist.md](rust-mcp-wishlist.md) for more detailed “what would have been more helpful” notes.

---

## What we like

- **`cargo_workspace_action` with `output_format: "toon"`** — Structured, compact diagnostics instead of raw terminal output. Running `cargo check` / `cargo clippy` / `cargo test` through the server keeps the context window small and avoids noisy logs. This alone makes the server worth using for routine Cargo workflows.

- **Single source of truth for Cargo** — Preferring the server over the terminal for check/build/test/clippy reduces drift (e.g. wrong directory or flags) and gives consistent, parseable results.

- **`cursor_init_rules`** — One call to bootstrap `.cursor/rules` and `AGENTS.md` for a Rust project. No manual file creation or copying.

- **Semantic tools in principle** — `analyze_codebase_symbol` (hover, references, definition, type, traits) and `inspect_codebase_structure` (diagnostics, items, snapshot) are the right abstraction: use the compiler/rust-analyzer instead of text search when answering “what is this type?” or “where is this used?”. We didn’t use them as much as we could; when we did, they were useful.

- **Refactor with `apply: true`** — Rename and similar refactors that use rust-analyzer and write to disk are valuable for safe renames across the workspace.

- **Clear tool descriptions** — Tool schemas and descriptions make it obvious what each tool does and what parameters (e.g. `workspace_root`, `output_format`) it accepts.

---

## Wishlist

Things that would make the server **much more helpful** for architecture docs, exploration, and daily Rust workflow:

1. **Workspace-only crate graph** — `generate_mermaid_diagram` with `crate_graph` returns the full transitive dependency graph. A mode or parameter that limits the graph to **workspace members only** (and optionally one level of external deps) would be ideal for high-level docs and onboarding. (See [rust-mcp-wishlist.md §1](rust-mcp-wishlist.md#1-workspace-only-crate-graph).)

2. **Crate-level module/structure diagram** — A diagram of **public modules and re-exports** per crate (e.g. `lib.rs` → `config`, `traits`, `react`, …), not just per-file structure. Helps answer “what’s in this crate and how is it organized?” without opening every file.

3. **“Where is this type/trait implemented?”** — A tool that, given a type or trait name (and optional crate), returns all impl blocks (file:line) or implementors across the workspace. Would have made “who implements `ToolExecutor`?” and “where is `Task` used?” trivial.

4. **Workspace summary without building** — A single call that returns workspace members, paths, direct dependencies, and optionally features/bin names **without** running `cargo build` or `cargo check`. Useful for docs and for agents to understand the layout before compiling.

5. **Symbol list per file (name, kind, visibility)** — For a given `.rs` file, a list of top-level items with name, kind (fn/struct/enum/trait/impl/mod), and visibility. Makes “key types” tables and navigation much easier.

6. **Doc comment extraction** — For a symbol or file, return the rustdoc (and optionally first paragraph). Helps keep architecture docs in sync with the code and auto-generate “purpose” bullets.

7. **Explicit workspace-root semantics** — Every tool that takes a path or project parameter should document clearly whether it expects the **workspace root** (repo root with root `Cargo.toml`) or a crate directory, and prefer workspace-root scope so one call works for any member.

8. **Optional “compact” or “summary” mode for diagnostics** — When `output_format: "toon"` isn’t applicable, a way to get a short summary (e.g. error count + first N lines) instead of full output would still help with context size.

---

## Suggestions

- **Promote “toon” by default for check/clippy** — In Cursor rules or server docs, recommend `output_format: "toon"` for `cargo_workspace_action` when running check or clippy, so agents and users get compact output by default.

- **Document when to use which tool** — A short “decision tree” (e.g. “Need diagnostics? → inspect_codebase_structure with action diagnostics” / “Need type at position? → analyze_codebase_symbol with action type”) would increase use of the semantic tools instead of falling back to grep/file read.

- **Package filter for test** — For `cargo_workspace_action` with `command: "test"`, a `package` (or crate) filter would allow “run tests for this crate only” without building the whole workspace, similar to `--package` in Cargo.

- **Stable workspace_root default** — If the server can infer workspace root from the current project, documenting that (and using it as the default when the client doesn’t pass `workspace_root`) would reduce boilerplate and mistakes.

- **List tools by category** — Grouping tools (e.g. “Cargo”, “Semantics”, “Refactor”, “Diagrams”, “Setup”) in the tool list or in docs would make discovery easier.

- **Lightweight “workspace metadata” tool** — Even without full “workspace summary without building”, a tool that returns member crate names and paths (e.g. by parsing `Cargo.toml` only) would help agents and docs without compiling.

- **Example snippets in tool descriptions** — One-line examples in the schema (e.g. `workspace_root: "/path/to/repo"`) for the most common parameters would speed up correct usage.

---

## Summary

| Section    | Takeaway |
|-----------|----------|
| **Likes** | Cargo (especially toon output) and cursor_init_rules are strong wins; semantic and refactor tools are underused but well-designed. |
| **Wishlist** | Workspace-only crate graph, crate-level structure, “where implemented?”, workspace summary without build, symbol list, doc extraction, and clear workspace-root semantics would all add a lot of value. |
| **Suggestions** | Default to toon where applicable, add a “when to use which tool” guide, package filter for test, and a lightweight workspace-metadata option. |

Implementing even **workspace-only crate graph** and **workspace summary (no build)** would make the server noticeably more helpful for documentation and exploration; the rest would compound that benefit.
