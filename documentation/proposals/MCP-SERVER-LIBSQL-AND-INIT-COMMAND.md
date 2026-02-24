# Proposal: MCP server with local libsql and `vanswarm init` (MCP server init)

## Summary

Two related additions:

1. **vanswarm-mcp-server + local libsql** — Allow the MCP server to spin up (or attach to) a **local libsql database** so that memory tools (episodic and, when implemented, Tier 3 semantic) can be **persistent** instead of in-memory-only. The DB can be file-based in a configurable directory or project path.

2. **Standard vanswarm project + MCP server init command** — Add a CLI command (e.g. **`vanswarm init`** or **`vanswarm mcp init`**) that, in the current directory (or a given path), creates or updates a **standard vanswarm project** and wires it for MCP: optional `vanswarm.toml`, Cursor MCP config snippet, and a convention for where the MCP server finds (or creates) its local libsql DB.

Together, users get: a one-command way to make a project “MCP-ready” with persistent memory, and an MCP server that can back its memory with libsql when desired.

---

## Part 1 — MCP server + local libsql

### 1.1 Goal

Today the vanswarm-mcp-server uses **in-memory** episodic memory (FIFO, 1 000-entry cap). When the process exits, all stored facts are lost. Optionally, the server should be able to:

- Open (or create) a **local libsql database** (single file, e.g. `vanswarm.db` or `memory.db`).
- Use it to persist **episodic** entries (and later Tier 3 semantic vectors) so that restarts and multiple Cursor sessions share the same memory.

No separate “libsql server” process is required: use **embedded libsql** (in-process, file on disk) via `libsql-client` with a `file:` URL or equivalent.

### 1.2 Behaviour

- **Default (unchanged):** If no DB path or env is set, the MCP server keeps using in-memory episodic memory only (current behaviour).
- **Optional persistent DB:**  
  - If **`VANSWARM_DB_PATH`** is set (or a project-local path is discovered, see Part 2), the server opens/creates that file as a libsql DB.  
  - On first run: create tables (episodic table: id, content, created_at, heat; optional Tier 3 table with vector column when that backend exists).  
  - Memory tools (`vanswarm_memory_store`, `vanswarm_memory_search`, `vanswarm_memory_recent`) read/write through this DB instead of the in-memory `VecDeque`.  
  - Tier 3 semantic (when we add a libsql backend) can use the same DB file (separate table with `F32_BLOB` and `vector_top_k()`).

### 1.3 Configuration

| Mechanism | Purpose |
|-----------|---------|
| **`VANSWARM_DB_PATH`** | Absolute or relative path to the libsql DB file (e.g. `./data/vanswarm.db`). If set, the server uses persistent storage. |
| **`VANSWARM_DB_IN_MEMORY`** | If set (e.g. `1`), use an in-memory libsql DB (useful for tests); overrides path. |

If neither is set, the server does not use libsql and keeps the current in-memory implementation.

### 1.4 Implementation sketch

- **Feature flag:** e.g. `libsql` in `vanswarm-mcp-server` (or a new crate `vanswarm-memory-libsql` that the server depends on when the feature is enabled).  
- **Server startup:** After provider detection, if `VANSWARM_DB_PATH` is set and the feature is enabled:  
  - Open/create libsql DB at that path (embedded, `libsql::Database::open()` or equivalent).  
  - Run a small migration or schema init: create episodic table (and optional Tier 3 table) if not present.  
  - Construct a `LibSqlEpisodicMemory` (or similar) that implements the same interface the server’s memory layer expects, and pass that into `FrameworkTools` instead of the in-memory store.  
- **No breaking changes:** Existing deployments that do not set `VANSWARM_DB_PATH` behave exactly as today.

### 1.5 Where the DB file lives (and who creates it)

- **Explicit:** User (or init command) sets `VANSWARM_DB_PATH` to a path. The MCP server creates the file if it doesn’t exist (and creates tables).  
- **Project-local convention (Part 2):** When running in a project that has been “inited”, the init command can create a default path (e.g. `.vanswarm/data/vanswarm.db`) and write an env snippet or instructions so that when the user runs the MCP server “for this project”, it uses that path. The server itself does not need to “discover” the project root; the env or the way the user starts the server (e.g. from `vanswarm mcp run` in the future) can set `VANSWARM_DB_PATH`.

