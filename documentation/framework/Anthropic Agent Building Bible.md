# **The Engineering Principles of Agentic Systems: A Comprehensive Manual for Autonomous AI Design**

The shift from large language models as passive text generators to active, goal-oriented agents represents a paradigm shift in software engineering. This evolution is predicated on the concept of the augmented large language model, which serves as the fundamental atom of any agentic system.1 An augmented model is not merely a transformer predicting the next token; it is a reasoning engine enhanced with retrieval mechanisms, external tools, and persistent memory.1 The engineering challenge lies in moving from deterministic code paths to probabilistic architectures that maintain reliability while leveraging the model's capacity for autonomous decision-making.

The most successful implementations of these systems are characterized by a philosophy of simplicity.1 While many complex frameworks exist to facilitate agent development, experience across diverse industries suggests that the most effective systems utilize simple, composable patterns rather than opaque abstractions.1 This simplicity ensures transparency, allowing developers to explicitly observe and debug the agent’s planning steps, a requirement that becomes increasingly critical as systems gain greater autonomy.

## **Definitions and Taxonomic Distinctions**

In the engineering of these systems, a clear distinction must be made between workflows and agents. A workflow is a structured sequence of predefined steps where the large language model and its tools are orchestrated through fixed code paths.1 Workflows provide high predictability and efficiency for well-defined, repetitive tasks. Conversely, an agent is a dynamic, autonomous system designed to perform tasks by adapting to changes, reasoning, and making decisions without requiring predefined steps.1 Agents are uniquely suited for open-ended or complex problems that require a high degree of flexibility and independent action.

The core of an agent’s capability is its ability to interact with the environment through an Agent-Computer Interface (ACI). This interface is the digital analog to human tool use, where the model utilizes objects or functions from its environment to achieve goals that extend its inherent physical or computational influence.3 Just as zoological tool use involves manipulating external objects to alter the state of another object, an AI agent utilizes API calls and code execution to alter data or system states.3 This behavior is categorized as agentic only when the model itself determines the appropriate tool and input parameters based on its reasoning.1

| System Component | Description | Role in Agentic Architecture |
| :---- | :---- | :---- |
| **Augmented LLM** | LLM \+ Retrieval \+ Tools \+ Memory | The core reasoning and execution unit. |
| **Workflow** | Predefined, fixed code paths | Ensures reliability for structured tasks. |
| **Autonomous Agent** | Dynamic, reasoning-based orchestration | Handles open-ended, complex problem solving. |
| **ACI** | Tool documentation and schemas | The interface between the model and external systems. |
| **MCP** | Standardized connection protocol | Provides universal connectivity to data and tools. |

1

The transition from a tool-using model to a "worker" represents the current frontier of agentic engineering.2 This shift involves the model moving from executing single commands to managing long-running, multi-step processes where it must continuously reconcile its progress with environmental feedback.1 In this context, the model’s performance is no longer measured solely by the quality of its text output but by the success and efficiency of its environmental outcomes.

## **Core Architectural Patterns**

Building effective agentic systems requires the application of specific architectural patterns that allow developers to trade off cost and latency for accuracy and reliability. These patterns are the building blocks of both structured workflows and autonomous agents.

### **Sequential and Routing Architectures**

Prompt chaining is the most fundamental pattern, involving the decomposition of a complex task into sequential subtasks.1 In this architecture, each step builds on the output of the previous one, allowing the model to focus on a narrow transformation at each stage. This modularity not only improves accuracy but also makes the system more maintainable, as each step can be individually tested and refined.6 For instance, a system designed for structured data extraction might first extract raw metrics, then convert them to standardized units, and finally format the results into a markdown table.1

Routing introduces a layer of intelligent task distribution. A "selector" model classifies the incoming input and directs it to a specialized process or departmental prompt.1 This is particularly effective for managing complex tasks with distinct categories of input, such as a customer support system that routes technical queries to a support specialist prompt and billing inquiries to a financial specialist prompt.1 Routing can also be used to optimize costs by sending simple queries to smaller, more efficient models and reserving larger, more capable models for harder problems.1

