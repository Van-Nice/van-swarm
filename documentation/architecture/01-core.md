# Core crate (rustmastra-core)

The core crate provides traits, configuration, messages, model providers, the ReAct agent loop, and log-centric durable execution.

## Module layout

```mermaid
flowchart LR
    subgraph core["rustmastra-core"]
        config[config]
        error[error]
        message[message]
        traits[traits]
        providers[providers]
        react[react]
        durable[durable]
    end

    react --> config
    react --> message
    react --> traits
    react --> providers
    durable --> traits
    traits --> message
    providers --> config
    providers --> message
```

## Trait hierarchy

```mermaid
flowchart TB
    Runnable[Runnable]
    Agent[Agent]
    Workflow[Workflow]
    Tool[Tool]
    ToolExecutor[ToolExecutor]

    Agent --> Runnable
    Workflow --> Runnable
    ToolExecutor --> Tool

    Runnable --> step["step() / run()"]
    Agent --> AgentContext[AgentContext]
    ToolExecutor --> tool_definitions["tool_definitions()"]
    ToolExecutor --> execute["execute()"]
```

| Trait | Purpose | Used by |
|-------|---------|--------|
| `Runnable` | Base for runnable components | `Agent`, `Workflow` |
| `Agent` | Probabilistic, model-driven; maintains message history and calls LLM each step | `ReActAgent` |
| `Workflow` | Deterministic; steps are explicit | Orchestrator / durable workflows |
| `Tool` | Single tool: `definition()` + `execute(arguments)` | Boxed in `LocalToolRegistry` |
| `ToolExecutor` | Registry: `tool_definitions()` + `execute(name, id, args)` | `ReActAgent`, `McpServer` |

## ReAct loop

The ReAct pattern: **Thought → Action (tool call) → Observation → repeat** until final answer or iteration cap.

```mermaid
sequenceDiagram
    participant User
    participant run_agent
    participant ReActAgent
    participant ModelProvider
    participant ToolExecutor

    User->>run_agent: user_input
    run_agent->>ReActAgent: step(ctx)
    ReActAgent->>ModelProvider: complete(request with tools)
    ModelProvider-->>ReActAgent: response (ToolUse or EndTurn)
    alt ToolUse
        ReActAgent-->>run_agent: AgentAction::CallTools
        run_agent->>ToolExecutor: execute(tool_name, args)
        ToolExecutor-->>run_agent: ContentBlock (result/error)
        run_agent->>run_agent: append to ctx.messages, loop
    else EndTurn / StopSequence
        ReActAgent-->>run_agent: AgentAction::FinalAnswer
        run_agent-->>User: content
    end
```

```mermaid
flowchart LR
    A[Create AgentContext] --> B[agent.step]
    B --> C{StopReason?}
    C -->|ToolUse| D[Dispatch tools]
    D --> E[Append results to messages]
    E --> B
    C -->|EndTurn / MaxTokens| F[Return FinalAnswer]
```

- **ReActAgent** owns: `AgentConfig`, `Arc<dyn ModelProvider>`, `Arc<dyn ToolExecutor>`.
- **run_agent(agent, user_input)** creates context, loops on `step()`, dispatches tool calls, returns final text.

## Config and messages

```mermaid
classDiagram
    class AgentConfig {
        +name: String
        +model: ModelConfig
        +system_prompt: Option~String~
        +max_iterations: usize
        +enable_chain_of_thought: bool
    }
    class ModelConfig {
        +model_id: String
        +temperature: Option~f32~
        +max_tokens: Option~u32~
    }
    class Message {
        +role: Role
        +content: Vec~ContentBlock~
    }
    class CompletionRequest {
        +model_id
        +messages
        +tools
    }
    class CompletionResponse {
        +message
        +stop_reason
        +usage
    }

    AgentConfig --> ModelConfig
    CompletionRequest --> Message
```

- **AgentConfig** — name, model config, system prompt, iteration cap, chain-of-thought flag.
- **Message** — role (System/User/Assistant) + content blocks (text, tool_use, tool_result).
- **CompletionRequest / CompletionResponse** — what providers consume and return; support streaming.

## Model providers

```mermaid
flowchart LR
    ModelProvider["ModelProvider trait"]
    OpenAI[OpenAiProvider]
    Anthropic[AnthropicProvider]
    Gemini[GeminiProvider]

    ModelProvider <|.. OpenAI
    ModelProvider <|.. Anthropic
    ModelProvider <|.. Gemini
```

- **ModelProvider**: `complete(Request) -> Result<CompletionResponse>`, optional streaming.
- Implementations: **OpenAI**, **Anthropic**, **Gemini**; credentials via env / `ProviderCredentials`.

## Durable execution (log-centric replay)

Durable workflows use a **journal**: every non-deterministic operation is logged; on restart the workflow is re-run from the start and the journal replays cached results.

```mermaid
flowchart TB
    subgraph live["Live path"]
        A[ctx.call_tool / sleep / run_once]
        B{Journal has entry for seq?}
        C[Execute side effect]
        D[Append entry to journal]
        E[Return result]
    end
    subgraph replay["Replay path"]
        F[Same call]
        G{Journal has entry?}
        H[Return cached result]
    end

    A --> B
    B -->|No| C
    C --> D
    D --> E
    B -->|Yes| H

    F --> G
    G -->|Yes| H
```

```mermaid
classDiagram
    class JournalBackend {
        <<async trait>>
        +get(workflow_id, seq)
        +put(workflow_id, entry)
        +load_all(workflow_id)
        +clear(workflow_id)
    }
    class JournalEntry {
        +seq: u64
        +kind: JournalKind
        +result: Value
        +recorded_at
        +duration_ms
    }
    class JournalKind {
        ToolCall
        Sleep
        Timestamp
        Custom
    }
    class DurableContext {
        +call_tool(name, args)
        +sleep(duration)
        +timestamp()
        +run_once(label, op)
        +resume()
    }
    class InMemoryJournal
    class FileJournal

    JournalBackend <|.. InMemoryJournal
    JournalBackend <|.. FileJournal
    DurableContext --> JournalBackend
    JournalEntry --> JournalKind
```

- **JournalBackend** — storage for entries (in-memory for tests, file NDJSON for local dev).
- **JournalEntry** — seq, kind (ToolCall / Sleep / Timestamp / Custom), result, timestamps.
- **DurableContext** — built over a backend; on each `call_tool` / `sleep` / `run_once` checks journal first (replay) or runs and appends (live). `resume()` used for recovery.

## Error type

- **FrameworkError** — unified error enum (provider, tool execution, serialization, durable, WASM, etc.).
- **Result&lt;T&gt;** = `Result<T, FrameworkError>`.
