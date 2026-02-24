# Proposal: Agent orchestration framework and project/agent CLI

## Summary

Define an **agent orchestration** model for managing **many agents** inside a single project, and extend the CLI so that:

1. **`vanswarm new <project>`** creates a **project** (a crate or workspace) that is structured to host multiple agents and an optional orchestrator.
2. **`vanswarm add agent <name>`** (run inside a project) scaffolds a **new agent** in that project and wires it into the project’s registry or runner.

Users get a clear path: create one project → add N agents → run the whole system (single agent, router, or graph).

---

## Part 1 — Agent orchestration framework (conceptual)

### 1.1 What “orchestration” means here

- **Many agents:** A project can define several ReAct agents (e.g. `researcher`, `writer`, `critic`), each with its own config, tools, and optionally model tier.
- **Management:** A single place (registry, config file, or graph) lists agents and how they are invoked. The framework already provides:
  - **vanswarm-orchestrator:** Graph-based workflows; nodes can be tasks that call one or more agents.
  - **vanswarm-core Router/Supervisor:** Route input to a tier (Tier1/2/3) or to different agents by complexity.
- **Orchestration framework** (this proposal) = **conventions + project layout + optional runner** so that “a project with many agents” has a standard structure and can be driven by the CLI or by code that discovers and runs agents.

### 1.2 Project layout (multi-agent)

A **project** is a Cargo crate (or workspace) that:

- Has a **manifest** listing agents (see below).
- Contains one **agent module** per agent (e.g. `src/agents/researcher.rs`), each exporting a function that builds a `ReActAgent` (or returns config + provider + executor).
- Optionally has a **runner** that:
  - loads all agents from the manifest,
  - uses a **Router** (or config) to choose which agent handles a request, or
  - runs a **graph** (orchestrator) whose nodes call into these agents.

Proposed layout:

```
<project>/
├── Cargo.toml
├── vanswarm.toml          # Project manifest: list of agents, default runner mode
├── .env.example
├── README.md
└── src/
    ├── main.rs              # Entry: parse args, run single agent or orchestrator
    ├── lib.rs               # Re-exports agents + runner
    ├── config.rs            # Shared config (provider, model defaults)
    ├── agents/
    │   ├── mod.rs           # Registry: map name -> builder
    │   ├── researcher.rs    # One agent
    │   └── writer.rs        # Another agent
    ├── tools/               # Optional shared tools
    │   └── mod.rs
    └── runner.rs            # Optional: router or graph that dispatches to agents
```

- **`vanswarm.toml`** (or `.vanswarm.toml`): lists agents and runner mode, e.g.:

```toml
[project]
name = "my_swarm"

[agents]
researcher = { module = "agents::researcher", provider = "anthropic", model = "claude-sonnet-4-20250514" }
writer     = { module = "agents::writer",     provider = "anthropic", model = "claude-sonnet-4-20250514" }

[runner]
# Optional: "single" | "router" | "graph"
mode = "single"
# If mode = "router", which router to use (e.g. keyword, llm)
# If mode = "graph", path to graph definition (future)
```

- **Runner modes:**
  - **single:** `main` runs one agent (by name, e.g. `--agent researcher`). Default if only one agent.
  - **router:** `main` uses a Router (e.g. KeywordRouter or LlmRouter) to select an agent by input.
  - **graph:** `main` runs an ExecutionGraph; some nodes invoke agents (future: graph defined in config or code).

### 1.3 How this uses existing crates

- **vanswarm-core:** ReActAgent, run_agent, Router, AgentConfig, tools.
- **vanswarm-orchestrator:** FlowRunner, Task, GraphBuilder — graph nodes can hold an agent and call `run_agent` in their `Task::run`.
- **vanswarm-memory / vanswarm-mcp:** Optional per-agent or shared; project template can add them.

No new framework crate is required; this is a **project structure and CLI convention** that sits on top of existing crates.

### 1.4 Swarm orchestration tactics (reference)

The following tactics are drawn from research and practice for building and managing agent swarms (see [Building and Managing Agent Swarms](../framework/Building%20and%20Managing%20Agent%20Swarms.md)). They inform how we design runner modes and when to use each pattern.

#### Quantitative principles for scaling

