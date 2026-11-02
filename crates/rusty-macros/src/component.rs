//! Expansion for the `#[component]` attribute.
//!
//! A component function is transformed into a braced struct whose fields are the
//! function parameters. The function body becomes the `Component::render` method.
//! That makes component usage a normal Rust struct literal:
//!
//! ```text
//! <UserCard user={user.clone()} />
//! // expands to approximately:
//! render_component(UserCard { user: user.clone() })
//! ```
//!
//! Struct literals give compile-time checking for missing props, extra props, and
//! property types without inventing a second, untyped props layer.

use proc_macro2::TokenStream;
use quote::quote;
use syn::ext::IdentExt;
use syn::{Error, FnArg, Ident, ItemFn, Pat, PatIdent, Result, Type};

/// Expands a `#[component]` function into a component struct and render impl.
pub(crate) fn expand(function: ItemFn) -> Result<TokenStream> {
    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = function;

    let name = &sig.ident;
    let readable_name = name.unraw().to_string();

    if !readable_name
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
    {
        return Err(Error::new(
            name.span(),
            "component function names must start with an uppercase ASCII letter so `<Name />` can be recognized",
        ));
    }

    if let Some(asyncness) = sig.asyncness {
        return Err(Error::new(
            asyncness.span,
            "async components are not supported yet; server components arrive in Milestone 7",
        ));
    }

    if !sig.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &sig.generics,
            "generic components are not supported yet; use concrete owned prop types",
        ));
    }

    if sig.constness.is_some()
        || sig.unsafety.is_some()
        || sig.abi.is_some()
        || sig.variadic.is_some()
    {
        return Err(Error::new(
            name.span(),
            "component functions must be ordinary safe Rust functions",
        ));
    }

    if let Some(where_clause) = &sig.generics.where_clause {
        return Err(Error::new_spanned(
            where_clause,
            "where clauses require generic components, which are not supported yet",
        ));
    }

    let mut fields = Vec::with_capacity(sig.inputs.len());

    for input in &sig.inputs {
        let FnArg::Typed(pat_type) = input else {
            return Err(Error::new_spanned(
                input,
                "component functions must not take `self`",
            ));
        };

        if let Some(attribute) = pat_type.attrs.first() {
            return Err(Error::new_spanned(
                attribute,
                "parameter attributes are not supported on components",
            ));
        }

        let Pat::Ident(PatIdent {
            by_ref,
            mutability,
            ident,
            subpat,
            ..
        }) = &*pat_type.pat
        else {
            return Err(Error::new_spanned(
                &pat_type.pat,
                "component parameters must be simple identifiers",
            ));
        };

        if by_ref.is_some() || subpat.is_some() {
            return Err(Error::new_spanned(
                &pat_type.pat,
                "component parameters must be simple identifiers without patterns",
            ));
        }

        fields.push(Field {
            ident: ident.clone(),
            ty: (*pat_type.ty).clone(),
            mutable: mutability.is_some(),
        });
    }

    let field_definitions = fields.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        let doc = format!("The `{}` prop.", ident.unraw());
        quote! {
            #[doc = #doc]
            #vis #ident: #ty
        }
    });

    let bindings = fields.iter().map(|field| {
        let ident = &field.ident;
        if field.mutable {
            quote! { mut #ident }
        } else {
            quote! { #ident }
        }
    });

    let mut struct_attrs = Vec::new();
    let mut impl_attrs = Vec::new();
    let mut method_attrs = Vec::new();

    for attr in attrs {
        let path = attr.path();

        if path.is_ident("cfg") || path.is_ident("cfg_attr") {
            impl_attrs.push(attr.clone());
            struct_attrs.push(attr);
        } else if path.is_ident("inline") || path.is_ident("cold") || path.is_ident("track_caller")
        {
            method_attrs.push(attr);
        } else {
            if path.is_ident("allow")
                || path.is_ident("warn")
                || path.is_ident("deny")
                || path.is_ident("forbid")
                || path.is_ident("expect")
            {
                method_attrs.push(attr.clone());
            }

            struct_attrs.push(attr);
        }
    }

    Ok(quote! {
        #(#struct_attrs)*
        #vis struct #name {
            #(#field_definitions),*
        }

        #(#impl_attrs)*
        impl ::rusty::__private::Component for #name {
            #(#method_attrs)*
            fn render(self) -> ::rusty::__private::Element {
                let Self { #(#bindings),* } = self;
                #block
            }
        }
    })
}

struct Field {
    ident: Ident,
    ty: Type,
    mutable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand_source(source: &str) -> Result<TokenStream> {
        let function: ItemFn =
            syn::parse_str(source).expect("test source should parse as a function");
        expand(function)
    }

    #[test]
    fn expands_component_with_props() {
        let expanded = expand_source(
            r#"
            pub fn Greeting(name: String) -> Element {
                view! { <p>{name}</p> }
            }
            "#,
        )
        .expect("component should expand");

        let output = expanded.to_string();
        assert!(output.contains("pub struct Greeting"));
        assert!(output.contains("pub name : String"));
        assert!(output.contains("impl :: rusty :: __private :: Component for Greeting"));
    }

    #[test]
    fn expands_component_without_props() {
        let expanded = expand_source("pub fn Home() -> Element { view! {} }")
            .expect("component should expand");
        assert!(expanded.to_string().contains("pub struct Home { }"));
    }

    #[test]
    fn rejects_lowercase_component_names() {
        let error = expand_source("fn greeting() -> Element { view! {} }")
            .expect_err("lowercase component must fail");
        assert!(error.to_string().contains("uppercase"));
    }

    #[test]
    fn rejects_generic_components() {
        let error = expand_source("fn Card<T>(item: T) -> Element { view! {} }")
            .expect_err("generic component must fail");
        assert!(error.to_string().contains("generic components"));
    }

    #[test]
    fn rejects_async_components() {
        let error = expand_source("async fn Card() -> Element { view! {} }")
            .expect_err("async component must fail");
        assert!(error.to_string().contains("async components"));
    }

    #[test]
    fn rejects_complex_parameter_patterns() {
        let error = expand_source("fn Card((left, right): (u8, u8)) -> Element { view! {} }")
            .expect_err("pattern parameter must fail");
        assert!(error.to_string().contains("simple identifiers"));
    }

    #[test]
    fn rejects_where_clauses() {
        let error = expand_source("fn Card() -> Element where String: Clone { view! {} }")
            .expect_err("where clause must fail");
        assert!(error.to_string().contains("where clauses"));
    }

    #[test]
    fn duplicates_cfg_attributes_on_the_impl() {
        let expanded = expand_source("#[cfg(test)] pub fn Card() -> Element { view! {} }")
            .expect("conditional component should expand");
        assert_eq!(expanded.to_string().matches("cfg").count(), 2);
    }

    #[test]
    fn moves_inline_and_lint_attributes_to_render() {
        let expanded =
            expand_source("#[inline] #[allow(dead_code)] pub fn Card() -> Element { view! {} }")
                .expect("attributed component should expand");
        let output = expanded.to_string();
        assert_eq!(output.matches("inline").count(), 1);
        assert_eq!(output.matches("allow").count(), 2);
    }
}
