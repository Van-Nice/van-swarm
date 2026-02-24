# Proposal: Additions and Improvements to vanswarm-mcp-server

## Summary

This proposal outlines how to extend and improve the **vanswarm-mcp-server** crate so it matches and exceeds the capabilities of framework MCP servers like Mastra’s (@mastra/mcp-docs-server) while staying aligned with VanSwarm’s Rust stack, three-tier memory, and existing architecture. It is informed by:

- **VanSwarm MCP Tutorial Feature** — Protocol-driven documentation, tutorials, semantic retrieval, WASM validation, sequential thinking, and metrics.
- **Researching Mastra.ai MCP Framework Features** — Transport detection, resource discovery, prompt integration, memory layers, guardrails, and observability.

The document is organized by area: **Resources**, **Prompts**, **Memory & semantic retrieval**, **Transport**, **Tools**, **Security**, **Observability**, and **Future directions**. Each section states goals, current gap, and concrete steps for the mcp-server (and related crates where needed).

---

## Current State (vanswarm-mcp-server)

| Area | Current behavior |
|------|------------------|
| **Transport** | Stdio only (rmcp `stdio()`). |
| **Tools** | Five tools: `vanswarm_framework_info`, `vanswarm_memory_store`, `vanswarm_memory_search`, `vanswarm_memory_recent`, `vanswarm_run_agent`. |
| **Resources** | None. No `resources/list` or `resources/read`. |
| **Prompts** | None. No `prompts/list` or `prompts/get`. |
| **Memory** | Episodic only (in-memory FIFO or libsql when `VANSWARM_DB_PATH` + `libsql` feature). No semantic tier exposed via MCP. |
| **Server capabilities** | `enable_tools()` only. No resources or prompts capability advertised. |

---

## 1. Resources (documentation as MCP resources)

### Goal

Expose VanSwarm documentation (and optionally examples) as **MCP resources** so IDE agents can read specific docs on demand (e.g. “how do I handle cycles in the orchestrator?”) without dumping the whole knowledge base. Mirrors Mastra’s docs server and the Tutorial doc’s “Resources in Knowledge Delivery.”

### Gap

- No `resources/list` / `resources/read` in the server.
- rmcp may need to be used with a resource handler (or equivalent) to advertise and serve resources.

### Proposal

1. **Enable MCP resources in the server**  
   - Use rmcp’s resource support (if available) or add a minimal resource layer: advertise `ServerCapabilities` with resources enabled and implement `resources/list` and `resources/read`.

2. **URI scheme and mapping**  
   - Introduce a **`vanswarm://`** (or similar) URI scheme for docs, e.g.:
     - `vanswarm://docs/architecture/04-mcp` → `documentation/architecture/04-mcp.md`
     - `vanswarm://docs/guides/04-mcp` → `documentation/guides/04-mcp.md`
   - Root can be configurable (e.g. `VANSWARM_DOCS_ROOT` or project root when running in a repo). Default: relative to the binary or a well-known docs path in the workspace.

3. **Scoped listing**  
   - `resources/list` returns a fixed or discovered set of resource URIs (e.g. under `documentation/architecture/*.md`, `documentation/guides/*.md`). Optionally restrict to safe extensions (e.g. `.md`, `.mdx`).

4. **Path safety**  
   - Resolve URIs to filesystem paths with **root-path anchoring** and **no path traversal** (reuse or mirror vanswarm-core’s ReadFileTool sanitization). Reject any path outside the configured docs root.

5. **Optional: semantic shortcut**  
   - Later, a tool or resource that “returns the N most relevant doc snippets for query Q” can sit on top of semantic memory (Tier 3); for phase 1, direct resource read by URI is enough.

### Dependencies

- No change to vanswarm-memory or vanswarm-core for phase 1; optional later integration with semantic memory for “best doc for this query” style endpoints.

---

## 2. Prompts (structured scaffolding templates)

### Goal

Expose **MCP prompts** (predefined templates) so clients can request “scaffold a new agent” or “scaffold a workflow” with parameters (name, model, etc.) and get back structured instructions and boilerplate. Aligns with Mastra’s prompt integration and the Tutorial doc’s “Prompts as Structured Scaffolding Templates.”

### Gap

- Server does not advertise or implement `prompts/list` or `prompts/get`.

### Proposal

1. **Enable MCP prompts in the server**  
   - Advertise prompts capability and implement `prompts/list` and `prompts/get` (per MCP spec and rmcp APIs).

2. **Initial prompt set**  
   - **new_agent_scaffold** — Parameters: `name`, `model` (optional). Returns instructions + suggested Rust boilerplate (e.g. `ReActAgent`, `AgentConfig`, `LocalToolRegistry`) consistent with vanswarm-macros and existing examples.
   - **new_workflow_scaffold** — Parameters: `name`. Returns instructions + suggested `GraphBuilder` / workflow pattern.
   - Optional: **add_tool_scaffold** — Parameters: `tool_name`. Returns instructions + `#[tool]`-style snippet.