### **Parallelization and Aggregation**

Parallelization allows multiple instances of a model to work simultaneously on a task, with their outputs aggregated programmatically. This pattern manifests in two variations: sectioning and voting.1 Sectioning involves breaking a task into independent subtasks that are run in parallel, such as generating a response while simultaneously screening the input for safety violations.1 Voting involves running the same task multiple times with diverse prompts to obtain a consensus or to increase confidence in a final result.1

Parallelization is highly effective when processing large volumes of independent items or when multiple perspectives are required to ensure the reliability of a high-stakes decision.6 It allows the system to overcome the serial processing limitations of a single model call, significantly reducing latency for tasks that can be logically split.1

### **Orchestration and Iterative Refinement**

The orchestrator-worker pattern is essential for tasks where the subtasks cannot be predicted in advance. A central "orchestrator" model dynamically breaks down a complex goal, delegates these subtasks to "worker" models, and then synthesizes the results into a final output.1 This pattern is the backbone of coding agents that must determine which files to modify and how to test their changes based on the evolving state of the codebase.1

| Coordination Pattern | Mechanism | Implementation Strategy |
| :---- | :---- | :---- |
| **Prompt Chaining** | Sequential dependency | Use distinct prompts for extraction, transformation, and formatting. |
| **Routing** | Input classification | Implement a high-accuracy classifier to direct tasks to specialized agents. |
| **Sectioning** | Parallel decomposition | Run independent checks (e.g., safety, grammar, logic) concurrently. |
| **Orchestrator-Worker** | Dynamic delegation | The orchestrator should plan, delegate, and synthesize based on worker feedback. |
| **Evaluator-Optimizer** | Iterative feedback loop | Use a critic model to provide specific, actionable feedback for refinement. |

1

For tasks requiring high precision, the evaluator-optimizer pattern utilizes a feedback loop where one model call generates a response and another provides evaluation and critique.1 The generator then uses this feedback to refine its output. This iterative process is particularly valuable when clear success criteria exist, such as in literary translation or complex search tasks where an initial answer might be incomplete or nuanced.1

## **The Agent-Computer Interface (ACI)**

The success of an agent is fundamentally tied to the quality of its interface with the digital environment. Designing a robust ACI requires more than just providing a list of API endpoints; it involves a meticulous approach to tool documentation, parameter design, and error-proofing.

### **Tool Documentation and Specification**

Tools should be documented as if they were being presented to a junior developer.1 A good tool definition includes not just a name and schema, but also clear usage examples, boundaries of when the tool should be used versus other tools, and descriptions of how to handle potential errors.1 The model must be able to infer the correct usage from the documentation alone, without requiring external context.1

Providing the model with examples of correct tool usage within the definition—a feature known as Tool Use Examples—helps resolve ambiguities that JSON schemas cannot capture.8 For example, if a tool requires a specific date format or has parameters that are correlated in complex ways, these should be demonstrated through diverse examples.8

### **Error-Proofing and "Poka-yoke"**

The concept of "Poka-yoke" (mistake-proofing) is central to agentic tool design. Developers should architect their tools to make it as difficult as possible for the model to make a mistake.1 This might include requiring absolute filepaths instead of relative ones to prevent errors when an agent changes its directory, or using enums for parameters to restrict the model to valid options.1

| ACI Design Element | Engineering Requirement | Impact on Reliability |
| :---- | :---- | :---- |
| **Function Naming** | Clear, specific, and unambiguous (e.g., fetch\_order\_history). | Reduces the likelihood of the model selecting the wrong tool. |
| **Parameter Types** | Strict typing with enums and constraints (min/max). | Ensures that the inputs are valid and reduces parsing errors. |
| **Descriptions** | Detailed prompts explaining when and how to use the tool. | Provides the model with the "why" behind the tool invocation. |
| **Error Handling** | Detailed error messages returned as tool results. | Allows the model to reason about its failure and attempt a correction. |

1

