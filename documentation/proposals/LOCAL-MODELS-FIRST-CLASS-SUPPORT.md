# Proposal: First-class support for agents using local models

## Summary

Add **first-class support for local LLM backends**, with **LM Studio** as the default local provider:

1. Users can run VanSwarm agents **without any cloud API key**, using LM Studio at **http://127.0.0.1:1234** (OpenAI-compatible API at `/v1`).
2. **vanswarm-mcp-server** uses LM Studio as the **default** when no cloud API key is set (Anthropic / OpenAI / Gemini checked first).
3. The framework uses LM Studio’s **OpenAI-compatible** endpoint (`/v1/chat/completions`, tool_calls); the implementation is aware of the LM Studio API surface (native and OpenAI-compatible) for docs and future extensions.

This proposal does **not** require new agent patterns or memory backends; it is a **provider and configuration** change that fits the current architecture.

---

## Part 1 — Motivation and scope

### 1.1 Why local models?

- **Privacy and air-gap:** Sensitive or regulated workloads can run without sending prompts to third-party APIs.
- **Cost and latency:** No per-token billing; lower latency when the model runs on the same machine or LAN.
- **Development and CI:** Contributors and CI pipelines can run agents without API keys; local models are easier to mock or constrain.
- **Ecosystem fit:** Ollama and OpenAI-compatible local servers are widely used; first-class support improves discoverability and reduces friction.

### 1.2 What “first-class” means here

| Area | Today | After |
|------|--------|--------|
| **Providers** | OpenAI, Anthropic, Gemini only | + **LM Studio** (local) with default `http://127.0.0.1:1234/v1` |
| **MCP server** | Detects provider via cloud API keys only | When no cloud key is set, **defaults to LM Studio** at 127.0.0.1:1234 |
| **CLI `vanswarm new`** | `--provider anthropic \| openai \| gemini` | (Future) `--provider lm-studio` or `local` |
| **Defaults** | Cloud default models only | Local default model `local` (override via `LM_STUDIO_MODEL` or `RUSTMASTRA_MODEL`) |
| **Credentials** | API key required for all cloud providers | Local: no API key; optional `LM_STUDIO_BASE_URL` (default `http://127.0.0.1:1234/v1`) |
| **Docs and examples** | Cloud-only in quick start / MCP docs | Local option in quick start, env table, and MCP server docblock |

### 1.3 LM Studio API (reference)

LM Studio exposes multiple API styles. VanSwarm uses the **OpenAI-compatible** API so the rest of the framework stays unchanged.

| API style | Base path / endpoints | Use in VanSwarm |
|-----------|------------------------|------------------|
| **OpenAI-compatible** | Base URL `http://127.0.0.1:1234/v1`; `POST /v1/chat/completions` (chat, tool_calls) | ✅ Used by `LmStudioProvider` |
| **LM Studio API (native)** | `GET /api/v1/models`, `POST /api/v1/chat`, `POST /api/v1/models/load`, `POST /api/v1/models/download`, `GET /api/v1/models/download/status/:job_id` | Referenced for docs and future model discovery / load |

No code changes are required for the native endpoints unless we add features like listing or loading models via the LM Studio API.

### 1.4 Out of scope (for this proposal)

- Custom inference runtimes (e.g. direct llama.cpp bindings, WASI-NN) — future work.
- “Local” as a separate crate; the local provider lives in **vanswarm-core** next to existing providers, reusing the same trait and message types.
- Changing the ReAct loop, tool schema, or memory APIs.

---

## Part 2 — Current state and reuse

### 2.1 ModelProvider and messages

- **ModelProvider** in `crates/core/src/providers/mod.rs`: `complete(CompletionRequest) -> CompletionResponse`, `stream(...)`, `provider_name()`.
- **CompletionRequest / CompletionResponse** and **ContentBlock** (text, tool_use, tool_result) are provider-agnostic; each provider translates to/from its wire format.
- **OpenAiProvider** already supports **base_url** via `ProviderCredentials::with_base_url()`. So any **OpenAI-compatible** HTTP endpoint (e.g. LM Studio at `http://127.0.0.1:1234/v1`) can be used by constructing credentials with that base URL and using `OpenAiProvider` (or a thin wrapper for a distinct `provider_name()`).

**Implemented:** `LmStudioProvider` in `crates/core/src/providers/lm_studio.rs` wraps `OpenAiProvider` with default base URL `http://127.0.0.1:1234/v1`, no API key, and `provider_name() == "lm-studio"`. MCP server uses it as the default when no cloud API key is set.

