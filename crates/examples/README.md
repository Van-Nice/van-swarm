# VanSwarm examples

Runnable example agents and workflows. Run from the **workspace root**.

## Prerequisites

Set one of: `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`.

## Examples

| Binary | Description |
|--------|-------------|
| **hello_react** | Minimal ReAct agent with one tool (current time). Prompts the model to use the `time` tool. |

### Run

```bash
# Default prompt ("What time is it right now?")
cargo run -p vanswarm-examples --bin hello_react

# Custom prompt
cargo run -p vanswarm-examples --bin hello_react -- "What's the date and time in UTC?"
```

## Adding more examples

Add a new binary under `src/bin/<name>.rs` and depend on `vanswarm-core` (and optionally `vanswarm-mcp`, `vanswarm-orchestrator`, etc.). List it in this README.
