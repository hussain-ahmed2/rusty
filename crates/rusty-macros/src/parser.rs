//! Recursive-descent parser for the `view!` macro.
//!
//! The parser operates on `syn`'s `ParseStream`. HTML's whitespace-insensitive
//! grammar maps cleanly onto token adjacency, so no whitespace tracking is
//! required.

use proc_macro2::Span;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::{braced, token, Ident, LitStr, Token};

use crate::ast::{Attribute, AttributeValue, HtmlElement, Name, Node, View};

impl Parse for View {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut nodes = Vec::new();

        while !input.is_empty() {
            nodes.push(parse_node(input)?);
        }

        Ok(Self { nodes })
    }
}

fn parse_node(input: ParseStream<'_>) -> syn::Result<Node> {
    if input.peek(LitStr) {
        return input.parse::<LitStr>().map(Node::Text);
    }

    if input.peek(token::Brace) {
        let _content;
        let brace = braced!(_content in input);
        return Err(syn::Error::new(
            brace.span.join(),
            "dynamic expressions in `view!` arrive in Milestone 2; use a string literal for now",
        ));
    }

    if input.peek(Token![<]) {
        return parse_element(input).map(Node::Element);
    }

    Err(input.error("expected an HTML element, a string literal, or a `{ ... }` expression"))
}

fn parse_element(input: ParseStream<'_>) -> syn::Result<HtmlElement> {
    let open = input.parse::<Token![<]>()?;

    if input.peek(Token![/]) {
        return Err(syn::Error::new(
            open.span,
            "unexpected closing tag; no element is open",
        ));
    }

    let tag = parse_tag_name(input)?;
    reject_component_tag(&tag)?;
    validate_tag_name(&tag)?;

    let mut attributes = Vec::new();
    while !input.peek(Token![>]) && !input.peek(Token![/]) {
        let attribute = parse_attribute(input)?;

        if attributes
            .iter()
            .any(|existing: &Attribute| existing.name.value == attribute.name.value)
        {
            return Err(syn::Error::new(
                attribute.name.span,
                format!("duplicate attribute `{}`", attribute.name.value),
            ));
        }

        attributes.push(attribute);
    }

    if input.peek(Token![/]) {
        input.parse::<Token![/]>()?;
        input.parse::<Token![>]>()?;

        return Ok(HtmlElement {
            tag,
            attributes,
            children: Vec::new(),
        });
    }

    input.parse::<Token![>]>()?;

    if rusty_core::is_void_element(&tag.value) {
        return Ok(HtmlElement {
            tag,
            attributes,
            children: Vec::new(),
        });
    }

    let children = parse_children(input, &tag, open.span)?;

    Ok(HtmlElement {
        tag,
        attributes,
        children,
    })
}

fn parse_children(input: ParseStream<'_>, tag: &Name, open_span: Span) -> syn::Result<Vec<Node>> {
    let mut children = Vec::new();

    loop {
        if input.is_empty() {
            return Err(syn::Error::new(
                open_span,
                format!("unclosed <{}> element", tag.value),
            ));
        }

        if input.peek(Token![<]) && input.peek2(Token![/]) {
            let closing = parse_closing_tag(input)?;

            if closing.value != tag.value {
                return Err(syn::Error::new(
                    closing.span,
                    format!(
                        "mismatched closing tag </{}>; expected </{}>",
                        closing.value, tag.value
                    ),
                ));
            }

            return Ok(children);
        }

        children.push(parse_node(input)?);
    }
}

fn parse_closing_tag(input: ParseStream<'_>) -> syn::Result<Name> {
    input.parse::<Token![<]>()?;
    input.parse::<Token![/]>()?;
    let name = parse_tag_name(input)?;
    input.parse::<Token![>]>()?;
    Ok(name)
}

fn parse_attribute(input: ParseStream<'_>) -> syn::Result<Attribute> {
    let name = parse_attribute_name(input)?;
    validate_attribute_name(&name)?;

    if input.peek(Token![=]) {
        input.parse::<Token![=]>()?;

        if input.peek(LitStr) {
            let value = input.parse::<LitStr>()?;
            return Ok(Attribute {
                name,
                value: AttributeValue::Text(value),
            });
        }

        if input.peek(token::Brace) {
            let _content;
            let brace = braced!(_content in input);
            return Err(syn::Error::new(
                brace.span.join(),
                "dynamic attribute values arrive in Milestone 2; use a string literal for now",
            ));
        }

        return Err(input.error("attribute values must be string literals until Milestone 2"));
    }

    Ok(Attribute {
        name,
        value: AttributeValue::Bool(true),
    })
}

