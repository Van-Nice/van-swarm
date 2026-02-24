# VanSwarm Platform — Complete Feature List

This document lists **100% of implemented features** in the VanSwarm agent framework, by crate and by domain. It reflects the current codebase and the completed items in the build checklist.

---

## Workspace & crates

| Crate                     | Purpose                                                                                                                            |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| **vanswarm-core**         | Traits, config, messages, providers, ReAct loop, durable execution, evaluators, supervisor, tools, guardrails, patterns, telemetry |
| **vanswarm-orchestrator** | Graph-based workflow engine (DAG/cycles), FlowRunner, Task, NextAction                                                             |
| **vanswarm-memory**       | Three-tier memory (Episodic, MidTerm, Semantic), MemoryManager                                                                     |
| **vanswarm-mcp**          | MCP client, server, McpToolExecutor, transports                                                                                    |
| **vanswarm-mcp-server**   | Production stdio MCP server binary (run_agent, memory, framework_info)                                                             |
| **vanswarm-runtime**      | WASM sandbox (Wasmtime), optional MCP bridge, optional Rhai scripting                                                              |
| **vanswarm-macros**       | `#[tool]`, `#[workflow]` procedural macros                                                                                         |

---

## 1. Core runtime (vanswarm-core)

### 1.1 Traits & base

- **Runnable** — base trait for executable components
- **Agent** — probabilistic, ReAct-style, model-driven tool use
- **Workflow** — deterministic, fixed code paths
- **WorkflowStep** — step interface for workflows
- **WorkflowStatus** — status type for workflows
- **Tool** — single tool: `definition()`, `execute(arguments)`
- **ToolExecutor** — registry: `tool_definitions()`, `execute(name, id, args)`
- Type-system separation between Workflow and Agent

### 1.2 Configuration

- **AgentConfig** — name, model config, system prompt, max iterations, chain-of-thought
- **ModelConfig** — model_id, temperature, max_tokens
- **ProviderCredentials** — credential injection for providers

### 1.3 Messages & completion

- **Message** — role (System/User/Assistant) + content blocks
- **ContentBlock** — text, tool_use, tool_result
- **CompletionRequest** / **CompletionResponse** — provider API surface
- **TokenUsage** — input/output token counts and delta
- **StopReason** — stop_sequence, max_tokens, end_turn
- **StreamChunk** / **ResponseStream** — streaming responses
- **ToolDefinition** — name, description, input_schema, examples
- **ToolExample** — tool-use examples for the model
- **extract_xml_blocks(text, tag)** — parse `<tag>…</tag>` (e.g. `<thinking>`)
- **new_tool_call_id()** — generate tool call IDs

### 1.4 Model providers

- **ModelProvider** — trait: `complete(CompletionRequest) -> CompletionResponse`
- **OpenAiProvider** — OpenAI chat completion, tool calls, streaming
- **AnthropicProvider** — Anthropic Messages API, tool calls, streaming
- **GeminiProvider** — Google Gemini, tool calls, streaming
- Credentials via env (e.g. OPENAI_API_KEY) or ProviderCredentials

### 1.5 ReAct agent

- **ReActAgent** — concrete Agent: config + ModelProvider + ToolExecutor
- **run_agent(agent, user_input)** — loop until final answer or max iterations
- **run_agent_with_metrics(agent, user_input)** — returns (answer, RunMetrics)
- **run_agent_traced(agent, user_input)** — returns (answer, RunTrace) for APM
- **AgentContext** — messages, iteration, tool_call_count, token_usage, max_iterations
- **AgentAction** — FinalAnswer, CallTools, NeedsClarification
- **ToolCall** — name, id, arguments
- **RunMetrics** — iterations, tool_call_count (path length for SPL)
- **extract_tool_calls(msg)** — extract tool calls from assistant message
- **has_pending_tool_calls(action)** — helper for loop control
- Chain-of-thought: optional system prompt prefix for `<thinking>` tags

### 1.6 Tools

- **LocalToolRegistry** — in-process registry of boxed Tools, fluent `.register()`
- **FilteredToolExecutor** — wrap executor; expose only tools matching keyword list (defer loading / reduce context)
- **TimeTool** — built-in: current UTC (ISO 8601), no params
- **ReadFileTool** — built-in: read file under root path, path traversal rejected
- **SearchTool** — built-in stub: query param, returns stub message

