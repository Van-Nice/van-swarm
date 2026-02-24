# Proposal: `vanswarm new` — Agent boilerplate generator

## Summary

Add a CLI command that scaffolds the base folder and files for an agent project using the VanSwarm framework. Users run something like `vanswarm new my-agent` (or `cargo run -p vanswarm-cli -- new my-agent`) and get a ready-to-build crate with a minimal ReAct agent, optional tools, and config.

---

## Goals

- **Fast onboarding:** One command to create a runnable agent crate.
- **Configurable:** Flags to include MCP, memory, a specific provider, or a lib instead of a binary.
- **Workspace-friendly:** Generated project can live inside the framework repo (e.g. `apps/my-agent`) or in a separate directory (path or git dependency on the framework).
- **Documented:** Generated `README.md` and `.env.example` so the user knows how to run and configure.

---

## Command

### Primary form

```bash
vanswarm new <NAME> [OPTIONS]
```

- **NAME** — Name of the agent project (used as crate name and folder name). Must be a valid Rust identifier (e.g. `my_agent`, `my-agent` → normalized to a valid crate name).
- **Output:** A new directory `./<name>/` (or `--path` if set) containing a Cargo.toml, `src/main.rs`, and optional extra files.

### Alternative (from workspace)

```bash
cargo run -p vanswarm-cli -- new <NAME> [OPTIONS]
```

When the CLI is not installed globally, users can run it from the framework workspace.

---

## Flags and options

| Flag / option             | Short | Default            | Description                                                                                                                                                                                                                                         |
| ------------------------- | ----- | ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--path <DIR>`            | `-p`  | `./<name>`         | Output directory. If not set, project is created in `./<name>`.                                                                                                                                                                                     |
| `--provider <PROVIDER>`   |       | `anthropic`        | Default provider for the generated main: `anthropic`, `openai`, or `gemini`. Only affects which provider is instantiated in the boilerplate `main.rs`.                                                                                              |
| `--model <ID>`            |       | (provider default) | Default model id in the generated config (e.g. `claude-sonnet-4-20250514`, `gpt-4o`, `gemini-2.0-flash`).                                                                                                                                           |
| `--with-tools`            |       | _off_              | Add a `src/tools.rs` module with a sample custom tool and register it in main.                                                                                                                                                                      |
| `--with-mcp`              |       | _off_              | Add `vanswarm-mcp` dependency and a commented/optional MCP client + McpToolExecutor example in main or a separate example.                                                                                                                          |
| `--with-memory`           |       | _off_              | Add `vanswarm-memory` dependency and EpisodicMemory usage snippet (e.g. store last query, search before answer).                                                                                                                                    |
| `--lib`                   |       | _off_              | Generate a library crate (`src/lib.rs`) instead of a binary; optional `examples/run_agent.rs` that uses the lib.                                                                                                                                    |
| `--framework-path <PATH>` |       | _path_             | How to depend on the framework: `path` (default, `path = "../../crates/core"` relative to generated crate), or `git` (git URL + optional rev). When generating outside the repo, user can pass `--framework-path git` and we use a placeholder URL. |
| `--no-readme`             |       | _off_              | Skip generating README.md in the project.                                                                                                                                                                                                           |
| `--no-env-example`        |       | _off_              | Skip generating .env.example.                                                                                                                                                                                                                       |
| `--force`                 | `-f`  | _off_              | Overwrite existing directory if it already exists.                                                                                                                                                                                                  |
| `--verbose`               | `-v`  | _off_              | Log each file created.                                                                                                                                                                                                                              |

### Provider defaults (when `--model` is not set)

| `--provider` | Default model id           |
| ------------ | -------------------------- |
| `anthropic`  | `claude-sonnet-4-20250514` |
| `openai`     | `gpt-4o`                   |
| `gemini`     | `gemini-2.0-flash`         |

---

## Generated layout (minimal, no extra flags)

```
<name>/
├── Cargo.toml          # name = "<name>", dependency on vanswarm-core (path or git)
├── .env.example        # ANTHROPIC_API_KEY=, OPENAI_API_KEY=, GEMINI_API_KEY=
├── README.md           # How to run: cargo run, set API key
└── src/
    └── main.rs         # ReActAgent + run_agent with chosen provider and no tools