| Principle                       | Finding                                                                                                                                                                              | Implication for this framework                                                                                                                                                        |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Alignment Principle**         | Centralized multi-agent coordination can improve performance by up to ~80% on tasks that are **naturally parallelizable** (e.g. simultaneous analysis of revenue and market trends). | Prefer a **central orchestrator** (graph or router) when work can be split across agents in parallel. Use **FlowRunner** and **Task** nodes that invoke different agents.             |
| **Sequential Penalty**          | On tasks requiring **strict step-by-step reasoning** (each step depends on the previous), multi-agent systems can **degrade performance 39–70%** due to communication overhead.      | Avoid routing simple sequential chains across many agents. Use a **single agent** or a short pipeline when the task is inherently sequential.                                         |
| **Tool-Coordination Trade-off** | As the number of **tools** grows (e.g. beyond ~16), coordinating those tools across multiple agents becomes a bottleneck that outweighs specialization benefits.                     | Cap or scope tools per agent (e.g. **FilteredToolExecutor**); prefer fewer, well-scoped agents over many agents each with huge tool sets.                                             |
| **Validation Bottleneck**       | **Centralized** systems contain error amplification to ~4.4×; **independent** (decentralized) systems can amplify errors ~17× without a central quality gate.                        | Use a **single point of control** (orchestrator as validation bottleneck): all agent invocations go through the runner/graph so policies, rate limits, and audits apply in one place. |

#### Swarm architecture patterns

| Pattern                 | Description                                                                                                                                                                                                                          | Mapping to runner / project layout                                                                                                                                                                                                                                                 |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Orchestrator–Worker** | A central coordinator distributes work to specialized agents, managing task allocation and conflict resolution. Critical for isolating failures and maintaining consistency.                                                         | **Runner mode `graph`**: FlowRunner as coordinator; nodes are tasks that call `run_agent` with a specific agent from the registry. **Runner mode `router`**: Router selects one agent per request; single “worker” per turn.                                                       |
| **Blackboard**          | Specialized agents (knowledge sources) collaborate by reading from and writing to a **shared knowledge repository** (the blackboard). Ideal for complex, self-organizing problem-solving where predefined workflows are unavailable. | **Shared memory**: Use **vanswarm-memory** (e.g. EpisodicMemory or SemanticMemory) as the blackboard; each agent reads/writes via the same `Memory` impl. Project layout: one `config.rs` or `blackboard.rs` holding the shared memory handle; agents receive it in their builder. |
| **Hierarchical Swarm**  | Agents are organized in **layers**: “Director” agents oversee “Worker” agents, breaking large projects (e.g. 50+ tasks) into executable subtasks.                                                                                    | **Runner mode `graph`**: Director node(s) run first (e.g. planner agent); subsequent nodes run worker agents with task descriptions. Alternatively, **router** where the first “director” agent’s output is used to choose the next agent.                                         |
| **Forest Swarm**        | **Dynamic routing** that selects the most suitable **agent or tree of agents** for a given task, optimizing for expertise and computational efficiency.                                                                              | **Runner mode `router`** with **LlmRouter** or **KeywordRouter**: route input to the best-matching agent. Optional: graph that branches by condition (conditional edges) to different agent trees.                                                                                 |

#### Consensus and voting

For consensus-building across multiple agent answers, systems may use **majority voting** or **similarity-based** selection: e.g. choose the answer that has highest cumulative similarity to all other answers. The framework’s **vanswarm-orchestrator** provides `majority_vote`, `majority_vote_owned`, `similarity_vote`, and `similarity_vote_owned` for this (see `orchestrator::patterns`). A project can run N agents in parallel on the same prompt and then pass their outputs to these functions to pick a final answer.

#### Cost and supervision

- **Model tiering / Supervisor:** Route simple queries to cheaper models and complex reasoning to expensive ones (e.g. **Router** → Tier1/Tier2/Tier3). Reduces cost and aligns with the Sequential Penalty (avoid overusing heavy models for trivial steps).
- **Prompt caching** and **KV cache reuse** (when supported by the provider) reduce token cost and latency; the framework’s **RunTrace** and **TraceStore** support cost attribution per step and per agent.

These tactics should be reflected in the **README** and **runner templates** for each mode (e.g. when to use `single` vs `router` vs `graph`, and when to add a shared blackboard).

---

## Part 2 — CLI: create a new project

### 2.1 Command

```bash
vanswarm new <PROJECT_NAME> [OPTIONS]
```

- **PROJECT_NAME** — Name of the project (directory and crate name). Creates a **multi-agent-ready** project (see layout above), not a single-agent binary.
- **Output:** `<project>/` with Cargo.toml, `vanswarm.toml`, `src/main.rs`, `src/lib.rs`, `src/config.rs`, `src/agents/mod.rs`, one default agent (e.g. `default` or `assistant`), optional `src/runner.rs`, .env.example, README.

### 2.2 Flags (aligned with existing CLI proposal)