3. **Template storage**  
   - Templates can live as static strings in the crate (e.g. in a `prompts` module), or in embedded files. No need for a database in phase 1.

4. **Consistency with vanswarm-macros**  
   - Text of prompts should reference the same patterns and types that vanswarm-macros and the docs use (e.g. `#\[tool\]`, `ToolExecutor`, `run_agent`), so that “create agent” flows are consistent across CLI, docs, and MCP.

### Dependencies

- None beyond mcp-server; optional later: `#[mcp_prompt]` or similar in vanswarm-macros for compile-time prompt registration.

---

## 3. Memory and semantic retrieval

### Goal

- Expose **semantic search** over framework knowledge (docs + optionally code) so the IDE agent can ask “what’s relevant to X?” and get back ranked snippets.
- Keep episodic memory as today (store/search/recent); add a path to **semantic (Tier 3) memory** when that backend exists.

### Gap

- Only episodic memory is used; no vector/semantic tier exposed. The Tutorial doc’s “Semantic Retrieval and Contextual Prompts” and Mastra’s “Semantic Recall” are the target.

### Proposal

1. **Semantic memory backend (memory crate)**  
   - Implement or adopt a Tier 3 backend (e.g. libsql with vector column, or another vector store). This may already be partially covered by other proposals (e.g. libsql default vector DB). The MCP server then depends on a `Memory`-like trait that supports “search by embedding” or “semantic search” in addition to episodic store/search/recent.

2. **New tool: `vanswarm_memory_semantic_search`**  
   - Parameters: `query: String`, `limit: Option<u32>`.  
   - Behavior: If semantic backend is available, embed `query`, run similarity search, return top-k snippets (with source URI or doc name). If not available, return a clear message (“Semantic memory not configured”) or fall back to full-text episodic search with a note.

3. **Resource + semantic shortcut (optional)**  
   - A resource URI like `vanswarm://docs/semantic?q=...` could return “best matching doc content” for a query; this can be implemented by calling the same semantic search and formatting the result as a single resource body.

4. **Config**  
   - Use existing `VANSWARM_DB_PATH` for libsql-backed semantic when the same DB supports vectors; or a separate env (e.g. `VANSWARM_SEMANTIC_DB_PATH`) if the implementation uses a different store.

### Dependencies

- vanswarm-memory: semantic tier interface and at least one backend (e.g. libsql vectors). vanswarm-mcp-server: depend on that interface and expose one new tool (and optionally the resource shortcut).

---

## 4. Transport and deployment

### Goal

Support **multiple transports** (e.g. Stdio + SSE or Streamable HTTP) so the server can be used both from local IDEs (stdio) and from remote or web clients. Aligns with Mastra’s “Transport Detection” and the Tutorial doc’s “Implementing the JSON-RPC 2.0 Layer.”

### Gap

- Only Stdio is used today.

### Proposal

1. **Keep Stdio as default**  
   - No change to default behavior; Cursor and similar clients continue to use stdio.

2. **Optional SSE or HTTP transport**  
   - Add a feature or binary flag (e.g. `--transport sse --port 8020`) to run the same server over SSE or Streamable HTTP for remote access. Use rmcp’s transport support if available (e.g. `rmcp::transport::sse` or equivalent).

3. **Single binary, transport selected at startup**  
   - One binary; transport chosen via env or CLI to avoid maintaining two binaries. Document in main.rs and in `vanswarm init` / docs.

4. **Cold start**  
   - Preserve “cold start <10 ms” where possible; lazy init of heavy resources (e.g. embedding model for semantic) if needed.

### Dependencies

- rmcp (and possibly additional transport features). No change to core or memory.

---

## 5. Tools (new and extended)

### Goal

- Add tools that support **documentation**, **validation**, and **workflows** without duplicating Mastra’s entire surface; focus on what fits VanSwarm’s architecture.

### Proposal

1. **Docs / resource helper tool (optional)**  
   - A tool such as `vanswarm_docs_read` with a `uri` or `path` parameter that returns the content of a single doc resource (same as `resources/read` but callable as a tool). Useful for clients that prefer tools over resources.

2. **Validation / WASM tool (later)**  
   - The Tutorial doc describes a “sandbox-validate-feedback” loop: agent suggests code → compile to WASM → run in vanswarm-runtime → return result. Expose this as an optional MCP tool (e.g. `vanswarm_validate_wasm` or `vanswarm_run_sandbox`) so IDE agents can validate snippets. Depends on vanswarm-runtime and safe sandboxing; can be feature-gated.

3. **Workflow run tool (later)**  
   - When the orchestrator exposes a stable API to run a workflow by name/ID, add `vanswarm_run_workflow` with input schema mapped from the workflow. Lower priority than resources/prompts/semantic.

4. **Tool metadata**  
   - Where useful, add hints (e.g. read-only vs destructive) so IDEs can show confirmations; align with MCP spec and rmcp’s tool schema (e.g. `description`, optional hints).

### Dependencies

