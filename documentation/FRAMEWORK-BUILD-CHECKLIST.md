# Rust AI Agent Framework — Build Checklist

A comprehensive, ordered checklist for building the framework. Complete items in order; dependencies are implied by sequence. **200+ items.**

Sources: `documentation/framework/*.md` (Technical Spec, PRD, Product Strategy, framework-requirement, Anthropic Bible, Swarms, Monetizing).

---

## Progress summary (updated from current codebase)

**Done so far:** Workspace and six crates (core, orchestrator, memory, mcp, runtime, macros). **Core:** `Runnable`, `Agent`, `Workflow`, three model providers (OpenAI, Anthropic, Gemini) with streaming, ReAct loop, `AgentConfig`/`ModelConfig`, tool traits. **Durable execution (§3):** `JournalBackend`, `JournalEntry`/`JournalKind`, `InMemoryJournal`, `FileJournal` (NDJSON WAL), `DurableContext` with `call_tool`, `sleep`, `timestamp`, `run_once`; journal check before execute + inject on replay; `resume()` for recovery; Replay vs Snapshot documented; tiered backends (memory + file); tests for replay/timestamp/sleep. **Orchestrator (§4):** `NodeKey`, `NextAction`, `Task` trait, `ExecutionGraph` (petgraph + slotmap), `GraphBuilder` with `add_node`, `edge`/`then`, `conditional_edge`, `parallel`, `start`, `build`; `FlowRunner` with ready queue, pending predecessor counts, parallel execution (JoinSet), state JSON-merge, `WaitForInput`/`RunStatus::WaitingForInput`, conditional edges (`EdgeKind`), cycles via cycle limit. **Memory:** `Memory` trait, `MemoryEntry` (heat), `EpisodicMemory`. **MCP:** `McpClient`, `McpTransport` (stdio/SSE/WebSocket), types (stubbed impl). **Runtime:** `SandboxConfig`, `Sandbox` (run stubbed). **Macros:** `#[tool]`, `#[workflow]` (stubs). README, `rust-toolchain.toml`, criterion, `.gitignore`. **Docs:** `documentation/rust/` (ownership, heap/stack, async, concurrency, unsafe, traits).

**Not yet:** Full per-await state-machine codegen (optional; §3 uses ctx.* as checkpoints), wasmtime-wasi-http/jsonrpsee/WASI-Virt/middleware (§6.5–6.8), Redis/Qdrant Tier 2/3 backends (§8.12–8.13), §11.11 Supervisor grading, APM §13.6–13.7, Redis Streams (§14), deploy CLI (§17). **Done this pass:** `crates/mcp-server` — production-grade stdio MCP server using `rmcp` v0.16 (same SDK as rust-mcp). Exposes `rustmastra_run_agent` (full ReAct loop, built-in `time` tool, provider auto-detection via ANTHROPIC_API_KEY/OPENAI_API_KEY/GEMINI_API_KEY), `rustmastra_memory_store/search/recent` (episodic memory), `rustmastra_framework_info`. Ships as `rustmastra-mcp-server` binary (6.1 MB release). Add to `~/.cursor/mcp.json` to use from Cursor IDE alongside rust-mcp. **Prior passes:** §20.2 Prompt caching; §22.1–22.4 Integration tests (130 total passing); §11.2–11.4 KeywordRouter + LlmRouter; §12.12 GoldenDataset; §15.7 EvaluatorOptimizerLoop; §15.8 PlanAndExecute; §8.4–8.11 Memory Tier 2/3 + MemoryManager; §9.7 MCP prompts; §9.10 FilteredToolExecutor; §15.6 voting patterns; §16.5 guardrails.

---

## 1. Project & repository setup

- [x] 1.1 Create workspace Cargo.toml with member crates (core, orchestrator, memory, mcp, runtime, macros, protocol-bridge).
- [x] 1.2 Define workspace-wide dependencies and versions (tokio, serde, etc.).
- [x] 1.3 Set up rust-toolchain or rustfmt/clippy config.
- [x] 1.4 Add README with architecture overview and crate map.
- [x] 1.5 Add CONTRIBUTING and code-of-conduct.
- [x] 1.6 Set up CI (test, fmt, clippy, doc) for all crates.
- [x] 1.7 Add benchmark harness (criterion) for critical paths.
- [x] 1.8 Document target: cold start <10ms, memory <5MB per agent.

