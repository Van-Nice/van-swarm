# Observability & Agentic APM (§13, §13.11)

Agentic APM differs from traditional APM because success depends on **path efficiency** and **token cost**, not only on request latency or error rate.

## Agentic APM vs traditional APM

| Concern | Traditional APM | Agentic APM |
|--------|------------------|-------------|
| **Path efficiency** | Not applicable (single request/response). | Reward short successful paths (SPL); penalize unnecessary tool calls and detours. |
| **Token cost** | Often not attributed per logical step. | Map token consumption to each step and run; optimize tier routing and context size. |
| **Success** | HTTP 200, no crash. | Task-level success (scorers, SPL) and convergence (TQGR). |
| **Traces** | Spans for RPC/DB. | Thought, tool call, observation, duration; why the agent chose a tool or path. |

Use **RunMetrics** (iterations, tool_call_count), **SPL**, and **scorers** to measure path efficiency and token cost in addition to latency and errors.
