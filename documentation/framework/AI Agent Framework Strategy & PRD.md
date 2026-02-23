# **Technical Product Requirement Document and Commercialization Strategy for the Next-Generation Rust AI Agent Framework**

The transition of artificial intelligence from experimental chatbots to production-grade autonomous agents represents a shift in software engineering comparable to the move from static web pages to dynamic, distributed web applications. In this emerging landscape, the current reliance on Python-centric orchestration frameworks has created a significant "Unreliability Tax," where organizations must over-provision infrastructure and expend vast engineering resources to manage the inherent latencies, memory inefficiencies, and non-deterministic failures of interpreted runtimes.1 The proposed initiative seeks to disrupt this paradigm by delivering an open-source, Rust-based AI Agent Framework that marries the high-performance execution characteristics of the Rust language with a "Vercel-like" deployment experience and comprehensive, agent-native observability.

## **Technical Architecture of the Framework**

The framework is architected to address the performance ceiling and architectural fragility of existing Python-based solutions. By utilizing Rust's zero-cost abstractions, memory safety without a garbage collector, and the Tokio asynchronous runtime, the framework provides a foundation capable of supporting high-throughput, enterprise-level agentic workloads that are currently unsustainable in the Python ecosystem.1

### **Core Runtime and Zero-Overhead Execution**

The core runtime is designed to achieve a sub-5MB memory footprint and cold-start latencies of less than 10ms, facilitating the deployment of thousands of concurrent agent isolates on minimal hardware.5 This efficiency is achieved through the elimination of the Python interpreter and the use of static dispatch in Rust’s trait system, which ensures that resource allocation is determined at compile-time rather than runtime.3

#### **WebAssembly (WASM) Sandboxing for Tool Execution**

To enable secure and portable tool execution, the framework integrates a WebAssembly sandboxing layer utilizing the Wasmtime runtime.6 Unlike traditional frameworks that execute external tools in the same process or rely on expensive Docker containerization, this architecture leverages WASM to provide lightweight, high-speed isolation.8

| Performance Metric | Traditional Container (Docker) | WASM Sandbox (Wasmtime) | Native Rust Isolate |
| :---- | :---- | :---- | :---- |
| **Cold Start Latency** | 100ms \- 2,000ms | \< 1ms \- 100ms | \< 5ms |
| **Memory Overhead** | 128MB \- 512MB | 10MB \- 30MB | \< 5MB |
| **Security Model** | Kernel Namespaces / cgroups | Capability-based (WASI) | Process-level / Typed |
| **Execution Speed** | Native | Near-Native | Native |

WASM modules are compiled Ahead-of-Time (AOT) to eliminate Just-In-Time (JIT) translation delays during execution, ensuring that agent tools—ranging from data processing scripts to API connectors—can be invoked with sub-millisecond overhead.7 The use of the WebAssembly System Interface (WASI) provides a capability-based security model, allowing the framework to restrict a tool's access to specific filesystems or network sockets, thus mitigating the risks associated with executing non-deterministic agent outputs.6

### **Graph-Based Orchestration and State Management**

Traditional linear chains in agent frameworks suffer from a "Sequential Penalty," where complex tasks must be processed in a rigid, one-way flow, leading to increased latency and an inability to handle the cyclical reasoning required for self-correction.11 The proposed orchestration engine utilizes a directed graph structure, supporting both Directed Acyclic Graphs (DAGs) and cyclical workflows similar to LangGraph.11

The orchestration layer is built on the principles of "ready set" scheduling and deterministic concurrency, allowing independent nodes to execute in parallel while maintaining strict control over state transitions.1 By defining agent behavior as a state machine where nodes represent computational steps (handlers) and edges represent conditional logic, the framework enables sophisticated multi-actor patterns.12

#### **State Management Logic**

State in this framework is a shared container that flows through the graph, accumulating results and providing context for subsequent decision-making.11

![][image1]  
Where ![][image2] represents the graph state and ![][image3] is the execution result of a specific node. The framework ensures that state is persistent across node executions, allowing workflows to be paused, resumed, or recovered after infrastructure failures.11 This is implemented through a Task trait that defines how nodes interact with the global context and decide the NextAction (e.g., Continue, Pause, or WaitingForInput).14

### **Native Three-Tier Memory System**