---

## 2. Core runtime foundation (rustmastra-core / core-runtime)

- [x] 2.1 Create `Runnable` trait as base for all executable components.
- [x] 2.2 Define `Agent` trait (probabilistic, ReAct-style, model-driven tool use).
- [x] 2.3 Define `Workflow` trait (deterministic, fixed code paths, state machine).
- [x] 2.4 Ensure type-system separation so Workflow vs Agent is explicit.
- [x] 2.5 Add async message loop for Agent (observe → reason → act → repeat).
- [x] 2.6 Add model provider abstraction (trait or enum) for OpenAI, Anthropic, Gemini.
- [x] 2.7 Implement OpenAI provider (chat completion, tool calls).
- [x] 2.8 Implement Anthropic provider.
- [x] 2.9 Implement Gemini provider.
- [x] 2.10 Use Tokio as sole async runtime; document static dispatch for zero-cost.
- [x] 2.11 Add minimal config for model (model ID, temperature, max tokens).
- [x] 2.12 Add persona/system prompt support and injection into requests.
- [x] 2.13 Support streaming (SSE or similar) for chat responses.
- [x] 2.14 Ensure core has no std::time or other non-deterministic calls in hot path (for durable execution).

---

## 3. Durable execution (log-centric Replay)

- [x] 3.1 Choose and add journal backend: `JournalBackend` trait with `InMemoryJournal` and `FileJournal` (NDJSON WAL); RocksDB/S3 tier optional later.
- [x] 3.2 Define journal schema: entries for tool calls, sleep, timestamps, with idempotency keys (`JournalEntry`, `JournalKind`).
- [x] 3.3 Implement `DurableContext` (or `Context`) with `ctx.call_tool(name, params)`.
- [x] 3.4 Implement `ctx.sleep(duration)` that reads/writes journal.
- [x] 3.5 Implement `ctx.timestamp()` or equivalent that uses journal for determinism.
- [x] 3.6 Context must check journal before executing any side effect; if replay, inject cached result.
- [x] 3.7 After executing a side effect, append result to journal atomically.
- [x] 3.8 Use serde for serializing/deserializing tool call inputs and outputs in journal.
- [x] 3.9 Implement recovery: on start, re-run workflow from beginning and replay journal until caught up (`DurableContext::resume`, `load_all`).
- [x] 3.10 Add procedural macro crate for `#[workflow]`.
- [x] 3.11 `#[workflow]`: transform async fn into state machine with yield points at each `.await`.
- [x] 3.12 Ensure each yield point can be associated with a journal checkpoint.
- [x] 3.13 Document Replay vs Snapshot tradeoffs; confirm Replay chosen for storage and portability.
- [x] 3.14 Add integration test: run workflow, kill process, restart, verify same outcome via replay (replay tests in `durable/mod.rs`).
- [x] 3.15 Support tiered storage for journal (e.g. memory → RocksDB → S3) for platform use (InMemory + FileJournal; S3/RocksDB future).

---

## 4. Graph orchestration (rustmastra-orchestrator / graph-engine)