### 2.2 MCP server provider detection

- **Implemented:** `detect_provider()` checks, in order, `ANTHROPIC_API_KEY` → `OPENAI_API_KEY` → `GEMINI_API_KEY`. If **none** are set, it falls back to **LM Studio**: `LmStudioProvider::from_env_or_default()` (default base URL `http://127.0.0.1:1234/v1`), default model `local` (overridable via `LM_STUDIO_MODEL` or `RUSTMASTRA_MODEL`). So local models are the default when no cloud keys are present.

### 2.3 CLI (`vanswarm new`)

- `crates/cli/src/main.rs`: `Provider` enum has `Anthropic`, `OpenAI`, `Gemini`; template code branches on provider.
- **Future:** Add `LmStudio` (or `Local`) to `Provider`; default model `local`; in templates, use `LmStudioProvider::from_env_or_default()` and in .env.example document `LM_STUDIO_BASE_URL` and `LM_STUDIO_MODEL` (no API key required).

---

## Part 3 — Design options

### 3.1 Option A: Reuse OpenAiProvider with env-driven base URL and dummy key

- **New:** In core, a constructor (e.g. `OpenAiProvider::for_ollama(base_url?)`) that builds `ProviderCredentials` with a placeholder key (Ollama typically ignores it) and the given base URL.
- **MCP server:** In `detect_provider()`, if `OLLAMA_BASE_URL` is set (or we use a default), call `OpenAiProvider::for_ollama(...)` and return it with default model `llama3.2` (or from `RUSTMASTRA_MODEL` / `OLLAMA_MODEL`).
- **CLI:** Add `Provider::Ollama`; in template, generate code that uses `OpenAiProvider::for_ollama(None)` (or from env) and model `llama3.2`.
- **Pros:** Minimal code; no new provider module; same wire format (OpenAI-compatible).  
- **Cons:** “Ollama” is really “OpenAI-compatible local”; provider_name remains “openai” unless we add a wrapper or a separate type alias.

### 3.2 Option B: Dedicated OllamaProvider (or LocalProvider) in core

- **New:** `crates/core/src/providers/ollama.rs` (or `local.rs`) with a struct that holds `base_url: String`, `default_model: String`, and uses the same HTTP client and request/response mapping as OpenAiProvider (Ollama is OpenAI-compatible).
- **Credentials:** No API key required; optional `OLLAMA_BASE_URL` and `OLLAMA_MODEL` (or `LOCAL_MODEL`).
- **MCP server:** Import `OllamaProvider`, add a branch in `detect_provider()`: if `OLLAMA_BASE_URL` is set or default endpoint is used, return `Arc::new(OllamaProvider::from_env_or_default())`.
- **CLI:** Add `Provider::Ollama`; templates generate `OllamaProvider::from_env_or_default()` and the chosen default model.
- **Pros:** Clear semantics, `provider_name() == "ollama"` in traces and framework_info; single place for local-specific defaults and behaviour.  
- **Cons:** Slight duplication of OpenAI-compatible request/response handling unless we extract a shared “OpenAI-compatible client” and have both OpenAiProvider and OllamaProvider use it.

### 3.3 Option C: Generic “OpenAI-compatible” provider with a name

- **New:** e.g. `OpenAiCompatibleProvider { base_url, api_key: Option<String>, provider_name: String }` so that one implementation can be used as “openai”, “ollama”, or “azure” by configuration.
- **Pros:** One implementation for all OpenAI-shaped APIs.  
- **Cons:** More generic config; naming and docs become a bit more abstract (“local” vs “ollama” vs “custom”).

**Recommendation:** **Option B** (dedicated OllamaProvider) for clear UX and traceability, with the implementation delegating to the same HTTP and wire logic as OpenAiProvider (either by sharing an internal client or by a small amount of duplicated code to avoid a large refactor). If we later add another local backend (e.g. llama.cpp server with the same API), we can add another small provider or unify under a “LocalProvider” that wraps base_url + model.

---

## Part 4 — Implemented and optional changes

### 4.1 vanswarm-core (implemented)

- **Added:** `crates/core/src/providers/lm_studio.rs`
  - `LmStudioProvider` wraps `OpenAiProvider` with default base URL `http://127.0.0.1:1234/v1` (LM Studio OpenAI-compatible API).
  - No API key required; optional env `LM_STUDIO_BASE_URL`.
  - `provider_name() == "lm-studio"`.