### 1.7 Durable execution

- **JournalBackend** — trait: get, put, load_all, clear
- **JournalEntry** — seq, kind, result, recorded_at, duration_ms
- **JournalKind** — ToolCall, Sleep, Timestamp, Custom
- **InMemoryJournal** — in-process journal (tests)
- **FileJournal** — NDJSON WAL on disk
- **DurableContext** — workflow_id, journal, optional executor
  - **call_tool(name, args)** — journaled tool execution
  - **sleep(duration)** — journaled sleep
  - **timestamp()** — deterministic “now”
  - **run_once(label, async_block)** — generic journaled side effect
  - **resume(workflow_id, journal, executor)** — load journal and replay

### 1.8 Supervisor / routing

- **Router** — trait: `route(input) -> Route`
- **Route** — Tier1, Tier2, Tier3 (fast/cheap, mid, frontier)
- **AlwaysTier1** — stub: always return Tier1
- **KeywordRouter** — keyword-based routing to tier
- **LlmRouter** — LLM-based routing
- **TqgrDecision** / **TqgrTracker** — trajectory-quality growth rate for convergence

### 1.9 Evaluators & scorers

- **Scorer** — trait: name(), score(ScoreInput) -> ScoreResult
- **ScoreInput** — messages, final_answer, optional expected
- **ScoreResult** — score (0–1), reason
- **batch_score(scorer, inputs)** — run scorer on N cases (CI)
- **NonEmptyScorer** — deterministic: non-empty final answer
- **ContainsScorer** — deterministic: final answer contains expected (case-insensitive)
- **LlmJudgeScorer** — LLM-as-judge for quality
- **CompletenessScorer** — covers key elements
- **RelevancyScorer** — answer relevancy
- **BiasScorer** — bias/toxicity
- **FaithfulnessScorer** — vs. given context (hallucination)
- **ToolAccuracyScorer** — right tool and parameters
- **SampledScorer** — sample rate for live evals
- **TrajectoryScorer** — evaluate full trajectory
- **BenchmarkTask** — expected, optimal_path_length (L_opt)
- **SplRun** — score, path_length (L_exec), optimal_path_length
- **spl(runs)** — Success weighted by Path Length formula
- **GoldenCase**, **GoldenDataset**, **GoldenDatasetSummary**, **GoldenDatasetEval** — golden dataset evals

### 1.10 Guardrails

- **GuardRail** — trait: check before/after model call
- **KeywordGuardRail** — block/redact by keywords
- **PromptInjectionGuardRail** — prompt injection detection
- **GuardedModelProvider** — wrap ModelProvider with guardrails

### 1.11 Patterns

- **EvaluatorOptimizerLoop** — critic feedback, generator refines (threshold, max_iterations)
- **EvalOptResult** — result type for eval-opt loop
- **PlanAndExecute** — plan then execute steps (max_steps)
- **PlanStep**, **PlanAndExecuteResult** — plan step and result types

### 1.12 Telemetry & APM

- **RunTrace** — full trace: steps, tool calls, tokens, duration, cost
- **RunTraceBuilder** — build trace during run
- **SpanEvent**, **AgentSpanKind** — span types
- **ModelPricing** — token pricing for cost estimation
- **default_pricing()** — default model pricing list
- **ContextMeter** — context window utilization
- **TraceStore** — trait for persisting traces
- **InMemoryTraceStore** — in-memory trace store
- **FileTraceStore** — file-based trace store
- **SamplingFilter** — sampling rate for live evals

### 1.13 Error

- **FrameworkError** — unified error enum (provider, tool, serialization, durable, WASM, etc.)
- **Result<T>** — alias for Result<T, FrameworkError>

---

## 2. Orchestrator (vanswarm-orchestrator)

- **NodeKey** — generational key (slotmap) for graph nodes
- **Task** — trait: State, run(key, state) -> (State, NextAction), name()
- **NextAction** — Continue, Parallelize(node_keys), WaitForInput(prompt), End
- **GraphBuilder** — add_node, edge, then, conditional_edge, parallel, start, build()
- **ExecutionGraph** — immutable graph (petgraph + slotmap), node_count(), start_node()
- **EdgeKind** — Always, Conditional(Predicate)
- **Predicate** — fn(&Value) -> bool for conditional edges
- **FlowRunner** — run(initial_state), resume(checkpoint, input)
- **RunResult** — state (JSON), status, checkpoint
- **RunStatus** — Completed, WaitingForInput(prompt, paused_at)
- **GraphCheckpoint** — serializable snapshot for resume
- **RunnerConfig** — runner configuration
- **ErasedTask** / **TaskAdapter** — type erasure for mixed State types
- Ready queue + pending predecessor counts
- Parallel execution of ready nodes (JoinSet)
- JSON-merge of node output state (right-wins)
- Cycle support with per-node iteration limit
- Alignment Principle: centralized orchestrator as validation bottleneck