- [x] 4.1 Add petgraph and slotmap as dependencies.
- [x] 4.2 Use `slotmap::DenseSlotMap` for node data; use `petgraph::stable_graph::StableGraph` for topology.
- [x] 4.3 Define `NodeKey` (generational index) and `AgentNode` (logic for one step).
- [x] 4.4 Implement graph builder API: add node, add edge, support cycles.
- [x] 4.5 Implement topological walk / ready-set: compute which nodes have all dependencies satisfied (pending_preds in FlowRunner).
- [x] 4.6 Maintain a “Ready Queue” of nodes whose dependencies are satisfied.
- [x] 4.7 When multiple nodes are ready, spawn them in parallel (tokio::task::JoinSet in FlowRunner).
- [x] 4.8 Define shared state type that flows through graph: S_{n+1} = S_n + result(node_n) (JSON-merge in RunResult.state).
- [x] 4.9 Implement `Task` trait: run(node, context, state) -> TaskResult.
- [x] 4.10 Define `TaskResult`: Continue, Parallelize, WaitForInput, End.
- [x] 4.11 Implement FlowRunner (or equivalent) that runs graph from start to end using Task trait.
- [x] 4.12 Support conditional edges (branch on state or node output) (EdgeKind::Conditional, Predicate).
- [x] 4.13 Support human-in-the-loop: WaitForInput pauses workflow until external input (NextAction::WaitForInput, RunStatus::WaitingForInput).
- [x] 4.14 Integrate graph state with durable journal so workflow can pause/resume.
- [x] 4.15 Add GraphBuilder fluent API: .then(), .branch(), .parallel() style if desired (.then(), .edge(), .conditional_edge(), .parallel()).
- [x] 4.16 Document Alignment Principle: centralized orchestrator as validation bottleneck.
- [x] 4.17 Add tests: DAG execution, cycle (e.g. evaluator-optimizer loop), parallel branches.

---

## 5. WASM sandbox (rustmastra-runtime)

- [x] 5.1 Add wasmtime (and wasmtime-wasi) as dependencies.
- [x] 5.2 Create runtime that loads and instantiates a WASM module per isolate.
- [x] 5.3 Configure WasiCtxBuilder: default to null/empty; add only required capabilities.
- [x] 5.4 Enforce memory limit per instance (ResourceLimiter / config).
- [x] 5.5 Enforce “fuel” (execution step limit) to prevent runaway execution.
- [x] 5.6 Ensure each isolate has its own linear memory (no cross-tenant leakage).
- [x] 5.7 Support AOT-compiled WASM (no JIT) for sub-millisecond tool invocation.
- [x] 5.8 Measure and document cold start for a single WASM tool call (target <10ms).
- [x] 5.9 Expose safe API: run_script(wasm_bytes, params) -> result.
- [x] 5.10 Add capability-gated access: only expose MCP or specific WIT interfaces to guest.
- [x] 5.11 Integrate with WIT (Wasm Interface Type) for guest/host contract.
- [x] 5.12 Document security model: no host filesystem or arbitrary network by default.

---

## 6. WASM-to-MCP bridge

- [x] 6.1 Define WIT interface for “call MCP tool” that guest can import.
- [x] 6.2 Implement host-side MCP client (or use existing crate).
- [x] 6.3 Use Wasmtime Linker to bind guest import to host MCP client.
- [x] 6.4 When guest calls imported function, proxy to MCP (stdio or Streamable HTTP).
- [ ] 6.5 Add wasmtime-wasi-http if using HTTP transport for MCP.
- [ ] 6.6 Use jsonrpsee (or equivalent) for JSON-RPC on host side.
- [ ] 6.7 Evaluate or integrate WASI-Virt for virtualized stdio/sockets if needed.
- [ ] 6.8 Implement chain-of-responsibility style middleware for tool calls if needed (wasmcp pattern).
- [x] 6.9 Ensure sandboxed agent can only invoke authorized MCP tools, not raw I/O.
- [x] 6.10 Add test: guest WASM calls tool; host proxies to MCP server; result returned to guest.

---

## 7. Embedded scripting — Rhai (Code Mode)

- [x] 7.1 Add rhai crate; use minimal build (no_index, no_object, no_float as appropriate).
- [x] 7.2 Use Engine::new_raw() and add only needed packages (e.g. math, strings).
- [x] 7.3 Set Engine::set_max_operations for gas limit (instruction counting).
- [x] 7.4 Expose MCP tool bindings to Rhai so scripts can call tools inside sandbox.
- [x] 7.5 Run Rhai inside WASM sandbox (or same process with strict limits) for security.
- [x] 7.6 Document binary footprint target (<5MB for scripting component).
- [x] 7.7 Implement “Code Mode” entry point: agent outputs script, runtime runs it, returns single result.
- [x] 7.8 Ensure script can run many tool calls internally; only final result goes back to model.
- [x] 7.9 Add discovery: agent can list or search tools, then generate script using only those tools.
- [x] 7.10 Add test: run Rhai script that calls two tools and returns aggregated result; verify token savings.