```

### With `--with-tools`

```
<name>/
├── ...
└── src/
    ├── main.rs         # Registers tools from tools.rs
    └── tools.rs        # One sample Tool impl (e.g. EchoTool or GreetTool)
```

### With `--with-mcp`

- `Cargo.toml`: add `vanswarm-mcp`.
- `src/main.rs` or `examples/mcp_agent.rs`: commented block or example showing McpClient + McpToolExecutor + refresh_tools.

### With `--with-memory`

- `Cargo.toml`: add `vanswarm-memory`.
- `src/main.rs`: optional EpisodicMemory in main or a small helper that stores/retrieves.

### With `--lib`

- `src/lib.rs`: export a function that builds the agent (config + provider + executor) and optionally run_agent.
- `examples/run_agent.rs`: binary that calls the lib and runs the agent.

---

## Cargo.toml (generated, path dependency)

When `--framework-path path` (default) and output is e.g. `apps/my_agent`:

```toml
[package]
name = "my_agent"
version = "0.1.0"
edition = "2021"

[dependencies]
vanswarm-core = { path = "../../crates/core" }
tokio = { version = "1", features = ["full"] }
```

When `--with-mcp`:

```toml
vanswarm-mcp = { path = "../../crates/mcp" }
```

When `--with-memory`:

```toml
vanswarm-memory = { path = "../../crates/memory" }
```

When `--framework-path git` (for use outside the repo):

```toml
vanswarm-core = { git = "https://github.com/Van-Nice/van-swarm", branch = "main" }
# optional: rev = "abc123"
```

---

## main.rs (generated, minimal)

Boilerplate will look like the current `basic_agent` example, with:

- Provider chosen by `--provider` (AnthropicProvider, OpenAiProvider, or GeminiProvider).
- Model id from `--model` or provider default.
- Optional tools registration if `--with-tools`.
- Single `run_agent(&agent, "Hello, run the agent.").await?` or a small prompt; README can tell user to change the prompt or read from stdin.

---

## Implementation notes

- **CLI crate:** Add `crates/cli` (or `vanswarm-cli`) to the workspace. Binary name: `vanswarm` so that after `cargo install --path crates/cli` the user can run `vanswarm new my-agent`.
- **Parsing:** Use `clap` (derive) for `new` subcommand and all flags above.
- **Files:** Use `std::fs` (or `tokio::fs`) to create directory and write files; no external templating required for the first version (format! or const strings).
- **Validation:** Reject invalid crate names (e.g. spaces, leading digits); normalize `my-agent` → `my_agent` for the crate name if desired, or keep folder as `my-agent` and set package name to `my_agent`.
- **Idempotency:** Without `--force`, exit with an error if the target directory already exists.

---

## Future extensions

- **Template variants:** `vanswarm new my-agent --template minimal|mcp|memory|full` as a shorthand for combinations of flags.
- **Interactive prompt:** If no name provided, prompt for name and key options.
- **Plugins:** `--add orchestrator` to add a small graph workflow example.
- **Update:** `vanswarm update` to refresh boilerplate in an existing project (e.g. bump framework path or add a new optional file without overwriting user edits).

---

## Implementation

The CLI is implemented in **crates/cli** (binary name: `vanswarm`).

- **Run from workspace:** `cargo run -p vanswarm-cli -- new <NAME> [OPTIONS]`
- **Install then run:** `cargo install --path crates/cli` then `vanswarm new <NAME> [OPTIONS]`

Generated projects include an empty `[workspace]` in Cargo.toml so they build correctly when created inside the framework repo.

---

## Acceptance criteria

- [x] Running `vanswarm new my_agent` (or from workspace `cargo run -p vanswarm-cli -- new my_agent`) creates `my_agent/` with Cargo.toml, src/main.rs, .env.example, README.md.
- [x] Generated project builds with `cargo build` from inside the generated directory (when run from framework repo with path deps).
- [x] Flags `--provider`, `--model`, `--with-tools`, `--path`, `--force`, `--no-readme`, `--no-env-example`, `--verbose`, `--lib`, `--framework-path` behave as specified.
- [ ] `--with-mcp` and `--with-memory` add dependency and commented/snippet code (optional follow-up).
- [x] Proposal doc (this file) is committed.
