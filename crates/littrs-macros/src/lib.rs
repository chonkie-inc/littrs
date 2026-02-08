//! Procedural macros for Littrs sandbox.
//!
//! This crate provides the `#[tool]` attribute macro for defining tools
//! with automatic type conversion and documentation generation.
//!
//! # Example
//!
//! ```ignore
//! use littrs_macros::tool;
//! use littrs::PyValue;
//!
//! /// Get current weather for a city.
//! ///
//! /// Args:
//! ///     city: The city name to look up
//! ///     unit: Temperature unit (celsius or fahrenheit)
//! #[tool]
//! fn fetch_weather(city: String, unit: Option<String>) -> PyValue {
//!     PyValue::Dict(vec![
//!         (PyValue::Str("city".to_string()), PyValue::Str(city)),
//!         (PyValue::Str("temp".to_string()), PyValue::Int(22)),
//!     ])
//! }
//!
//! // Register with sandbox
//! sandbox.register(fetch_weather::Tool);
//! ```

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use std::collections::HashMap;
use syn::{
    Attribute, Expr, FnArg, ItemFn, Lit, LitStr, Meta, Pat, PatType, ReturnType, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

/// Parsed arguments for the #[tool(...)] attribute
struct ToolArgs {
    description: Option<String>,
}

impl Parse for ToolArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut description = None;

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            if ident == "description" {
                let lit: LitStr = input.parse()?;
                description = Some(lit.value());
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(ToolArgs { description })
    }
}

/// Parsed documentation from doc comments
struct ParsedDocs {
    /// The main description (first paragraph)
    description: String,
    /// Argument descriptions from the Args: section
    args: HashMap<String, String>,
}

/// Parse doc comments to extract description and argument descriptions.
///
/// Expected format:
/// ```text
/// /// Main description here.
/// /// Can span multiple lines.
/// ///
/// /// Args:
/// ///     param1: Description of param1
/// ///     param2: Description of param2
/// ```
fn parse_doc_comments(attrs: &[Attribute]) -> ParsedDocs {
    let mut lines: Vec<String> = Vec::new();

    // Extract all doc comment lines
    for attr in attrs {
        if attr.path().is_ident("doc")
            && let Meta::NameValue(meta) = &attr.meta
            && let Expr::Lit(expr_lit) = &meta.value
            && let Lit::Str(lit_str) = &expr_lit.lit
        {
            lines.push(lit_str.value());
        }
    }

    let mut description_lines: Vec<String> = Vec::new();
    let mut args: HashMap<String, String> = HashMap::new();
    let mut in_args_section = false;
    let mut current_arg: Option<(String, String)> = None;

    for line in lines {
        let trimmed = line.trim();

        // Check for Args: section header
        if trimmed == "Args:" || trimmed == "Arguments:" {
            in_args_section = true;
            // Save any pending arg
            if let Some((name, desc)) = current_arg.take() {
                args.insert(name, desc.trim().to_string());
            }
            continue;
        }

        if in_args_section {
            // Check if this line starts a new argument (has a colon after the name)
            if let Some(colon_pos) = trimmed.find(':') {
                let potential_name = trimmed[..colon_pos].trim();
                // Valid arg names are single words without spaces
                if !potential_name.is_empty() && !potential_name.contains(' ') {
                    // Save previous arg if any
                    if let Some((name, desc)) = current_arg.take() {
                        args.insert(name, desc.trim().to_string());
                    }
                    // Start new arg
                    let desc = trimmed[colon_pos + 1..].trim().to_string();
                    current_arg = Some((potential_name.to_string(), desc));
                    continue;
                }
            }

            // Continuation of previous arg description
            if let Some((_, ref mut desc)) = current_arg
                && !trimmed.is_empty()
            {
                desc.push(' ');
                desc.push_str(trimmed);
            }
        } else {
            // Before Args section - this is part of the description
            if trimmed.is_empty() {
                // Empty line might separate paragraphs, but we take the whole thing
                if !description_lines.is_empty() {
                    description_lines.push(String::new());
                }
            } else {
                description_lines.push(trimmed.to_string());
            }
        }
    }

    // Save final arg if any
    if let Some((name, desc)) = current_arg {
        args.insert(name, desc.trim().to_string());
    }

    // Join description, collapsing multiple empty lines
    let description = description_lines
        .into_iter()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    ParsedDocs { description, args }
}