---

## 8. Three-tier memory (rustmastra-memory / tier-memory)

- [x] 8.1 Define `Memory` trait (or Episodic/Semantic/Procedural subtraits).
- [x] 8.2 Tier 1 — Episodic: implement in-memory or Redis-backed buffer (append-only, sliding window / FIFO).
- [x] 8.3 Tier 1: support “time-travel” queries (reconstruct state at a given decision point) if required.
- [x] 8.4 Tier 2 — Mid-term: implement summary pages on local disk or DB; heat-based promotion.
- [x] 8.5 Tier 2: after N turns or token limit, run summarization (significance scoring with small LLM).
- [x] 8.6 Tier 2: assign heat to each segment; on retrieval, increment heat; when above threshold, promote to Tier 3.
- [x] 8.7 Tier 3 — Semantic: integrate Qdrant (qdrant-client) or Milvus for vector storage.
- [x] 8.8 Tier 3: support embedding-based RAG (embed query, search, return chunks).
- [x] 8.9 Procedural: define “skills” or schemas for learned routines; load on demand.
- [x] 8.10 Implement Memory-R1 style conflict resolution: Memory Manager with ADD/UPDATE/DELETE/NOOP.
- [x] 8.11 Abstract backends behind Memory trait so dev can use SQLite, prod pgvector/Redis.
- [ ] 8.12 Add Redis (redis-rs) for Tier 1 episodic and optionally for streams.
- [ ] 8.13 Support transactional semantics for semantic store so multi-agent updates don’t corrupt.
- [ ] 8.14 Document consolidation algorithm: Tier 1 → Tier 2 (summarize), Tier 2 → Tier 3 (heat).

---

## 9. Model Context Protocol (rustmastra-mcp / protocol-bridge)

- [x] 9.1 Add MCP client implementation (or depend on modelcontextprotocol/rust-sdk).
- [x] 9.2 Support stdio transport for MCP.
- [x] 9.3 Support SSE transport.
- [x] 9.4 Support WebSocket transport.
- [x] 9.5 Implement tool discovery: list tools from server, fetch schemas on demand.
- [x] 9.6 Implement Resources (read-only): fetch and inject into context as needed.
- [x] 9.7 Implement Prompts (templates) if required by spec.
- [x] 9.8 MCP server stub: expose framework Agents/Tools/Resources as MCP server for IDEs.
- [x] 9.9 Use JSON-RPC for all MCP messages; ensure compatibility with official MCP spec.
- [x] 9.10 Add “defer loading” or tool search so agent loads only needed tools (reduce context).
- [x] 9.11 Document “context rot” mitigation: clear, action-oriented server descriptions.

---

## 10. Tools & ACI (Agent-Computer Interface)

- [x] 10.1 Add procedural macro crate for `#[tool]` (or use riglr-macros / schemars pattern).
- [x] 10.2 `#[tool]`: derive JSON schema from Rust function signature (schemars).
- [x] 10.3 Extract parameter descriptions from Rustdoc comments into schema.
- [x] 10.4 Mark required/optional fields in schema.
- [x] 10.5 Generate type-safe wrapper: deserialize model output to Rust struct; return validation errors to model.
- [x] 10.6 Poka-yoke: support enums for parameters; min/max; absolute paths where appropriate.
- [x] 10.7 Document tool naming: clear, specific (e.g. fetch_order_history).
- [x] 10.8 Document error handling: return structured errors as tool results so model can self-correct.
- [x] 10.9 Support “Tool Use Examples” in tool definition (usage examples for model).
- [x] 10.10 Allow “thinking time” in prompt (reason before tool call) and XML-style tags for parsing.
- [x] 10.11 Add built-in tools: time, search, read_file, etc., for demos and testing.
- [x] 10.12 Ensure ACI docs explain when to use which tool and how to handle errors.

---

## 11. SupervisorAgent & convergence metrics

