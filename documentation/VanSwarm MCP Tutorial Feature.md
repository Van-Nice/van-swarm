# **Architecting Autonomous Developer Experience: Implementation of Protocol-Driven Knowledge Retrieval and Interactive Tutorials in Rust-Based Agent Frameworks**

The maturation of Large Language Models (LLMs) has shifted the paradigm of developer experience (DX) from passive documentation consumption to active, agent-mediated interaction. In this evolving landscape, the Model Context Protocol (MCP) has emerged as a critical infrastructure layer, standardizing how AI agents access tools, resources, and knowledge bases.1 Frameworks such as Mastra.ai have leveraged this protocol to build sophisticated documentation servers and tutorial features that integrate directly with modern Integrated Development Environments (IDEs) like Cursor, Windsurf, and Claude Desktop.4 For a Rust-native framework such as VanSwarm, replicating and exceeding these capabilities requires a deep integration of existing primitives—such as the three-tier memory system, WASM-based runtime, and graph-based orchestration—into a protocol-compliant MCP server architecture.6 This report provides an exhaustive technical analysis of the Mastra.ai implementation and a detailed blueprint for implementing a comparable, high-performance tutorial and documentation system within the VanSwarm ecosystem.

## **The Evolution of the Model Context Protocol as a Knowledge Bus**

The Model Context Protocol (MCP) acts as a universal translator between AI models and external data sources, effectively functioning as a "USB-C for AI context".2 Prior to its adoption, agents were forced to rely on fragmented retrieval methods such as Retrieval-Augmented Generation (RAG) pipelines or massive, context-draining files like llms.txt.4 Mastra.ai’s strategic use of MCP addresses these limitations by providing a structured, on-demand query interface that preserves the LLM’s context window for actual reasoning tasks rather than documentation storage.4

### **Comparative Analysis of Framework Capabilities and Implementation Targets**

| Capability | Mastra.ai Implementation | VanSwarm Target Implementation | Causal Impact on DX |
| :---- | :---- | :---- | :---- |
| **Documentation Access** | @mastra/mcp-docs-server via Stdio/SSE.4 | vanswarm-mcp-server resource handlers.8 | Reduces search latency for API signatures and patterns.4 |
| **Agentic Scaffolding** | create mastra CLI and example agents.9 | vanswarm-macros with MCP prompt templates.11 | Ensures adherence to framework-specific architectural patterns.12 |
| **Stateful Tutorials** | Guided walkthroughs in local playground.13 | Orchestrator-driven workflows with durable execution.15 | Allows for persistent, multi-session learning paths.16 |
| **Code Validation** | TypeScript-based tool validation.10 | WASM-sandboxed execution and compilation checks.17 | Provides immediate, secure feedback on generated code.18 |
| **Knowledge Retrieval** | Searchable knowledge base including blog/changelogs.4 | Semantic search over architecture docs via semantic memory.19 | Synchronizes agent knowledge with the latest framework updates.4 |

The primary mechanism for this transition is the MCPServer class, which in Mastra.ai exposes existing tools, agents, and workflows as callable endpoints for external clients.1 This architecture allows an external IDE agent to "ask" the framework-expert agent for implementation details, effectively creating an "Agent-to-Agent" (A2A) protocol for technical support.20

## **Dissecting the Mastra.ai Documentation and Tutorial Ecosystem**

Mastra.ai’s tutorial features are not contained in a single monolithic tool but are distributed across its CLI, its documentation server, and its agent orchestration layer.4 The framework utilizes the @mastra/mcp-docs-server to provide AI assistants with real-time access to documentation, code examples, and package changelogs.4 This server is particularly effective because it allows agents to query specific contextual information rather than overwhelming them with a full knowledge dump.4

### **The Role of Resources in Knowledge Delivery**

In the MCP specification, resources represent "file-like" data that can be read by clients.23 Mastra.ai uses resources to expose its markdown-based documentation as URIs.26 When an agent in Cursor or Windsurf needs to understand how to "add evaluations to an agent," it can list available resources and read the specific document related to evaluators.4

For the VanSwarm platform, which already possesses an exhaustive list of 130+ integration tests and a 220+ item checklist, these documents serve as the primary resource set \[User Query\]. By exposing the documentation/architecture/\*.md and documentation/guides/\*.md directories through the vanswarm-mcp-server resource handler, the framework provides the same level of granular accessibility.19

