//! # rustmastra-macros
//!
//! Procedural macros for the RustMastra framework (checklist §10, §3.10).
//!
//! ## `#[tool]`  (checklist §10.1–10.5, §10.9)
//! Annotate an async function with `#[tool]` to automatically:
//! * Derive a JSON schema from the function's parameter types (via `schemars`).
//! * Extract the tool description from the Rustdoc comment.
//! * Generate a type-safe wrapper that deserialises the model's output.
//! * Optionally add tool-use examples: `#[tool(example(description = "...", input = r#"{}"#, output = r#""""#))]`.
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
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, FnArg, ItemFn, LitStr, Pat, Token};

/// Mark an async function as a framework tool (§10.2–10.5, §10.9).
///
/// Derives JSON schema from the function's parameter types (schemars),
/// uses the function's Rustdoc as the tool description, and generates
/// a type-safe wrapper that deserializes the model's JSON and returns
/// validation errors so the model can self-correct.
///
/// Optional: `example(description = "...", input = "{...}", output = "...")` —
/// one or more tool-use examples shown to the model (§10.9).
///
/// The crate using this macro must depend on `rustmastra-core` and `schemars`.
#[proc_macro_attribute]
pub fn tool(args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as ItemFn);
    let tool_args = if args.is_empty() {
        ToolMacroArgs::default()
    } else {
        match syn::parse2::<ToolMacroArgs>(args.into()) {
            Ok(a) => a,
            Err(e) => return e.to_compile_error().into(),
        }
    };

    match expand_tool(item, &tool_args) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Parsed `#[tool(example(...), ...)]` arguments.
struct ToolMacroArgs {
    examples: Vec<ToolExampleAttr>,
}

impl Default for ToolMacroArgs {
    fn default() -> Self {
        Self {
            examples: Vec::new(),
        }
    }
}

struct ToolExampleAttr {
    description: LitStr,
    input: LitStr,
    output: LitStr,
}

impl Parse for ToolMacroArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut examples = Vec::new();
        while !input.is_empty() {
            let _: syn::Ident = input.parse()?;
            let content;
            syn::parenthesized!(content in input);
            let attr: ToolExampleAttr = content.parse()?;
            examples.push(attr);
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(ToolMacroArgs { examples })
    }
}

impl Parse for ToolExampleAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let k_desc: syn::Ident = input.parse()?;
        if k_desc != "description" {
            return Err(syn::Error::new(k_desc.span(), "expected `description`"));
        }
        input.parse::<Token![=]>()?;
        let description: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;
        let k_in: syn::Ident = input.parse()?;
        if k_in != "input" {
            return Err(syn::Error::new(k_in.span(), "expected `input`"));
        }
        input.parse::<Token![=]>()?;
        let input_lit: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;
        let k_out: syn::Ident = input.parse()?;
        if k_out != "output" {
            return Err(syn::Error::new(k_out.span(), "expected `output`"));
        }
        input.parse::<Token![=]>()?;
        let output: LitStr = input.parse()?;
        Ok(ToolExampleAttr {
            description,
            input: input_lit,
            output,
        })
    }
}