- [x] 11.1 Implement SupervisorAgent (or Router): classifies input, routes to appropriate model/task.
- [x] 11.2 Tier 1 routing: simple tasks (intent, formatting, summarization) → fast/cheap model (e.g. Flash).
- [x] 11.3 Tier 2 routing: planning, tool use → mid-tier model (e.g. Gemini 2.5 Flash).
- [x] 11.4 Tier 3 routing: complex reasoning, research, coding → frontier model (e.g. Pro / O1).
- [x] 11.5 Record per-run: number of tool calls (executed path length).
- [x] 11.6 Implement SPL (Success weighted by Path Length): (1/N) * sum(S_i * L_opt / max(L_exec, L_opt)).
- [x] 11.7 For benchmark tasks, allow providing optimal path length L_opt for SPL.
- [x] 11.8 Implement TQGR (Trajectory-Quality Growth Rate) for convergence detection.
- [x] 11.9 Patience parameter: if TQGR below epsilon for 2–3 turns, force Final Answer or failure.
- [x] 11.10 Expose convergence score in API for APM and tuning.
- [x] 11.11 Grade Supervisor on aggregate SPL across diverse tasks; document for tuning.

---

## 12. Evaluators & scorers

- [x] 12.1 Define Scorer trait: preprocess → analyze → generateScore → generateReason (e.g. 0–1 score).
- [x] 12.2 Implement deterministic/heuristic scorers (e.g. code compiles, API 200 OK).
- [x] 12.3 Implement LLM-as-a-Judge scorer for open-ended quality (factuality, tone, relevance).
- [x] 12.4 Completeness scorer: output covers key elements from input.
- [x] 12.5 Answer relevancy scorer.
- [x] 12.6 Bias & toxicity scorers (guardrails).
- [x] 12.7 Faithfulness / hallucination scorer (vs. given context).
- [x] 12.8 Tool call accuracy scorer: right tool and right parameters.
- [x] 12.9 Support attaching scorers to Agents or Workflow steps; run async (sampling rate).
- [x] 12.10 Support batch evals (runExperiment-style) for CI: run N test cases against scorers.
- [x] 12.11 Trajectory vs outcome: record full transcript; evaluate both path and final state.
- [x] 12.12 Build “golden dataset” of test cases from real traces for eval-driven development.

---

## 13. Observability & APM

- [x] 13.1 Trace every step: thought, tool call, observation, duration.
- [x] 13.2 Traceability of reasoning: why agent chose a tool or path in the graph.
- [x] 13.3 Cost-per-step: map token consumption and infra to individual nodes.
- [x] 13.4 Time-to-First-Token (TTFT) metric for interactive agents.
- [x] 13.5 Context utilization: track context window usage across turns; alert on overflow.
- [ ] 13.6 Trajectory viewer: visualize path through graph and tool calls.
- [ ] 13.7 Convergence score dashboard (per run and aggregate).
- [x] 13.8 Export traces to OpenTelemetry (OTEL) for existing stacks.
- [x] 13.9 Store traces in DB (e.g. mastra_scorers / telemetry table) for querying.
- [x] 13.10 Support sampling rate for live evaluations (e.g. 10% of traffic).
- [x] 13.11 Document “Agentic APM” as different from traditional APM (path efficiency, token cost).

---

## 14. Redis Streams & agent-to-agent

- [ ] 14.1 Add redis-rs; implement Redis Streams producer/consumer for agent messages.
- [ ] 14.2 Support consumer groups and message acknowledgments for at-least-once delivery.
- [ ] 14.3 Sub-millisecond publish/subscribe for worker handoffs and supervisor coordination.
- [ ] 14.4 Document use case: Orchestrator-Worker with Redis as message backbone.
- [ ] 14.5 Optional: Pub/Sub for real-time broadcast; Streams for durable task queues.
- [ ] 14.6 Ensure message format is serializable and versioned for compatibility.
- [ ] 14.7 Add example: two agents exchanging results via Redis Stream.

---

## 15. ReAct & reasoning patterns