### **Prompts as Structured Scaffolding Templates**

Prompts in MCP are pre-defined templates that guide the model through specific tasks.23 Mastra.ai implements "smart prompts" for tasks like creating daily note templates or summarizing content.26 In a development context, these prompts function as parameterized scaffolding tools.28

A developer might invoke a prompt such as new-agent-scaffold with parameters for the agent's name and its intended model.28 The MCP server then returns a structured set of instructions and boilerplate code to the host application, which the host's agent then implements in the user's project.24 This pattern is critical for VanSwarm’s vanswarm-macros crate, as it can provide templates for \#\[tool\] and \#\[workflow\] implementations that are guaranteed to follow the framework’s type-safety requirements.11

## **The Rust Ecosystem for Model Context Protocol Development**

Building a high-performance MCP server in Rust involves a choice between several foundational crates, each offering different trade-offs in terms of ergonomics, performance, and specification compliance.7

### **Evaluating Rust MCP Implementations**

| Crate | Spec Version | Transport Support | Key Feature |
| :---- | :---- | :---- | :---- |
| **rmcp** | 2024-11-05+ | Stdio, SSE, IO.6 | Official SDK with procedural macro support.11 |
| **mcpx** | 2025-03-26 | Stdio, WebSocket, SSE.7 | Comprehensive specification support and smart connection management.7 |
| **ultrafast-mcp** | 2025-06-18 | Streamable HTTP, Stdio.30 | 10-100x performance gains over TypeScript implementations; zero-copy deserialization.15 |
| **mcpr** | 2024-11-05 | Stdio, SSE.31 | Project generation utilities and modular architecture.31 |

The rmcp crate, as the official SDK, provides the most seamless integration with VanSwarm's existing macro-heavy architecture.6 Its \#\[tool\] macro allows for the automatic generation of JSON schemas from Rust structs, which is essential for the "Tool Call" mechanism in MCP.11 However, for the documentation server's performance needs, the ultrafast-mcp crate offers significant advantages, particularly in the rapid retrieval of large markdown resources.15

### **Implementing the JSON-RPC 2.0 Layer in Rust**

The core of MCP is JSON-RPC 2.0 communication over standard input/output (stdio) or Server-Sent Events (SSE).27 A robust Rust implementation must handle the request-response cycle asynchronously to maintain the "cold start \<10 ms" target established by the VanSwarm project.8

Rust

// Basic request dispatch logic using rmcp primitives \[8, 35\]  
async fn dispatch(&self, request: \&JsonRpcRequest) \-\> Result\<JsonRpcResponse\> {  
    match request.method.as\_str() {  
        "initialize" \=\> self.handle\_initialize(request).await,  
        "tools/list" \=\> self.handle\_tools\_list().await,  
        "resources/list" \=\> self.handle\_resources\_list().await,  
        "prompts/get" \=\> self.handle\_prompts\_get(request).await,  
        \_ \=\> Err(JsonRpcError::method\_not\_found()),  
    }  
}

The vanswarm-mcp-server binary currently exposes tools like vanswarm\_run\_agent and vanswarm\_memory\_search \[User Query\]. Extending this binary to support documentation retrieval requires the implementation of the resources/list and resources/read methods, which can be hooked directly into the ReadFileTool and LocalToolRegistry logic.8

## **Engineering the VanSwarm Documentation Server**

The VanSwarm documentation server should function as an authoritative expert on the framework's own code and architecture. By integrating the existing vanswarm-memory system, the documentation server can go beyond simple file serving to provide semantic search and contextual relevance.36

### **Semantic Retrieval and Contextual Prompts**

The vanswarm-memory crate includes episodic, mid-term, and semantic memory tiers \[User Query\]. The semantic memory tier, utilizing cosine similarity, is ideally suited for finding the most relevant documentation snippets for a given user query.38 When a developer asks, "How do I handle cycles in the orchestrator?", the MCP server can:

1. Convert the query into an embedding using the SemanticMemory tier.9  
2. Search the knowledge base for documentation related to ExecutionGraph and GraphBuilder.39  
3. Inject the most relevant markdown blocks into the agent’s context as a resource read.4

This process mirrors Mastra.ai’s "context optimization" strategy, where only the required information is provided to the agent, leaving the remainder of the context window for the user’s code.4

### **Mapping VanSwarm Primitives to MCP Types**