- **providers/mod.rs:** `pub mod lm_studio;` and `pub use lm_studio::LmStudioProvider`.
- **lib.rs:** Re-export `LmStudioProvider`.

### 4.2 vanswarm-mcp-server (implemented)

- **main.rs:**
  - `detect_provider()`: after Anthropic / OpenAI / Gemini, if none matched, returns `LmStudioProvider::from_env_or_default()` with default model `local` (overridable by `LM_STUDIO_MODEL` or `RUSTMASTRA_MODEL`).
  - Docblock updated: provider table includes LM Studio as default when no cloud key is set; `LM_STUDIO_BASE_URL` and `LM_STUDIO_MODEL` documented.
- **server.rs:** No change; `vanswarm_framework_info` shows `provider: lm-studio` when so configured.

### 4.3 CLI (vanswarm new) — future

- Add `LmStudio` to `Provider` enum and `FromStr` (“lm-studio” / “local”).
- `default_model()` for LmStudio: `"local"`.
- In templates, use `LmStudioProvider::from_env_or_default()` and document `LM_STUDIO_BASE_URL`, `LM_STUDIO_MODEL` in .env.example.

### 4.4 Documentation

- **Quick start / MCP:** Document that with no cloud API key, the MCP server uses LM Studio at http://127.0.0.1:1234; set `LM_STUDIO_BASE_URL` / `LM_STUDIO_MODEL` as needed.
- **PLATFORM-FEATURES.md:** Under “Model providers”, add LM Studio: default `http://127.0.0.1:1234/v1`, no API key, optional env vars.

### 4.5 Tests — future

- **Unit:** `LmStudioProvider` with a mock HTTP server returning OpenAI-shaped JSON; assert `complete()` / `stream()` and `provider_name() == "lm-studio"`.
- **Integration:** Optional: LM Studio running in CI for one ReAct step.

---

## Part 5 — Environment variables (summary)

| Variable | Purpose | Default |
|----------|---------|--------|
| `LM_STUDIO_BASE_URL` | Base URL for LM Studio OpenAI-compatible API | `http://127.0.0.1:1234/v1` |
| `LM_STUDIO_MODEL` | Default model name for LM Studio (loaded model id) | `local` |
| `RUSTMASTRA_MODEL` | Override default model for any provider (existing) | — |

(Exact names can be tuned; e.g. `LOCAL_MODEL` instead of `OLLAMA_MODEL` if we want a generic “local” name.)

---

## Part 6 — Risks and mitigations

- **LM Studio not running:** `run_agent` will fail at first request. Mitigation: document that LM Studio must be running at 127.0.0.1:1234 with a model loaded; optional health check and clear error message (“LM Studio not reachable at …”).
- **Model name:** Default model id is `local`; user can set `LM_STUDIO_MODEL` or `RUSTMASTRA_MODEL` to the model identifier shown in LM Studio.
- **Tool-call format:** LM Studio’s OpenAI-compatible API supports tool_calls; we use the same schema as OpenAI. If a specific model misbehaves, document known-good models.

---

## Part 7 — Future extensions

- **Other local backends:** Ollama, llama.cpp server, or other OpenAI-compatible endpoints could be added as separate providers (e.g. `OllamaProvider`) with their own default URLs.
- **LM Studio native API:** Use `GET /api/v1/models` to list models and `POST /api/v1/models/load` to load a model before running the agent (optional discovery/load step).
- **Local-first discovery:** MCP server could probe `http://127.0.0.1:1234` at startup and log “LM Studio detected” for better UX.
- **CLI `vanswarm run`:** If a future `vanswarm run` command exists, it could accept `--provider lm-studio` and use `LmStudioProvider`.

---

## Summary table

| Component | Status |
|-----------|--------|
| **core/providers** | ✅ `lm_studio.rs` — `LmStudioProvider` (default `http://127.0.0.1:1234/v1`), re-exported. |
| **mcp-server** | ✅ `detect_provider()` falls back to LM Studio when no cloud key; docblock updated. |
| **cli** | Future: `Provider::LmStudio`, template and .env.example. |
| **docs** | Quick start / MCP / PLATFORM-FEATURES: document LM Studio as default local. |
| **tests** | Future: unit test with mocked LM Studio HTTP. |

Local models (LM Studio at **http://127.0.0.1:1234**) are now the **default** when no cloud API key is set; the implementation is aligned with the LM Studio API (OpenAI-compatible and native endpoints referenced for docs and future work).