The intelligence of an AI agent is fundamentally constrained by its memory architecture. Current systems often rely on a single vector database, which serves well for retrieval but fails to capture the nuanced temporal and procedural knowledge required for long-term autonomy.16 The framework implements a native "Three-Tier Memory" system, abstracting storage backends directly into the Rust type system.

| Memory Tier | Cognitive Function | Mutability | Storage Target |
| :---- | :---- | :---- | :---- |
| **Episodic** | Stores specific past events and raw interactions with temporal context. | Append-only | Redis Streams / Local Log |
| **Semantic** | Stores factual knowledge, conceptual patterns, and learned interpretations. | Governed | Vector DB (Qdrant/Milvus) |
| **Procedural** | Internalizes and automates repetitive tasks and "how-to" routines. | Mutable State | Managed Postgres / KV |

The **Episodic Memory** layer acts as a chronological audit trail, enabling "time-travel" queries to reconstruct an agent's knowledge at the moment a specific decision was made.16 **Semantic Memory** leverages Retrieval-Augmented Generation (RAG) to provide deep domain context, utilizing transactional semantics to ensure that multiple agents can update shared knowledge without causing data corruption.16 **Procedural Memory** manages the operative conditions of the system, such as active session states and account balances, requiring high data freshness to satisfy the "Decision Coherence Law".16

### **Protocol Integration: MCP and Redis Streams**

Interoperability is achieved through native support for the Model Context Protocol (MCP) and Redis Streams. MCP serves as a standardized interface for connecting agents to diverse data sources and tools without the need for bespoke integrations.18 The framework provides both client and server stubs, utilizing JSON-RPC over stdio, SSE, or WebSocket transports to facilitate universal tool discovery.20

For agent-to-agent communication, Redis Streams provide a sub-millisecond backbone for high-performance messaging.1 This allows for the orchestration of "Worker" agents by "Supervisor" agents in real-time, ensuring that message history and tool outputs are propagated across the distributed system with minimal overhead.1

## **The Platform: Managed Serverless Infrastructure**

The commercialization of the framework is centered on a managed infrastructure platform that follows the "Vercel Playbook," offering a frictionless transition from local development to global scale.

### **High-Density Multi-Tenant Runtime**

The platform's architecture deviates from traditional container-based serverless models by utilizing Rust-powered V8 isolates or WASM runtimes to achieve extreme density.24 By isolating execution contexts rather than entire operating system environments, the platform can run over 1,500 agent isolates on a standard 8GB VPS, significantly reducing the cost-per-agent for enterprise users.5

#### **"Deploy to Cloud" CLI**

The developer experience is prioritized through a CLI that removes the "Docker Hell" associated with modern cloud deployments.5 The command agent deploy automatically packages the Rust binary and its WASM tool dependencies, pushing them to a managed edge network. This process abstracts the complexity of Kubernetes manifests and registry management, allowing developers to focus purely on agent logic.26

### **Agentic APM: The "LangSmith" Killer**

Monitoring non-deterministic agent behavior requires a departure from standard observability metrics. The platform provides an "Agentic APM" dashboard that traces execution paths across the orchestration graph, logging "Reasoning Traces" alongside traditional outcomes.23

Requirements for the APM include:

* **Traceability of Reasoning**: Recording why an agent selected a specific tool or followed a certain path in the cyclical graph.23  
* **Cost-per-Step Analysis**: Mapping token consumption and infrastructure usage to individual nodes in the workflow to identify inefficient prompts or expensive model calls.23  
* **Time-to-First-Token (TTFT)**: Monitoring the perceived latency of interactive agents.31  
* **Context Utilization**: Tracking how much of the available context window is consumed across multi-turn interactions to prevent "context overflow" and quadratic cost growth.2

### **Durable Execution and Managed State**

To support long-running workflows that may require human intervention or involve slow API responses, the platform implements a "Durable Execution" engine inspired by Restate.15 This engine automatically persists the agent's progress journal to a managed backend (using a tiered storage model of memory, RocksDB, and S3), allowing agents to be suspended and resumed without losing context.15 This eliminates the need for developers to maintain their own persistent state layers for session management or multi-turn history.15

## **Commercialization and Go-To-Market Strategy**

The monetization strategy is designed to capture value from the "Unreliability Tax" and the "Friction of Production." As agents move from pilots to enterprise-grade systems, the costs of failure, latency, and infrastructure management become the primary drivers for platform adoption.2

### **Tiered Pricing Model**

Pricing is structured to align with the value delivered at different stages of the developer lifecycle, transitioning from ease-of-use to operational efficiency and security.

