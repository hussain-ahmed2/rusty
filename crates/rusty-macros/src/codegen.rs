//! Token generation for the `view!` macro.
//!
//! Expansions target a small, stable surface in `rusty::__private`. Keeping the
//! generated paths behind helpers means the facade can evolve internally without
//! breaking already-compiled macro output.

use proc_macro2::TokenStream;
use quote::quote;
use syn::LitStr;

use crate::ast::{Attribute, AttributeValue, HtmlElement, Node, View};

/// Expands a parsed view into a Rust expression that produces an `Element`.
pub(crate) fn expand(view: &View) -> TokenStream {
    match view.nodes.as_slice() {
        [node] => node_tokens(node),
        nodes => {
            let children = nodes.iter().map(node_tokens);
            quote! {
                ::rusty::__private::Element::fragment(::std::vec![#(#children),*])
            }
        }
    }
}

fn node_tokens(node: &Node) -> TokenStream {
    match node {
        Node::Element(element) => element_tokens(element),
        Node::Text(text) => {
            quote! {
                ::rusty::__private::Element::text(#text)
            }
        }
    }
}

fn element_tokens(element: &HtmlElement) -> TokenStream {
    let tag = LitStr::new(&element.tag.value, element.tag.span);
    let attributes = element.attributes.iter().map(attribute_tokens);
    let children = element.children.iter().map(node_tokens);

    quote! {
        ::rusty::__private::element(
            #tag,
            ::std::vec![#(#attributes),*],
            ::std::vec![#(#children),*],
        )
    }
}

fn attribute_tokens(attribute: &Attribute) -> TokenStream {
    let name = LitStr::new(&attribute.name.value, attribute.name.span);

    match &attribute.value {
        AttributeValue::Text(value) => {
            quote! {
                ::rusty::__private::attribute(
                    #name,
                    ::rusty::__private::AttributeValue::text(#value),
                )
            }
        }
        AttributeValue::Bool(value) => {
            quote! {
                ::rusty::__private::attribute(
                    #name,
                    ::rusty::__private::AttributeValue::boolean(#value),
                )
            }
        }
    }
}