---

## Part 2 — Standard vanswarm project and MCP server init command

### 2.1 Goal

Provide a single command that prepares a directory as a **standard vanswarm project** and wires it for **MCP** (Cursor or other MCP clients), including an optional convention for the local libsql DB used by the vanswarm-mcp-server.

### 2.2 Command: `vanswarm init` (or `vanswarm mcp init`)

**Name:** Either **`vanswarm init`** (general “make this dir a vanswarm project”) or **`vanswarm mcp init`** (explicitly “set up MCP for this project”). This proposal uses **`vanswarm init`** as the main command, with optional **`--mcp-only`** to only add MCP-related files.

**Behaviour:**

- **Run in an existing directory** (e.g. a repo root or an empty folder).  
- **Idempotent where possible:** Create only what’s missing; overwrite only when requested (e.g. `--overwrite` for config snippets).

**Creates / updates:**

1. **Project manifest (optional)**  
   - If the directory does not look like a vanswarm project (no `vanswarm.toml`), create a minimal **`vanswarm.toml`** (or `.vanswarm.toml`) with project name and optional `[mcp]` section (see below).  
   - If the directory is already a Cargo project, `vanswarm.toml` can coexist with `Cargo.toml`.

2. **MCP config for Cursor**  
   - Write a **`.cursor/mcp.json`** (project-level) or append/print a snippet for **`~/.cursor/mcp.json`** so that the **vanswarm-mcp-server** (and optionally **rust-mcp**) are configured.  
   - Project-level is preferable when the repo is shared: everyone gets the same MCP setup. Cursor can merge or prefer project-level MCP config.  
   - Snippet should include:  
     - **vanswarm** server: `command` pointing to the user’s `vanswarm-mcp-server` binary (or `cargo run -p vanswarm-mcp-server` if we support that), and **env** with optional `VANSWARM_DB_PATH` (see below).  
     - Optional: **rust-mcp** server if the project is Rust and we want to offer it by default.

3. **Local libsql DB convention**  
   - Create a directory, e.g. **`.vanswarm/data/`**, and a placeholder or instructions.  
   - In **`.env.example`** or a small **`.vanswarm/README`** (or comments in `vanswarm.toml`), document:  
     - `VANSWARM_DB_PATH=.vanswarm/data/vanswarm.db`  
   - So that when the user runs the MCP server with this project root as cwd (or sets env in Cursor’s MCP config), the server persists memory to this file.  
   - **Init command can create `.vanswarm/data/`** and add to `.gitignore` (e.g. `.vanswarm/data/*.db`) so the DB is local and not committed.

4. **Optional: Cursor rules**  
   - If **`--cursor-rules`** (or similar) is set, call or document the use of **rust-mcp’s `cursor_init_rules`** so the project gets `.cursor/rules/` for Rust. This can be a follow-up or a separate subcommand.

### 2.3 Example layout after `vanswarm init`

```
<project>/
├── .cursor/
│   └── mcp.json              # vanswarm + optional rust-mcp; env VANSWARM_DB_PATH
├── .vanswarm/
│   ├── data/                 # created; .gitignore *.db
│   │   └── .gitkeep
│   └── README                # optional: explains VANSWARM_DB_PATH
├── .gitignore                # append .vanswarm/data/*.db if present
├── vanswarm.toml             # minimal [project], optional [mcp]
├── Cargo.toml                # (unchanged if existing)
└── ...
```

**Example `vanswarm.toml` (minimal):**

```toml
[project]
name = "my_agent"

[mcp]
# Path to vanswarm-mcp-server binary (optional; default from PATH or cargo).
# server_path = "/path/to/vanswarm-mcp-server"
# DB path used when running MCP server for this project.
db_path = ".vanswarm/data/vanswarm.db"
```

