# Contributing to OpenSwarm

Thank you for your interest in contributing to the Rust AI Agent Framework.

## Development setup

- **Rust:** Use the version specified in `rust-toolchain.toml` (or latest stable).
- **Build:** `cargo build`
- **Test:** `cargo test`
- **Lint:** `cargo clippy` and `cargo fmt -- --check`

## Before submitting

1. Run `cargo fmt` and `cargo clippy` so CI passes.
2. Add or update tests for new behavior.
3. Update docs (rustdoc, README, or `documentation/`) as needed.

## Areas to contribute

- **Core (§2):** Providers, ReAct loop, config.
- **Durable execution (§3):** Journal backends, `#[workflow]` macro.
- **Orchestrator (§4):** Graph builder, FlowRunner, conditional edges.
- **Memory (§8), MCP (§9), Runtime (§5):** See `documentation/FRAMEWORK-BUILD-CHECKLIST.md`.

## Code of conduct

This project adheres to the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating you agree to uphold it.

## Questions

Open a [Discussion](https://github.com/your-org/rust-agent-framework/discussions) or an [Issue](https://github.com/your-org/rust-agent-framework/issues) (for bugs or feature requests).