| Tier | Target | Pricing Model | Key Features |
| :---- | :---- | :---- | :---- |
| **Hobby** | Individual Developers | Free (with soft caps) | 1,000 execution minutes, community support, shared runtime. |
| **Pro** | Startups & Scale-ups | Usage-based (minutes \+ tokens) | Managed durable state, full APM dashboard, sub-10ms starts. |
| **Enterprise** | Large Organizations | Hybrid (Base \+ Performance) | VPC peering, RBAC, SSO, dedicated isolates, SLA guarantees. |

Value metrics are anchored on "Execution Minutes" and "Successful Outcomes," ensuring that customers only pay for productive work rather than failed iterations or idle resources.32

### **FinOps and Supervisor Agent Gating**

The platform introduces specialized FinOps features to help organizations manage the "Quadratic Token Growth" inherent in autonomous agents.2 "Supervisor Agents"—models that classify incoming queries and route them to the most cost-effective tier (e.g., using a small model for simple tasks and an expensive one for complex reasoning)—are feature-gated as a premium optimization capability.2 By providing these routing patterns as a platform feature, we allow enterprise users to tune their "flexible thinking budget" to balance accuracy and cost.2

## **Architecture Diagram Description**

The system is bifurcated into the local development environment (Framework) and the cloud-native execution environment (Platform).

1. **Rust Crate Structure (Framework Layer):**  
   * core-runtime: The Tokio-based asynchronous engine and Wasmtime sandbox manager.  
   * graph-engine: The workflow orchestrator handling DAGs, cycles, and state transitions.  
   * tier-memory: The trait-based abstraction layer for episodic, semantic, and procedural memory.  
   * protocol-bridge: Native implementations of MCP and Redis Streams for inter-agent communication.  
2. **Cloud Control Plane (Platform Layer):**  
   * **The Isolate Mesh**: A fleet of high-density nodes running Rust-native isolates.  
   * **Durable State Store**: A distributed journal system (Bifrost/RocksDB) for workflow persistence.  
   * **The APM Hub**: A centralized telemetry processor for tracing reasoning and analyzing costs.  
   * **The CLI Gateway**: The endpoint for the deploy command and configuration management.

## **Gap Analysis: Comparative Framework Evaluation**

The following table contextualizes the proposed solution against existing Python and Rust frameworks, highlighting the specific architectural gaps we address.

| Dimension | LangChain/LangGraph (Python) | Rig/ZeroClaw (Rust) | Proposed Solution |
| :---- | :---- | :---- | :---- |
| **Cold Start** | 200ms \- 2,000ms 6 | \~100ms 35 | \< 10ms 5 |
| **Memory/Agent** | 300MB+ 5 | \~50MB \- 100MB 3 | \< 5MB 5 |
| **Orchestration** | Complex Cyclical 12 | Modular/Niche 3 | High-Perf DAG & Cycles 1 |
| **Tool Isolation** | Process-based/Docker 8 | Native (Unsandboxed) 3 | WASM Sandbox 6 |
| **State Persistence** | Manual/External 12 | Local/Manual 36 | Managed Durable Execution 15 |
| **DX** | High (Python ecosystem) | Low (Steep curve) 3 | Vercel-like CLI 5 |

## **Feature Roadmap for v1.0 Launch**

The roadmap is prioritized to capture the performance-conscious enterprise market while building the foundation for the Vercel-like ecosystem.

### **Phase 1: High-Performance Foundations**

* **Must-Have**: The WorkflowGraph engine supporting cyclical state management and parallel node execution.1  
* **Must-Have**: Native implementation of the Model Context Protocol (MCP) for tool discovery.20  
* **Must-Have**: WASM tool sandboxing with AOT compilation to ensure secure, sub-100ms tool execution.6

### **Phase 2: Production Readiness**

* **Must-Have**: The "Deploy to Cloud" CLI command that bundles Rust agents and pushes them to managed isolates.26  
* **Must-Have**: Initial Agentic APM with reasoning trace visualization and cost attribution.23  
* **Must-Have**: Managed state persistence (Durable Execution) for basic multi-turn sessions.15

### **Phase 3: Enterprise Scale**

* **Should-Have**: Supervisor Agent feature-gating for cost-optimized routing between model tiers.2  
* **Should-Have**: Redis Streams integration for sub-millisecond multi-agent orchestration at scale.1  
* **Should-Have**: Advanced FinOps dashboard with budget alerts and token-caching analytics.2