| Flag                      | Short | Default            | Description                                                      |
| ------------------------- | ----- | ------------------ | ---------------------------------------------------------------- |
| `--path <DIR>`            | `-p`  | `./<name>`         | Output directory.                                                |
| `--provider <PROVIDER>`   |       | `anthropic`        | Default provider for the first agent.                            |
| `--model <ID>`            |       | (provider default) | Default model for the first agent.                               |
| `--with-tools`            |       | _off_              | Add shared `src/tools/` and a sample tool.                       |
| `--with-mcp`              |       | _off_              | Add vanswarm-mcp and MCP snippet.                                |
| `--with-memory`           |       | _off_              | Add vanswarm-memory and EpisodicMemory snippet.                  |
| `--runner <MODE>`         |       | `single`           | Runner mode: `single`, `router`, or `graph` (graph may be stub). |
| `--framework-path <PATH>` |       | `path`             | `path` or `git` for framework deps.                              |
| `--no-readme`             |       | _off_              | Skip README.                                                     |
| `--no-env-example`        |       | _off_              | Skip .env.example.                                               |
| `--force`                 | `-f`  | _off_              | Overwrite existing directory.                                    |
| `--verbose`               | `-v`  | _off_              | Log each file created.                                           |

### 2.3 Generated project contents (minimal)

- **Cargo.toml:** vanswarm-core, tokio; optional mcp, memory, orchestrator depending on flags.
- **vanswarm.toml:** One agent entry (e.g. `assistant`) and `runner.mode = "single"`.
- **src/main.rs:** Parse CLI (e.g. `--agent <name>`), load agent by name from registry, run_agent.
- **src/lib.rs:** Re-export `agents::registry()`, `config`, and runner if present.
- **src/config.rs:** Shared provider/model defaults (from env or config).
- **src/agents/mod.rs:** `registry()` returns a map of name → builder (closure or fn that returns `ReActAgent`); one default agent.
- **src/agents/assistant.rs** (or default): Single ReAct agent builder.

This gives a single default agent at first; users add more with `vanswarm add agent <name>`.

---

## Part 3 — CLI: add an agent to an existing project

### 3.1 Command

```bash
vanswarm add agent <AGENT_NAME> [OPTIONS]
```

- **AGENT_NAME** — Name of the new agent (snake_case, e.g. `researcher`, `code_reviewer`). Used as the module name and key in `vanswarm.toml`.
- **Context:** Must be run from **inside a project** that already has the multi-agent layout (i.e. contains `vanswarm.toml` and `src/agents/mod.rs`). The CLI detects the project root by walking up to find `vanswarm.toml` (or a fallback like `Cargo.toml` with a `[package.metadata.vanswarm]` section).

### 3.2 Flags

| Flag                     | Short | Default            | Description                                                                      |
| ------------------------ | ----- | ------------------ | -------------------------------------------------------------------------------- |
| `--provider <PROVIDER>`  |       | (project default)  | Provider for this agent: anthropic, openai, gemini.                              |
| `--model <ID>`           |       | (provider default) | Model id for this agent.                                                         |
| `--with-tools`           |       | _off_              | Add a dedicated tools module for this agent (e.g. `src/agents/<name>_tools.rs`). |
| `--system-prompt <TEXT>` |       | (none)             | Optional system prompt snippet for this agent.                                   |
| `--dry-run`              |       | _off_              | Print what would be created without writing files.                               |
| `--verbose`              | `-v`  | _off_              | Log each file created or updated.                                                |

### 3.3 What “add agent” does

1. **Create** `src/agents/<name>.rs` — module that exports a single function, e.g. `pub fn build(config: &AppConfig) -> ReActAgent` (or returns the components needed to construct the agent).
2. **Update** `src/agents/mod.rs` — add `pub mod <name>;` and register the agent in the registry map (e.g. `registry.insert("<name>", <name>::build);`).
3. **Update** `vanswarm.toml` — append an `[agents.<name>]` section with provider, model, and optional system_prompt.

No overwrite of existing files without a `--force` flag; if `src/agents/<name>.rs` already exists, the command errors and suggests a different name or `--force`.

### 3.4 Example workflow

```bash
# Create project with one default agent
vanswarm new my_swarm --runner single

cd my_swarm

# Add two more agents
vanswarm add agent researcher --provider anthropic --model claude-sonnet-4-20250514
vanswarm add agent writer --provider openai --model gpt-4o --with-tools

# Run a specific agent
cargo run -- --agent researcher "Summarize the latest news on Rust 2.0"

# Or (if runner mode is router) run with automatic routing
cargo run -- "Summarize the latest news on Rust 2.0"
```

---

## Part 4 — Project manifest (vanswarm.toml)

### 4.1 Schema (proposed)

