# Prompting & ACI best practices (§25)

Effective prompting is the highest-leverage skill in agent development. These guidelines are distilled from production experience and the Anthropic usage guidelines that inform VanSwarm's design.

---

## 1. Use XML tags for structured output (§25.1)

XML-style delimiters prevent the model from confusing instructions with content and make parsing deterministic.

**Good:**

```
Extract the action items from the meeting notes below.

<notes>
{{meeting_notes}}
</notes>

Return your answer as:
<action_items>
  <item>...</item>
</action_items>
```

**Why it works:** The model treats `<notes>` content as data, not as instructions. The structured output block is trivially parseable without regex fragility.

**In VanSwarm:** Use XML tags in `AgentConfig.system_prompt` and pass structured data in the first user message:

```rust
let config = AgentConfig {
    system_prompt: Some(
        "You extract action items from meeting notes.\n\
         Always return results inside <action_items> tags.".into()
    ),
    ..AgentConfig::default()
};
```

---

## 2. Prescriptive, not prohibitive prompts (§25.2)

Tell the model what **to do**, not only what **not to do**.

| Prohibitive (weaker)                 | Prescriptive (better)                                 |
| ------------------------------------ | ----------------------------------------------------- |
| "Do not make up facts."              | "Only state facts present in the provided documents." |
| "Don't use casual language."         | "Use formal, professional language."                  |
| "Don't add unnecessary information." | "Respond in three sentences or fewer."                |

Prohibitive prompts activate the behaviour you want to suppress. Prescriptive prompts shape the model's output space directly.

---

## 3. Effort control (§25.3)

Avoid vague effort instructions like "be thorough" or "be concise" in the **system** prompt — these are easily overridden by user messages and are inconsistently interpreted.

Instead, use **system-level constraints** that the model cannot override:

```rust
// Hard token budget
let config = ModelConfig {
    max_tokens: 512, // enforce conciseness structurally
    ..ModelConfig::default()
};

// Or: explicit length instruction in system prompt
let system = "Respond in at most 3 sentences. Never exceed this limit.";
```

For **agentic tasks**, set `max_iterations` on `AgentConfig` to control maximum effort:

```rust
let config = AgentConfig {
    max_iterations: 5, // cap exploratory tool calls
    ..AgentConfig::default()
};
```

---

## 4. Soft tool language (§25.4)

Models are sensitive to how tool use is framed. Mandatory language ("You must use the search tool") often leads to unnecessary tool calls even when the model already knows the answer.

**Preferred framing:**

```
Use the `search` tool when looking up information that may have changed after your
training cutoff, or when you need specific figures you're not confident about.
```

**Why:** This framing is permission-based ("when it would help") rather than obligation-based ("you must"). The model calls the tool when genuinely useful, reducing path length and cost.

**In system prompts:**

```rust
let system = "You have access to a search tool. \
              Use it when you need up-to-date information or specific facts. \
              Answer directly when you already know the answer.";
```

---

## 5. Match prompt style to desired output style (§25.5)

The model mirrors the style of its inputs. If your system prompt uses heavy markdown (headers, bullets, bold), the model's responses will too — even when you want plain prose.

| Desired output        | System prompt style                 |
| --------------------- | ----------------------------------- |
| Bullet-point list     | Use bullets in your prompt examples |
| Plain narrative prose | Write the system prompt as prose    |
| JSON                  | Show a JSON example in the prompt   |
| Minimal markdown      | Use minimal markdown in the prompt  |

**Anti-pattern:** Writing a richly formatted system prompt and then expecting the model to return a single unformatted string.

**Fix:** Match styles explicitly, or use the `max_tokens` cap to discourage verbose formatting.

---

## 6. Define "done" and starting state clearly (§25.6)

Long-running tasks suffer from **drift** — the model loses track of what it was asked to do or declares success prematurely.

**Best practices:**

1. **State the goal explicitly at the top** of the system prompt, not buried in a list.
2. **Define what "done" looks like**: success criteria, expected output shape, or a specific termination signal.
3. **State what the model already has**: describe the starting state so the model doesn't spend tool calls discovering it.

```rust
let system = format!(
    "Goal: produce a JSON report of all security vulnerabilities in the codebase.\n\
     You have access to: read_file, search_code, static_analysis tools.\n\
     You are done when you have populated a JSON object with keys: \
     [\"critical\", \"high\", \"medium\", \"low\"], each containing a list of findings.\n\
     Starting state: the codebase is at path /workspace/src.",
);
```

**TQGR patience check** (§11.8–11.9): VanSwarm's convergence detector monitors trajectory quality. If the model is not making progress (TQGR below epsilon for 2–3 turns), the runner forces a Final Answer. Clear "done" definitions improve TQGR precision.

---

## 7. Context caching for cost and latency (§25.7)

Large system prompts or reference documents sent on every turn are expensive. Cache stable prefixes to reduce cost and TTFT.

**Enable prompt caching on Anthropic:**

```rust
let config = AgentConfig {
    cache_system_prompt: true, // set the cache_control breakpoint
    ..AgentConfig::default()
};
```

**What to cache:**

- System instructions (static per agent session)
- Tool definitions (these are part of the system context)
- Reference documents injected at the top of context
- Few-shot examples

**What not to cache:**

- Per-request dynamic data (user query, current state)
- Timestamps or nonces (invalidates the cache)

**Cost impact:** Anthropic's prompt caching reduces input token cost by ~90% for cache hits. For a 10,000-token system prompt called 100 times, this translates to a ~10× reduction in prompt cost for that prefix.

**Gemini and OpenAI:** Both providers support context caching via their API mechanisms. VanSwarm's provider layer passes through the `cache_system_prompt` flag; provider-specific caching is controlled server-side.

---

## Summary

| Practice             | One-line rule                                               |
| -------------------- | ----------------------------------------------------------- |
| XML tags             | Delimit instructions from data; make output parseable       |
| Prescriptive prompts | Say what to do, not only what to avoid                      |
| Effort control       | Use `max_tokens` / `max_iterations` structurally            |
| Soft tool language   | "Use when helpful" not "you must use"                       |
| Style matching       | Match prompt style to desired output style                  |
| Define "done"        | State goal, success criteria, and starting state explicitly |
| Context caching      | Cache stable prefixes; never cache dynamic data             |