/// Map a Rust type to a Python type string.
fn rust_type_to_python(ty: &Type) -> String {
    let ty_str = quote!(#ty).to_string().replace(" ", "");

    match ty_str.as_str() {
        "String" | "&str" => "str".to_string(),
        "i64" | "i32" | "i16" | "i8" | "isize" => "int".to_string(),
        "u64" | "u32" | "u16" | "u8" | "usize" => "int".to_string(),
        "f64" | "f32" => "float".to_string(),
        "bool" => "bool".to_string(),
        "()" => "None".to_string(),
        _ if ty_str.starts_with("Option<") => {
            // Extract inner type
            let inner = &ty_str[7..ty_str.len() - 1];
            let inner_py = match inner {
                "String" | "&str" => "str",
                "i64" | "i32" | "i16" | "i8" | "isize" => "int",
                "u64" | "u32" | "u16" | "u8" | "usize" => "int",
                "f64" | "f32" => "float",
                "bool" => "bool",
                _ => "any",
            };
            inner_py.to_string()
        }
        _ if ty_str.starts_with("Vec<") => {
            let inner = &ty_str[4..ty_str.len() - 1];
            let inner_py = match inner {
                "String" | "&str" => "str",
                "i64" | "i32" | "i16" | "i8" | "isize" => "int",
                "u64" | "u32" | "u16" | "u8" | "usize" => "int",
                "f64" | "f32" => "float",
                "bool" => "bool",
                _ => "any",
            };
            format!("list[{}]", inner_py)
        }
        _ if ty_str.starts_with("HashMap<") || ty_str.starts_with("std::collections::HashMap<") => {
            "dict".to_string()
        }
        "PyValue" => "any".to_string(),
        _ => "any".to_string(),
    }
}

/// Check if a type is Option<T>
fn is_option_type(ty: &Type) -> bool {
    let ty_str = quote!(#ty).to_string().replace(" ", "");
    ty_str.starts_with("Option<")
}

/// The `#[tool]` attribute macro for defining sandbox tools.
///
/// This macro transforms a function into a tool that can be registered with
/// a Littrs sandbox, with automatic:
/// - Type conversion from PyValue using `FromPyValue`
/// - Error handling for type mismatches
/// - Documentation generation for LLM system prompts
///
/// # Documentation Format
///
/// Use standard Rust doc comments with an optional `Args:` section:
///
/// ```ignore
/// /// Add two numbers together.
/// ///
/// /// Args:
/// ///     a: First number
/// ///     b: Second number
/// #[tool]
/// fn add(a: i64, b: i64) -> i64 {
///     a + b
/// }
/// ```
///
/// # Generated Code
///
/// The macro generates a module containing:
/// - `INFO`: Static `ToolInfo` with metadata
/// - `call`: Function `fn(Vec<PyValue>) -> PyValue`
/// - `Tool`: Unit struct implementing `littrs::Tool` trait
///
/// # Registration
///
/// ```ignore
/// // Ergonomic (using Tool struct)
/// sandbox.register(add::Tool);
///
/// // Explicit
/// sandbox.register_tool(add::INFO.clone(), add::call);
/// ```
#[proc_macro_attribute]
pub fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as ToolArgs);
    let input_fn = parse_macro_input!(item as ItemFn);

    // Parse doc comments for description and arg descriptions
    let parsed_docs = parse_doc_comments(&input_fn.attrs);

    // Use explicit description if provided, otherwise use doc comment
    let description = args
        .description
        .unwrap_or_else(|| parsed_docs.description.clone());

    let fn_name = &input_fn.sig.ident;
    let fn_name_str = fn_name.to_string();
    let mod_name = format_ident!("{}", fn_name);

    // Extract arguments
    let mut arg_infos = Vec::new();
    let mut arg_names = Vec::new();
    let mut arg_conversions = Vec::new();

    for (i, arg) in input_fn.sig.inputs.iter().enumerate() {
        if let FnArg::Typed(PatType { pat, ty, .. }) = arg
            && let Pat::Ident(pat_ident) = pat.as_ref()
        {
            let arg_name = &pat_ident.ident;
            let arg_name_str = arg_name.to_string();
            let python_type = rust_type_to_python(ty);
            let is_optional = is_option_type(ty);
            // Get arg description from parsed docs
            let doc = parsed_docs
                .args
                .get(&arg_name_str)
                .cloned()
                .unwrap_or_default();

            arg_names.push(arg_name.clone());

            // Generate ToolInfo arg
            if is_optional {
                arg_infos.push(quote! {
                    .arg_optional(#arg_name_str, #python_type, #doc)
                });
            } else {
                arg_infos.push(quote! {
                    .arg_required(#arg_name_str, #python_type, #doc)
                });
            }

            // Generate argument conversion
            let idx = i;
            if is_optional {
                arg_conversions.push(quote! {
                    let #arg_name: #ty = match args.get(#idx) {
                        Some(v) => <#ty as littrs::FromPyValue>::from_py_value(v)
                            .map_err(|e| littrs::ToolCallError::type_error(#arg_name_str, e))?,
                        None => None,
                    };
                });
            } else {
                arg_conversions.push(quote! {
                    let #arg_name: #ty = match args.get(#idx) {
                        Some(v) => <#ty as littrs::FromPyValue>::from_py_value(v)
                            .map_err(|e| littrs::ToolCallError::type_error(#arg_name_str, e))?,
                        None => return Err(littrs::ToolCallError::missing_argument(#arg_name_str)),
                    };
                });
            }
        }
    }

    // Extract return type
    let return_python_type = match &input_fn.sig.output {
        ReturnType::Default => "None".to_string(),
        ReturnType::Type(_, ty) => rust_type_to_python(ty),
    };

    // Get the original function body and signature (without #[arg] attributes)
    let fn_vis = &input_fn.vis;
    let fn_block = &input_fn.block;
    let fn_output = &input_fn.sig.output;

    // Create function inputs for the implementation
    let clean_inputs: Vec<_> = input_fn
        .sig
        .inputs
        .iter()
        .map(|arg| {
            if let FnArg::Typed(pat_type) = arg {
                let pat = &pat_type.pat;
                let ty = &pat_type.ty;
                quote! { #pat: #ty }
            } else {
                quote! { #arg }
            }
        })
        .collect();

    let expanded = quote! {
        #fn_vis mod #mod_name {
            use super::*;

            /// Tool metadata for registration
            pub static INFO: std::sync::LazyLock<littrs::ToolInfo> = std::sync::LazyLock::new(|| {
                littrs::ToolInfo::new(#fn_name_str, #description)
                    #(#arg_infos)*
                    .returns(#return_python_type)
            });

            /// The actual implementation
            fn implementation(#(#clean_inputs),*) #fn_output #fn_block

            /// Wrapper that converts PyValue args and handles errors
            pub fn call(args: Vec<littrs::PyValue>) -> littrs::PyValue {
                match try_call(args) {
                    Ok(v) => v,
                    Err(e) => {
                        // Return error as a dict with error info
                        littrs::PyValue::Dict(vec![
                            (littrs::PyValue::Str("error".to_string()), littrs::PyValue::Str(e.to_string())),
                        ])
                    }
                }
            }

            fn try_call(args: Vec<littrs::PyValue>) -> Result<littrs::PyValue, littrs::ToolCallError> {
                #(#arg_conversions)*

                let result = implementation(#(#arg_names),*);
                Ok(result.into())
            }

            /// Unit struct for ergonomic Tool trait registration.
            ///
            /// Use with `sandbox.register(add::Tool)` for ergonomic registration,
            /// or use `sandbox.register_tool(add::INFO.clone(), add::call)` for explicit registration.
            pub struct Tool;

            impl littrs::Tool for Tool {
                fn info() -> &'static littrs::ToolInfo {
                    &*INFO
                }

                fn call(args: Vec<littrs::PyValue>) -> littrs::PyValue {
                    call(args)
                }
            }
        }
    };

    TokenStream::from(expanded)
}
