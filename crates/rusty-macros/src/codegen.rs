//! Token generation for the `view!` macro.
//!
//! Expansions target a small, stable surface in `rusty::__private`. Keeping the
//! generated paths behind helpers means the facade can evolve internally without
//! breaking already-compiled macro output.

use proc_macro2::TokenStream;
use quote::quote;
use syn::LitStr;

use crate::ast::{
    Attribute, AttributeValue, ComponentElement, EachNode, Event, HtmlElement, IfNode, Node, View,
};

/// Expands a parsed view into a Rust expression that produces an `Element`.
pub(crate) fn expand(view: &View) -> TokenStream {
    nodes_expression(&view.nodes)
}

fn nodes_expression(nodes: &[Node]) -> TokenStream {
    match nodes {
        [node] => node_tokens(node),
        nodes => {
            let children = nodes.iter().map(node_tokens);
            quote! {
                ::rusty::__private::Element::fragment(::std::vec![#(#children),*])
            }
        }
    }
}

fn empty_expression() -> TokenStream {
    quote! {
        ::rusty::__private::Element::fragment(::std::vec![])
    }
}

fn node_tokens(node: &Node) -> TokenStream {
    match node {
        Node::Element(element) => element_tokens(element),
        Node::Component(component) => component_tokens(component),
        Node::Text(text) => {
            quote! {
                ::rusty::__private::Element::text(#text)
            }
        }
        Node::Dynamic(expression) => {
            quote! {
                ::rusty::__private::IntoElement::into_element(#expression)
            }
        }
        Node::If(conditional) => if_tokens(conditional),
        Node::Each(each) => each_tokens(each),
    }
}

fn element_tokens(element: &HtmlElement) -> TokenStream {
    let tag = LitStr::new(&element.tag.value, element.tag.span);
    let attributes = element.attributes.iter().map(html_attribute_tokens);
    let children = element.children.iter().map(node_tokens);

    if element.events.is_empty() {
        quote! {
            ::rusty::__private::element(
                #tag,
                ::std::vec![#(#attributes),*],
                ::std::vec![#(#children),*],
            )
        }
    } else {
        let events = element.events.iter().map(event_tokens);
        quote! {
            ::rusty::__private::element_with_events(
                #tag,
                ::std::vec![#(#attributes),*],
                ::std::vec![#(#children),*],
                ::std::vec![#(#events),*],
            )
        }
    }
}

fn event_tokens(event: &Event) -> TokenStream {
    let name = LitStr::new(&event.name.value, event.name.span);
    let handler = &event.handler;

    quote! {
        ::rusty::__private::event_listener(#name, #handler)
    }
}

fn html_attribute_tokens(attribute: &Attribute) -> TokenStream {
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
        AttributeValue::Dynamic(expression) => {
            quote! {
                ::rusty::__private::dynamic_attribute(#name, #expression)
            }
        }
    }
}

fn component_tokens(component: &ComponentElement) -> TokenStream {
    let name = &component.name.ident;
    let fields = component.attributes.iter().map(component_field_tokens);

    let children = if component.self_closing {
        None
    } else {
        let children = nodes_expression(&component.children);
        Some(quote! {
            children: ::std::convert::Into::into(::rusty::__private::children(#children))
        })
    };

    quote! {
        ::rusty::__private::render_component(
            #name {
                #(#fields,)*
                #children
            }
        )
    }
}

fn component_field_tokens(attribute: &Attribute) -> TokenStream {
    let name = &attribute.name.ident;

    let value = match &attribute.value {
        AttributeValue::Text(value) => quote! { #value },
        AttributeValue::Bool(value) => quote! { #value },
        AttributeValue::Dynamic(expression) => quote! { #expression },
    };

    quote! {
        #name: ::std::convert::Into::into(#value)
    }
}

fn if_tokens(conditional: &IfNode) -> TokenStream {
    let mut branches = conditional.branches.iter();

    let Some(first) = branches.next() else {
        return empty_expression();
    };

    let first_condition = &first.condition;
    let first_body = nodes_expression(&first.children);
    let mut tokens = quote! {
        if #first_condition {
            #first_body
        }
    };

    for branch in branches {
        let condition = &branch.condition;
        let body = nodes_expression(&branch.children);
        tokens.extend(quote! {
            else if #condition {
                #body
            }
        });
    }

    let else_body = conditional
        .else_children
        .as_deref()
        .map_or_else(empty_expression, nodes_expression);

    tokens.extend(quote! {
        else {
            #else_body
        }
    });

    tokens
}

fn each_tokens(each: &EachNode) -> TokenStream {
    let iterable = &each.iterable;
    let pattern = &each.pattern;
    let body = nodes_expression(&each.children);

    quote! {
        ::rusty::__private::each(#iterable, |#pattern| #body)
    }
}
