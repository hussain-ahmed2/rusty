//! Expansion for `#[server]`.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{Expr, FnArg, ItemFn, Lit, LitStr, Pat, PathArguments, ReturnType, Type, TypePath};

pub(crate) fn expand(attributes: TokenStream, item: TokenStream) -> TokenStream {
    match expand_inner(attributes, item) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

fn expand_inner(attributes: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let function: ItemFn = syn::parse2(item)?;
    validate_function(&function)?;

    let name = function.sig.ident.clone();
    let default_path = format!("/_rusty/rpc/{}", name.to_string().trim_start_matches("r#"));
    let path = parse_path(attributes, default_path)?;
    let marker = format_ident!("{}ServerFn", pascal_case(&name.to_string()));
    let visibility = function.vis.clone();
    let attributes = function.attrs.clone();
    let signature = function.sig.clone();
    let inputs = signature.inputs.clone();

    let output = result_output(&function)?;
    let return_type = signature.output.clone();
    let body = function.block.clone();
    let arguments = function
        .sig
        .inputs
        .iter()
        .map(|argument| match argument {
            FnArg::Typed(argument) => Ok((
                match &*argument.pat {
                    Pat::Ident(ident) if ident.subpat.is_none() && ident.by_ref.is_none() => {
                        Ok(ident.ident.clone())
                    }
                    _ => Err(syn::Error::new(
                        argument.pat.span(),
                        "`#[server]` arguments must be simple identifiers",
                    )),
                }?,
                (*argument.ty).clone(),
            )),
            FnArg::Receiver(receiver) => Err(syn::Error::new(
                receiver.span(),
                "`#[server]` functions cannot take `self`",
            )),
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let identifiers = arguments.iter().map(|(ident, _)| ident).collect::<Vec<_>>();
    let types = arguments.iter().map(|(_, ty)| ty).collect::<Vec<_>>();
    let args_type = match types.as_slice() {
        [] => quote!(()),
        [single] => quote!((#single,)),
        _ => quote!((#(#types),*)),
    };

    Ok(quote! {
        #(#attributes)*
        #[cfg(not(target_arch = "wasm32"))]
        #signature #body

        #(#attributes)*
        #[cfg(target_arch = "wasm32")]
        #visibility fn #name(#inputs) #return_type {
            let __rusty_args = (#(#identifiers,)*);
            ::rusty::rpc::invoke::<#marker>(&__rusty_args)
        }

        #[doc(hidden)]
        #[allow(dead_code)]
        #[derive(Debug, Default, Clone, Copy)]
        #visibility struct #marker;

        impl ::rusty::rpc::ServerFunction for #marker {
            const PATH: &'static str = #path;
            type Args = #args_type;
            type Output = #output;
        }

        #[cfg(not(target_arch = "wasm32"))]
        impl ::rusty::rpc::ServerInvoker for #marker {
            fn invoke(
                &self,
                __rusty_args: &[u8],
            ) -> ::std::result::Result<::std::vec::Vec<u8>, ::rusty::rpc::RpcError> {
                let (#(#identifiers,)*): <Self as ::rusty::rpc::ServerFunction>::Args =
                    ::rusty::rpc::__private::from_slice(__rusty_args)
                        .map_err(|error| ::rusty::rpc::RpcError::Deserialize(error.to_string()))?;
                let __rusty_output = #name(#(#identifiers),*)?;
                ::rusty::rpc::__private::to_vec(&__rusty_output)
                    .map_err(|error| ::rusty::rpc::RpcError::Serialize(error.to_string()))
            }
        }
    })
}

fn validate_function(function: &ItemFn) -> syn::Result<()> {
    if let Some(asyncness) = function.sig.asyncness {
        return Err(syn::Error::new(
            asyncness.span(),
            "`#[server]` does not support async functions yet",
        ));
    }
    if !function.sig.generics.params.is_empty() || function.sig.generics.where_clause.is_some() {
        return Err(syn::Error::new(
            function.sig.generics.span(),
            "`#[server]` functions cannot be generic",
        ));
    }
    if function.sig.receiver().is_some() {
        return Err(syn::Error::new(
            function.sig.span(),
            "`#[server]` functions cannot take `self`",
        ));
    }
    if let Some(unsafety) = function.sig.unsafety {
        return Err(syn::Error::new(
            unsafety.span(),
            "`#[server]` functions cannot be unsafe",
        ));
    }
    if let Some(constness) = function.sig.constness {
        return Err(syn::Error::new(
            constness.span(),
            "`#[server]` functions cannot be const",
        ));
    }
    if let Some(abi) = &function.sig.abi {
        return Err(syn::Error::new(
            abi.span(),
            "`#[server]` functions cannot use an explicit ABI",
        ));
    }
    if let Some(variadic) = &function.sig.variadic {
        return Err(syn::Error::new(
            variadic.span(),
            "`#[server]` functions cannot be variadic",
        ));
    }
    Ok(())
}

fn parse_path(attributes: TokenStream, default: String) -> syn::Result<String> {
    if attributes.is_empty() {
        return Ok(default);
    }

    let literal = if let Ok(literal) = syn::parse2::<LitStr>(attributes.clone()) {
        literal
    } else {
        let meta: syn::MetaNameValue = syn::parse2(attributes).map_err(|_| {
            syn::Error::new(
                Span::call_site(),
                "expected a path literal: `#[server(\"/custom/path\")]` or `#[server(path = \"/custom/path\")]`",
            )
        })?;
        if !meta.path.is_ident("path") {
            return Err(syn::Error::new(
                meta.path.span(),
                "the only supported `#[server]` option is `path`",
            ));
        }
        match meta.value {
            Expr::Lit(literal) => match literal.lit {
                Lit::Str(literal) => literal,
                other => {
                    return Err(syn::Error::new(
                        other.span(),
                        "the `path` option must be a string literal",
                    ))
                }
            },
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "the `path` option must be a string literal",
                ))
            }
        }
    };

    let path = literal.value();
    if !path.starts_with('/') {
        return Err(syn::Error::new(
            literal.span(),
            "server function paths must start with `/`",
        ));
    }
    if path.contains(['?', '#']) {
        return Err(syn::Error::new(
            literal.span(),
            "server function paths cannot contain a query string or fragment",
        ));
    }
    Ok(path)
}

fn result_output(function: &ItemFn) -> syn::Result<Type> {
    let ReturnType::Type(_, ty) = &function.sig.output else {
        return Err(syn::Error::new(
            function.sig.span(),
            "`#[server]` functions must return `Result<T, RpcError>`",
        ));
    };
    let Type::Path(TypePath { path, .. }) = &**ty else {
        return Err(syn::Error::new(
            ty.span(),
            "`#[server]` functions must return `Result<T, RpcError>`",
        ));
    };
    let Some(segment) = path.segments.last() else {
        return Err(syn::Error::new(
            ty.span(),
            "`#[server]` functions must return `Result<T, RpcError>`",
        ));
    };
    if segment.ident != "Result" {
        return Err(syn::Error::new(
            ty.span(),
            "`#[server]` functions must return `Result<T, RpcError>`",
        ));
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(syn::Error::new(
            ty.span(),
            "`#[server]` functions must return `Result<T, RpcError>`",
        ));
    };
    let types = arguments
        .args
        .iter()
        .filter_map(|argument| match argument {
            syn::GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })
        .collect::<Vec<_>>();
    if types.len() != 2 {
        return Err(syn::Error::new(
            ty.span(),
            "`#[server]` functions must return `Result<T, RpcError>`",
        ));
    }
    let error_is_rpc = matches!(
        types[1],
        Type::Path(TypePath { path, .. })
            if path.segments.last().is_some_and(|segment| segment.ident == "RpcError")
    );
    if !error_is_rpc {
        return Err(syn::Error::new(
            types[1].span(),
            "the server function error type must be `RpcError`",
        ));
    }
    Ok(types[0].clone())
}

fn pascal_case(name: &str) -> String {
    let mut output = String::new();
    for segment in name.trim_start_matches("r#").split('_') {
        let mut characters = segment.chars();
        if let Some(first) = characters.next() {
            output.extend(first.to_uppercase());
            output.extend(characters);
        }
    }
    if output.is_empty() {
        output.push_str("Server");
    }
    output
}