```toml
[project]
name = "my_swarm"
# Optional: default provider/model for new agents
default_provider = "anthropic"
default_model = "claude-sonnet-4-20250514"

[runner]
mode = "single"   # "single" | "router" | "graph"
# mode = "router": optional
# router = "keyword" | "llm"

[agents.assistant]
module = "agents::assistant"
provider = "anthropic"
model = "claude-sonnet-4-20250514"
# system_prompt = "..."

[agents.researcher]
module = "agents::researcher"
provider = "anthropic"
model = "claude-sonnet-4-20250514"

[agents.writer]
module = "agents::writer"
provider = "openai"
model = "gpt-4o"
```

- **Project discovery:** CLI looks for `vanswarm.toml` in the current directory, then parent directories. If not found but `Cargo.toml` exists and has `[package.metadata.vanswarm]`, the CLI can use that as the manifest (with a different schema if needed).

### 4.2 Alternative: manifest in Cargo.toml

Instead of a separate file, agents could be listed under package metadata:

```toml
[package.metadata.vanswarm]
runner = "single"
agents = ["assistant", "researcher", "writer"]
```

Agent details (provider, model) would then live in code (e.g. in each `agents/<name>.rs`). This keeps one less file but makes “add agent” a pure code generator (no manifest merge). The proposal above prefers a dedicated **vanswarm.toml** so the CLI can add/update entries without parsing Rust.

---

## Part 5 — Implementation phases

### Phase 1 (minimal)

- **`vanswarm new <project>`** — Generate the multi-agent layout with one default agent, `vanswarm.toml`, and a `main` that runs a single agent by name (e.g. `--agent assistant`). Reuse or extend the existing `new` implementation in crates/cli.
- **`vanswarm add agent <name>`** — Create `src/agents/<name>.rs`, update `src/agents/mod.rs`, and append to `vanswarm.toml`. Project detection: require `vanswarm.toml` in cwd or parent.

### Phase 2

- **Runner mode `router`:** Template for `runner.rs` that uses a Router (e.g. KeywordRouter) to select an agent from the registry by input. `main` calls the runner instead of dispatching by `--agent` only.
- **`vanswarm list agents`** — List agents from `vanswarm.toml` (or from registry in code).

### Phase 3

- **Runner mode `graph`:** Stub or example that builds a small ExecutionGraph and invokes agents in nodes. Optional graph definition in config.
- **`vanswarm add workflow`** (future) — Scaffold a graph workflow that uses existing agents as nodes.

---

## Part 6 — Relation to existing pieces

| Existing                   | Role in this proposal                                                                                                                                                   |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **vanswarm new** (current) | Becomes “new project” with multi-agent layout; existing single-agent `new` can remain as `vanswarm new --single-agent` or be deprecated in favor of the project layout. |
| **vanswarm-orchestrator**  | Used when runner mode is `graph`; nodes in the graph can call `run_agent` with the appropriate agent from the registry.                                                 |
| **Router / Supervisor**    | Used when runner mode is `router`; Router::route(input) returns a tier or agent name, then the runner looks up the agent and runs it.                                   |
| **vanswarm.toml**          | New artifact: project manifest for agent list and runner mode. CLI reads/writes it for `add agent` and `list agents`.                                                   |

---

## Part 7 — Acceptance criteria

- [ ] **Proposal** (this document) is agreed and committed.
- [ ] **`vanswarm new <project>`** generates the multi-agent layout (vanswarm.toml, src/agents/, one default agent, main that supports `--agent <name>`).
- [ ] **`vanswarm add agent <name>`** when run inside a project: creates `src/agents/<name>.rs`, updates `src/agents/mod.rs`, updates `vanswarm.toml`; fails clearly if not in a project or if agent name exists.
- [ ] **Project detection** is defined (vanswarm.toml in cwd or ancestor) and documented.
- [ ] **README** in generated project explains how to add agents and run with `--agent`.
- [ ] Optional: **`vanswarm list agents`** prints agents from vanswarm.toml.

---

## Summary

- **Orchestration:** A project is the unit that “manages many agents”; it has a manifest (`vanswarm.toml`), an agent registry in code, and an optional runner (single, router, or graph) using existing framework crates.
- **Swarm tactics:** The proposal aligns runner modes and project layout with established swarm patterns (Orchestrator–Worker, Blackboard, Hierarchical Swarm, Forest Swarm) and scaling principles (Alignment, Sequential Penalty, Tool-Coordination Trade-off, Validation Bottleneck), plus consensus (voting) and cost/supervision. See §1.4.
- **CLI:**
  - **`vanswarm new <project>`** — create a new multi-agent project.
  - **`vanswarm add agent <name>`** — add an agent to the current project.
- **Future:** `list agents`, runner modes `router`/`graph`, and optional `add workflow` for graph-based orchestration.

---

## References

- **Swarm orchestration tactics and scaling principles:** [Building and Managing Agent Swarms](../framework/Building%20and%20Managing%20Agent%20Swarms.md) (documentation/framework).