**Example `.cursor/mcp.json` (project-level):**

```json
{
  "mcpServers": {
    "vanswarm": {
      "command": "/path/to/van-swarm/target/release/vanswarm-mcp-server",
      "env": {
        "VANSWARM_DB_PATH": ".vanswarm/data/vanswarm.db"
      }
    }
  }
}
```

The init command would resolve `server_path` (e.g. from `vanswarm.toml` or from `cargo metadata` when run from the framework repo) and write the appropriate absolute or relative path so Cursor can start the server.

### 2.4 CLI interface (draft)

```text
vanswarm init [OPTIONS] [PATH]

OPTIONS:
  --mcp-only        Only add MCP config and .vanswarm/data; skip vanswarm.toml if not needed
  --cursor-rules    Also initialize Cursor rules (e.g. via rust-mcp cursor_init_rules)
  --overwrite       Overwrite existing .cursor/mcp.json or vanswarm.toml
  --no-libsql       Do not set up .vanswarm/data or VANSWARM_DB_PATH (in-memory only)
  -v, --verbose     Log each file created or updated
```

Default `PATH` is current directory.

### 2.5 Relation to `vanswarm new`

- **`vanswarm new <name>`** — Creates a **new** directory with a full Cargo crate (or workspace), provider, tools, memory, MCP example, etc. It’s for **greenfield** projects.  
- **`vanswarm init`** — Prepares an **existing** directory as a vanswarm project and wires MCP (and optional libsql). Use when:  
  - You already have a repo and want to add MCP + persistent memory, or  
  - You want only MCP config and DB convention without generating a full crate.

We can later add a flag to **`vanswarm new`** (e.g. `--init-mcp`) that runs the equivalent of `vanswarm init` inside the newly created project so that `new` + `init` behaviour is available in one shot.

---

## Part 3 — Summary table

| Component | Change |
|-----------|--------|
| **vanswarm-mcp-server** | Optional: when `VANSWARM_DB_PATH` is set (and libsql feature enabled), open/create libsql DB and use it for episodic (and later Tier 3) memory. |
| **vanswarm-cli** | New subcommand **`vanswarm init`** (or **`vanswarm mcp init`**): create/update `vanswarm.toml`, `.cursor/mcp.json`, `.vanswarm/data/`, and document `VANSWARM_DB_PATH`. |
| **Convention** | Project-local DB path: `.vanswarm/data/vanswarm.db`; init creates dir and adds to .gitignore. |
| **Docs** | Document `VANSWARM_DB_PATH`, `vanswarm init`, and the MCP server’s use of libsql in guides and PLATFORM-FEATURES. |

---

## Part 4 — Risks and mitigations

- **Project-level `.cursor/mcp.json`** — Not all MCP clients may support project-level config. Mitigation: document both project-level and user-level (`~/.cursor/mcp.json`) options; init can print a snippet for the latter.  
- **Path to vanswarm-mcp-server** — Init may not know the absolute path to the binary. Mitigation: allow `vanswarm.toml` to set `mcp.server_path`; or use `cargo run -p vanswarm-mcp-server` in a wrapper script; or document manual edit.  
- **libsql dependency** — Keep it optional (feature or separate crate) so the default MCP server build stays minimal.  
- **Schema evolution** — First version of the episodic table should be simple (id, content, created_at, heat); Tier 3 table can be added later with migrations or a versioned schema.

---

## Conclusion

It is **possible and desirable** to:

1. **Have the vanswarm-mcp-server spin up (or attach to) a local libsql DB** when `VANSWARM_DB_PATH` is set, using it for persistent episodic (and later semantic) memory without a separate DB process.  
2. **Provide a standard vanswarm project and an MCP server init command** (`vanswarm init`) that creates a minimal project manifest, project-level Cursor MCP config, and a convention for a local libsql DB (`.vanswarm/data/vanswarm.db`) so that one command makes a directory “MCP-ready” with optional persistent memory.

This proposal outlines the behaviour, config, and CLI surface; implementation can be phased (e.g. init command first with in-memory-only, then libsql support in the MCP server).
