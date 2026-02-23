# WASM-to-MCP Bridge Workflow

Agents run in a **WASM sandbox** (Wasmtime) for security and sub-10ms cold starts. The **WASM-to-MCP bridge** lets sandboxed code call external tools via the Model Context Protocol without giving the guest host access.

## Problem: Sandbox vs MCP

MCP usually needs stdio or network; a default WASM sandbox has neither.

```mermaid
flowchart LR
    subgraph Guest["WASM Guest (Agent)"]
        A[Agent code]
    end

    subgraph Host["Host"]
        MCP[MCP Client]
        Tools[External tools]
    end

    A -.->|"Wanted: call tools"| MCP
    MCP --> Tools
    Note[Guest cannot open stdio/sockets]
```

## Solution: Capability Tunneling

The host defines **WIT interfaces** that the guest imports. The Wasmtime linker binds these imports to the host’s MCP client so tool calls are proxied, not given raw I/O.

```mermaid
sequenceDiagram
    participant Guest as WASM Guest
    participant Linker as Wasmtime Linker
    participant Host as Host MCP Client
    participant Tool as MCP Server / Tool

    Guest->>Linker: call_tool(name, params) [WIT import]
    Linker->>Host: Forward to host implementation
    Host->>Tool: JSON-RPC (stdio/SSE/WebSocket)
    Tool-->>Host: Result
    Host-->>Linker: Serialized result
    Linker-->>Guest: Return to guest
```

## wasmcp-Style Architecture

A chain of responsibility: the host handles MCP transport; one or more middleware components can handle or delegate tool calls.

```mermaid
flowchart TB
    subgraph Transport["Transport Layer"]
        HTTP[wasmtime-wasi-http]
        JSONRPC[JSON-RPC]
    end

    subgraph Host["Host"]
        Linker[Wasmtime Linker]
        MCPClient[MCP Client]
        WasiCtx[WasiCtxBuilder: minimal env]
    end

    subgraph Guest["Guest (WASM)"]
        Agent[Agent]
    end

    HTTP --> JSONRPC
    JSONRPC --> Linker
    Linker --> MCPClient
    Linker --> Agent
    WasiCtx --> Linker
```

## Component Roles

| Component | Role |
|-----------|------|
| **Wasmtime Linker** | Binds guest imports to host MCP functions |
| **WASI-Virt** | Virtualizes stdio/sockets so guest sees a bridge to MCP transport |
| **wasmcp** | Composes tool-calling components (reference pattern) |
| **jsonrpsee** | JSON-RPC (de)serialization on host |

## Strict Resource Limits

The guest gets only what it needs: no host filesystem or arbitrary network.

```mermaid
flowchart TB
    WasiCtx[WasiCtxBuilder]
    WasiCtx --> Null[Default: null/empty]
    WasiCtx --> Add[Explicit: add MCP interfaces only]
    Add --> Safe[Agent can call authorized tools only]
```

## Cold Start

Wasmtime can pre-initialize these interfaces quickly, keeping cold start under the &lt;10ms target while preserving a strong security boundary.

## References

- Technical Specification: WASM-to-MCP bridge, WASI-Virt, wasmcp, capability tunneling.
- PRD: WASM tool sandboxing, AOT, &lt;10ms tool execution.