Another critical aspect of ACI is the formatting of inputs and outputs. The model should be given enough "thinking time" to reason before it generates a tool call.1 This can be achieved through specific prompt instructions that require the model to explain its plan before invoking a function.1 Furthermore, the formatting of data should remain close to natural patterns observed on the internet, avoiding unnecessary overhead like complex string-escaping in JSON where possible.1

## **Advanced Tool Use and Context Management**

As agentic systems scale to handle hundreds or thousands of tools, traditional approaches to tool calling become inefficient. The context window is a finite resource, and every token used for tool definitions is a token that cannot be used for reasoning or data processing.

### **Programmatic Tool Calling (PTC)**

Programmatic Tool Calling (PTC) represents an evolution where the agent orchestrates its tools through code execution rather than individual natural language API calls.8 In this model, the model writes a script—typically in Python—that can execute multiple tool calls, handle loops, and perform data transformations in a single step.8

PTC offers significant advantages in terms of latency and context efficiency. By executing an orchestration script in a sandboxed environment, the model avoids the need for a full inference pass for every single tool call.8 Moreover, it prevents context pollution by keeping large, intermediate results (such as raw database records or massive log files) within the execution environment.8 Only the final, relevant output is returned to the model's context window, preserving tokens for higher-level reasoning.8

### **Tool Search and Discovery**

For systems with massive toolsets, the Tool Search Tool allows the agent to discover capabilities on demand.8 Instead of loading all definitions upfront, tools are marked with a "defer loading" flag.8 The model is provided with a search capability that it uses to find the specific tools required for its current subtask. Only those relevant tools are then expanded into full definitions within the context window.8

This approach can reduce context consumption by as much as 95%, allowing agents to operate effectively in environments with over 50 tools while maintaining a small footprint.8 It also makes the system prompt more stable, enhancing the effectiveness of prompt caching.8

## **Model Context Protocol (MCP)**

The fragmentation of data sources and tool interfaces is a primary bottleneck in building connected AI systems. The Model Context Protocol (MCP) provides a universal standard for connecting AI applications to external systems.5 MCP replaces the need for a custom integration for every pairing of an agent and a data source with a single, open protocol.5

### **Architecture and Primitives**

The MCP architecture is built around three core components: hosts (like an IDE or desktop app), clients (the agent application), and servers (which expose data and tools).5 The protocol defines standardized primitives that enable agents to interact with the world:

* **Tools:** Model-controlled actions that allow the agent to perform tasks, such as querying a Postgres database or searching the web. Each tool includes a description and a JSON schema for its arguments.10  
* **Resources:** Read-only data sources that provide context, such as files, database views, or API responses. These can be pulled into the model's context window as needed.12  
* **Prompts:** Predefined templates that guide specific workflows, allowing the user or system to provide the model with structured instructions and dynamic context.12

### **Code Execution with MCP**

When combined with code execution, MCP becomes even more powerful. Instead of the agent calling MCP tools individually, it can treat MCP servers as code APIs.10 The model can browse a server's capabilities as if it were a local filesystem, reading only the tool definitions it needs and writing scripts to interact with them.10

This paradigm shift eliminates the "message loop" overhead where every intermediate result must pass through the model.10 In a workflow involving the transfer of data between two systems, such as Google Drive and Salesforce, code execution with MCP can reduce token usage by over 98%, as the raw data flows directly between the tools in the execution environment rather than through the model’s context.10 This also provides a significant security benefit, as sensitive information can be tokenized or filtered before it ever reaches the model.10

## **Memory and Persistent State**

One of the most complex aspects of building agents is the management of state. Because large language models are stateless by default, the developer must implement a memory layer that allows the agent to maintain context across conversations, learn from its past, and adapt its behavior over time.13

### **Multi-Level Memory Architecture**

Effective memory systems for agents typically involve a tiered approach:

* **Short-term Memory (Working Context):** This serves as the agent's immediate working memory, maintaining the coherence of a single interaction. It is often implemented as a message buffer that stores the most recent messages in a conversation.13  
* **Core Memory:** This consists of editable, in-context blocks that are pinned to the agent's window. These blocks might contain information about the user’s identity, current task objectives, or the agent’s own persona.15  
* **Long-term Memory:** This allows the agent to store and recall information across different sessions. It is typically divided into episodic memory (specific past events), semantic memory (general knowledge and facts), and procedural memory (learned skills and operational knowledge).13

