//! Procedural macros for Litter sandbox.
//!
//! This crate provides the `#[tool]` attribute macro for defining tools
//! with automatic type conversion and documentation generation.
//!
//! # Example
//!
//! ```ignore
//! use litter_macros::tool;
//!
//! #[tool(description = "Get current weather for a city.")]
//! fn fetch_weather(
//!     /// The city name to look up
//!     city: String,
//!     /// Temperature unit (celsius or fahrenheit)
//!     unit: Option<String>,
//! ) -> PyValue {
//!     PyValue::Dict(vec![
//!         ("city".to_string(), PyValue::Str(city)),
//!         ("temp".to_string(), PyValue::Int(22)),
//!     ])
//! }
//!
//! // Register with sandbox
//! sandbox.register_tool(fetch_weather::INFO.clone(), fetch_weather::call);
//! ```

use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{
    parse_macro_input, parse::{Parse, ParseStream},
    ItemFn, FnArg, Pat, Type, Attribute, Expr, Lit, Meta,
    PatType, ReturnType, Token, LitStr,
};

/// Parsed arguments for the #[tool(...)] attribute
struct ToolArgs {
    description: String,
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

        Ok(ToolArgs {
            description: description
                .ok_or_else(|| input.error("missing required attribute: description"))?,
        })
    }
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
            let inner = &ty_str[7..ty_str.len()-1];
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
            let inner = &ty_str[4..ty_str.len()-1];
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

/// Extract doc comments from attributes
fn extract_doc_comment(attrs: &[Attribute]) -> Option<String> {
    let docs: Vec<String> = attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("doc") {
                if let Meta::NameValue(meta) = &attr.meta {
                    if let Expr::Lit(expr_lit) = &meta.value {
                        if let Lit::Str(lit_str) = &expr_lit.lit {
                            return Some(lit_str.value().trim().to_string());
                        }
                    }
                }
            }
            None
        })
        .collect();

    if docs.is_empty() {
        None
    } else {
        Some(docs.join(" "))
    }
}

/// The `#[tool]` attribute macro for defining sandbox tools.
///
/// This macro transforms a function into a tool that can be registered with
/// a Litter sandbox, with automatic:
/// - Type conversion from PyValue using `FromPyValue`
/// - Error handling for type mismatches
/// - Documentation generation for LLM system prompts
///
/// # Attributes
///
/// - `description`: Required. A description of what the tool does.
///
/// # Example
///
/// ```ignore
/// #[tool(description = "Add two numbers together.")]
/// fn add(a: i64, b: i64) -> i64 {
///     a + b
/// }
///
/// // This generates:
/// // - add::INFO: ToolInfo with the function metadata
/// // - add::call: fn(Vec<PyValue>) -> PyValue wrapper
/// //
/// // Register with:
/// sandbox.register_tool(add::INFO.clone(), add::call);
/// ```
#[proc_macro_attribute]
pub fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as ToolArgs);
    let input_fn = parse_macro_input!(item as ItemFn);

    let description = args.description;

    let fn_name = &input_fn.sig.ident;
    let fn_name_str = fn_name.to_string();
    let mod_name = format_ident!("{}", fn_name);

    // Extract arguments
    let mut arg_infos = Vec::new();
    let mut arg_names = Vec::new();
    let mut arg_types = Vec::new();
    let mut arg_conversions = Vec::new();

    for (i, arg) in input_fn.sig.inputs.iter().enumerate() {
        if let FnArg::Typed(PatType { pat, ty, attrs, .. }) = arg {
            if let Pat::Ident(pat_ident) = pat.as_ref() {
                let arg_name = &pat_ident.ident;
                let arg_name_str = arg_name.to_string();
                let python_type = rust_type_to_python(ty);
                let is_optional = is_option_type(ty);
                let doc = extract_doc_comment(attrs).unwrap_or_default();

                arg_names.push(arg_name.clone());
                arg_types.push(ty.clone());

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
                            Some(v) => <#ty as litter::FromPyValue>::from_py_value(v)
                                .map_err(|e| litter::ToolCallError::type_error(#arg_name_str, e))?,
                            None => None,
                        };
                    });
                } else {
                    arg_conversions.push(quote! {
                        let #arg_name: #ty = match args.get(#idx) {
                            Some(v) => <#ty as litter::FromPyValue>::from_py_value(v)
                                .map_err(|e| litter::ToolCallError::type_error(#arg_name_str, e))?,
                            None => return Err(litter::ToolCallError::missing_argument(#arg_name_str)),
                        };
                    });
                }
            }
        }
    }

    // Extract return type
    let return_python_type = match &input_fn.sig.output {
        ReturnType::Default => "None".to_string(),
        ReturnType::Type(_, ty) => rust_type_to_python(ty),
    };

    // Get the original function body
    let fn_vis = &input_fn.vis;
    let fn_block = &input_fn.block;
    let fn_inputs = &input_fn.sig.inputs;
    let fn_output = &input_fn.sig.output;

    let expanded = quote! {
        #fn_vis mod #mod_name {
            use super::*;

            /// Tool metadata for registration
            pub static INFO: std::sync::LazyLock<litter::ToolInfo> = std::sync::LazyLock::new(|| {
                litter::ToolInfo::new(#fn_name_str, #description)
                    #(#arg_infos)*
                    .returns(#return_python_type)
            });

            /// The actual implementation
            fn implementation(#fn_inputs) #fn_output #fn_block

            /// Wrapper that converts PyValue args and handles errors
            pub fn call(args: Vec<litter::PyValue>) -> litter::PyValue {
                match try_call(args) {
                    Ok(v) => v,
                    Err(e) => {
                        // Return error as a dict with error info
                        litter::PyValue::Dict(vec![
                            ("error".to_string(), litter::PyValue::Str(e.to_string())),
                        ])
                    }
                }
            }

            fn try_call(args: Vec<litter::PyValue>) -> Result<litter::PyValue, litter::ToolCallError> {
                #(#arg_conversions)*

                let result = implementation(#(#arg_names),*);
                Ok(result.into())
            }
        }
    };

    TokenStream::from(expanded)
}