## **Detailed Analysis of Causal Relationships and Future Outlook**

The strategic necessity of this framework is driven by the divergence of agentic software from traditional request-response patterns. As agents begin to operate continuously—taking irreversible actions in real-world environments such as approving transactions or updating customer records—the "Decision Coherence Law" dictates that infrastructure must provide a single, authoritative representation of reality.16 The reliance on caches or replicas with inherent lag, common in Python frameworks, is no longer acceptable in a system where an agent must act with millisecond precision.16

The "Unreliability Tax" is the single greatest barrier to the adoption of autonomous AI in the enterprise.2 By providing a Rust-native runtime that eliminates the non-deterministic overhead of the Python interpreter and garbage collector, the framework reduces the risk profile of AI deployments. The transition to WASM sandboxing further isolates failure domains, ensuring that a single faulty tool call cannot bring down the entire agentic system.6

Furthermore, the "Vercel Playbook" addresses the talent gap in the AI industry. While Rust offers unparalleled performance and safety, its steep learning curve has limited its adoption among AI/ML researchers.3 By abstracting the complexities of infrastructure, multi-tenancy, and state management into a high-level CLI and managed platform, we enable the broader population of developers to harness the power of Rust without mastering its low-level intricacies.5

In the future, the platform will evolve toward "Outcome-Based Orchestration," where the pricing and resource allocation of agents are automatically adjusted based on the success of their tasks. This "Self-Optimizing Agent Cloud" will leverage the integrated APM and Supervisor patterns to dynamically shift workloads to the most efficient models and hardware, ultimately delivering the highest ROI for autonomous AI at scale.32 This framework is not just a replacement for LangChain; it is the infrastructure for the next generation of digital workers.

#### **Works cited**

