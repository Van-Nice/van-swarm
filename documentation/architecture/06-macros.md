# Macros crate (rustmastra-macros)

Procedural macros for the RustMastra framework: **`#[tool]`** and **`#[workflow]`**.

## Crate role

```mermaid
flowchart LR
    User[User code]
    tool_attr["#[tool]"]
    workflow_attr["#[workflow]"]
    core[rustmastra-core]

    User --> tool_attr
    User --> workflow_attr
    tool_attr --> core
    workflow_attr --> core
```

- Macros live in **rustmastra-macros** and depend on **rustmastra-core** for types referenced in error messages and validation (e.g. `DurableContext`).

## #[tool]

```mermaid
flowchart LR
    A["async fn my_tool(…)"]
    B["#[tool]"]
    C[Current: pass-through]
    D[Planned: schema, validation, wrapper]

    A --> B
    B --> C
    C -.-> D
```

- **Current behaviour**: Pass-through; the attribute does not change the function body.
- **Planned** (checklist §10): Derive JSON schema from parameter types (e.g. via schemars), extract description from Rustdoc, generate type-safe wrapper that deserializes model output and returns structured errors so the agent can self-correct.

## #[workflow]

```mermaid
flowchart TB
    A["async fn my_workflow(ctx: Arc<DurableContext>, …)"]
    B["#[workflow]"]
    C[Parse first parameter]
    D{First param type}
    E[Contains DurableContext?]
    F[Yes: accept]
    G[No: compile error]

    A --> B
    B --> C
    C --> D
    D --> E
    E -->|Yes| F
    E -->|No| G
```

- **Current behaviour**: Validates that the first parameter is `ctx: Arc<DurableContext>` (or equivalent path). If not, emits a compile error. Body is passed through (no transformation).
- **Intent**: Mark durable workflows; checkpoints are provided by `ctx.call_tool`, `ctx.sleep`, `ctx.run_once`. On recovery the function is re-run from the start and the journal replays side effects.

## Signature validation (workflow)

```mermaid
flowchart LR
    FirstParam[First parameter]
    Typed[FnArg::Typed]
    Type[Type]
    Path[TypePath]
    Arc[Arc]
    DurableContext[DurableContext]

    FirstParam --> Typed
    Typed --> Type
    Type --> Path
    Type --> Group
    Path --> segment["last segment = DurableContext"]
    Path --> AngleBracketed["AngleBracketed args"]
    AngleBracketed --> Arc
    Arc --> DurableContext
```

- The macro uses `syn` to inspect the first parameter; it accepts types whose path ends in `DurableContext` or that contain such a type (e.g. `Arc<DurableContext>`). Other types produce a clear compile error.