- Docs tool: same as Resources (docs root, path checks). Validation tool: vanswarm-runtime. Workflow tool: vanswarm-orchestrator/orchestration API.

---

## 6. Security

### Goal

Harden the server for local (and optional remote) use: path traversal prevention, safe tool parameters, and optional guardrails. Aligns with the Tutorial doc’s “Security Considerations” and Mastra’s processor/guardrail ideas.

### Proposal

1. **Path traversal**  
   - All file/resource access must resolve under a configured root; reject `..` and absolute paths outside the root. Reuse or mirror vanswarm-core’s ReadFileTool/sanitization logic in the mcp-server for any path derived from tool arguments or resource URIs.

2. **Tool allow-list**  
   - Server only exposes the tools it defines; no dynamic execution of arbitrary commands. Keep using a fixed tool router (e.g. `FrameworkTools` + `#[tool_router]`).

3. **Optional guardrails (later)**  
   - If the server ever processes untrusted user content for storage or forwarding (e.g. in memory_store or in prompts), consider input validation or redaction (e.g. PII, dangerous patterns). Not required for phase 1 if all inputs are considered trusted (e.g. local IDE user).

4. **Documentation**  
   - Document in the crate that the server is intended for local/trusted use; when exposing SSE/HTTP, recommend binding to localhost or putting behind auth in production.

### Dependencies

- None for basic path checks; optional shared util in vanswarm-core for path sanitization if we want a single implementation.

---

## 7. Observability and metrics

### Goal

Make it easier to see what the server is doing (traces, token usage, run metrics) and, in the long term, to measure tutorial/doc effectiveness (e.g. SPL-style metrics). Aligns with Mastra’s “AI Tracing” and the Tutorial doc’s “Metric-Driven Improvement.”

### Proposal

1. **Structured logging**  
   - Already using tracing; ensure key operations (tool calls, resource reads, run_agent start/end) are logged with stable fields (e.g. tool name, run id, duration). Use stderr only so stdout remains JSON-RPC.

2. **Run metrics in run_agent**  
   - If vanswarm-core already exposes run traces, token usage, or cost estimation, the server can optionally return a summary in the tool result (e.g. “Answer: … (tokens: 1234, iterations: 5)”). No new tools required; just enrich `vanswarm_run_agent` response when available.

3. **Optional export**  
   - Later: export spans/metrics to OpenTelemetry or a file for integration with Langfuse, Arize, etc. Lower priority than resources and prompts.

4. **SPL-style metrics (future)**  
   - If we add interactive tutorials with steps, track “optimal steps” vs “actual steps” and success; store in a dedicated store or pass to an evaluator. Out of scope for initial mcp-server work.

### Dependencies

- vanswarm-core: any existing RunMetrics/RunTrace types can be surfaced in the MCP response or logs.

---

## 8. Future directions

- **Sequential thinking** — Expose a “thinking session” style endpoint (stepwise reasoning) if rmcp or a Rust MCP implementation supports it; benchmark and document.
- **Tasks (SEP-1686)** — For long-running operations (e.g. indexing docs into semantic memory, large runs), implement `tasks/list`, `tasks/get`, `tasks/result` so clients can poll instead of blocking.
- **GitHub / issue integration** — As in the Tutorial doc: search issues, scaffold fixes, optional PR creation via a separate GitHub MCP server or tool. Not part of the core mcp-server; can be a separate tool or server.
- **Skills / agent instructions** — Mastra-style “Skills” (structured how-to files) can be exposed as resources (e.g. `vanswarm://skills/...`) or as part of the default server instructions so the IDE agent always has framework best practices in context.

---

## Implementation order (suggested)

| Phase | Scope | Deliverables |
|-------|--------|--------------|
| **1** | Resources + path safety | `resources/list`, `resources/read`, `vanswarm://` docs mapping, root anchoring, no path traversal. |
| **2** | Prompts | `prompts/list`, `prompts/get`, 2–3 scaffold prompts (agent, workflow, optional tool). |
| **3** | Semantic memory | Tier 3 backend in memory crate; `vanswarm_memory_semantic_search` tool; optional `vanswarm://docs/semantic?q=...` resource. |
| **4** | Transport | Optional SSE or HTTP transport, single binary, documented. |
| **5** | Tools + polish | Optional `vanswarm_docs_read` tool; tool hints; optional WASM validation tool (feature-gated). |
| **6** | Observability | Structured fields in logs; run metrics in run_agent response if available. |

---

## References

- **VanSwarm MCP Tutorial Feature** — `documentation/VanSwarm MCP Tutorial Feature.md`
- **Researching Mastra.ai MCP Framework Features** — `documentation/Researching Mastra.ai MCP Framework Features.md`
- **MCP server + libsql + init** — `documentation/proposals/MCP-SERVER-LIBSQL-AND-INIT-COMMAND.md`
- **MCP architecture** — `documentation/architecture/04-mcp.md`
- **Current server** — `crates/mcp-server/src/main.rs`, `crates/mcp-server/src/server.rs`