| VanSwarm Primitive | MCP Entity | Implementation Mechanism |
| :---- | :---- | :---- |
| AgentConfig & ReActAgent | ask\_agent Tool | Automatically wraps the agent's run\_agent method in an MCP tool definition.20 |
| GraphBuilder Workflows | run\_workflow Tool | Maps workflow inputSchema to MCP tool arguments.40 |
| ReadFileTool & documentation/ | Resource | Exposes project files via URI schemes like vanswarm://docs/.25 |
| Evaluators & Scorers | Validation Tool | Provides a tool to "score" a generated implementation against criteria.42 |

The vanswarm-macros crate can be extended to support these mappings. For instance, a \#\[mcp\_prompt\] macro could be introduced to declare prompt templates directly in the Rust code, allowing for compile-time validation of the template variables.11

## **Interactive Tutorials: The Path to Autonomous Skill Acquisition**

Mastra.ai’s "tutorial" feature is designed to be beginner-friendly, providing a "Playground" for rapid prototyping.9 For the VanSwarm framework, the goal is to create a "hands-on game development tutorial" or similar guided experience that teaches the nuances of Rust-based AI engineering.18

### **Leveraging WASM for Secure Interactive Learning**

The vanswarm-runtime crate, which provides a WASM sandbox using Wasmtime, is a unique asset for interactive tutorials.44 Unlike JavaScript frameworks that struggle with secure local execution, VanSwarm can allow an agent to:

1. **Generate a Tool**: The tutorial agent suggests a Rust implementation for a custom tool (e.g., a "MathTool").  
2. **Compile to WASM**: The MCP server invokes a compilation tool that produces a .wasm binary.  
3. **Execute and Validate**: The tutorial tool loads the binary into the vanswarm-runtime sandbox and executes it against a test suite.17  
4. **Feedback Loop**: The agent receives the execution results and refined logs, allowing it to "teach" the user how to fix compilation or logic errors.4

This "sandbox-validate-feedback" loop is a third-order insight into the potential of MCP servers: they move from being passive knowledge providers to active execution environments for educational validation.17

### **Implementing the Tutorial State Machine**

Interactive tutorials require a way to track the user’s progress. This can be implemented using the vanswarm-orchestrator as a state-machine based engine for the tutorial steps.10

Rust

// Conceptual tutorial workflow using vanswarm-orchestrator \[User Query\]  
let mut builder \= GraphBuilder::new();  
builder.add\_node("lesson\_1\_core\_concepts", Lesson1Task);  
builder.edge("lesson\_1\_core\_concepts", "lesson\_2\_tool\_creation", Predicate::Completed);  
builder.add\_node("lesson\_2\_tool\_creation", Lesson2Task);  
//... build the full learning path

By exposing this workflow through the MCP server, the IDE agent can guide the user through a deterministic set of tasks, pausing for user input or code implementation before resuming the execution.14

## **Advanced Protocol Features: Sequential Thinking and Long-Running Tasks**

One of the most powerful implementations in the Rust MCP ecosystem is the "Sequential Thinking" server, which provides a structured approach to problem-solving.15 This pattern is directly applicable to a framework tutorial system.

### **The Impact of Sequential Thinking on Developer Support**

Sequential Thinking allows the agent to "think out loud" in a controlled way, breaking complex technical puzzles into discrete steps.15 In the context of a VanSwarm tutorial, this means the agent can:

* **Initialize a Thinking Session**: Breakdown a complex task like "Implementing a Multi-Agent Swarm with Dynamic Routing."  
* **Verify Hypotheses**: Generate potential solutions for the routing logic and verify them against the framework's architecture resources.15  
* **Preserve Context**: Maintain the reasoning chain across multiple user prompts, ensuring that the tutorial doesn't lose track of previous explanations.15

Benchmarks show that Rust-based implementations of Sequential Thinking can process thoughts in \~0.1ms, compared to 1-5ms in TypeScript.15 This performance advantage ensures that the "interactive" part of the tutorial remains fluid and responsive even for deep architectural discussions.

### **Task Management and SEP-1686 Compliance**

The rmcp crate implements the task lifecycle from SEP-1686, allowing for the enqueuing and polling of long-running operations.32 This is critical for tutorial steps that might involve:

1. **Downloading large model weights**.  
2. **Running intensive benchmarks** using the vanswarm-core evaluators.  
3. **Indexing large codebases** into the semantic memory tier.32