### Orchestrator patterns

- **majority_vote** / **majority_vote_owned** — consensus over multiple answers
- **similarity_vote** / **similarity_vote_owned** — similarity-based consensus

---

## 3. Memory (vanswarm-memory)

### 3.1 Common

- **Memory** — trait: store, recent, search, delete
- **MemoryEntry** — id, content, created_at, heat, optional embedding

### 3.2 Tier 1 — Episodic

- **EpisodicMemory** — in-memory VecDeque, FIFO, max capacity, substring search
- **entries_before(id)** — time-travel: entries before given id (chronological)
- **recent_ordered(limit)** — most recent N entries in chronological order

### 3.3 Tier 2 — Mid-term

- **MidTermMemory** — disk-backed NDJSON (optional path), heat-based
- **SummaryEntry** — id, content, source_ids, created_at, last_accessed, heat
- **store_summary**, **consolidate(entries, summary_text)** — store/consolidate
- **recent_summaries(limit)** — recent summaries, bumps heat
- **search_summaries(query, limit)** — full-text search, bumps heat
- **hot_entries()** — entries with heat >= threshold (Tier 3 promotion candidates)

### 3.4 Tier 3 — Semantic

- **SemanticMemory** — in-memory vector store, cosine similarity (zero-dependency default)
- **store_with_embedding(entry, embedding)** — store with vector
- **semantic_search(query_embedding, limit)** — top-k by similarity
- **cosine_similarity(a, b)** — exported utility
- **Default persistent vector DB (recommended):** libsql (embedding-based RAG; optional Qdrant/pgvector for scale-out)

### 3.5 Memory manager

- **MemoryManager** — conflict resolution (Memory-R1 style)
- **MemoryAction** — Add, Update(target_id), Delete(target_id), Noop
- **evaluate(new_fact, existing)** — returns recommended action (similarity_threshold, keyword heuristics)

---

## 4. MCP (vanswarm-mcp)

- **McpClient** — connect to MCP server
  - **stdio(cmd, args)** — spawn subprocess transport
  - **http(endpoint)** — HTTP/SSE transport
  - **channel(ChannelTransport)** — in-memory for tests
  - **initialize()** — MCP handshake
  - **list_tools()**, **list_resources()**, **list_prompts()**
  - **call_tool(name, arguments)**, **read_resource(uri)**, **get_prompt()**
- **McpToolExecutor** — implements ToolExecutor; forwards to McpClient
  - **refresh_tools()** — cache tool definitions after initialize
- **McpServer** — expose ToolExecutor as MCP server
  - **new(name, version, executor)** — build server
  - **serve_stdio()** — block on stdio JSON-RPC
  - **serve_channel()** — in-memory transport
  - **handle_request()** — single request handling
- **Transport** — trait; **StdioTransport**, **HttpTransport**, **ChannelTransport**
- **Protocol types** — InitializeResult, ListToolsResult, McpTool, CallToolResult, resources, prompts, ServerInfo, ServerCapabilities, PROTOCOL_VERSION
- Context rot mitigation documented (clear, action-oriented descriptions)

---

## 5. MCP server binary (vanswarm-mcp-server)

- Stdio MCP server using **rmcp** (same SDK as rust-mcp)
- **Provider auto-detection** — ANTHROPIC_API_KEY → Anthropic (claude-opus-4-6), OPENAI_API_KEY → OpenAI (gpt-4o), GEMINI_API_KEY → Gemini (gemini-2.0-flash)
- **RUSTMASTRA_MODEL** — override default model
- **Tools exposed:**
  - **vanswarm_run_agent** — full ReAct loop (prompt, system_prompt, model, max_iterations), built-in time tool, auto-detected provider
  - **vanswarm_memory_store** — store episodic entry (content)
  - **vanswarm_memory_search** — search episodic memory (query, limit)
  - **vanswarm_memory_recent** — recent episodic entries (limit)
  - **vanswarm_framework_info** — framework name/version and provider info