Long-term memory is often implemented using vector databases for semantic search or relational/graph databases for structured knowledge.13 When the agent needs specific context, it uses a retrieval mechanism to pull relevant information from these stores back into its active context window.13

### **Context Engineering and Summarization**

As the interaction history grows, the system must employ context engineering to prevent bloat. This involves treating the context as a compiled view over a richer, stateful system.17 Strategies include:

* **Summarization:** Periodically condensing the conversation history into a concise summary using a model call. This preserves the essential details while removing redundant tokens.13  
* **Eviction:** Pruning or de-prioritizing older or less relevant events based on deterministic rules or model-based analysis.15  
* **Filtering:** Pruning framework noise or irrelevant tool outputs before they reach the model.17

| Memory Type | Implementation Strategy | Scaling Strategy |
| :---- | :---- | :---- |
| **Episodic** | Log key events, actions, and outcomes in a database. | Use vector search for semantic retrieval of past events. |
| **Semantic** | Store general knowledge and rules in a knowledge base or graph. | Use RAG techniques to pull relevant facts on demand. |
| **Procedural** | Record successful action sequences and refine them over time. | Store optimized scripts or prompt templates as "skills." |
| **Working** | Use a sliding window or checkpointer for recent messages. | Use recursive summarization to maintain a "gist" of the dialogue. |

13

Context caching is another critical engineering tool. By caching stable prefixes of the context window—such as system instructions and long-lived summaries—developers can significantly reduce latency and cost for subsequent model calls.17 This allows agents to operate efficiently even with very large system prompts or extensive task histories.19

## **Evaluation and Reliability in Agentic Systems**

Evaluating AI agents is uniquely challenging because they are non-deterministic and operate through multi-step reasoning paths where errors can compound.20 Traditional software testing, which focuses on deterministic code paths, must be supplemented with specialized agentic evaluation frameworks.

### **Trajectory vs. Outcome Evaluation**

A central distinction in agent evaluation is between the transcript (or trajectory) and the outcome. The transcript is the complete record of everything the agent did: its reasoning steps, tool calls, and intermediate results.20 The outcome is the final state of the environment at the end of the trial.20

Engineering best practices suggest that while the outcome is the primary measure of success, the trajectory is essential for ensuring that the agent followed necessary logic and did not arrive at the right answer through luck or hallucination.20 For example, a travel agent might successfully book a flight (outcome), but if it did so without first checking the user's passport expiration date (trajectory), it has failed its business logic.22

### **A Taxonomy of Graders**

A robust evaluation framework utilizes a combination of three grader types:

1. **Code-based (Deterministic) Graders:** These are fast, cheap, and objective. They are best suited for verifying fixed answers, tool call parameters, or environmental states (e.g., "does a file exist at this path?").20  
2. **Model-based Graders (LLM-as-judge):** These use a high-capability model to evaluate open-ended responses against a scoring rubric. They are flexible and can assess nuanced qualities like factuality, logical coherence, and tone.20  
3. **Human Graders:** These remain the "gold standard" for high-stakes, ethical, or safety-critical tasks. Human review is also essential for calibrating model-based judges and defining the success criteria for complex benchmarks.20

| Metric Category | Metric Name | Definition | Importance |
| :---- | :---- | :---- | :---- |
| **Performance** | Success Rate | The proportion of tasks completed satisfactorily. | Primary measure of end-to-end capability. |
| **Efficiency** | Convergence Score | Whether the task was achieved in an efficient number of steps. | Measures how well the agent plans and avoids thrashing. |
| **Accuracy** | Parameter Accuracy | Whether the agent provided the correct inputs to tool calls. | Critical for the reliability of API and system interactions. |
| **Reliability** | Logic Consistency | Whether the agent followed the required sequence of operations. | Ensures compliance with business and safety rules. |

21

