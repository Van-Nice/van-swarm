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
//! Annotate an `async fn` whose first parameter is `ctx: Arc<DurableContext>`.
//! The macro validates the signature; journal checkpoints are provided by
//! `ctx.call_tool`, `ctx.sleep`, `ctx.run_once` (see rustmastra_core::durable).
//!
//! Both macros are stub pass-throughs for body transformation; signature
//! validation and full implementation come in §3 and §10.

extern crate proc_macro;

use proc_macro::TokenStream;
use syn::{parse_macro_input, ItemFn};

/// Mark an async function as a framework tool.
///
/// Full behaviour (schema generation, validation wrapper) coming in §10.
#[proc_macro_attribute]
pub fn tool(_args: TokenStream, input: TokenStream) -> TokenStream {
    input
}

/// Mark an async function as a durable workflow (§3.11–3.12).
///
/// Validates that the function's first parameter is `ctx: Arc<DurableContext>`
/// (or equivalent path). The function is re-run from the start on recovery;
/// checkpoints are provided by `ctx.call_tool`, `ctx.sleep`, `ctx.run_once`.
#[proc_macro_attribute]
pub fn workflow(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as ItemFn);

    if let Some(ty) = item.sig.inputs.first().and_then(|arg| match arg {
        syn::FnArg::Receiver(_) => None,
        syn::FnArg::Typed(typed) => Some(typed.ty.as_ref()),
    }) {
        if !type_contains_durable_context(ty) {
            return syn::Error::new_spanned(
                ty,
                "#[workflow] requires first parameter to be ctx: Arc<DurableContext> (or Arc<rustmastra_core::DurableContext>)",
            )
            .to_compile_error()
            .into();
        }
    } else {
        return syn::Error::new_spanned(
            &item.sig,
            "#[workflow] requires at least one parameter: ctx: Arc<DurableContext>",
        )
        .to_compile_error()
        .into();
    }

    quote::quote!(#item).into()
}

fn type_contains_durable_context(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(p) => {
            path_ends_with_durable_context(&p.path)
                || p.path.segments.last().and_then(|s| {
                    use syn::GenericArgument;
                    let args = match &s.arguments {
                        syn::PathArguments::AngleBracketed(a) => &a.args,
                        _ => return None,
                    };
                    args.iter().find_map(|arg| {
                        if let GenericArgument::Type(t) = arg {
                            Some(type_contains_durable_context(t))
                        } else {
                            None
                        }
                    })
                }).unwrap_or(false)
        }
        syn::Type::Group(g) => type_contains_durable_context(&g.elem),
        _ => false,
    }
}

fn path_ends_with_durable_context(path: &syn::Path) -> bool {
    path.segments
        .last()
        .map(|s| s.ident == "DurableContext")
        .unwrap_or(false)
}
