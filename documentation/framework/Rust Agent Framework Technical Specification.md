# **Architectural Specification for High-Performance Rust AI Agent Runtimes: Addressing the Persistence, Orchestration, and Execution Gaps**

The evolution of agentic workflows from simple linear scripts to complex, multi-turn interactions requires a fundamental shift in systems architecture. In the context of building a high-performance open-source Rust agent framework, the primary objectives remain centered on low-latency execution—specifically cold starts under 10 milliseconds—secure sandboxing via WebAssembly (WASM), and the ability to maintain durable state across distributed environments. However, achieving these goals in the Rust ecosystem introduces significant low-level engineering hurdles, ranging from the non-serializable nature of asynchronous futures to the complexities of self-referential graph structures under the ownership model. This report provides an exhaustive technical investigation into six critical architectural gaps, identifying the specific crates, design patterns, and mathematical frameworks necessary to build a production-grade agent runtime that rivals existing paradigms like LangGraph while leveraging the unique safety and performance guarantees of the Rust language.

## **The Durable State and Resume Problem in Asynchronous Rust**

The most significant barrier to long-running, distributed agent workflows is the "Durable Execution" problem. In a standard Rust application, asynchronous tasks are represented by Future objects, which the compiler transforms into complex, anonymous state machines. These state machines encapsulate local variables, the instruction pointer, and the current progress of the task. Crucially, these structures are not naturally serializable to a database or a byte stream, meaning that if a process crashes or needs to be migrated to another machine, the current execution state is lost.1 For agents tasked with workflows that may span days—such as a research agent waiting for a human approval or a recurring data analysis task—the ability to pause and resume execution is non-negotiable.

### **Replay vs. Snapshot Persistence Strategies**

The architectural choice for persistence typically bifurcates into two methodologies: the "Replay" approach and the "Snapshot" approach. The Replay approach, as implemented in the Temporal.io Rust SDK and Restate, focuses on event sourcing.4 Instead of saving the memory state of the task, the system logs every non-deterministic input and side effect—such as timestamps, random numbers, and tool call results—into a durable event history.6 When a workflow needs to resume, the runtime simply re-executes the code from the beginning. During this re-execution, whenever the code reaches a point where an external side effect occurred, the SDK intercepts the call and injects the recorded result from the history rather than performing the action again.8

The Snapshot approach, utilized by platforms like Golem Cloud and Flawless, relies on the ability of the execution environment—usually a WASM runtime—to serialize its entire linear memory and stack.10 This allows the system to "freeze" an execution at any instruction point and save it as a binary blob. Recovery involves loading this blob into a new instance and continuing from the exact same instruction pointer.12

| Persistence Metric | Replay Approach (Temporal/Restate) | Snapshot Approach (Golem/Flawless) |
| :---- | :---- | :---- |
| **State Portability** | High; code can be re-run on different architectures as long as the binary is available.9 | Low; snapshots are often architecture-specific or tied to a specific runtime version.12 |
| **Developer UX** | Requires explicit wrapping of side effects in specialized APIs.13 | Transparent; runs standard code without modification.11 |
| **Storage Overhead** | Low; only logs inputs and results of side effects.5 | High; requires saving the entire memory state of the virtual machine.10 |
| **Recovery Latency** | Proportional to the number of steps in the workflow history.7 | Near-zero; the state is loaded directly into memory.11 |
| **Code Evolution** | Difficult; changing code structure can break replay determinism.6 | Moderate; snapshots are tied to a specific version of the WASM binary.17 |

For a lightweight agent runtime aiming for sub-10ms cold starts and high density (thousands of agents per host), the **Replay approach** is generally more viable. The Snapshot approach, while elegant, introduces massive storage overhead that scales with the number of agents, potentially leading to I/O bottlenecks when resuming large swarms of agents simultaneously. Furthermore, the Replay approach leverages the "deterministic execution" guarantees of the Rust compiler, where the state of a function is entirely determined by its inputs and the sequence of its asynchronous points.2

### **Recommended Implementation Strategy: Log-Centric Durable RPC**

The recommended architecture follows the **Durable RPC (Restate)** pattern rather than the heavy-weight event-sourcing model of Temporal. Restate's triad of low latency, low cost, and fast durability through quorum replication provides a modern blueprint for Rust-based agents.5 The implementation should focus on a bidirectional connection between the agent handler and a durable log.