fn expand_tool(mut item: ItemFn, tool_args: &ToolMacroArgs) -> Result<proc_macro2::TokenStream, syn::Error> {
    let fn_name = &item.sig.ident;
    let tool_name = fn_name.to_string();
    let description = extract_doc_description(&item.attrs).unwrap_or_else(|| tool_name.clone());

    // Collect (param_name, param_type) for non-receiver args
    let mut params: Vec<(syn::Ident, syn::Type)> = Vec::new();
    for arg in &item.sig.inputs {
        let (pat, ty) = match arg {
            FnArg::Receiver(_) => {
                return Err(syn::Error::new_spanned(
                    arg,
                    "#[tool] does not support methods (self); use a free function",
                ));
            }
            FnArg::Typed(typed) => (&typed.pat, &typed.ty),
        };
        let ident = match pat.as_ref() {
            Pat::Ident(pi) => pi.ident.clone(),
            _ => {
                return Err(syn::Error::new_spanned(
                    pat,
                    "#[tool] parameter must be a simple identifier",
                ));
            }
        };
        params.push((ident, ty.as_ref().clone()));
    }

    // Names for generated types: FetchOrderHistory -> FetchOrderHistoryParams, FetchOrderHistoryTool
    let pascal = snake_to_pascal(&tool_name);
    let params_struct_name = syn::Ident::new(&format!("{pascal}Params"), fn_name.span());
    let tool_struct_name = syn::Ident::new(&format!("{pascal}Tool"), fn_name.span());

    // Build the params struct fields
    let params_fields = params.iter().map(|(ident, ty)| {
        quote::quote! { pub #ident: #ty }
    });
    let params_struct = quote::quote! {
        #[derive(::serde::Deserialize, ::schemars::JsonSchema)]
        struct #params_struct_name {
            #(#params_fields),*
        }
    };

    // Call site: original_fn(param1, param2, ...)
    let call_args = params.iter().map(|(ident, _)| {
        quote::quote! { params.#ident }
    });

    // Remove the #[tool] attribute so we don't recurse
    item.attrs.retain(|a| !a.path().is_ident("tool"));

    let examples_tokens = tool_args.examples.iter().map(|ex| {
        let desc = &ex.description;
        let input_lit = &ex.input;
        let output_lit = &ex.output;
        quote::quote! {
            ::rustmastra_core::ToolExample {
                description: (#desc).to_string(),
                input: ::serde_json::from_str(#input_lit).unwrap_or_default(),
                output: ::serde_json::from_str(#output_lit).unwrap_or_default(),
            }
        }
    });
    let examples_vec = quote::quote! { vec![ #(#examples_tokens),* ] };

    let expanded = quote::quote! {
        #item

        #params_struct

        struct #tool_struct_name;

        #[::async_trait::async_trait]
        impl ::rustmastra_core::Tool for #tool_struct_name {
            fn definition(&self) -> ::rustmastra_core::ToolDefinition {
                let root = ::schemars::schema_for!(#params_struct_name);
                let parameters = ::serde_json::to_value(&root.schema)
                    .unwrap_or(::serde_json::json!({"type": "object", "properties": {}}));
                ::rustmastra_core::ToolDefinition {
                    name: #tool_name.to_string(),
                    description: #description.to_string(),
                    parameters,
                    examples: #examples_vec,
                }
            }

            async fn execute(&self, arguments: ::serde_json::Value) -> ::rustmastra_core::Result<String> {
                let params: #params_struct_name = ::serde_json::from_value(arguments)
                    .map_err(|e| ::rustmastra_core::FrameworkError::tool_exec(#tool_name, e.to_string()))?;
                let result = #fn_name(#(#call_args),*).await;
                result.map_err(|e| ::rustmastra_core::FrameworkError::tool_exec(#tool_name, e.to_string()))
            }
        }
    };

    Ok(expanded)
}

/// Parse `= "string"` from doc attribute token stream.
struct DocLiteral(String);

impl syn::parse::Parse for DocLiteral {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let _: syn::token::Eq = input.parse()?;
        let lit: syn::LitStr = input.parse()?;
        Ok(DocLiteral(lit.value().trim().to_string()))
    }
}

fn extract_doc_description(attrs: &[syn::Attribute]) -> Option<String> {
    let doc_lines: Vec<String> = attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .filter_map(|a| {
            let doc: DocLiteral = a.parse_args().ok()?;
            let s = doc.0;
            if s.is_empty() { None } else { Some(s) }
        })
        .collect();
    if doc_lines.is_empty() {
        return None;
    }
    Some(doc_lines.join(" "))
}

fn snake_to_pascal(snake: &str) -> String {
    snake
        .split('_')
        .map(|s| {
            let mut chars = s.chars();
            let first = chars.next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
            let rest: String = chars.collect();
            format!("{first}{rest}")
        })
        .collect()
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
