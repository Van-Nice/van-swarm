//! # rustmastra-macros
//!
//! Procedural macros for the RustMastra framework (checklist §10, §3.10).
//!
//! ## `#[tool]`  (checklist §10.1–10.5)
//! Annotate an async function with `#[tool]` to automatically:
//! * Derive a JSON schema from the function's parameter types (via `schemars`).
//! * Extract the tool description from the Rustdoc comment.
//! * Generate a type-safe wrapper that deserialises the model's output.
//!
//! ## `#[workflow]`  (checklist §3.10–3.12)
//! Transform an `async fn` decorated with `#[workflow]` into a durable state
//! machine that injects journal checkpoints at every `.await` point.
//!
//! Both macros are stub pass-throughs in this initial build; full
//! implementation comes in §3 and §10 of the checklist.

extern crate proc_macro;

use proc_macro::TokenStream;

/// Mark an async function as a framework tool.
///
/// Full behaviour (schema generation, validation wrapper) coming in §10.
#[proc_macro_attribute]
pub fn tool(_args: TokenStream, input: TokenStream) -> TokenStream {
    // Stub: pass the function through unchanged; full implementation §10.
    input
}

/// Mark an async function as a durable workflow.
///
/// Full behaviour (state-machine transformation, journal checkpoints)
/// coming in §3.
#[proc_macro_attribute]
pub fn workflow(_args: TokenStream, input: TokenStream) -> TokenStream {
    // Stub: pass the function through unchanged; full implementation §3.
    input
}
