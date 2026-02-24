//! # rustmastra-core
//!
//! Core traits, model providers, and ReAct loop for the RustMastra agent
//! framework.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use rustmastra_core::{
//!     config::{AgentConfig, ModelConfig, ProviderCredentials},
//!     providers::AnthropicProvider,
//!     react::{run_agent, ReActAgent},
//!     traits::tool::LocalToolRegistry,
//! };
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> rustmastra_core::Result<()> {
//!     let provider = AnthropicProvider::from_env()?;
//!     let executor = LocalToolRegistry::new();
//!     let config = AgentConfig::new("assistant", ModelConfig::new("claude-opus-4-6"))
//!         .with_system_prompt("You are a helpful assistant.");
//!
//!     let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(executor));
//!     let answer = run_agent(&agent, "Hello, world!").await?;
//!     println!("{answer}");
//!     Ok(())
//! }
//! ```
//!
//! ## Crate map
//!
//! | Module        | Purpose                                     |
//! |---------------|---------------------------------------------|
//! | `error`       | `FrameworkError` and `Result<T>` alias      |
//! | `config`      | `ModelConfig`, `AgentConfig`, credentials   |
//! | `message`     | Conversation messages, tool types, streaming|
//! | `traits`      | `Runnable`, `Agent`, `Workflow`, `Tool`     |
//! | `providers`   | `ModelProvider` + OpenAI / Anthropic / Gemini|
//! | `react`       | `ReActAgent` and `run_agent` loop           |

extern crate self as rustmastra_core;

pub mod config;
pub mod durable;
pub mod error;
pub mod message;
pub mod providers;
pub mod react;
pub mod traits;

// ── Top-level re-exports ─────────────────────────────────────────────────────

pub use error::{FrameworkError, Result};
pub use message::{
    CompletionRequest, CompletionResponse, ContentBlock, Message, ResponseStream, Role, StopReason,
    StreamChunk, ToolDefinition, ToolExample, TokenUsage,
};
pub use traits::{
    Agent, AgentAction, AgentContext, LocalToolRegistry, Runnable, Tool, ToolCall, ToolExecutor,
    Workflow, WorkflowStatus, WorkflowStep,
};
pub use providers::{ModelProvider, AnthropicProvider, GeminiProvider, OpenAiProvider};
pub use react::{ReActAgent, run_agent};
pub use config::{AgentConfig, ModelConfig, ProviderCredentials};
pub use durable::{DurableContext, FileJournal, InMemoryJournal, JournalBackend, JournalEntry, JournalKind};