fn parse_tag_name(input: ParseStream<'_>) -> syn::Result<Name> {
    let first = Ident::parse_any(input)?;
    let span = first.span();
    let mut value = first.unraw().to_string();

    while input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        let part = Ident::parse_any(input)?;
        value.push('-');
        value.push_str(&part.unraw().to_string());
    }

    Ok(Name { value, span })
}

fn parse_attribute_name(input: ParseStream<'_>) -> syn::Result<Name> {
    let first = Ident::parse_any(input)?;
    let span = first.span();
    let mut value = first.unraw().to_string();

    loop {
        let separator = if input.peek(Token![-]) {
            Some('-')
        } else if input.peek(Token![:]) {
            Some(':')
        } else {
            None
        };

        let Some(separator) = separator else {
            break;
        };

        if separator == '-' {
            input.parse::<Token![-]>()?;
        } else {
            input.parse::<Token![:]>()?;
        }

        let part = Ident::parse_any(input)?;
        value.push(separator);
        value.push_str(&part.unraw().to_string());
    }

    Ok(Name { value, span })
}

fn reject_component_tag(tag: &Name) -> syn::Result<()> {
    let starts_uppercase = tag
        .value
        .chars()
        .next()
        .is_some_and(|character| character.is_uppercase());

    if starts_uppercase {
        return Err(syn::Error::new(
            tag.span,
            format!(
                "component elements such as <{}> arrive in Milestone 2; only HTML elements are supported for now",
                tag.value
            ),
        ));
    }

    Ok(())
}

fn validate_tag_name(tag: &Name) -> syn::Result<()> {
    rusty_core::TagName::new(tag.value.clone())
        .map(|_| ())
        .map_err(|error| syn::Error::new(tag.span, error.to_string()))
}

fn validate_attribute_name(attribute: &Name) -> syn::Result<()> {
    rusty_core::AttributeName::new(attribute.value.clone())
        .map(|_| ())
        .map_err(|error| syn::Error::new(attribute.span, error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> syn::Result<View> {
        syn::parse_str(source)
    }

    #[test]
    fn parses_nested_elements_and_text() {
        let view = parse(r#"<main><h1>"Title"</h1></main>"#).expect("view should parse");
        assert_eq!(view.nodes.len(), 1);

        let Node::Element(main) = &view.nodes[0] else {
            panic!("expected the root to be an element");
        };
        assert_eq!(main.tag.value, "main");
        assert_eq!(main.children.len(), 1);

        let Node::Element(heading) = &main.children[0] else {
            panic!("expected a nested element");
        };
        assert_eq!(heading.tag.value, "h1");
        assert!(matches!(heading.children[0], Node::Text(_)));
    }

    #[test]
    fn parses_static_and_boolean_attributes() {
        let view = parse(r#"<input type="text" disabled>"#).expect("view should parse");
        let Node::Element(input) = &view.nodes[0] else {
            panic!("expected an input element");
        };

        assert_eq!(input.attributes.len(), 2);
        assert!(matches!(input.attributes[0].value, AttributeValue::Text(_)));
        assert!(matches!(
            input.attributes[1].value,
            AttributeValue::Bool(true)
        ));
    }

    #[test]
    fn void_elements_do_not_consume_following_nodes() {
        let view = parse(r#"<div><br>"after"</div>"#).expect("view should parse");
        let Node::Element(div) = &view.nodes[0] else {
            panic!("expected a div");
        };

        assert_eq!(div.children.len(), 2);
        assert!(matches!(&div.children[0], Node::Element(br) if br.tag.value == "br"));
        assert!(matches!(&div.children[1], Node::Text(_)));
    }

    #[test]
    fn rejects_mismatched_closing_tags() {
        let error = parse(r#"<main></section>"#)
            .err()
            .expect("closing tag must not match");
        assert!(error.to_string().contains("mismatched closing tag"));
    }

    #[test]
    fn rejects_unclosed_elements() {
        let error = parse("<main><h1>\"text\"</h1>")
            .err()
            .expect("element must be closed");
        assert!(error.to_string().contains("unclosed <main>"));
    }

    #[test]
    fn rejects_component_tags_until_milestone_two() {
        let error = parse("<Widget></Widget>")
            .err()
            .expect("components are not supported yet");
        assert!(error.to_string().contains("Milestone 2"));
    }

    #[test]
    fn rejects_dynamic_expressions_until_milestone_two() {
        let error = parse("<p>{value}</p>")
            .err()
            .expect("dynamic nodes are not supported yet");
        assert!(error.to_string().contains("Milestone 2"));
    }

    #[test]
    fn rejects_duplicate_attributes() {
        let error = parse(r#"<div class="a" class="b"></div>"#)
            .err()
            .expect("duplicate attributes must be rejected");
        assert!(error.to_string().contains("duplicate attribute"));
    }
}
