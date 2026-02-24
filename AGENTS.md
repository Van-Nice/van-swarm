# Rust project

- Project rules live in `.cursor/rules/`; the Agent applies them based on context and globs.
- After editing Rust code, run `cargo check` or `cargo clippy` to verify the build.
- Use rust-analyzer or MCP refactor tools for renames and semantic edits rather than text-only find-replace.