1. How We Built The First Open-Source Rust Core Agentic AI Framework, accessed February 22, 2026, [https://dev.to/yeahiasarker/how-we-built-the-first-open-source-rust-core-agentic-ai-framework-3kfc](https://dev.to/yeahiasarker/how-we-built-the-first-open-source-rust-core-agentic-ai-framework-3kfc)  
2. The Hidden Economics of AI Agents: Managing Token Costs and, accessed February 22, 2026, [https://online.stevens.edu/blog/hidden-economics-ai-agents-token-costs-latency/](https://online.stevens.edu/blog/hidden-economics-ai-agents-token-costs-latency/)  
3. Market Competition Dynamics of Eliza, GAME, Rig, and ZerePy, accessed February 22, 2026, [https://www.binance.com/en/square/post/18374340822258](https://www.binance.com/en/square/post/18374340822258)  
4. Building the Rig AI framework with Rust \- YouTube, accessed February 22, 2026, [https://www.youtube.com/watch?v=euq7dhs-nQU](https://www.youtube.com/watch?v=euq7dhs-nQU)  
5. ZeroClaw vs Everything Else: The Numbers Are Insane \- YouTube, accessed February 22, 2026, [https://www.youtube.com/watch?v=0CtcjeyVVPs](https://www.youtube.com/watch?v=0CtcjeyVVPs)  
6. WebAssembly Is Eating the Cloud: Why Devs Should Care \- DZone, accessed February 22, 2026, [https://dzone.com/articles/webassembly-is-eating-the-cloud-why-devs-should-ca](https://dzone.com/articles/webassembly-is-eating-the-cloud-why-devs-should-ca)  
7. Wasmtime at Scale: Isolation, Cold-Start and Tail-Latency Trade-offs, accessed February 22, 2026, [https://www.researchgate.net/publication/395678680\_Wasmtime\_at\_Scale\_Isolation\_Cold-Start\_and\_Tail-Latency\_Trade-offs\_of\_WebAssembly\_Microservices\_in\_Production](https://www.researchgate.net/publication/395678680_Wasmtime_at_Scale_Isolation_Cold-Start_and_Tail-Latency_Trade-offs_of_WebAssembly_Microservices_in_Production)  
8. restyler/awesome-sandbox: Awesome Code Sandboxing for AI, accessed February 22, 2026, [https://github.com/restyler/awesome-sandbox](https://github.com/restyler/awesome-sandbox)  
9. WASM Runtimes vs. Containers: Cold Start Delays (Part 1), accessed February 22, 2026, [https://levelup.gitconnected.com/wasm-runtimes-vs-containers-performance-evaluation-part-1-454cada7da0b](https://levelup.gitconnected.com/wasm-runtimes-vs-containers-performance-evaluation-part-1-454cada7da0b)  
10. The Five-Millisecond Cloud: Rust \+ WebAssembly Will (Sometimes, accessed February 22, 2026, [https://gothartech.com/en/insights/rust-wasm-containers-2025](https://gothartech.com/en/insights/rust-wasm-containers-2025)  
11. rrag\_graph \- Rust \- Docs.rs, accessed February 22, 2026, [https://docs.rs/rrag-graph](https://docs.rs/rrag-graph)  
12. Orchestrating Multi-Step Agents: Temporal/Dagster/LangGraph, accessed February 22, 2026, [https://kinde.com/learn/ai-for-software-engineering/ai-devops/orchestrating-multi-step-agents-temporal-dagster-langgraph-patterns-for-long-running-work/](https://kinde.com/learn/ai-for-software-engineering/ai-devops/orchestrating-multi-step-agents-temporal-dagster-langgraph-patterns-for-long-running-work/)  
13. AgentFlow-RS: LangGraph-Inspired AI Agent Framework for Rust, accessed February 22, 2026, [https://lib.rs/crates/agentflow-rs](https://lib.rs/crates/agentflow-rs)  
14. a-agmon/rs-graph-llm: High-performance framework for ... \- GitHub, accessed February 22, 2026, [https://github.com/a-agmon/rs-graph-llm](https://github.com/a-agmon/rs-graph-llm)  
15. Durable AI Loops: Fault Tolerance across Frameworks and without ..., accessed February 22, 2026, [https://www.restate.dev/blog/durable-ai-loops-fault-tolerance-across-frameworks-and-without-handcuffs](https://www.restate.dev/blog/durable-ai-loops-fault-tolerance-across-frameworks-and-without-handcuffs)  
16. AI Agent Memory Architecture: The Three Layers Production ..., accessed February 22, 2026, [https://tacnode.io/post/ai-agent-memory-architecture-explained](https://tacnode.io/post/ai-agent-memory-architecture-explained)  
17. Episodic Memory in AI Agents: Long-Term Context & Learning, accessed February 22, 2026, [https://www.centron.de/en/tutorial/episodic-memory-in-ai-agents-long-term-context-learning/](https://www.centron.de/en/tutorial/episodic-memory-in-ai-agents-long-term-context-learning/)  
18. I built a Rust implementation of Anthropic's Model Context Protocol, accessed February 22, 2026, [https://www.reddit.com/r/rust/comments/1ja1vjg/i\_built\_a\_rust\_implementation\_of\_anthropics\_model/](https://www.reddit.com/r/rust/comments/1ja1vjg/i_built_a_rust_implementation_of_anthropics_model/)  
19. Build an MCP server \- Model Context Protocol, accessed February 22, 2026, [https://modelcontextprotocol.io/docs/develop/build-server](https://modelcontextprotocol.io/docs/develop/build-server)  
20. modelcontextprotocol/rust-sdk: The official Rust SDK for the ... \- GitHub, accessed February 22, 2026, [https://github.com/modelcontextprotocol/rust-sdk](https://github.com/modelcontextprotocol/rust-sdk)  
21. model\_context\_protocol \- Rust \- Docs.rs, accessed February 22, 2026, [https://docs.rs/model-context-protocol](https://docs.rs/model-context-protocol)  
22. conikeec/mcpr: Model Context Protocol (MCP) implementation in Rust, accessed February 22, 2026, [https://github.com/conikeec/mcpr](https://github.com/conikeec/mcpr)  
23. How AI Analytics is Transforming Data Processing: Agents, Tokens ..., accessed February 22, 2026, [https://www.gravitee.io/blog/how-ai-analytics-is-transforming-data-processing-agents-tokens-and-beyond](https://www.gravitee.io/blog/how-ai-analytics-is-transforming-data-processing-agents-tokens-and-beyond)  
24. Building Modern Serverless Runtimes | Azion, accessed February 22, 2026, [https://www.azion.com/en/blog/building-modern-serverless-runtimes/](https://www.azion.com/en/blog/building-modern-serverless-runtimes/)  
25. Hydra: Virtualized Multi-Language Runtime for High-Density ... \- arXiv, accessed February 22, 2026, [https://arxiv.org/html/2212.10131v3](https://arxiv.org/html/2212.10131v3)  
26. Rust and WebAssembly Serverless functions in Vercel \- WasmEdge, accessed February 22, 2026, [https://wasmedge.org/docs/start/usage/serverless/vercel/](https://wasmedge.org/docs/start/usage/serverless/vercel/)  
27. Build Your Own Vercel-like Platform with Node js and AWS \- Medium, accessed February 22, 2026, [https://medium.com/@agrawaljash99/unleash-scalability-build-your-own-vercel-like-platform-with-node-js-and-aws-29ff4075d6bf](https://medium.com/@agrawaljash99/unleash-scalability-build-your-own-vercel-like-platform-with-node-js-and-aws-29ff4075d6bf)  
28. second-state/vercel-wasm-runtime: A template project for ... \- GitHub, accessed February 22, 2026, [https://github.com/second-state/vercel-wasm-runtime](https://github.com/second-state/vercel-wasm-runtime)  
29. TRAIL: Trace Reasoning and Agentic Issue Localization \- arXiv, accessed February 22, 2026, [https://arxiv.org/html/2505.08638v1](https://arxiv.org/html/2505.08638v1)  
30. Agentic Observability \- Making LLM Apps Debuggable, Trustworthy, accessed February 22, 2026, [https://www.youtube.com/watch?v=bcTR3lFvL00](https://www.youtube.com/watch?v=bcTR3lFvL00)  
31. AI Agent Performance Testing in the DevOps Pipeline: Orchestrating, accessed February 22, 2026, [https://devops.com/ai-agent-performance-testing-in-the-devops-pipeline-orchestrating-load-latency-and-token-level-monitoring/](https://devops.com/ai-agent-performance-testing-in-the-devops-pipeline-orchestrating-load-latency-and-token-level-monitoring/)  
32. Selling Intelligence: The 2026 Playbook For Pricing AI Agents, accessed February 22, 2026, [https://www.chargebee.com/blog/pricing-ai-agents-playbook/](https://www.chargebee.com/blog/pricing-ai-agents-playbook/)  
33. The Complete Guide to Agentic AI Pricing Models (Usage-Based, accessed February 22, 2026, [https://www.getmonetizely.com/articles/the-complete-guide-to-agentic-ai-pricing-models-usage-based-fixed-and-hybrid](https://www.getmonetizely.com/articles/the-complete-guide-to-agentic-ai-pricing-models-usage-based-fixed-and-hybrid)  
34. 8 AI Agent Pricing Models Explained \- Ema, accessed February 22, 2026, [https://www.ema.co/additional-blogs/addition-blogs/ai-agents-pricing-strategies-models-guide](https://www.ema.co/additional-blogs/addition-blogs/ai-agents-pricing-strategies-models-guide)  
35. SkillRuntime — WebAssembly in Rust // Lib.rs, accessed February 22, 2026, [https://lib.rs/crates/skill-runtime](https://lib.rs/crates/skill-runtime)  
36. Rust Agent Runtime Showdown: MicroClaw vs ZeroClaw vs Moltis ..., accessed February 22, 2026, [https://medium.com/@everettjf/rust-agent-runtime-showdown-microclaw-vs-zeroclaw-vs-moltis-df1ecb85c676](https://medium.com/@everettjf/rust-agent-runtime-showdown-microclaw-vs-zeroclaw-vs-moltis-df1ecb85c676)

[image1]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAmwAAAAiCAYAAADiWIUQAAADVklEQVR4Xu3dOYhsRRQG4FLBXREVEY3d10hR0EAM3BBMNFHBHfUpCC7gEohLoiguiRo8EBXEREQwckMDwTVwRdxAUDASF0xczuHey9Qc30x3v9cww+P74KerTjUz1RMd6t6+0xoAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABsWofVwiZ2eC0AAGyP1yNPRH6OXBLZb/Xyhnk68kOpfRXZvdSuasP+X22L7z9/fn7uG7va95GvI992te21S+S2WgQAWMTfZf5umW+UrW1onC7oavd248lxkRO6+aL7PzryUuTfUj+kzHfEs7UAALCI2qjkSdVm8E3k5VLLk7Dq9si+3XzR/V8XuaGt/jvs3Y2X4czIQbUIADCvbFR+ibwY2aOsbZTX2rCv9yPXd/XaXKYL21DP/V9R1ubx8fi6JfL4OL5vfF2WvCz6WC0CACzivMijbWh8zhlru64sr+nyyMG1uCS/tqHR6b1T5pN92rD/v9rK/ueVp2uT/Pw5/6CrLcubtQAAMI+TyjwblulE6+1+YRvyffn+I+rCkrxVC+GVMq/7zz1N++/va1vPsd34n8jnbeWkbZkWvbcOAKBd3Ib7xHrfteFk7ZE2nAhdvXr5f2Y1bHevk/WcHLm0FsOfZZ77362b52XHaf9ftNX7P7QbT/ILC738gkN+pgO72vGRh9twn1zmjsiJkdPG9b0iD43jdGXk7G4+ub8WAABm+ShyUxvu4Xou8km3dmo3vjVyV8np41o2N0eO42V6sm37kmy9hy33/1kbGqbc/3Q5NPf/5fSm0W9tePzH5M42NIA/Rfbv6rd042zW8ksJebk1L7tm05cnZedHDoi80IZHiTw4vn/r+Nqf2k3zU0oNAGCmqSk7I3JWv9CGU6RJfhFhz5LpVCsbqKPG8TK9VwujT2uhDc9kq182yP0/VWp5onhtqc2SzWo9mcsmb2rwfo9cNI7zb/LHOK4uqwUAgB2VJ1b50NpsztaSlyyzeckTpmV5JvJjW/v+uTx1y9OuWXL/edrV7//5bjyv/H03R+5pw8N482e+MdbSuW04DXxgnGfjmJdh8zEevQ/LHABgp3ZMW/1fCTa7a2oBAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACAndZ/O3N8ewIIXZAAAAAASUVORK5CYII=>

[image2]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAA0AAAAYCAYAAAAh8HdUAAABAklEQVR4Xu3SP0tCcRTG8ZOKFSW4Ce5NEoENEeHSFAWNtQXh1NDg4htwNIiIegUu7W3SFrSEtjRJtAYNjrlUfg/ncu/vHm1zCXzgw4XzR+VcRf5tFlBDFUtRrYzleMKljgdc4x4fOMYAhWAuThNPKAa1NfzgOajFqeAb675BHnHhixr9ll+s+obYTz3wRc2N2FILGdcLD5LKodiS+sQdTrEYDvnomRv4kmRZdZEL5qZmBfu4xEhscS81EWXDF6KciS3pM5WS2Muclm2xpU3fOMIbsr5BrvAuk9eUW7FPO3H1HQzFzj2RHs7xij7a6OBF/jiAZit65sX+2fpudpP2PLPLGJWFLMbXxFTsAAAAAElFTkSuQmCC>

[image3]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACsAAAAYCAYAAABjswTDAAACMElEQVR4Xu2WS6hOURTHF/KmpOtSuknyjoEMbpQBIjLwTknyyOPmkVKilPIsr9SdGGAir0wUY0UihCgTj8SAiRgQJY/f39rHOWd1h1/t1PerX985a+3va3/rrL33MWvS5P9mJr7G9/gBz9fTf3mAb/CF+dgTtWwGLuFn/IWjQq4H7sNbOLyeysNT3Im/revKHcFlMZiDsXgFB+FX/IT9aiPMbmNriGVhI3ak69Pm1d1Qpq0vPqrcZ+Uijk/Xk8wnq7YomIWdlfusPAn3N80nPCPd78clZTofRb9WWWw+2SKufm0p0/nYZGW/Fmireos/cAw+rKfzcRknxiDsNq/uXTwVclnohs/TZ0SP/Zv5hBeGnBiKW8335Akppt9Zh4dwHG7BFSkntKusxKO4oBLvg2vxMM6uxGuswmfYPSYS5/AnDg5xHdGPcSC24dkUP2g+djW+NF+gOqLFkHSt3aY33kjxAXgfp+F08+/V0Oz1HqDKSR2zmkBkCt4LMVXnFR7DbXjSvI3U5xovDphXTxMZmWIX8Lr5nq7KL01xFUTjhQ6l4ik1hKn4HUdgr5AruIPzQ+wLLjL/AwX6gzox2yuxhqJH/87KPu+Jc3GY+WJVG2gXUZU0dnsap+1vcroW6ln9xkfzVhKa/PJ/IxrELtyLO8wf6WjzLe6aeVtcxT3mLz9FJeeZn4KbzXt7ToqvwTO4Ho9beRA1FPVu/xDTwtHKFqpsRAs5Llah72l3adKkK/4A66pfXBwvtJoAAAAASUVORK5CYII=>