The convergence score is particularly important for agents. It measures whether the agent achieved its goal within an acceptable and efficient number of steps, allowing developers to diagnose inefficiencies where the agent might be stuck in a loop or re-running unnecessary commands.23

### **The Development Lifecycle**

Agent development should be a cyclical process driven by evaluations. This "eval-driven development" (EDD) involves building a set of test cases—a "golden dataset"—early in the development cycle.20 This dataset should include realistic scenarios and edge cases drawn from actual agent traces.

As the system evolves, every modification should be followed by a run against the eval suite. Capability evals target tasks the agent currently struggles with, measuring its growth, while regression evals ensure that new features do not break existing functionality.20 This structured approach transforms agent development from a process of "vibe-based" trial and error into a rigorous engineering discipline.24

## **Prompt Engineering for Agents**

The way we prompt models for agentic tasks has evolved alongside the models' capabilities. In production environments, prompts must be clear, specific, and designed to guide the model through complex reasoning without introducing unnecessary noise.

### **Steering and Formatting**

One of the most effective ways to steer a model's output is through the use of XML tags. This allows for clear separation of instructions, context, and the desired output format.1 For example, a model can be instructed to provide its reasoning within \<thinking\> tags and its final tool call within \<tool\_call\> tags, making it easy for the system to parse and process the response.1

Instructions should be prescriptive about the starting state and the definition of done.27 For long-running tasks, the model should be encouraged to plan its work systematically and save progress to memory before the context window refreshes.28 It is also essential to match the prompt style to the desired output style; for instance, removing markdown from a prompt can reduce the volume of markdown in the model's response.28

### **Adjusting for Advanced Capabilities**

As models become more proactive, many traditional prompt engineering techniques have become counterproductive. Instructions like "be thorough," "think carefully," or "do not be lazy" were common workarounds for earlier models but can cause runaway thinking or excessive verbosity in modern, agent-optimized models.28

| Prompting Best Practice | Strategy for Agents | Reason for Efficacy |
| :---- | :---- | :---- |
| **Prescriptive Action** | Tell the model *what* to do instead of *what not* to do. | Direct guidance is easier for the model to follow than constraints. |
| **XML Indicators** | Wrap different components of the response in specific tags. | Enables reliable programmatic parsing of structured outputs. |
| **Effort Control** | Use system-level effort settings rather than prompt constraints. | Adjusts the model's proactivity without cluttering the context window. |
| **ACI-Specific Language** | "Use \[tool\] when it would enhance your understanding." | Allows the model to use its judgment rather than forced invocation. |

28

Instead, developers should use "effort" as the primary control lever, lowering the model's proactivity if it becomes overly aggressive.28 Language regarding tool use should also be softened, moving from "You must use \[tool\]" to "Use \[tool\] when it would enhance your understanding of the problem," allowing the model's reasoning to dictate its actions.28

## **The Future of the Agentic Paradigm**

The ultimate goal of agentic engineering is to move from the model as a "tool" to the model as a "worker".2 This transition is being realized through the integration of agentic capabilities directly into enterprise workflows. Organizations are now deploying agentic AI to handle multi-step processes like processing insurance claims, conducting compliance reviews, and managing complex network operations in the telecommunications sector.30

The economic and labor implications of this shift are profound. As agents become capable of performing nearly any computer-based task—from software engineering to product management and design—the very definition of work is being transformed.32 This disruption necessitates a focus on building resilient, responsible systems that prioritize transparency and human-in-the-loop oversight for high-stakes decisions.26

The path forward lies in creating systems that are not just capable but also verifiable and maintainable. By grounding agent design in simple patterns, robust ACIs, and rigorous evaluation frameworks, we can harness the full potential of large language models to solve real-world problems in ways that were previously impossible. The engineering of agents is a journey toward the creation of truly intelligent, connected, and autonomous digital systems.

#### **Works cited**