1. **Crate Selection**: Utilize tokio for the asynchronous runtime and serde for serializing input/output of tool calls. For the underlying storage of execution history, an embedded replicated log like Bifrost (conceptually similar to RocksDB with a WAL) should be used.1  
2. **Deterministic Wrapping**: The runtime must provide a "Context" object that intercepts all non-deterministic operations. Instead of using std::time, the developer uses ctx.sleep() or ctx.call\_tool(). These methods check the internal journal to see if a result is already present before proceeding.6  
3. **Future Interception**: To avoid forcing developers to write manual state machines, use a procedural macro (e.g., \#\[workflow\]) that transforms the async function. This macro can inject "yield" points and log entries at every .await call, effectively creating a "virtualized execution" environment where progress is persisted atomically with the completion of each asynchronous step.9

This design solves the "Resume Problem" by ensuring that even if the underlying tokio task is dropped, the sequence of events leading to the current state is preserved in a durable log, allowing a new task to catch up to the previous state in milliseconds.

## **Graph Orchestration: Navigating Cycles and Parallelism in the Rust Ownership Model**

Designing an orchestration engine for agentic loops requires a graph structure that supports cycles—essential for "Evaluator-Optimizer" patterns where an agent refines its output based on feedback.20 However, Rust's ownership model famously struggles with self-referential or cyclical structures, as traditional pointer-based graphs lead to memory leaks (if using Rc/Arc) or lifetime entanglements that the borrow checker cannot resolve.20

### **Arena Allocation vs. Actor Model**

The technical investigation compares two primary paradigms for graph orchestration: the **Arena Allocation** model and the **Actor Model**.

In the Arena model, all nodes and edges are stored in a centralized, flat data structure (an "arena"). Instead of using pointers, nodes are referenced by stable indices (e.g., NodeIndex or a generational ID). The petgraph crate is the industry standard for this approach, providing robust support for directed graphs, cycles, and standard algorithms.20 The slotmap crate enhances this by providing generational indexing, which prevents the "ABA problem" where a reference to a deleted node accidentally points to a new node that occupied the same memory slot.25

The Actor Model, conversely, treats each agent node as an independent tokio::task. These actors communicate via channels (mpsc, oneshot, or broadcast). In this paradigm, the graph is not a data structure but a dynamic network of communicating processes.1

| Orchestration Metric | Arena Allocation (petgraph/slotmap) | Actor Model (tokio/channels) |
| :---- | :---- | :---- |
| **Ownership** | Single owner (the Arena); easy to pass around Send+Sync handles.23 | Decentralized; each actor owns its state, leading to complex global management.1 |
| **Cycles** | Natively supported via indices.20 | Implicitly supported but can lead to deadlocks or infinite message loops without careful routing. |
| **Parallelism** | Requires a separate executor to traverse the graph and spawn tasks.26 | Native; every actor runs concurrently by default.1 |
| **State Inspection** | Global visibility; easy to snapshot the entire graph state.23 | Difficult; requires querying each actor individually for its current state. |
| **Performance** | High cache locality; low overhead for graph traversals.25 | Higher overhead due to message passing and task switching.1 |

### **The Alignment Principle and Parallel Execution**

The "Alignment Principle" in agentic systems dictates that multi-agent coordination dramatically improves performance for parallelizable tasks but can degrade it for sequential ones due to communication overhead.21 A centralized orchestrator in an Arena model achieved an 80.9% improvement on financial reasoning tasks compared to independent agents, as the orchestrator acts as a "validation bottleneck," catching errors before they propagate.21

### **Recommended Pattern: Generational Index Arena with Directed Execution**

The most ergonomic and performant solution for a Rust agent framework is the **Generational Arena** implemented via the slotmap and petgraph crates.

1. **Crate Selection**: Use slotmap::DenseSlotMap to store node data and petgraph::stable\_graph::StableGraph for topology management. This combination ensures that node indices remain stable across removals, which is critical for long-running workflows where nodes might be added or removed dynamically.23  
2. **Implementation Strategy**: Define the graph as a SlotMap\<NodeKey, AgentNode\>. Each AgentNode contains the logic for that step. The orchestrator performs a topological walk of the graph. When multiple nodes have their dependencies satisfied, the orchestrator spawns them in parallel using tokio::spawn or rayon for CPU-heavy planning.23  
3. **Parallelism Constraints**: To satisfy the Parallel Execution requirement, the orchestrator should maintain a "Ready Queue" of nodes. This aligns with the "Centralized" model identified in research as the most effective balance between success rate and error containment.21

This approach allows for cyclical "agent loops" while maintaining a clean ownership model that the Rust compiler can verify, and it provides the necessary hooks for the durable execution layer described in the previous section.

## **The WASM-to-MCP Bridge: Proxying Capabilities into the Sandbox**

To maintain security and sub-10ms cold starts, agents must be sandboxed. WebAssembly, specifically the Wasmtime runtime, is the preferred environment for this isolation. However, agents need to interact with external tools through the Model Context Protocol (MCP), a JSON-RPC based protocol that traditionally requires access to standard input/output (stdio) or network sockets—capabilities that are strictly denied in a default WASM sandbox.34

### **Host Capabilities and WASI-Virt**

The WASM Component Model (WASI Preview 2\) introduces a refined way to handle "Host Capabilities." Instead of giving the guest agent raw access to the system, the host defines specific interfaces that the guest can import.37 To support MCP without breaking the sandbox, the framework must implement a proxy layer that bridges the guest's WASI calls to the host's MCP client.

A key technology here is **WASI-Virt**, a tool for virtualizing WASI components. It allows the host to encapsulate a component and provide it with a virtualized file system, environment variables, and, most importantly, virtualized stdio or sockets.39

### **Implementation Strategy: The wasmcp Architectural Pattern**

The **wasmcp** framework provides a reference for building polyglot MCP servers using WASM components. It uses a "Chain of Responsibility" pattern where multiple middleware components handle specific tool calls and delegate others downstream.35

1. **Transport Layer Integration**: The host application implements the MCP transport (either stdio or Streamable HTTP). It uses wasmtime-wasi-http to handle incoming JSON-RPC requests.35  
2. **Capability Tunneling**: When a sandboxed agent attempts to call a tool, it invokes a function defined in a WIT (Wasm Interface Type) file. The host's Wasmtime Linker redirects this call to the host's own MCP client, which then communicates with the external tool.38  
3. **Strict Resource Limits**: Use WasiCtxBuilder to configure "null" or "empty" environments by default, explicitly adding only the necessary MCP interfaces. This ensures the agent can speak to authorized tools but cannot access the host file system or unapproved network hosts.38

| Component | Role in MCP Bridge |
| :---- | :---- |
| **Wasmtime Linker** | Binds guest imports to host-provided MCP functions.38 |
| **WASI-Virt** | Intercepts stdio/socket calls to provide a virtualized bridge to the MCP transport.39 |
| **wasmcp** | Composes multiple tool-calling components into a single standalone binary.35 |
| **jsonrpsee** | Handles the JSON-RPC serialization/deserialization on the host side.43 |

This architectural pattern allows agents to remain "component-model-native" while benefiting from the extensive tool ecosystem of the Model Context Protocol. The cold start remains low because Wasmtime can pre-initialize these interfaces in milliseconds, satisfying the \<10ms constraint while maintaining a high-security posture.

## **Embedded Scripting for "Code Mode": Rhai vs. Starlark for Agentic Logic**

To support "Programmatic Tool Calling"—where an agent writes and executes code to perform complex calculations or data manipulation—the runtime needs an embedded scripting engine. This engine must be lightweight (\<5MB binary footprint), fast (no runtime compilation), and safe (strict resource limits).44

### **Scripting Engine Evaluation: Rhai, Starlark, and Boa**

The technical evaluation considered several candidates for the "Code Mode" scripting engine.

**Rhai** is a pure Rust scripting language designed for minimal binary size and easy embedding. It is not a port of an existing language but was built from the ground up for the Rust ecosystem. It supports a "minimal build" configuration that can reduce the engine size significantly by opting out of features like object maps, arrays, or floating-point support.45

**Starlark** (as implemented in starlark-rust by Meta) is a deterministic subset of Python used in build systems like Bazel. It is specifically designed to be highly secure and deterministic, which prevents certain classes of infinite loops or non-deterministic behaviors.46

**Boa** is an experimental JavaScript engine written in Rust. While highly expressive, its current binary size and performance overhead for tiny scripts are often higher than those of Rhai or Starlark.48

| Feature | Rhai | Starlark (starlark-rust) | Boa (JavaScript) |
| :---- | :---- | :---- | :---- |
| **Binary Size** | \<1MB (Minimal Build).45 | \~2.5MB.46 | \~10MB+. |
| **Language Style** | JavaScript/C-like.45 | Python-like.46 | Full ECMAScript. |
| **Resource Limits** | Built-in on\_progress and op-counts.45 | Instruction counting (recent addition).50 | Manual through engine context. |
| **Rust Interop** | High; direct type sharing with serde.51 | High; custom type support via macros.52 | Moderate. |
| **Cold Start** | Very Low (AST walker or bytecode).48 | Low (Bytecode interpreter).46 | Moderate. |

### **Recommended Pattern: Optimized Rhai with Minimal Build and Gas Metering**

For a high-performance agent framework, **Rhai** is the recommended choice due to its superior binary size and granular control over the execution environment.

1. **Configuration**: Use the no\_index, no\_object, and no\_float features to minimize the binary size. For 32-bit embedded targets or WASM, the only\_i32 feature further prunes unnecessary code for 64-bit integers.45  
2. **Gas Metering**: Implement "Instruction Counting" by using Engine::set\_max\_operations. This serves as a "gas limit" for the agent's code, preventing it from entering infinite loops or consuming excessive CPU time.45  
3. **Implementation**: Use Engine::new\_raw() to create a bare-bones engine, then selectively add only the necessary packages (e.g., math, strings) to keep the footprint under the 5MB constraint.45

This strategy provides the agent with a powerful tool for on-the-fly logic while ensuring that the host application remains protected from misbehaving or malicious scripts.

## **Memory Consolidation Algorithms: From Episodic to Semantic Storage**

AI agents utilize a "Three-Tier Memory" design: Tier 1 (Episodic/Working Memory for the current session), Tier 2 (Summarized/Mid-term Memory), and Tier 3 (Semantic/Long-term Memory in a Vector Database).55 The primary gap is the algorithm for "Memory Consolidation"—deciding when and how to move data between these tiers without losing critical information.57

### **Significance Scoring and Cognitive Compression**

Current research highlights two primary methods for memory consolidation: **Heat-based Prioritization** and **Cognitive Compression**.

In the **MemoryOS** model, information is organized into hierarchical layers. Transition between tiers is governed by "Heat" values, which are updated based on access frequency and recency. When a heat-based threshold (e.g., ![][image1]) is exceeded, information is promoted from mid-term to long-term memory.38

The **Agent Cognitive Compressor (ACC)** approach introduces a "Compressed Cognitive State" (CCS). Instead of a simple transcript, the agent maintains a bounded internal state. At each turn, a "Cognitive Compressor Model" (CCM) evaluates the new interaction, retrieves relevant artifacts, and commits only the "invariants" (goals, policy constraints, confirmed decisions) to the CCS, discarding irrelevant details.58

### **Recommended Algorithm: Hierarchical Heat-Based Consolidation**

The framework should implement a **Heat-based Segmented Paging** algorithm for memory consolidation.

1. **Crate Selection**: Use Qdrant (via the qdrant-client crate) for Tier 3 semantic storage and Redis (via redis-rs) for Tier 1 episodic storage.59  
2. **Algorithm Implementation**:  
   * **Tier 1 to Tier 2**: After every ![][image2] turns or when the context window reaches a specific token limit, trigger a summarization process. Use a "Significance Scoring" prompt that asks a smaller LLM to identify "state-altering" information (e.g., a user's preference for Python).57  
   * **Tier 2 to Tier 3**: Assign a "Heat" score to each summary segment. Every time a summary is retrieved to answer a query, increment its heat. When a segment's heat exceeds a threshold, move it to the vector database for long-term retrieval.38  
3. **Conflict Resolution**: When consolidating, use the **Memory-R1** approach, which employs a "Memory Manager" agent to perform CRUD-style operations (ADD, UPDATE, DELETE, NOOP) to ensure that new information correctly evolves the memory bank rather than just appending to it.62

| Memory Tier | Storage Mechanism | Consolidation Algorithm |
| :---- | :---- | :---- |
| **Tier 1: Episodic** | Redis / In-memory | Sliding Window / FIFO.57 |
| **Tier 2: Mid-term** | Local Disk / Summary Pages | Heat-based Promotion.55 |
| **Tier 3: Semantic** | Qdrant / Vector DB | Embedding-based RAG.57 |

This hierarchical approach balances the need for high-fidelity recent context with the requirement for massive, searchable long-term knowledge, all while minimizing token costs by pruning low-significance data.58

## **Mathematical Definition of "Convergence": Metrics for Supervisor Performance**

To optimize the "SupervisorAgent"—the component responsible for routing tasks and deciding when a workflow is complete—we need a mathematical metric to quantify "Trajectory Efficiency." The goal is to determine if the agent took the shortest path to the correct answer without unnecessary detours or loops.21

### **Trajectory Efficiency and SPL**

The most robust metric for this is **Success weighted by Path Length (SPL)**, a standard in embodied AI and navigation research that is increasingly applied to agentic reasoning.65

The SPL is formally defined as:

![][image3]  
Where:

* ![][image2] is the number of evaluation episodes.65  
* ![][image4] is a binary success indicator (![][image5] if the goal was reached, ![][image6] otherwise).65  
* ![][image7] is the "Ideal Path Length"—the shortest possible sequence of tool calls or reasoning steps required to solve the task.65  
* ![][image8] is the "Executed Path Length"—the actual number of steps the agent took.

This metric penalizes detours and rewarded efficiency. An agent that reaches the correct answer in the minimal number of steps receives a score of ![][image9], while an agent that takes a circuitous route or fails entirely receives a lower score.65

### **Convergence Scores via TQGR**

To detect when an agent is no longer making progress (convergence), we can use the **Trajectory-Quality Growth Rate (TQGR)**.68 This identifies the relationship between the improvement in solution quality and the computational cost incurred at each step.

![][image10]  
In a "SupervisorAgent" context, if the TQGR falls below a certain epsilon over several iterations, the system can mathematically conclude that the agent has converged on a solution or is stuck in an unproductive loop, triggering a termination or a backtracking strategy.58

### **Implementation in the SupervisorAgent Router**

1. **Metric Calculation**: For every workflow, the Supervisor records the number of tool calls (![][image8]). For a subset of benchmark tasks, provide the "Optimal Path Length" (![][image7]).  
2. **Performance Grading**: The Supervisor's performance is graded on its aggregate ![][image11] across a suite of diverse tasks.65  
3. **Online Optimization**: Use the ![][image12] to implement a "Patience" parameter. If the TQGR remains negative for 2-3 turns (indicating that more steps are leading to worse or stagnant answers), the Supervisor forces the agent to return a "Final Answer" or a failure state.63

By quantifying efficiency through ![][image11] and ![][image12], the framework can objectively compare different agent architectures and use reinforcement learning to fine-tune the Supervisor's decision-making logic over time.21

## **Technical Synthesis and Conclusion**

The construction of a high-performance agent runtime in Rust requires a synthesis of low-level systems engineering and high-level cognitive modeling. By addressing the six identified gaps with the recommended crates and patterns, the framework can achieve a level of reliability and efficiency that surpasses traditional Python-based implementations.

The **Durable Execution** problem is solved not by snapshotting memory, but by a log-centric replayer that ensures every side effect is journaled, allowing for sub-millisecond recovery times.5 The **Graph Orchestration** utilizes a generational arena via slotmap and petgraph, providing a memory-safe way to handle complex agentic cycles and parallel node execution while remaining compliant with the Alignment Principle.21

Secure tool interaction is managed through a **WASM-to-MCP Bridge**, utilizing WASI-Virt and the WASM Component Model to proxy capabilities into the sandbox without exposing the host system.35 For on-the-fly scripting, an optimized **Rhai** engine provides a Python-like experience for agents within a \<1MB binary footprint, protected by strict gas metering.45

Finally, the agent's "intelligence" is preserved through a **Hierarchical Memory** system that uses heat-based consolidation to manage the transition from episodic to semantic storage, ensuring that the most significant information is always within the context window.55 The performance of the entire system is governed by a mathematical framework of **SPL** and **TQGR**, providing a rigorous way to measure and optimize agent trajectories.65

This technical specification provides the roadmap for a high-performance, durable, and secure agent framework that leverages the full power of the Rust ecosystem to enable the next generation of autonomous AI systems.

#### **Works cited**

1. Rust Async Programming: Future Executors and Task Scheduling, accessed February 22, 2026, [https://dev.to/leapcell/rust-async-programming-future-executors-and-task-scheduling-56bk](https://dev.to/leapcell/rust-async-programming-future-executors-and-task-scheduling-56bk)  
2. Async/Await in Rust: A Beginner's Guide \- DEV Community, accessed February 22, 2026, [https://dev.to/leapcell/asyncawait-in-rust-a-beginners-guide-237h](https://dev.to/leapcell/asyncawait-in-rust-a-beginners-guide-237h)  
3. Async Programming in Rust: Understanding Futures and Tokio, accessed February 22, 2026, [https://thenewstack.io/async-programming-in-rust-understanding-futures-and-tokio/](https://thenewstack.io/async-programming-in-rust-understanding-futures-and-tokio/)  
4. Temporal Workflow Execution overview, accessed February 22, 2026, [https://docs.temporal.io/workflow-execution](https://docs.temporal.io/workflow-execution)  
5. Building a modern Durable Execution Engine from First Principles ..., accessed February 22, 2026, [https://restate.dev/blog/building-a-modern-durable-execution-engine-from-first-principles/](https://restate.dev/blog/building-a-modern-durable-execution-engine-from-first-principles/)  
6. Develop code that durably executes \- Learn Temporal, accessed February 22, 2026, [https://learn.temporal.io/tutorials/go/background-check/durable-execution/](https://learn.temporal.io/tutorials/go/background-check/durable-execution/)  
7. Restate – Low-latency durable workflows for JavaScript/Java, in Rust, accessed February 22, 2026, [https://news.ycombinator.com/item?id=40659160](https://news.ycombinator.com/item?id=40659160)  
8. About Temporal SDKs | Temporal Platform Documentation, accessed February 22, 2026, [https://docs.temporal.io/encyclopedia/temporal-sdks](https://docs.temporal.io/encyclopedia/temporal-sdks)  
9. The definitive guide to Durable Execution \- Temporal, accessed February 22, 2026, [https://temporal.io/blog/what-is-durable-execution](https://temporal.io/blog/what-is-durable-execution)  
10. Durable Execution Is Not Just for Failures \- Golem Cloud, accessed February 22, 2026, [https://www.golem.cloud/post/durable-execution-is-not-just-for-failures](https://www.golem.cloud/post/durable-execution-is-not-just-for-failures)  
11. Get started with durable computing | by John Coleman \- Medium, accessed February 22, 2026, [https://medium.com/@qbyteconsulting/get-started-with-durable-computing-ced456ed2772](https://medium.com/@qbyteconsulting/get-started-with-durable-computing-ced456ed2772)  
12. Save WASM state and resume · Issue \#3017 \- GitHub, accessed February 22, 2026, [https://github.com/bytecodealliance/wasmtime/issues/3017](https://github.com/bytecodealliance/wasmtime/issues/3017)  
13. Temporal: Durable Execution Solutions, accessed February 22, 2026, [https://temporal.io/](https://temporal.io/)  
14. Durable Execution: Build reliable software in an unreliable world, accessed February 22, 2026, [https://thenewstack.io/temporal-durable-execution-platform/](https://thenewstack.io/temporal-durable-execution-platform/)  
15. flawless \- durable execution engine for rust : r/rust \- Reddit, accessed February 22, 2026, [https://www.reddit.com/r/rust/comments/17g0zvm/flawless\_durable\_execution\_engine\_for\_rust/](https://www.reddit.com/r/rust/comments/17g0zvm/flawless_durable_execution_engine_for_rust/)  
16. WebAssembly and Unikernels: A Comparative Study for Serverless, accessed February 22, 2026, [https://arxiv.org/html/2509.09400v1](https://arxiv.org/html/2509.09400v1)  
17. Flawless – Durable execution engine for Rust \- Hacker News, accessed February 22, 2026, [https://news.ycombinator.com/item?id=38010267](https://news.ycombinator.com/item?id=38010267)  
18. Mastering Durable Execution in Distributed Systems \- Temporal, accessed February 22, 2026, [https://temporal.io/blog/durable-execution-in-distributed-systems-increasing-observability](https://temporal.io/blog/durable-execution-in-distributed-systems-increasing-observability)  
19. Why we built Restate, accessed February 22, 2026, [https://restate.dev/blog/why-we-built-restate/](https://restate.dev/blog/why-we-built-restate/)  
20. Arenas in Rust \- In Pursuit of Laziness, accessed February 22, 2026, [https://manishearth.github.io/blog/2021/03/15/arenas-in-rust/](https://manishearth.github.io/blog/2021/03/15/arenas-in-rust/)  
21. Towards a science of scaling agent systems \- Google Research, accessed February 22, 2026, [https://research.google/blog/towards-a-science-of-scaling-agent-systems-when-and-why-agent-systems-work/](https://research.google/blog/towards-a-science-of-scaling-agent-systems-when-and-why-agent-systems-work/)  
22. Are Graphs Hard in Rust? \- Payas Rajan \- YouTube, accessed February 22, 2026, [https://www.youtube.com/watch?v=kGaU5kU-5rw](https://www.youtube.com/watch?v=kGaU5kU-5rw)  
23. petgraph \- Rust \- Docs.rs, accessed February 22, 2026, [https://docs.rs/petgraph/](https://docs.rs/petgraph/)  
24. Graphs in Rust: An Introduction to Petgraph | Depth-First, accessed February 22, 2026, [https://depth-first.com/articles/2020/02/03/graphs-in-rust-an-introduction-to-petgraph/](https://depth-first.com/articles/2020/02/03/graphs-in-rust-an-introduction-to-petgraph/)  
25. A comparison of every\* Arena in Rust : r/rust \- Reddit, accessed February 22, 2026, [https://www.reddit.com/r/rust/comments/1etbfym/a\_comparison\_of\_every\_arena\_in\_rust/](https://www.reddit.com/r/rust/comments/1etbfym/a_comparison_of_every_arena_in_rust/)  
26. mapgraph \- Rust \- Docs.rs, accessed February 22, 2026, [https://docs.rs/mapgraph](https://docs.rs/mapgraph)  
27. Module 5.1: Deep Dive into Async Rust (Futures, Tokio ... \- Dev Genius, accessed February 22, 2026, [https://blog.devgenius.io/module-5-1-deep-dive-into-async-rust-futures-tokio-and-alternatives-d0fa7b55132e](https://blog.devgenius.io/module-5-1-deep-dive-into-async-rust-futures-tokio-and-alternatives-d0fa7b55132e)  
28. Recommendations for 2-phase cyclic graph structure app \- help, accessed February 22, 2026, [https://users.rust-lang.org/t/recommendations-for-2-phase-cyclic-graph-structure-app/109452](https://users.rust-lang.org/t/recommendations-for-2-phase-cyclic-graph-structure-app/109452)  
29. The crate ships with parallel graph algorithms and APIs to ... \- Reddit, accessed February 22, 2026, [https://www.reddit.com/r/rust/comments/yxtln1/we\_are\_happy\_to\_announce\_graph\_v030\_the\_crate/](https://www.reddit.com/r/rust/comments/yxtln1/we_are_happy_to_announce_graph_v030_the_crate/)  
30. Performance Comparison of Graph Representations Which Support, accessed February 22, 2026, [https://arxiv.org/html/2502.13862v1](https://arxiv.org/html/2502.13862v1)  
31. A Framework For Task Automation Through Multi-agent Collaboration, accessed February 22, 2026, [https://www.researchgate.net/publication/381851016\_BMW\_Agents\_--\_A\_Framework\_For\_Task\_Automation\_Through\_Multi-agent\_Collaboration](https://www.researchgate.net/publication/381851016_BMW_Agents_--_A_Framework_For_Task_Automation_Through_Multi-agent_Collaboration)  
32. petgraph/petgraph: Graph data structure library for Rust. \- GitHub, accessed February 22, 2026, [https://github.com/petgraph/petgraph](https://github.com/petgraph/petgraph)  
33. (PDF) ScholarTrack: A Multi-Agent System for Autonomous, accessed February 22, 2026, [https://www.researchgate.net/publication/400249774\_ScholarTrack\_A\_Multi-Agent\_System\_for\_Autonomous\_Academic\_Research\_Hierarchical\_Agent\_Orchestration\_for\_End-to-End\_Research\_Automation](https://www.researchgate.net/publication/400249774_ScholarTrack_A_Multi-Agent_System_for_Autonomous_Academic_Research_Hierarchical_Agent_Orchestration_for_End-to-End_Research_Automation)  
34. Wasmtime \- The WebAssembly Component Model, accessed February 22, 2026, [https://component-model.bytecodealliance.org/running-components/wasmtime.html](https://component-model.bytecodealliance.org/running-components/wasmtime.html)  
35. Build MCP Servers with Wasmcp and Spin | Spin Docs, accessed February 22, 2026, [https://spinframework.dev/blog/mcp-with-wasmcp](https://spinframework.dev/blog/mcp-with-wasmcp)  
36. WASI and the WebAssembly Component Model: Current Status, accessed February 22, 2026, [https://eunomia.dev/blog/2025/02/16/wasi-and-the-webassembly-component-model-current-status/](https://eunomia.dev/blog/2025/02/16/wasi-and-the-webassembly-component-model-current-status/)  
37. wasmtime-wasi-http 41.0.0 \- Docs.rs, accessed February 22, 2026, [https://docs.rs/crate/wasmtime-wasi-http/latest](https://docs.rs/crate/wasmtime-wasi-http/latest)  
38. wasmtime\_wasi::p2 \- Rust \- Docs.rs, accessed February 22, 2026, [https://docs.rs/wasmtime-wasi/latest/wasmtime\_wasi/p2/index.html](https://docs.rs/wasmtime-wasi/latest/wasmtime_wasi/p2/index.html)  
39. How to virtualize WebAssembly components with WASI Virt, accessed February 22, 2026, [https://wasmcloud.com/blog/how-to-virtualize-webassembly-components-with-wasi-virt/](https://wasmcloud.com/blog/how-to-virtualize-webassembly-components-with-wasi-virt/)  
40. bytecodealliance/WASI-Virt: Virtual implementations of WASI APIs, accessed February 22, 2026, [https://github.com/bytecodealliance/WASI-Virt](https://github.com/bytecodealliance/WASI-Virt)  
41. wasmcp-wasi \- WebAssembly \- Lib.rs, accessed February 22, 2026, [https://lib.rs/crates/wasmcp-wasi](https://lib.rs/crates/wasmcp-wasi)  
42. WASI: secure capability based networking \- JDriven Blog, accessed February 22, 2026, [https://jdriven.com/blog/2022/08/WASI-capability-based-networking](https://jdriven.com/blog/2022/08/WASI-capability-based-networking)  
43. WebAssembly — list of Rust libraries/crates // Lib.rs, accessed February 22, 2026, [https://lib.rs/wasm](https://lib.rs/wasm)  
44. Rust embedded binary size \- Stack Overflow, accessed February 22, 2026, [https://stackoverflow.com/questions/58075821/rust-embedded-binary-size](https://stackoverflow.com/questions/58075821/rust-embedded-binary-size)  
45. Minimal \- Rhai \- Embedded Scripting for Rust, accessed February 22, 2026, [https://rhai.rs/book/start/builds/minimal.html](https://rhai.rs/book/start/builds/minimal.html)  
46. A Rust implementation of the Starlark language \- GitHub, accessed February 22, 2026, [https://github.com/facebook/starlark-rust](https://github.com/facebook/starlark-rust)  
47. starlark \- Rust \- Docs.rs, accessed February 22, 2026, [https://docs.rs/starlark/latest/starlark/](https://docs.rs/starlark/latest/starlark/)  
48. r/rust on Reddit: What is the lightest and fastest scripting language, accessed February 22, 2026, [https://www.reddit.com/r/rust/comments/1hyszmr/what\_is\_the\_lightest\_and\_fastest\_scripting/](https://www.reddit.com/r/rust/comments/1hyszmr/what_is_the_lightest_and_fastest_scripting/)  
49. A Rust implementation of the Starlark language \- GitHub, accessed February 22, 2026, [https://github.com/alexchoi0/starlark-rust](https://github.com/alexchoi0/starlark-rust)  
50. Activity · facebook/starlark-rust \- GitHub, accessed February 22, 2026, [https://github.com/facebook/starlark-rust/activity](https://github.com/facebook/starlark-rust/activity)  
51. Rhai: An embedded scripting language for Rust | Hacker News, accessed February 22, 2026, [https://news.ycombinator.com/item?id=42738753](https://news.ycombinator.com/item?id=42738753)  
52. starlark\_module in starlark \- Rust \- Docs.rs, accessed February 22, 2026, [https://docs.rs/starlark/latest/starlark/attr.starlark\_module.html](https://docs.rs/starlark/latest/starlark/attr.starlark_module.html)  
53. starlark\_module \- crates.io: Rust Package Registry, accessed February 22, 2026, [https://crates.io/crates/starlark\_module](https://crates.io/crates/starlark_module)  
54. Rhai \- Embedded scripting language and engine for Rust \- GitHub, accessed February 22, 2026, [https://github.com/rhaiscript](https://github.com/rhaiscript)  
55. Memory OS of AI Agent \- ACL Anthology, accessed February 22, 2026, [https://aclanthology.org/2025.emnlp-main.1318.pdf](https://aclanthology.org/2025.emnlp-main.1318.pdf)  
56. Memory Optimization Agent \- Emergent Mind, accessed February 22, 2026, [https://www.emergentmind.com/topics/memory-optimization-agent](https://www.emergentmind.com/topics/memory-optimization-agent)  
57. Memory Optimization Strategies in AI Agents | by Nirdiamant \- Medium, accessed February 22, 2026, [https://medium.com/@nirdiamant21/memory-optimization-strategies-in-ai-agents-1f75f8180d54](https://medium.com/@nirdiamant21/memory-optimization-strategies-in-ai-agents-1f75f8180d54)  
58. AI Agents Need Memory Control Over More Context \- arXiv, accessed February 22, 2026, [https://arxiv.org/html/2601.11653](https://arxiv.org/html/2601.11653)  
59. Building AI Agents with Persistent Memory: A Unified Database, accessed February 22, 2026, [https://www.tigerdata.com/learn/building-ai-agents-with-persistent-memory-a-unified-database-approach](https://www.tigerdata.com/learn/building-ai-agents-with-persistent-memory-a-unified-database-approach)  
60. A Multi-Graph based Agentic Memory Architecture for AI Agents \- arXiv, accessed February 22, 2026, [https://arxiv.org/pdf/2601.03236](https://arxiv.org/pdf/2601.03236)  
61. Building smarter AI agents: AgentCore long-term memory deep dive, accessed February 22, 2026, [https://aws.amazon.com/blogs/machine-learning/building-smarter-ai-agents-agentcore-long-term-memory-deep-dive/](https://aws.amazon.com/blogs/machine-learning/building-smarter-ai-agents-agentcore-long-term-memory-deep-dive/)  
62. Memory-R1: Enhancing Large Language Model Agents to Manage, accessed February 22, 2026, [https://arxiv.org/html/2508.19828v5](https://arxiv.org/html/2508.19828v5)  
63. COMPRESSED STEP INFORMATION MEMORY FOR END-TO-END, accessed February 22, 2026, [https://openreview.net/pdf?id=vUG2hpVJWR](https://openreview.net/pdf?id=vUG2hpVJWR)  
64. Toward Efficient Agents: Memory, Tool learning, and Planning, accessed February 22, 2026, [https://www.researchgate.net/publication/399953342\_Toward\_Efficient\_Agents\_Memory\_Tool\_learning\_and\_Planning/fulltext/6970eb52e806a472e6a4f109/Toward-Efficient-Agents-Memory-Tool-learning-and-Planning.pdf?origin=scientificContributions](https://www.researchgate.net/publication/399953342_Toward_Efficient_Agents_Memory_Tool_learning_and_Planning/fulltext/6970eb52e806a472e6a4f109/Toward-Efficient-Agents-Memory-Tool-learning-and-Planning.pdf?origin=scientificContributions)  
65. SPL: Navigation Efficiency Metric \- Emergent Mind, accessed February 22, 2026, [https://www.emergentmind.com/topics/navigation-efficiency-metric-spl](https://www.emergentmind.com/topics/navigation-efficiency-metric-spl)  
66. UNIVERSIT\`A DEGLI STUDI DI PADOVA, accessed February 22, 2026, [https://thesis.unipd.it/retrieve/d048fdd1-8912-4aac-8477-38e4c7352dec/Golan\_Rodrigo.pdf](https://thesis.unipd.it/retrieve/d048fdd1-8912-4aac-8477-38e4c7352dec/Golan_Rodrigo.pdf)  
67. Success Weighted by Completion Time \- Navigation \- ResearchGate, accessed February 22, 2026, [https://www.researchgate.net/publication/357111196\_Success\_Weighted\_by\_Completion\_Time\_A\_Dynamics-Aware\_Evaluation\_Criteria\_for\_Embodied\_Navigation](https://www.researchgate.net/publication/357111196_Success_Weighted_by_Completion_Time_A_Dynamics-Aware_Evaluation_Criteria_for_Embodied_Navigation)  
68. Computationally efficient and sub-optimal trajectory planning ... \- PMC, accessed February 22, 2026, [https://pmc.ncbi.nlm.nih.gov/articles/PMC9649449/](https://pmc.ncbi.nlm.nih.gov/articles/PMC9649449/)  
69. Success Rate (SR) and Success weighted by Path Length (SPL), accessed February 22, 2026, [https://www.researchgate.net/figure/Success-Rate-SR-and-Success-weighted-by-Path-Length-SPL-outcomes-from-GPT-models-with\_fig2\_380907646](https://www.researchgate.net/figure/Success-Rate-SR-and-Success-weighted-by-Path-Length-SPL-outcomes-from-GPT-models-with_fig2_380907646)  
70. LogPPO: A Log-Based Anomaly Detector Aided with Proximal Policy, accessed February 22, 2026, [https://www.mdpi.com/2624-6511/9/1/5](https://www.mdpi.com/2624-6511/9/1/5)

[image1]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAC4AAAAYCAYAAACFms+HAAABc0lEQVR4Xu2WvSuFYRTAj898lWISIWQxEYMwyaCUBYuYRAySSTEof4BSslDiDzDL17XIYMFgl5EymYjfuY/7Ove9Lu7gvnd4fvUbnnPO2z33fc7z9Ip4PJ6omMARbMJy7MEFrLdFucg5vofcwmJblIvE8AIvxTU8nJTNYU6xMRxMh85PM7YYdcYqbFGWOJE/NN6B15I6Uwn3v0qzxjHO4SFe4QaW2oJavMVZ7MUlPMA+cSe5GyuD6lR0N3Rb9TB9ZwzPPmtUfZPt+uAvaMM7WISFuI53YnZhDNsSC9jEabOOilbMN+sGcbu/a2IBefiAXeFEBGgvFn3z2vh9KB6nE1+xJJz4Af2BfhzIwOr4k+kZwmccNbECcY0/mljAmrh5z4QyXMaVDNSb6ydm8A0nTaxGXONHJhZwg3vhYARUiTvEOh4JFsX9Gb1AktDvgRecCiciYl7cjbSK2/iE4yafRJ2kHooo0bOm8z4obhc8Ho/nn/gArdRKT/iiMB4AAAAASUVORK5CYII=>

[image2]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABIAAAAYCAYAAAD3Va0xAAABF0lEQVR4Xu2TvUoDQRRGr1qZSkTSpBQMIjZ5AAvjA0jE0jYJ1jZ2go1VQLCxsgkk8SUELRQUkQTSBEJIo502kkLUnHFm2J27m8J+Dxx25n7D/C0jkvFftnGIr/iGzTD+4xFHOBA7thGkijZ+4A+uqmwBT/AWC2GUpItH+CvpK57hvi5qiniNS/iJ75gLRojcYV7VEtTw0LUvxe6qGsWyiM+x/kxauO7am2InMkf1lPEi1p/Ji+rfiJ1sy/VPcS+K0/H3E6cidiJfN/ezEsXp1CW6H4/53WP8wjV8CuN0Orihi3Asdlf3eK6yBHPYd1+NOcpE7GS7KktwgD2c14HjCr9xWQeeHbHvyqxoNE/DvDlNCR90MSMDpvMbNCf6RtASAAAAAElFTkSuQmCC>

[image3]: <data:image/png;base64,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>

[image4]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABIAAAAXCAYAAAAGAx/kAAABOklEQVR4XuXUvytFcRjH8Y8f+ZHfGZRVSsnCgGQxiTKSRYlJKRaD1UhJ4i+gKKNNNmURFpNkVQYjix/vb8+993zP497ruuv91Ktbz/P0Ped8v+dcqSJThXEMoiFT60ZjbqKELOES+zjHC+bwiJZormg2cI32qNaLL9xEtaLpxycGfINcYccXCyXczTeafUP2mNO+WCgHsoW2UO168ab/mRnZQsErTrGI+niolIQjX8e7kgWDC9RGcyWnCVPYxYdsscnUhLSMNldTl+y9yZdR2UJDvpEvs3hCjW+QPTwr2fw+bGMkNxHlUHbVBVcfw5vsxELCYpuYx0l2KM4tVvGAO9kVj3Cv9N50oAdnss/oV4Yzv3WyDzUc+UTSTqVTdpetvvHfrOBYdqHsDZSV8C8QXtQ13ygn4T2rlPwAaPUy9w5oicoAAAAASUVORK5CYII=>

[image5]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAoAAAAWCAYAAAD5Jg1dAAAAX0lEQVR4XmNgGJpAAYjj0QVhQBOIO4D4LBD/A+KtqNIIYAbESUBsDMQ/GfAoRAZDReF2dEFsgCSFO9AF0QErEP8C4kNAzIImBwZeQHwbiJ8C8ScofgnEt4CYH0ndCAMAwVEZeCrPn+cAAAAASUVORK5CYII=>

[image6]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAoAAAAWCAYAAAD5Jg1dAAAAx0lEQVR4Xt3QMctBYRiH8TthkGTEzCAy2Egmq9XrI9hkYTLJF7BIJrIok7Kb2KwGs+H9AO/2FtfpuZ+6D5+Af/2W63nO6XREPmsRtDHFANnwsVscOxzQxBB31OylYD38ImFa8OYboqbJBXsbWAsPNHxIa1j5oKtqH/tQ0LDwQVfSPvch+OBQ0BW1b33Ia1j6oCtrn/mQxL+YJ3V1cRdHNh5xtoF1xV2s2PiDP+RMW4t7wdsmuKKPDU5IhW6YZdAR9ydiL2ffsyeA0yVvb/qBbwAAAABJRU5ErkJggg==>

[image7]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABMAAAAYCAYAAAAYl8YPAAABEklEQVR4Xu2Tu2pCQRCGJ0a0sLHyDRQLG03AzsI2hQ9grdjZiGArlr5DKkmbQBQRfIX0kngDCxshxEsV1H+YRdZRhLOW+sEHZ+dfhj1z9hDduZYsHMIx/DbPP7Brb/LKB9zBnA5cmMM/6NeBV2Ikp2rrwIUCSbOKDlxokTR71oELM/gLH3XglSjJqT51YAjDgLWOwxdrfUSRpFlVB4Z3GLHWJZIZn+WNpFlaByABO7p4Cb5fSzq9X0HYg3mrxqeqwwerdiBJcir96zzBPlzBkKnxnFJwQDK3Axk4gmuSZhs4gVO4gFv4D1/NfoavDb827/FZdWeasKGLLvBpeL7869VU5hke+hcsk8zuaviL81e+NfYJTzD2J/MUTAAAAABJRU5ErkJggg==>

[image8]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABIAAAAYCAYAAAD3Va0xAAABKklEQVR4Xu2TvytGYRiGnyg/kmSiCIMyGJWS5TMqZdA3YRFlUBZlYLApg02SxKJk9ReISfkHEElhkkEswvX2nMPrdtD5Rrnq6tR9v+c9b+c5x+yfvPTiKV7jbXI9xzO8wAMcxYr0ht/YwVfsibIqnEry5Sj/kXCKeyzXAi7xGVu0UJrNn7qnBZThA75g2+fqK8PmG81oAX3m3ZbkmaybL+6SvBWP8AobpMskTC0cfR5ncQ63zd/ZJtZ/LP2e9P0c42DiABbMpxZTh+OSvTNivtGC5Fl046qGKRvmGxUkz034ep+wUguhH5ewUYtAh/lp9rUQmnAM13AyLsJvcIJ35qd5NB/xULwoohNr8QbbpctNmOShhqWwixM4jdXS5WIFF7GoRSnUaPBHeQNM9zi75R2uHgAAAABJRU5ErkJggg==>

[image9]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABkAAAAXCAYAAAD+4+QTAAABRklEQVR4Xu2UvytGURiA3/wsKVI2g8HArMjgD2CSBUkilJhMmAwKg5FsNvkD/BpMYmQShTIZGJUBhed1zr2d+3YvnbKo76mnr/uc03m/bvdekRL/gWYctfEX6nEE13Acq7PLjjZcxXP8wP3s8o804RWuYxdu4TU2hJuUDnH/oB1fJW7ILu6ZdobbpmWIGdIobv+c6cv4IgW3TYkZ0oefOGa6DtXebXpKzJBJcYcNmT7j+4DpKTFDFiT/sGnf9TcXHXJgYwHz4g4bND0ZMmV6ig45tLGACXGHDZs+63u/6SkxQ3ok/7Ys+t5peooOObLRo49sS3BdI+5RXQmaoi/kE5ab/k0lvuEJVpg15R7fxX16EjbxAsv8dRXe4VKyIaEXb/EBn72PeIN1wb4dvMTaoOmhx169bae4Eaz/Ka3i3hf9DpYoIfIFcfxH6saN3DMAAAAASUVORK5CYII=>

[image10]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAmwAAAAsCAYAAADYUuRgAAAFYUlEQVR4Xu3deaimYxjH8csyGGvZt9GEQYmGhJjsf5AlWyFLxz4oyR/IVkNCRJaxp0EZij8wwwwxdoqUJWuyJEtk14jE9eu+757rvc/zzHvOODPmeL+funru+3qeOS//Xd2rGQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAICl7liPXepkZQ2PverkYlqvTgAAAGDRFnr8XSezqR6zPSbm/lUefzavR22C9f5WbO8a2gAAAMg28zjJUuG0XPVORdpnHqtW+XkeW1S50YhF2sUdbQAAAGTv5+fJHq/FF5betY28rWDNKNt2Hofn9uYeB+W2bO9xvMcmISflbyp/cG6vnvNTPLa0NBI3yWNDj+U9VvHY1NJvAwAADIz1PVYK/bo4U/+vKld0TWvG9h/5uZOlwq3o+r7+fYm520IbAABgIGg9WvSFxz6hr2KtrYiSrkIrtjfOT02t3hLyXd+3/VYsGNmsAAAABsoMj3OrnAorFU275/7VuV/bzWNB6HcVXb+H9u2h3fV9ae8QcpoyPc7jmpADAAAYCB96rFYnLRVNT+S23mvTwboea3s8nfN6r92jRVvRtZXH5bm9oqWCbVrut30f2+eHnNasvWzD19cBAAD8r61sqThaVETPe/zicZbHN9aMwBVzLI3GqSjTv30w57/1uN7jAo/fLK2XUwH4s8d0jwdy+5L8vXKPWFPoFVoLV29cAAAAGEa7GF+yNNIT41WP58J3N3vM8rjS472Qj86xtAPzHktnnM30OC+/03Tjxx5f56cKnLvzu2XBOx6fWircliQdMXJXbu8bXwAAAHTRbQAqonR0hdZ/aSRJuyz3tKZ4OdDjutwWFWTaIRlpUf+Loa8bBvS36kNjyyiXpgRV1JXjL/5rQ5b+2z6o8mNN07G/2tjdrgAAAAZAHEX7ytIUYaG1XZraK2u8IhU3Gp2T7zzeCu8KHUQb6byxR0N/b4+PQh8AAAB9qAgrU5iyo7Uvite0nr7VbQIaadMRFRv1fJHons7oTmt2SupojB/COwAAAIyAirCdQ19XKl0W+oUKOU3rqXDTmjYt3h8JjaY97vGGpaMx2oo8ucPSyF+MBR7PWPtoHwAAwEDY1mNuldNmhK2rnLxizUjZs9a7Tu1CjyctFX83hbzOHIu7NPf3uCj0AQAA0MfD1ju6Jro8/fTcfsjSRgEVZxrtKg7zuDH0Cx13EX3i8WPo6/e0q7SNflOje10BAAAwkL63dBBspMvJdfyGzgpT+2xLO0fXih9ZWsN2TOjrjDKNskUaXYsjeJrmVGF2r3EWGQAAwL+madGjLZ1VJm3nh23gcYI1O0dH4tA6sZSpiDyqTga6iUBFpjZLaAr3zd7XAAAAyx4VOPdZU7iNZ5Mt/f+8XeWLSy2NFBbzPd4N/ZEaqhMAAABL0n4et9bJcUjnwelGhiEbfl2V6OqphXXSRl+wrePxU50EAABAfyrWdP+nnGbpGq5IRVy5NzSakp/bWLoX9ABLR5QU93ucaunqLVGBp7+l0TkAAACMwhmhrcKtHmVT/9oqF30e2k9Z+hsTPKbm3In5eaYN/9sAAADo44o64b702CP0VWTpgN/arPyMRdg0jxtyW9Ooeqfz64SCDQAAYDHEq7cKXciuwkoX1oumSdsKrUPyM76bYen8ukkhd4qlI1KmW/OtRuAAAADQh44c0VEdOiNO68p0OX0JFVaPNZ/aTEvXbhVHhPYLlg4Rltfzc7I1Nz7oeBNRAdhW+AEAAGAM6UqtI+ukpcODtfmgKAXcmiFXTKwTAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAwLjyD+TRApcUm9rBAAAAAElFTkSuQmCC>

[image11]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACgAAAAYCAYAAACIhL/AAAACTElEQVR4Xu2WOWhVQRSG/2hwQQU7wYBVXECCuGFCSKGkEAW10iKgiIKKWIhLAilSpEwgJEGLEKxcEBtFQUESDAoiBLURBDcUNxBxaTRF1P/nzH1v3nnz8JLGwvvDx4XznzN33p2ZMw8o9B+rjrSRdWReiC0l80sZ/1AHyBgZJjfIR7KHPCOLorxL5DX5EHLekOfkRXgOkZWl7EpdJa9guRr3ZXhuj5NSOk3uk8VRbDn5RSajWKZV5De55eKryQPynTQ4L9NaWK3eN9d5SWnQadLkDeoe6fdBai/sJSe8Qe2HeSPeCDoM8/VRckmJKljoDdhypz7/KKxmvTdQHu+cN4IuwvyN3qilM7CCXjLLefFhiaV985XM9gY1QabICm8EvSXfkK5NagdsguITuQxbplr7Q3tLudddXC88FLxu52XSvpavQ5hbai3HyQ+UJypuk/ooL1MHzB8nnaQL9vV1gp+QXeXUKh2E1Z70Rh4tINvIAPkJG2hrRYYp23+nyE7YCqiuMU6qoQuw2g3eCFriA2t8IOgIbCA9vfSl9ANSe/Nveg9rQan9pwOnHlqSZqvGnFILbIL+lC4L8Tsunkdq3qq96Y2gK6Q5DuyGdfTUrxmEdXx/qvfBXtLj4nmUrYr2rFc7eeqDZ2EFarqxWskXWIvx0jWnms3eyKFrsNr4K+kKPQrbMlU/+iE5Bjt5j0gfOU8eo/pwaHPr/tVJ12Cfyd2KjNrS0une1rWpCer+1uq8g/VL3WJ6Vh2yTeE5B/YPRr1vS9kuVKhQoZnqD/hthHY5Q/8TAAAAAElFTkSuQmCC>

[image12]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAADkAAAAYCAYAAABA6FUWAAADoUlEQVR4Xu2XaYhOURjHHztZCtmzr9llS7KWfPDFUqLIKIXssn+wlkh2pZAPIhFllyWKsociso/skX3f//95zpk595n7vu/M+DbeX/1q3uc599x7z33OMiJp0vyXlILdYT9Yw+RSURK2ktzXVoDVg9//RBd4MQ+egSdhG70si6ZwHzwPV8H18Bk8AqsF7eKoCtfCj/AC3CDa12rRa8/BTtmtlckwEz53PoL34EN4GS6HtXzjkI3wiuhI1hYdvaPwD+wPq7j4CBdrpJfJbPgKjnS/PWXhDfgAljM5zyT4Dm6GlU1uBvzgLG5yHr7gW1ja/WYltRd9j9diXpSd3IKVwqDo14i7yWNYDK6EP2GvaDqbvqIDMs8mwHjR3FibcHCQf4tWQxx1RK/fbxNgjGhuYRjsA5eGAdBMtOFhEy8KL4l+XebnR7JRWG5sw/Yhg0VfgNWTDH6RWTboGCba91SbAAtEc5H+M2DDMCA5ozHTxMvDRfAJfCNalsn4JdrOw/YvRftuEcTj4FfsbIMOvgD7aGfi/Ai34SdY1+RysUO0EzvpCUePuWU2YeCCxHb3g9g0F7sZxBLRABaxQccd0cHjS3k6ii5+nHrdgnhCXsD3onPPclz0QXvbhMEPxsEgdtbFIvMln3BBYR+sCM5JLpB8MVYNF0K/ECWFZcRODtkEKAG/iXbIPSwZfjCmBDGOPmOtg5iH5cWvV8/9XR9WDBs4/HzkVuLh6nwdXhV9xpSME+1kuk2ILtNcNLjyJoNz9zt8Csu4GFfpH/Crb2TYDk+I5nl/luSoSAtlk8TPxyUu3tXEY9kl2pg1Hgc33i8SnQ8WHgjYR4aJc+9knIeARHCL+iw6oHHw/twf7f33SN6mUdZEZ60nmo9ki2hnXFgIV+C9ops7YRkxv9j9DuFJhrmBNuFoIppnqcfBMmY+bn/koYS5tjZh4WrKhsdsIoBnS86t3bAnnOjiLBeefDhn416QsIx5/OIDNTc5zvFTovefY3Ke0RI/lTgPORX84POUxYHPpqZoCWSKfkGWIuWZ8K7oAmBhKV8TLRuOKjdtnj058Tu4NiwnPx9DeFrhosZ78Itx894GT8MBcJ3k9OGZILoV8QTGUuYg85nDhcnPSZ5dd8JBQa7AcCHhWXG46AmIpyY+oGeu6AslgudfXjdE9D+QPK2KKWgJh0rqg0aB4WjzpLECHoBbo+nCQQ/RcqH8D6FxNF04YPlyX1sjuqmnSZMm//wFqpDfNfdndtoAAAAASUVORK5CYII=>