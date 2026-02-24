# MCP crate (rustmastra-mcp)

Model Context Protocol client and server: connect agents to external MCP servers (e.g. filesystem, fetch) and expose RustMastra tools as an MCP server.

## Components

```mermaid
flowchart TB
    subgraph client_side["Client side"]
        McpClient[McpClient]
        McpToolExecutor[McpToolExecutor]
        Transport[Transport]
    end

    subgraph server_side["Server side"]
        McpServer[McpServer]
        ToolExecutor[ToolExecutor]
    end

    subgraph transports["Transports"]
        Stdio[StdioTransport]
        Http[HttpTransport]
        Channel[ChannelTransport]
    end

    McpClient --> Transport
    Transport --> Stdio
    Transport --> Http
    Transport --> Channel

    McpToolExecutor --> McpClient
    McpToolExecutor ..> ToolExecutor

    McpServer --> ToolExecutor
    McpServer --> Channel
```

## McpClient

```mermaid
flowchart LR
    A[McpClient::stdio cmd args]
    B[McpClient::http endpoint]
    C[McpClient::channel transport]
    D[initialize]
    E[list_tools / list_resources]
    F[call_tool / read_resource]

    A --> D
    B --> D
    C --> D
    D --> E
    E --> F
```

- **Constructors**: `stdio(command, args)` — spawn subprocess; `http(endpoint)` — HTTP; `channel(ChannelTransport)` — in-memory for tests.
- **Protocol**: Call `initialize()` before any tools/resources; then `list_tools()`, `call_tool(name, arguments)`, and optional resource APIs.

## McpToolExecutor (bridge to ReActAgent)

```mermaid
sequenceDiagram
    participant Agent as ReActAgent
    participant Exec as McpToolExecutor
    participant Client as McpClient
    participant Server as MCP Server

    Agent->>Exec: tool_definitions()
    Exec->>Exec: return cached list (from refresh_tools)
    Agent->>Exec: execute(name, id, args)
    Exec->>Client: call_tool(name, args)
    Client->>Server: JSON-RPC
    Server-->>Client: result
    Client-->>Exec: CallToolResult
    Exec-->>Agent: ContentBlock (tool_result / tool_error)
```

- **McpToolExecutor** implements core’s **ToolExecutor**: `tool_definitions()` returns cached MCP tool schemas; `execute()` forwards to `McpClient::call_tool()` and maps result to `ContentBlock`.
- **refresh_tools()** must be called after `client.initialize()` so the agent sees the server’s tools.

## McpServer (expose tools as MCP)

```mermaid
flowchart TB
    LocalRegistry[LocalToolRegistry / ToolExecutor]
    McpServer[McpServer]
    handle_request[handle_request]
    serve_stdio[serve_stdio]
    serve_channel[serve_channel]

    LocalRegistry --> McpServer
    McpServer --> handle_request
    McpServer --> serve_stdio
    McpServer --> serve_channel
```

- **McpServer::new(name, version, executor)** — any `Arc<dyn ToolExecutor>` (e.g. `LocalToolRegistry`).
- **serve_stdio()** — blocks on stdin/stdout (IDE / Claude Desktop).
- **serve_channel()** — returns a transport and handle for in-process tests.
- **add_resource()** — register static resources (name, content provider).

## Protocol and transport

```mermaid
flowchart LR
    jsonrpc[JSON-RPC]
    protocol[protocol types]
    transport[Transport enum]

    jsonrpc --> protocol
    protocol --> InitializeResult
    protocol --> ListToolsResult
    protocol --> CallToolResult
    transport --> StdioTransport
    transport --> HttpTransport
    transport --> ChannelTransport
```

- **protocol** — MCP types: `InitializeResult`, `ListToolsResult`, `McpTool`, `CallToolResult`, resources, etc.
- **jsonrpc** — parse/serialize JSON-RPC requests and responses.
- **transport** — abstraction over stdio, HTTP, or in-memory channel so client and server can be tested without subprocesses.
