//! Proc macros for scamp action registration.
//!
//! `#[rpc]` — marks an async function as a SCAMP action handler, auto-registered via inventory.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, Expr, ExprLit, ItemFn, Lit, Meta, Token};

/// Convert snake_case to camelCase: `set_login_data` → `setLoginData`
fn snake_to_camel(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;
    for ch in s.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(ch.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// Parsed #[rpc(...)] arguments.
struct RpcArgs {
    metas: Punctuated<Meta, Token![,]>,
}

impl Parse for RpcArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(RpcArgs {
            metas: Punctuated::parse_terminated(input)?,
        })
    }
}

fn meta_str_value(meta: &Meta) -> Option<String> {
    if let Meta::NameValue(nv) = meta {
        if let Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) = &nv.value {
            return Some(s.value());
        }
    }
    None
}

fn meta_int_value(meta: &Meta) -> Option<u32> {
    if let Meta::NameValue(nv) = meta {
        if let Expr::Lit(ExprLit { lit: Lit::Int(i), .. }) = &nv.value {
            return i.base10_parse().ok();
        }
    }
    None
}

/// `#[rpc]` — register an async function as a SCAMP action handler.
///
/// # Attributes
/// - Bare flags: `noauth`, `read`, `public`, `create`, `update`, `destroy`
/// - `version = N` (default 1)
/// - `timeout = N` (emits `tN` flag)
/// - `namespace = "Custom.Override"` (default: derived from module path)
/// - `sector = "background"` (default: service default)
/// - `name = "customName"` (default: camelCase of fn name)
///
/// Handler signature: `async fn(RequestContext, &S) -> ScampReply`
#[proc_macro_attribute]
pub fn rpc(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let fn_name_str = fn_name.to_string();
    let wire_name_default = snake_to_camel(&fn_name_str);

    // Parse attributes
    let args = if attr.is_empty() {
        RpcArgs { metas: Punctuated::new() }
    } else {
        parse_macro_input!(attr as RpcArgs)
    };

    let mut version: u32 = 1;
    let mut flags: Vec<String> = Vec::new();
    let mut namespace_override: Option<String> = None;
    let mut sector_override: Option<String> = None;
    let mut name_override: Option<String> = None;

    for meta in &args.metas {
        match meta {
            // Bare ident flags: noauth, read, public, etc.
            Meta::Path(path) => {
                if let Some(ident) = path.get_ident() {
                    flags.push(ident.to_string());
                }
            }
            // Key=value pairs
            Meta::NameValue(nv) => {
                if let Some(ident) = nv.path.get_ident() {
                    let key = ident.to_string();
                    match key.as_str() {
                        "version" => {
                            if let Some(v) = meta_int_value(meta) {
                                version = v;
                            }
                        }
                        "timeout" => {
                            if let Some(t) = meta_int_value(meta) {
                                flags.push(format!("t{}", t));
                            }
                        }
                        "namespace" => namespace_override = meta_str_value(meta),
                        "sector" => sector_override = meta_str_value(meta),
                        "name" => name_override = meta_str_value(meta),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    let wire_name = name_override.unwrap_or(wire_name_default);
    let flags_tokens: Vec<_> = flags.iter().map(|f| quote! { #f }).collect();

    let ns_expr = match namespace_override {
        Some(ns) => quote! { #ns.to_string() },
        None => quote! { ::scamp::rpc_support::module_path_to_namespace(module_path!()) },
    };

    let sector_expr = match sector_override {
        Some(s) => quote! { Some(#s.to_string()) },
        None => quote! { None },
    };

    let expanded = quote! {
        #input_fn

        ::inventory::submit! {
            ::scamp::rpc_support::RpcRegistration {
                namespace_fn: || #ns_expr,
                wire_name: #wire_name,
                version: #version,
                flags: &[#(#flags_tokens),*],
                sector_fn: || #sector_expr,
                make_handler: || ::scamp::rpc_support::make_handler_erased(
                    |ctx, state| Box::pin(async move {
                        ::scamp::rpc_support::IntoScampReply::into_scamp_reply(
                            #fn_name(ctx, state).await
                        )
                    })
                ),
            }
        }
    };

    expanded.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snake_to_camel() {
        assert_eq!(snake_to_camel("set_login_data"), "setLoginData");
        assert_eq!(snake_to_camel("create_api_key"), "createApiKey");
        assert_eq!(snake_to_camel("fetch"), "fetch");
        assert_eq!(snake_to_camel("journalentries"), "journalentries");
        assert_eq!(snake_to_camel("health_check"), "healthCheck");
        assert_eq!(snake_to_camel("pdf"), "pdf");
    }
}