By using tasks/get and tasks/result, the MCP client can wait for these operations to complete without blocking the main interaction thread, providing a modern, asynchronous DX.32

## **Metric-Driven Improvement of Tutorial Content**

The VanSwarm framework places a heavy emphasis on metrics, specifically the Success weighted by Path Length (SPL) formula used in its evaluators \[User Query\]. This formula can be applied to the tutorials themselves to evaluate their effectiveness.

### **Applying SPL to Developer Education**

![][image1]  
In this educational context:

* ![][image2] is a binary indicator of whether a user (or the user's agent) successfully implemented a framework feature.  
* ![][image3] is the optimal number of steps or tool calls required to complete the tutorial lesson.  
* ![][image4] is the actual number of steps taken by the user.

By tracking these metrics within the vanswarm-mcp-server using the RunMetrics and RunTrace systems, framework maintainers can identify "bottlenecks" in the documentation where users are taking suboptimal paths.15 This provides a causal link between the quality of the MCP documentation server and the proficiency of the developers using the framework.

## **Security Considerations for a Local Documentation Server**

Exposing a local project’s filesystem and an execution sandbox to an LLM via MCP introduces significant security risks.49 Mastra.ai handles this through structured prompting and local development boundaries.10 VanSwarm’s ReadFileTool already rejects path traversal, but an MCP-native implementation requires additional layers of protection \[User Query\].

### **Security Implementation Matrix**

| Risk Factor | VanSwarm Mitigation Strategy | Protocol Requirement |
| :---- | :---- | :---- |
| **Path Traversal** | Root-path anchoring and sanitization in vanswarm-core \[User Query\]. | Servers MUST validate all tool inputs.49 |
| **Command Injection** | Restricted tool list via FilteredToolExecutor \[User Query\]. | Sanitize system commands and validate parameters.52 |
| **Unauthorized Action** | Human-in-the-loop (HITL) requirements in the orchestrator \[User Query\]. | Present confirmation prompts for sensitive operations.49 |
| **Data Exfiltration** | Guardrails for redacting sensitive keywords in vanswarm-core \[User Query\]. | Audit tool usage and implement rate limiting.52 |

The vanswarm-macros crate plays a role here by allowing developers to mark certain tools as destructiveHint: true or readOnlyHint: true.52 These hints are passed through the MCP server to the IDE, which can then provide a visual confirmation dialog to the user before the tool is executed.49

## **Future Outlook: Autonomous Framework Maintenance and Growth**

The integration of MCP documentation servers is merely the first step toward fully autonomous development environments. Future iterations of the VanSwarm platform can extend the tutorial server to include "self-healing" and "self-expanding" capabilities.4

### **GitHub Issue Integration and Automatic PR Scaffolding**

Mastra.ai has indicated plans to expand its docs server to allow agents to query GitHub issues and prefill issue templates.4 For VanSwarm, this could be realized by:

1. **Monitoring Failures**: When a TrajectoryScorer identifies a failing agent path, the framework can automatically search the documentation and GitHub issues for similar failure patterns.4  
2. **Scaffolding Fixes**: Using an MCP prompt, the agent can generate a proposed fix and a corresponding integration test.4  
3. **Autonomous Submission**: The agent can use a GitHub MCP server to open a Pull Request, effectively allowing the framework to improve itself based on real-world usage data.4

This vision represents the pinnacle of agentic DX: a framework that not only teaches you how to use it but actively works with you to fix its bugs and expand its features.4

## **Technical Recommendations for the VanSwarm Project**

Based on the research into Mastra.ai and the Rust MCP ecosystem, the following technical actions are recommended for the VanSwarm development team:

* **Primary Crate Selection**: Standardize the vanswarm-mcp-server on the rmcp crate for its macro support, but implement a custom high-performance resource loader inspired by ultrafast-mcp for the documentation layer.6  
* **Resource Mapping**: Explicitly map the documentation/ and examples/ directories to a vanswarm:// URI scheme within the MCP server.25  
* **Prompt Scaffolding**: Create a library of prompts that cover the 12 domains listed in the platform feature list, from core-runtime to swarm-multi-agent.28  
* **Validation Tools**: Expose the vanswarm-runtime compilation and execution logic as an MCP tool to allow for real-time validation of agent-generated code.17  
* **Metric Integration**: Connect the RunMetrics tracker to the MCP server to provide the LLM with feedback on the efficiency of its implementation strategies.15

By implementing these features, VanSwarm will not only match the tutorial capabilities of Mastra.ai but will set a new standard for high-performance, type-safe AI engineering environments in the Rust ecosystem. The result is a framework that serves as both a powerful engine for production agents and a sophisticated mentor for the developers who build them.

#### **Works cited**

1. MCP Overview | MCP | Mastra Docs, accessed February 24, 2026, [https://mastra.ai/docs/mcp/overview](https://mastra.ai/docs/mcp/overview)  
2. Why We're All-In on MCP \- Mastra Blog, accessed February 24, 2026, [https://mastra.ai/blog/mastra-mcp](https://mastra.ai/blog/mastra-mcp)  
3. modelcontextprotocol/typescript-sdk \- GitHub, accessed February 24, 2026, [https://github.com/modelcontextprotocol/typescript-sdk](https://github.com/modelcontextprotocol/typescript-sdk)  
4. Introducing Mastra MCP Documentation Server \- Mastra Blog, accessed February 24, 2026, [https://mastra.ai/blog/introducing-mastra-mcp](https://mastra.ai/blog/introducing-mastra-mcp)  
5. Mastra Docs Server | Build with AI | Mastra Docs, accessed February 24, 2026, [https://mastra.ai/docs/build-with-ai/mcp-docs-server](https://mastra.ai/docs/build-with-ai/mcp-docs-server)  
6. The official Rust SDK for the Model Context Protocol \- GitHub, accessed February 24, 2026, [https://github.com/modelcontextprotocol/rust-sdk](https://github.com/modelcontextprotocol/rust-sdk)  
7. MCPX: Model Context Protocol SDK for Rust \- Lib.rs, accessed February 24, 2026, [https://lib.rs/crates/mcpx](https://lib.rs/crates/mcpx)  
8. How to Build an MCP Server in Rust \- OneUptime, accessed February 24, 2026, [https://oneuptime.com/blog/post/2026-01-07-rust-mcp-server/view](https://oneuptime.com/blog/post/2026-01-07-rust-mcp-server/view)  
9. Quickstart | Mastra Docs, accessed February 24, 2026, [https://mastra.ai/guides/getting-started/quickstart](https://mastra.ai/guides/getting-started/quickstart)  
10. Build your first agent in 5 minutes with Mastra \- DEV Community, accessed February 24, 2026, [https://dev.to/mastra\_ai/build-your-first-agent-in-5-minutes-with-mastra-2ah3](https://dev.to/mastra_ai/build-your-first-agent-in-5-minutes-with-mastra-2ah3)  
11. A Coder's Guide to the Official Rust MCP Toolkit ( rmcp ) \- HackMD, accessed February 24, 2026, [https://hackmd.io/@Hamze/S1tlKZP0kx](https://hackmd.io/@Hamze/S1tlKZP0kx)  
12. Node/TypeScript MCP Server Implementation Guide \- GitHub, accessed February 24, 2026, [https://github.com/anthropics/skills/blob/main/skills/mcp-builder/reference/node\_mcp\_server.md](https://github.com/anthropics/skills/blob/main/skills/mcp-builder/reference/node_mcp_server.md)  
13. Mastra: The TypeScript AI Framework, accessed February 24, 2026, [https://mastra.ai/](https://mastra.ai/)  
14. Show HN: Mastra – Open-source JS agent framework, by the, accessed February 24, 2026, [https://news.ycombinator.com/item?id=43103073](https://news.ycombinator.com/item?id=43103073)  
15. techgopal/ultrafast-mcp-sequential-thinking: Rust-based ... \- GitHub, accessed February 24, 2026, [https://github.com/techgopal/ultrafast-mcp-sequential-thinking](https://github.com/techgopal/ultrafast-mcp-sequential-thinking)  
16. KODEGEN.ᴀɪ | MCP Servers \- LobeHub, accessed February 24, 2026, [https://lobehub.com/mcp/cyrup-ai-kodegen](https://lobehub.com/mcp/cyrup-ai-kodegen)  
17. MCP-UI MCP Server: The Definitive Guide for AI Engineers, accessed February 24, 2026, [https://skywork.ai/skypage/en/MCP-UI-MCP-Server-The-Definitive-Guide-for-AI-Engineers/1972134266675625984](https://skywork.ai/skypage/en/MCP-UI-MCP-Server-The-Definitive-Guide-for-AI-Engineers/1972134266675625984)  
18. SQLite MCP Server: 73 AI-Native Database Tools | Adamic, accessed February 24, 2026, [https://adamic.tech/articles/sqlite-mcp-server](https://adamic.tech/articles/sqlite-mcp-server)  
19. MCPX: Model Context Protocol SDK for Rust \- Crates.io, accessed February 24, 2026, [https://crates.io/crates/mcpx](https://crates.io/crates/mcpx)  
20. llms.txt \- Mastra, accessed February 24, 2026, [https://mastra.ai/reference/tools/mcp-server/llms.txt](https://mastra.ai/reference/tools/mcp-server/llms.txt)  
21. Accelerating LLM-Powered Apps with MCP and A2A Protocols, accessed February 24, 2026, [https://medium.com/@roberto.g.infante/accelerating-llm-powered-apps-with-mcp-and-a2a-protocols-73d388fb4338](https://medium.com/@roberto.g.infante/accelerating-llm-powered-apps-with-mcp-and-a2a-protocols-73d388fb4338)  
22. Mastra MCP documentation server \- YouTube, accessed February 24, 2026, [https://www.youtube.com/watch?v=vciV57lF0og](https://www.youtube.com/watch?v=vciV57lF0og)  
23. Understanding MCP servers \- Model Context Protocol, accessed February 24, 2026, [https://modelcontextprotocol.io/docs/learn/server-concepts](https://modelcontextprotocol.io/docs/learn/server-concepts)  
24. Building your first MCP server: How to extend AI tools with custom, accessed February 24, 2026, [https://github.blog/ai-and-ml/github-copilot/building-your-first-mcp-server-how-to-extend-ai-tools-with-custom-capabilities/](https://github.blog/ai-and-ml/github-copilot/building-your-first-mcp-server-how-to-extend-ai-tools-with-custom-capabilities/)  
25. Resources \- Model Context Protocol, accessed February 24, 2026, [https://modelcontextprotocol.io/specification/2025-06-18/server/resources](https://modelcontextprotocol.io/specification/2025-06-18/server/resources)  
26. Guide: Building a Notes MCP Server | Mastra Docs, accessed February 24, 2026, [https://mastra.ai/guides/guide/notes-mcp-server](https://mastra.ai/guides/guide/notes-mcp-server)  
27. Build Your First MCP Server with Plain and TypeScript | Medium, accessed February 24, 2026, [https://thecraftman.medium.com/build-your-first-mcp-server-with-plain-and-typescript-6dd13494b95e](https://thecraftman.medium.com/build-your-first-mcp-server-with-plain-and-typescript-6dd13494b95e)  
28. gpaul-mcp/MCP\_prompt\_localDev \- GitHub, accessed February 24, 2026, [https://github.com/gpaul-mcp/mcp\_prompt\_localdev](https://github.com/gpaul-mcp/mcp_prompt_localdev)  
29. sparesparrow/mcp-prompts: Model Context Protocol server ... \- GitHub, accessed February 24, 2026, [https://github.com/sparesparrow/mcp-prompts](https://github.com/sparesparrow/mcp-prompts)  
30. ultrafast\_mcp \- Rust \- Docs.rs, accessed February 24, 2026, [https://docs.rs/ultrafast-mcp](https://docs.rs/ultrafast-mcp)  
31. conikeec/mcpr: Model Context Protocol (MCP) implementation in Rust, accessed February 24, 2026, [https://github.com/conikeec/mcpr](https://github.com/conikeec/mcpr)  
32. rmcp \- Rust \- Docs.rs, accessed February 24, 2026, [https://docs.rs/rmcp](https://docs.rs/rmcp)  
33. ultrafast-mcp \- Lib.rs, accessed February 24, 2026, [https://lib.rs/crates/ultrafast-mcp](https://lib.rs/crates/ultrafast-mcp)  
34. MCP in Rust: A Practical Guide using \- HackMD, accessed February 24, 2026, [https://hackmd.io/@Hamze/SytKkZP01l](https://hackmd.io/@Hamze/SytKkZP01l)  
35. Building a Personal Assistant with Mastra and MCP, accessed February 24, 2026, [https://mastra.ai/blog/personal-assistant](https://mastra.ai/blog/personal-assistant)  
36. Vertex AI Memory Bank MCP Server | M... · LobeHub, accessed February 24, 2026, [https://lobehub.com/mcp/yourusername-vertex-ai-memory-bank-mcp](https://lobehub.com/mcp/yourusername-vertex-ai-memory-bank-mcp)  
37. Vibeframe: Add UIs directly in the IDE for your MCP Servers \- Reddit, accessed February 24, 2026, [https://www.reddit.com/r/mcp/comments/1k24a7k/vibeframe\_add\_uis\_directly\_in\_the\_ide\_for\_your/](https://www.reddit.com/r/mcp/comments/1k24a7k/vibeframe_add_uis_directly_in_the_ide_for_your/)  
38. Shaping your AI prompts: using an agent to reduce MCP overhead, accessed February 24, 2026, [https://www.mux.com/blog/mux-mastra-local-mcp](https://www.mux.com/blog/mux-mastra-local-mcp)  
39. Using Tools | Agents | Mastra Docs, accessed February 24, 2026, [https://mastra.ai/docs/agents/using-tools](https://mastra.ai/docs/agents/using-tools)  
40. Reference: MCPServer | Tools & MCP | Mastra Docs, accessed February 24, 2026, [https://mastra.ai/reference/tools/mcp-server](https://mastra.ai/reference/tools/mcp-server)  
41. Your first project \- IDE \- Docs \- Kiro, accessed February 24, 2026, [https://kiro.dev/docs/getting-started/first-project/](https://kiro.dev/docs/getting-started/first-project/)  
42. toolsdk-ai/toolsdk-mcp-registry: ToolSDK.ai's Awesome ... \- GitHub, accessed February 24, 2026, [https://github.com/toolsdk-ai/toolsdk-mcp-registry](https://github.com/toolsdk-ai/toolsdk-mcp-registry)  
43. Build an MCP Server with Mastra \- YouTube, accessed February 24, 2026, [https://www.youtube.com/watch?v=415Qzt5\_0SY](https://www.youtube.com/watch?v=415Qzt5_0SY)  
44. Build a Full-Stack Stock Portfolio Agent with Mastra and AG-UI | Blog, accessed February 24, 2026, [https://www.copilotkit.ai/blog/build-a-full-stack-stock-portfolio-agent-with-mastra-and-ag-ui](https://www.copilotkit.ai/blog/build-a-full-stack-stock-portfolio-agent-with-mastra-and-ag-ui)  
45. From the team behind Gatsby, Mastra is a framework for building AI, accessed February 24, 2026, [https://github.com/mastra-ai/mastra](https://github.com/mastra-ai/mastra)  
46. modelcontextprotocol/servers: Model Context Protocol ... \- GitHub, accessed February 24, 2026, [https://github.com/modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers)  
47. mcp\_rust\_examples \- crates.io: Rust Package Registry, accessed February 24, 2026, [https://crates.io/crates/mcp\_rust\_examples](https://crates.io/crates/mcp_rust_examples)  
48. Tools \- What is the Model Context Protocol (MCP)?, accessed February 24, 2026, [https://modelcontextprotocol.io/specification/draft/server/tools](https://modelcontextprotocol.io/specification/draft/server/tools)  
49. Tools \- Model Context Protocol, accessed February 24, 2026, [https://modelcontextprotocol.io/specification/2025-06-18/server/tools](https://modelcontextprotocol.io/specification/2025-06-18/server/tools)  
50. Dangers of AI prompts: Palo Alto Networks CEO shares why even your wife shouldn't know what you are chatting with ChatGPT or Gemini, accessed February 24, 2026, [https://m.economictimes.com/news/new-updates/dangers-of-ai-prompts-palo-alto-networks-ceo-shares-why-even-your-wife-shouldnt-know-what-you-are-chatting-with-chatgpt-or-gemini/articleshow/128605409.cms](https://m.economictimes.com/news/new-updates/dangers-of-ai-prompts-palo-alto-networks-ceo-shares-why-even-your-wife-shouldnt-know-what-you-are-chatting-with-chatgpt-or-gemini/articleshow/128605409.cms)  
51. Tools \- Model Context Protocol, accessed February 24, 2026, [https://modelcontextprotocol.io/legacy/concepts/tools](https://modelcontextprotocol.io/legacy/concepts/tools)  
52. Tools \- Model Context Protocol （MCP）, accessed February 24, 2026, [https://modelcontextprotocol.info/docs/concepts/tools/](https://modelcontextprotocol.info/docs/concepts/tools/)  
53. rmcp-openapi \- crates.io: Rust Package Registry, accessed February 24, 2026, [https://crates.io/crates/rmcp-openapi](https://crates.io/crates/rmcp-openapi)  
54. Introducing Mastra Docs Chatbot: Your AI Documentation Assistant, accessed February 24, 2026, [https://mastra.ai/blog/introducing-docs-chatbot](https://mastra.ai/blog/introducing-docs-chatbot)

[image1]: <data:image/png;base64,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>

[image2]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABIAAAAXCAYAAAAGAx/kAAAA70lEQVR4XuWQPQ4BQRiGvwQtohAnUDgABVFpqTUOoCAhKnEBEaIkGgdwAtxBo6LXKEi0/t7Jzu7Ofvu/JU/yJLvPfDPZHaK/JA+XsK60gfLsSwK+4QomYRV+4Ag+lDlfxKYKj6T1IY9urEnb4ITo4msDIYa9DgqMftCUL4RlQuZhugvLRAi6ZD/sZJmIQI3c7+3GgyANxzxKOuR8kCPicns8SnbwyVqDvRu84J1HECfta1JKO8MmnCvNQAzv4RUWZGvJXtaHQAbG4AWWlG6gb87BGTzAvrlsI/CdedGGW5jlC2HZkPYHR74QhSIPP8wXW1Iz2Noo8qsAAAAASUVORK5CYII=>

[image3]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACkAAAAYCAYAAABnRtT+AAABl0lEQVR4Xu2WSytFURTHl0dRIhkwIXNfwcRjoCSFiZShgUwMDRiaMpFMPEoiH4KiPG55jSQjMfDIAAPy/q/WPt19/+6je29Xe3B/9au11zrnnrUf53RFihQJmyX4DH88n+CMf1EoRA0GS4lYg0dcCIkxsSa7uRASdxL4VivBn0dFGzzgZEhkOo8dnMiBKk4Qh7CGkz4Pkn6r9ZuZDz3wlJPENCeYdOdxErZzMktOYB8ns6FMrMEzLoAmSd78NVyBr25cCTfhGpx38QRskfgCqBvuep9muAUXueAzK/YDPNMFl+dt8psegXvwPEktVcxcwjr4xQVlFb6LFb8lccY6/hRbqcboBrHVnvPG4/DexUNw36tFjfXK34ky+tIMcjJX9MH+G/gitprKBexycanEmzyG/S5ORbqVzpoPWO/iakncIn2QNqfswFYvr+j51HuUK9jp4jaxLddarcvlRbnYEdGDvkw1beYGPop9ciL007INB7zcrtjKR7zBdW9cEIZhjJMZ4EkWlAZ4C6dgBdVSMSr217DIv/AL3QxjY1k+puAAAAAASUVORK5CYII=>

[image4]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAADEAAAAYCAYAAABTPxXiAAABu0lEQVR4Xu2WyytEURzHf0JWpDxKUcqGsmNpMWSBlKQsbS1YsLHyF3iulBULW2UnWQhJUoREFqTknWfkkdf355zT/PzMjKlZjFP3U5/Oud/fnTtz7jnn3iEKCAiIxRi8h5/COzgkT/IFNwBvSSEzgDVd8IkOMoNo0AWfOCfPlxLj/X5geAArOvSJv/ZDjQ6SwK0ONJcUeynxOyOZ8JOzRYeaWPuhF1br8L+RSmYAm7oAiuj34DLgNRyHeyJfgJNwEB7AdVFjJuAqPFT5AFyCWzBH1ZhpeAXTdEEyTOaHNqt81OYbIsuymWPZtl0ic3Vu3Rfzciy1/UxYZfv7sNH2+fxa23fkwxIyL+AmVfuG78wrfIcfFF5SLB+/wSdY6D4AzuAJXIQXME/UHHrmym02Cx9gu80jzXI04j0vLvhifGei0Ulm+iU98EhlTB/c1mEE6unnsk0YnoUKcdxvW565VjJ3us5mp7Ytgze2z+TCNhii8HJksm3OTyK+nuOZzKzNiSxhHuE83BEZLxXetLwHXshs3nRRH4G7ZP7uV4p8Cs6QecSHRM4bucD2u8lcWy5rLyjWgY8c6yDgP/MFJ/Bp7DVWWqQAAAAASUVORK5CYII=>