- Ships as **vanswarm-mcp-server** binary (~6.1 MB release)
- Cursor IDE: add to ~/.cursor/mcp.json

---

## 6. Runtime (vanswarm-runtime)

- **SandboxConfig** — max_memory_bytes, max_fuel, allow_mcp, optional mcp_client (mcp-bridge feature)
- **Sandbox** — new(config), compile(wasm_bytes), load_aot(aot_bytes), run_compiled(module, params)
- **CompiledModule** — cheap to clone, reusable
- Per-invocation: fresh **Store**, **ResourceLimiter**, fuel cap, **Linker** (WASI preview1, empty context by default)
- **run_json** calling convention: memory, alloc(len), run_json(ptr, len) -> i64 (result_ptr|len or -1)
- No host imports by default; **allow_mcp** + **mcp-bridge** feature: guest can call host MCP tools
- **Rhai scripting** (scripting feature): RhaiEngine, ScriptConfig, ScriptContext; gas limit (set_max_operations); MCP tool bindings; Code Mode: agent outputs script, runtime runs it, single result
- Features: **default**, **wasm**, **mcp-bridge**, **scripting**

---

## 7. Macros (vanswarm-macros)

- **#[tool]** — on async fn: derive JSON schema (schemars), description from Rustdoc, type-safe wrapper, validation errors to model
  - **#[tool(example(description, input, output))]** — one or more tool-use examples for the model
- **#[workflow]** — validates first parameter is `Arc<DurableContext>` (or path ending in DurableContext); compile error otherwise; body pass-through (checkpoints via ctx.call_tool, ctx.sleep, ctx.run_once)

---

## 8. Project & repository

- Workspace Cargo.toml with members: core, orchestrator, memory, mcp, mcp-server, runtime, macros
- Workspace-wide dependencies (tokio, serde, reqwest, etc.) and rust-version 1.82
- rust-toolchain / rustfmt / clippy config
- README with architecture and crate map
- CONTRIBUTING and code-of-conduct
- CI: test, fmt, clippy, doc
- Criterion benchmark harness
- Target: cold start <10 ms, memory <5 MB per agent (documented)

---

## 9. Testing & quality (implemented)

- Unit tests: Task, state accumulation, graph transitions
- Integration: full ReAct loop with mock LLM and one tool
- Integration: workflow with cycle (evaluator-optimizer), durable replay
- Integration: MCP client discovers and calls tool
- Integration: memory Tier 1 → Tier 2 summarization, Tier 2 → Tier 3 heat promotion
- Fuzz/property: journal replay determinism
- 130+ integration tests (per checklist)

---

## 10. Documentation (in repo)

- **documentation/HOW-IT-WORKS.md** — high-level how everything works
- **documentation/architecture/** — 00-overview, 01-core, 02-orchestrator, 03-memory, 04-mcp, 05-runtime, 06-macros, 07-observability-apm
- **documentation/guides/** — quick start, building an agent, tools, MCP, durable workflows, orchestrator, memory, evaluation, runtime, examples reference
- **documentation/FRAMEWORK-BUILD-CHECKLIST.md** — 220+ item checklist with progress
- **documentation/PLATFORM-FEATURES.md** — this file (100% feature list)

---

## 11. Swarm & multi-agent (documented)

- Orchestrator-Worker pattern (FlowRunner, TaskResult)
- Hierarchical Swarm (Director/Worker)
- Blackboard pattern (shared knowledge)
- Forest Swarm (dynamic routing)
- Tool-Coordination Trade-off (>16 tools)
- Sequential Penalty (multi-agent vs sequential)
- Majority / similarity voting (orchestrator::patterns)

---

## 12. Prompting & ACI best practices (documented)

- XML tags in prompts (e.g. &lt;thinking&gt;, &lt;tool_call&gt;)
- Prescriptive prompts; effort control; soft tool language
- Context caching (prompt caching §20.2) for cost/latency
- “Done” and starting state in long-running task prompts

---

_This list is exhaustive for the current codebase. For checklist status (including not-yet-done items), see [FRAMEWORK-BUILD-CHECKLIST.md](FRAMEWORK-BUILD-CHECKLIST.md)._
