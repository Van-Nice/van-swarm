# Advanced Rust Topics

This folder contains short guides to **advanced Rust concepts** used in the framework. Each doc uses **Mermaid diagrams** and plain explanations, and ends with **why it’s useful** and **when to use it**.

## Docs

| File | Topic | Why it matters here |
|------|--------|----------------------|
| [01-ownership-and-lifetimes.md](01-ownership-and-lifetimes.md) | Ownership, borrowing, lifetimes | Foundation of the memory model; no GC, no use-after-free. |
| [02-heap-stack-and-smart-pointers.md](02-heap-stack-and-smart-pointers.md) | Stack vs heap, `Box`, `Rc`, `Arc` | When to allocate, how we share providers and avoid unnecessary ref counts. |
| [03-async-rust.md](03-async-rust.md) | Futures, `Poll`, Waker, `Pin`, async/await | How the async runtime works; why we use Tokio and how durable execution fits. |
| [04-concurrency-send-sync.md](04-concurrency-send-sync.md) | `Send`, `Sync`, channels, `Mutex`/`RwLock` | Safe concurrency; no data races in safe code. |
| [05-unsafe-and-interior-mutability.md](05-unsafe-and-interior-mutability.md) | Interior mutability (`RefCell`, `Mutex`), unsafe | When we need to mutate through `&self`; when and why we avoid unsafe. |
| [06-traits-generics-zero-cost.md](06-traits-generics-zero-cost.md) | Traits, generics, static vs dynamic dispatch | How `Agent`, `Workflow`, `ModelProvider` stay zero-cost and flexible. |

## Reading order

- **Ownership and lifetimes** first — they underpin everything else.
- **Heap/stack and smart pointers** next — clarifies allocation and sharing.
- **Async** and **concurrency** — for the runtime and multi-agent design.
- **Interior mutability** and **unsafe** — when you hit borrow-checker limits or need to understand library internals.
- **Traits and generics** — for API design and performance.

## Viewing Mermaid

Mermaid diagrams render in GitHub, GitLab, VS Code (with a Mermaid extension), and Cursor.