1. Building Effective AI Agents \\ Anthropic, accessed February 22, 2026, [https://www.anthropic.com/research/building-effective-agents](https://www.anthropic.com/research/building-effective-agents)  
2. I just read Anthropic's blog on 'Building effective agents', I love it. Is, accessed February 22, 2026, [https://www.reddit.com/r/ClaudeAI/comments/1hiww4y/i\_just\_read\_anthropics\_blog\_on\_building\_effective/](https://www.reddit.com/r/ClaudeAI/comments/1hiww4y/i_just_read_anthropics_blog_on_building_effective/)  
3. Tool use (zoology) | History | Research Starters \- EBSCO, accessed February 22, 2026, [https://www.ebsco.com/research-starters/history/tool-use-zoology](https://www.ebsco.com/research-starters/history/tool-use-zoology)  
4. Tool use by non-humans \- Wikipedia, accessed February 22, 2026, [https://en.wikipedia.org/wiki/Tool\_use\_by\_non-humans](https://en.wikipedia.org/wiki/Tool_use_by_non-humans)  
5. Introducing the Model Context Protocol \- Anthropic, accessed February 22, 2026, [https://www.anthropic.com/news/model-context-protocol](https://www.anthropic.com/news/model-context-protocol)  
6. Building Effective Agents with Spring AI (Part 1), accessed February 22, 2026, [https://spring.io/blog/2025/01/21/spring-ai-agentic-patterns/](https://spring.io/blog/2025/01/21/spring-ai-agentic-patterns/)  
7. Building Effective Agents :: Spring AI Reference, accessed February 22, 2026, [https://docs.spring.io/spring-ai/reference/api/effective-agents.html](https://docs.spring.io/spring-ai/reference/api/effective-agents.html)  
8. Introducing advanced tool use on the Claude Developer ... \- Anthropic, accessed February 22, 2026, [https://www.anthropic.com/engineering/advanced-tool-use](https://www.anthropic.com/engineering/advanced-tool-use)  
9. Function Calling \- Hugging Face, accessed February 22, 2026, [https://huggingface.co/docs/hugs/guides/function-calling](https://huggingface.co/docs/hugs/guides/function-calling)  
10. Code execution with MCP: building more efficient AI agents \\ Anthropic, accessed February 22, 2026, [https://www.anthropic.com/engineering/code-execution-with-mcp](https://www.anthropic.com/engineering/code-execution-with-mcp)  
11. Model Context Protocol, accessed February 22, 2026, [https://modelcontextprotocol.io/](https://modelcontextprotocol.io/)  
12. Getting Started With MCP Servers: A Technical Deep Dive \- Neo4j, accessed February 22, 2026, [https://neo4j.com/blog/developer/model-context-protocol/](https://neo4j.com/blog/developer/model-context-protocol/)  
13. How to Build AI Agents with Redis Memory Management, accessed February 22, 2026, [https://redis.io/blog/build-smarter-ai-agents-manage-short-term-and-long-term-memory-with-redis/](https://redis.io/blog/build-smarter-ai-agents-manage-short-term-and-long-term-memory-with-redis/)  
14. What Is AI Agent Memory? | IBM, accessed February 22, 2026, [https://www.ibm.com/think/topics/ai-agent-memory](https://www.ibm.com/think/topics/ai-agent-memory)  
15. Agent Memory: How to Build Agents that Learn and Remember \- Letta, accessed February 22, 2026, [https://www.letta.com/blog/agent-memory](https://www.letta.com/blog/agent-memory)  
16. Memory and state in AI agents \- Medium, accessed February 22, 2026, [https://medium.com/motleycrew-ai/memory-and-state-in-ai-agents-39a064ebc2b3](https://medium.com/motleycrew-ai/memory-and-state-in-ai-agents-39a064ebc2b3)  
17. Architecting efficient context-aware multi-agent framework for, accessed February 22, 2026, [https://developers.googleblog.com/architecting-efficient-context-aware-multi-agent-framework-for-production/](https://developers.googleblog.com/architecting-efficient-context-aware-multi-agent-framework-for-production/)  
18. Context caching \- Agent Development Kit (ADK) \- Google, accessed February 22, 2026, [https://google.github.io/adk-docs/context/caching/](https://google.github.io/adk-docs/context/caching/)  
19. Context caching | Gemini API | Google AI for Developers, accessed February 22, 2026, [https://ai.google.dev/gemini-api/docs/caching](https://ai.google.dev/gemini-api/docs/caching)  
20. Demystifying evals for AI agents \\ Anthropic, accessed February 22, 2026, [https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents)  
21. What is AI Agent Evaluation? | IBM, accessed February 22, 2026, [https://www.ibm.com/think/topics/ai-agent-evaluation](https://www.ibm.com/think/topics/ai-agent-evaluation)  
22. Evaluating Agents with ADK \- Google Codelabs, accessed February 22, 2026, [https://codelabs.developers.google.com/adk-eval/instructions](https://codelabs.developers.google.com/adk-eval/instructions)  
23. Evaluating AI Agents in 2025: A Practical Guide \- Turing College, accessed February 22, 2026, [https://www.turingcollege.com/blog/evaluating-ai-agents-practical-guide](https://www.turingcollege.com/blog/evaluating-ai-agents-practical-guide)  
24. Agent Evaluation \- Arize AI, accessed February 22, 2026, [https://arize.com/ai-agents/agent-evaluation/](https://arize.com/ai-agents/agent-evaluation/)  
25. Evaluating AI Agents \- DeepLearning.AI, accessed February 22, 2026, [https://www.deeplearning.ai/short-courses/evaluating-ai-agents/](https://www.deeplearning.ai/short-courses/evaluating-ai-agents/)  
26. AI Evaluations 101: Testing LLMs, Agents, and Everything in Between, accessed February 22, 2026, [https://www.domo.com/blog/ai-evaluations-101-testing-llms-agents-and-everything-in-between](https://www.domo.com/blog/ai-evaluations-101-testing-llms-agents-and-everything-in-between)  
27. Testing Agent Skills Systematically with Evals \- OpenAI for developers, accessed February 22, 2026, [https://developers.openai.com/blog/eval-skills/](https://developers.openai.com/blog/eval-skills/)  
28. Prompting best practices \- Claude API Docs, accessed February 22, 2026, [https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices)  
29. Ultimate Guide to Prompt Engineering | by Sunil Rao \- Towards AI, accessed February 22, 2026, [https://pub.towardsai.net/ultimate-guide-to-prompt-engineering-940d463ba0e5](https://pub.towardsai.net/ultimate-guide-to-prompt-engineering-940d463ba0e5)  
30. Infosys and Anthropic Announce Collaboration to Unlock AI Value across Complex, Regulated Industries, accessed February 22, 2026, [https://www.aninews.in/news/business/infosys-and-anthropic-announce-collaboration-to-unlock-ai-value-across-complex-regulated-industries20260217104134](https://www.aninews.in/news/business/infosys-and-anthropic-announce-collaboration-to-unlock-ai-value-across-complex-regulated-industries20260217104134)  
31. Infosys, Anthropic collaborate to roll out agentic AI for regulated industries, accessed February 22, 2026, [https://www.indiatoday.in/technology/news/story/infosys-anthropic-collaborate-to-roll-out-agentic-ai-for-regulated-industries-2869528-2026-02-17](https://www.indiatoday.in/technology/news/story/infosys-anthropic-collaborate-to-roll-out-agentic-ai-for-regulated-industries-2869528-2026-02-17)  
32. Anthropic’s lead engineer has a Doomsday prediction for engineers, product managers and designers; says: AI Agents are going to expand to any kind of work that you, accessed February 22, 2026, [https://timesofindia.indiatimes.com/technology/tech-news/anthropics-lead-engineer-has-a-doomsday-prediction-for-engineers-product-managers-and-designers-says-ai-agents-are-going-to-expand-to-any-kind-of-work-that-you-/articleshow/128678080.cms](https://timesofindia.indiatimes.com/technology/tech-news/anthropics-lead-engineer-has-a-doomsday-prediction-for-engineers-product-managers-and-designers-says-ai-agents-are-going-to-expand-to-any-kind-of-work-that-you-/articleshow/128678080.cms)