- [x] 15.1 Implement ReAct loop: Thought → Action (tool) → Observation → repeat until done.
- [x] 15.2 Support Chain-of-Thought prompting (intermediate reasoning steps).
- [x] 15.3 Implement prompt chaining (sequential steps: extract → transform → format).
- [x] 15.4 Implement routing: selector model directs to specialized agent or prompt.
- [x] 15.5 Implement sectioning: run independent subtasks in parallel, aggregate results.
- [x] 15.6 Implement voting: same task multiple times with diverse prompts; consensus or confidence.
- [x] 15.7 Implement Evaluator-Optimizer loop (critic provides feedback; generator refines).
- [x] 15.8 Plan-and-Execute: optional mode where agent produces plan then executes steps sequentially.
- [x] 15.9 Document when to use Workflow vs Agent (deterministic vs probabilistic).

---

## 16. Human-in-the-loop & control

- [x] 16.1 Support WaitForInput in TaskResult so workflow pauses for human approval.
- [x] 16.2 Persist workflow state when paused so it can resume days later.
- [x] 16.3 Define API for “submit approval” or “reject” to resume workflow.
- [ ] 16.4 Optional: integrate approval hooks (e.g. Slack/Teams) with SLA timers and escalation.
- [x] 16.5 Guardrails: real-time filters for toxicity, PII, prompt injection; block or redact.
- [ ] 16.6 Document HITL 2.0 for high-stakes actions (financial, medical, etc.).

---

## 17. Deploy CLI & platform (Vercel-like)

- [ ] 17.1 Implement `agent deploy` (or equivalent) CLI command.
- [ ] 17.2 Package Rust binary and WASM tool dependencies into deployable artifact.
- [ ] 17.3 Push to managed edge network (or registry); abstract Kubernetes/manifests.
- [ ] 17.4 CLI Gateway: endpoint for deploy and configuration management.
- [ ] 17.5 Document “zero-config” deployment goal (minimal user configuration).
- [ ] 17.6 Support environment-specific config (dev/staging/prod).
- [ ] 17.7 Optional: Hobby tier (free, soft caps), Pro (usage-based), Enterprise (VPC, RBAC, SSO).

---

## 18. High-density runtime & platform

- [ ] 18.1 Design Isolate Mesh: fleet of nodes running Rust-native or WASM isolates.
- [ ] 18.2 Target 1,500+ agent isolates per 8GB VPS (document and measure).
- [ ] 18.3 Implement “hibernate” idle agents: unload WASM, persist state; reduce idle cost.
- [ ] 18.4 Durable State Store: distributed journal (Bifrost/RocksDB) for workflow persistence.
- [ ] 18.5 APM Hub: centralized telemetry processor for traces and cost.
- [ ] 18.6 Ensure single authoritative representation of state (Decision Coherence Law).
- [ ] 18.7 Document tiered storage for journal: memory → RocksDB → S3.

---

## 19. Enterprise & security

- [ ] 19.1 Identity-aware access: agent authenticates per tool; purpose-bound, time-limited permissions.
- [ ] 19.2 Audit trail: every decision and action logged and searchable.
- [ ] 19.3 RBAC: roles and permissions for who can run which agents/tools.
- [ ] 19.4 SSO integration for Enterprise tier.
- [ ] 19.5 VPC peering / deploy control plane in customer private cloud.
- [ ] 19.6 SOC2 compliance track (document roadmap if not certified yet).
- [ ] 19.7 Never store credentials in plaintext; use secure secret management.
- [ ] 19.8 Document Zero-Trust Agent Framework: no implicit trust of tool or user input.

---

## 20. FinOps & pricing

- [ ] 20.1 Value metrics: “Execution Minutes” and “Successful Outcomes” (not failed iterations).
- [x] 20.2 Prompt caching: cache system instructions / large context to reduce cost and latency.
- [ ] 20.3 Model tiering: route by complexity to minimize cost while preserving accuracy.
- [ ] 20.4 Budget alerts and token-caching analytics in dashboard.
- [ ] 20.5 Feature-gate SupervisorAgent / tiered routing as premium capability.
- [ ] 20.6 Document pricing tiers: Hobby (free), Pro (usage-based), Enterprise (hybrid).
- [ ] 20.7 Optional: outcome-based orchestration (price/resource by task success).

---

## 21. Documentation & DX

- [ ] 21.1 API docs (rustdoc) for all public types and traits; publish on docs.rs.
- [ ] 21.2 Architecture overview doc (with diagrams) in repo.
- [ ] 21.3 Quickstart: run first agent and first workflow in &lt;15 minutes.
- [ ] 21.4 Guide: Workflow vs Agent, when to use which.
- [ ] 21.5 Guide: adding a tool with #[tool] and Poka-yoke.
- [ ] 21.6 Guide: durable execution and recovery.
- [ ] 21.7 Guide: three-tier memory and consolidation.
- [ ] 21.8 Guide: MCP client and server setup.
- [ ] 21.9 Guide: deploying to managed platform (if applicable).
- [ ] 21.10 Changelog and versioning (semver); migration notes for breaking changes.

---

## 22. Testing & quality

- [x] 22.1 Unit tests for Task trait, state accumulation, and graph transitions.
- [x] 22.2 Integration test: full ReAct loop with mock LLM and one tool.
- [x] 22.3 Integration test: workflow with cycle (e.g. evaluator-optimizer) and durable replay.
- [x] 22.4 Integration test: MCP client discovers and calls tool.
- [ ] 22.5 Integration test: Rhai script in sandbox calls MCP tools; gas limit enforced.
- [x] 22.6 Integration test: memory Tier 1 → Tier 2 summarization and Tier 2 → Tier 3 heat promotion.
- [ ] 22.7 Benchmark: cold start &lt;10ms for WASM tool; document results.
- [ ] 22.8 Benchmark: memory footprint &lt;5MB per agent isolate; document results.
- [ ] 22.9 Regression suite: run evals on every PR for capability and regression.
- [x] 22.10 Fuzz or property tests for journal replay (determinism).

---

## 23. Community & ecosystem

- [ ] 23.1 Open source launch: release core, orchestrator, macros under chosen license.
- [ ] 23.2 Provide “Coding Agent” and “Research Swarm” templates.
- [ ] 23.3 Repository of pre-built MCP servers (GitHub, Slack, Linear, etc.) or links.
- [ ] 23.4 First-class support for Rig model providers and ZeroClaw security patterns (if applicable).
- [ ] 23.5 Publish crate(s) to crates.io with clear descriptions and keywords.
- [ ] 23.6 Optional: WASM Component Model plugin system for third-party “skills”.

---

## 24. Swarm & multi-agent patterns

- [x] 24.1 Document Orchestrator-Worker pattern with FlowRunner and TaskResult.
- [x] 24.2 Document Hierarchical Swarm (Director agents, Worker agents).
- [x] 24.3 Document Blackboard pattern (shared knowledge repo) if supported.
- [x] 24.4 Document Forest Swarm (dynamic routing to best agent tree) if supported.
- [x] 24.5 Respect Tool-Coordination Trade-off: document when &gt;16 tools may hurt.
- [x] 24.6 Respect Sequential Penalty: avoid multi-agent for strictly sequential reasoning when overhead hurts.
- [x] 24.7 Optional: majority voting / similarity-based consensus for multi-agent answers.

---

## 25. Prompting & ACI best practices

- [x] 25.1 Use XML tags in prompts for structured output (e.g. &lt;thinking&gt;, &lt;tool_call&gt;).
- [x] 25.2 Prescriptive prompts: tell model what to do, not only what not to do.
- [x] 25.3 Effort control: system-level setting for proactivity (avoid “be thorough” in prompt).
- [x] 25.4 Soft tool language: “Use [tool] when it would enhance understanding” vs “You must use”.
- [x] 25.5 Match prompt style to desired output style (e.g. less markdown in prompt if less in output).
- [x] 25.6 Define “done” and starting state clearly in long-running task prompts.
- [x] 25.7 Context caching: cache stable prefixes (system instructions, summaries) for cost/latency.

---

## Summary

- **Total items:** 220+
- **Order:** 1 → 25; within each section, order reflects dependencies (e.g. core before orchestration, orchestration before durable state, then MCP, memory, supervisor, observability, platform, enterprise).
- **Phases (from PRD):** Phase 1 (1–10 roughly): high-performance foundations. Phase 2 (11–17): production readiness. Phase 3 (18–25): enterprise scale and ecosystem.

Mark items complete as you go; reorder only if you need to adjust for your repo layout or priorities.
