//! Recursive-descent parser for the `view!` macro.
//!
//! The parser operates on `syn`'s `ParseStream`. HTML's whitespace-insensitive
//! grammar maps cleanly onto token adjacency, so no whitespace tracking is
//! required. Control-flow directives use `{#...}` markers so they cannot be
//! confused with ordinary Rust expressions.

use proc_macro2::{Span, TokenStream};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::{braced, token, Expr, Ident, LitStr, Pat, Token};

use crate::ast::{
    Attribute, AttributeValue, ComponentElement, EachNode, Event, HtmlElement, IfBranch, IfNode,
    Name, Node, View,
};

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
        // A brace group is one token in the outer stream, so inspect its
        // contents through a fork rather than using `peek2` on the outer input.
        let fork = input.fork();
        let content;
        let _brace = braced!(content in fork);

        if content.peek(Token![:]) || content.peek(Token![/]) {
            return Err(input.error(
                "unexpected control marker; `{:else}` and `{/...}` must close an open `#if` or `#each` block",
            ));
        }

        if content.peek(Token![#]) {
            return parse_control(input);
        }

        return parse_dynamic(input);
    }

    if input.peek(Token![<]) {
        return parse_element(input);
    }

    Err(input.error(
        "expected an HTML element, a component, a string literal, or a `{ ... }` expression",
    ))
}

fn parse_dynamic(input: ParseStream<'_>) -> syn::Result<Node> {
    let content;
    let _brace = braced!(content in input);
    let expression = content.parse::<Expr>()?;
    ensure_empty(&content, "unexpected tokens after expression")?;
    Ok(Node::Dynamic(expression))
}

fn parse_element(input: ParseStream<'_>) -> syn::Result<Node> {
    let open = input.parse::<Token![<]>()?;

    if input.peek(Token![/]) {
        return Err(syn::Error::new(
            open.span,
            "unexpected closing tag; no element is open",
        ));
    }

    let tag = parse_tag_name(input)?;
    let is_component = starts_uppercase(&tag.value);

    if is_component {
        if tag.has_separators {
            return Err(syn::Error::new(
                tag.span,
                "component tags must be a single identifier; import components with `use`",
            ));
        }
    } else {
        validate_tag_name(&tag)?;
    }

    let mut attributes = Vec::new();
    let mut events = Vec::new();
    let mut seen_names = Vec::new();

    while !input.peek(Token![>]) && !input.peek(Token![/]) {
        let attribute = parse_attribute(input)?;

        if is_component && attribute.name.has_separators {
            return Err(syn::Error::new(
                attribute.name.span,
                "component property names must be valid Rust identifiers without `-` or `:`",
            ));
        }

        if seen_names
            .iter()
            .any(|name: &String| name == &attribute.name.value)
        {
            return Err(syn::Error::new(
                attribute.name.span,
                format!("duplicate attribute `{}`", attribute.name.value),
            ));
        }
        seen_names.push(attribute.name.value.clone());

        if !is_component {
            if let Some(event_name) = attribute.name.value.strip_prefix("on:") {
                events.push(parse_event(&attribute, event_name)?);
                continue;
            }
        }

        attributes.push(attribute);
    }

    if input.peek(Token![/]) {
        input.parse::<Token![/]>()?;
        input.parse::<Token![>]>()?;

        return Ok(if is_component {
            Node::Component(ComponentElement {
                name: tag,
                attributes,
                children: Vec::new(),
                self_closing: true,
            })
        } else {
            Node::Element(HtmlElement {
                tag,
                attributes,
                events,
                children: Vec::new(),
            })
        });
    }

    input.parse::<Token![>]>()?;

    if !is_component && rusty_core::is_void_element(&tag.value) {
        return Ok(Node::Element(HtmlElement {
            tag,
            attributes,
            events,
            children: Vec::new(),
        }));
    }

    let children = parse_children(input, &tag, open.span)?;

    if is_component {
        if attributes
            .iter()
            .any(|attribute| attribute.name.value == "children")
        {
            return Err(syn::Error::new(
                tag.span,
                "`children` cannot be passed as a prop on a paired component; use nested content instead",
            ));
        }

        Ok(Node::Component(ComponentElement {
            name: tag,
            attributes,
            children,
            self_closing: false,
        }))
    } else {
        Ok(Node::Element(HtmlElement {
            tag,
            attributes,
            events,
            children,
        }))
    }
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

fn parse_event(attribute: &Attribute, event_name: &str) -> syn::Result<Event> {
    rusty_core::EventName::new(event_name)
        .map_err(|error| syn::Error::new(attribute.name.span, error.to_string()))?;

    let AttributeValue::Dynamic(handler) = &attribute.value else {
        return Err(syn::Error::new(
            attribute.name.span,
            "event listeners require a `{ handler }` expression",
        ));
    };

    Ok(Event {
        name: Name {
            value: event_name.to_owned(),
            ident: attribute.name.ident.clone(),
            has_separators: false,
            span: attribute.name.span,
        },
        handler: handler.clone(),
    })
}

fn parse_attribute(input: ParseStream<'_>) -> syn::Result<Attribute> {
    let name = parse_attribute_name(input)?;
    validate_attribute_name(&name)?;

    if !input.peek(Token![=]) {
        return Ok(Attribute {
            name,
            value: AttributeValue::Bool(true),
        });
    }

    input.parse::<Token![=]>()?;

    if input.peek(LitStr) {
        let value = input.parse::<LitStr>()?;
        return Ok(Attribute {
            name,
            value: AttributeValue::Text(value),
        });
    }

    if input.peek(token::Brace) {
        let content;
        let _brace = braced!(content in input);
        let expression = content.parse::<Expr>()?;
        ensure_empty(&content, "unexpected tokens after attribute expression")?;
        return Ok(Attribute {
            name,
            value: AttributeValue::Dynamic(expression),
        });
    }

    Err(input.error("attribute values must be string literals or `{ ... }` expressions"))
}

fn parse_tag_name(input: ParseStream<'_>) -> syn::Result<Name> {
    let first = Ident::parse_any(input)?;
    let span = first.span();
    let mut value = first.unraw().to_string();
    let mut has_separators = false;

    while input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        let part = Ident::parse_any(input)?;
        value.push('-');
        value.push_str(&part.unraw().to_string());
        has_separators = true;
    }

    Ok(Name {
        value,
        ident: first,
        has_separators,
        span,
    })
}

fn parse_attribute_name(input: ParseStream<'_>) -> syn::Result<Name> {
    let first = Ident::parse_any(input)?;
    let span = first.span();
    let mut value = first.unraw().to_string();
    let mut has_separators = false;

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
        has_separators = true;
    }

    Ok(Name {
        value,
        ident: first,
        has_separators,
        span,
    })
}

fn parse_control(input: ParseStream<'_>) -> syn::Result<Node> {
    let content;
    let brace = braced!(content in input);
    let span = brace.span.join();

    content.parse::<Token![#]>()?;
    let directive = Ident::parse_any(&content)?;

    match directive.to_string().as_str() {
        "if" => {
            let condition = content.parse::<Expr>()?;
            ensure_empty(&content, "unexpected tokens after `#if` condition")?;
            parse_if(input, condition, span)
        }
        "each" => {
            let iterable = parse_iterable(&content)?;
            content.parse::<Token![as]>()?;
            let pattern = Pat::parse_single(&content)?;
            ensure_empty(&content, "unexpected tokens after `#each` pattern")?;
            parse_each(input, iterable, pattern, span)
        }
        other => Err(syn::Error::new(
            directive.span(),
            format!(
                "unknown control directive `#{other}`; supported directives are `#if` and `#each`"
            ),
        )),
    }
}

fn parse_iterable(input: ParseStream<'_>) -> syn::Result<Expr> {
    let mut tokens = TokenStream::new();

    while !input.is_empty() && !input.peek(Token![as]) {
        let token = input.step(|cursor| {
            if let Some((token, rest)) = cursor.token_tree() {
                Ok((token, rest))
            } else {
                Err(cursor.error("expected an iterator expression"))
            }
        })?;
        tokens.extend(std::iter::once(token));
    }

    if !input.peek(Token![as]) {
        return Err(input.error("expected `as <pattern>` after the `#each` iterator expression"));
    }

    if tokens.is_empty() {
        return Err(input.error("expected an iterator expression before `as`"));
    }

    syn::parse2(tokens)
}

fn parse_if(input: ParseStream<'_>, first_condition: Expr, open_span: Span) -> syn::Result<Node> {
    let mut branches = Vec::new();
    let mut condition = first_condition;

    loop {
        let children = parse_body_until_marker(input, open_span, "if")?;
        branches.push(IfBranch {
            condition,
            children,
        });

        let marker = parse_control_marker(input)?;

        match marker.kind {
            MarkerKind::ElseIf(next_condition) => {
                condition = next_condition;
            }
            MarkerKind::Else => {
                let else_children = parse_body_until_marker(input, open_span, "if")?;
                let closing = parse_control_marker(input)?;

                return match closing.kind {
                    MarkerKind::CloseIf => Ok(Node::If(IfNode {
                        branches,
                        else_children: Some(else_children),
                    })),
                    _ => Err(syn::Error::new(
                        closing.span,
                        "expected `{/if}` after `{:else}`",
                    )),
                };
            }
            MarkerKind::CloseIf => {
                return Ok(Node::If(IfNode {
                    branches,
                    else_children: None,
                }));
            }
            MarkerKind::CloseEach => {
                return Err(syn::Error::new(
                    marker.span,
                    "found `{/each}` while closing an `#if` block",
                ));
            }
        }
    }
}

fn parse_each(
    input: ParseStream<'_>,
    iterable: Expr,
    pattern: Pat,
    open_span: Span,
) -> syn::Result<Node> {
    let children = parse_body_until_marker(input, open_span, "each")?;
    let marker = parse_control_marker(input)?;

    match marker.kind {
        MarkerKind::CloseEach => Ok(Node::Each(Box::new(EachNode {
            iterable,
            pattern,
            children,
        }))),
        MarkerKind::CloseIf => Err(syn::Error::new(
            marker.span,
            "found `{/if}` while closing an `#each` block",
        )),
        MarkerKind::Else | MarkerKind::ElseIf(_) => Err(syn::Error::new(
            marker.span,
            "`{:else}` is only valid inside an `#if` block",
        )),
    }
}

fn parse_body_until_marker(
    input: ParseStream<'_>,
    open_span: Span,
    construct: &str,
) -> syn::Result<Vec<Node>> {
    let mut nodes = Vec::new();

    loop {
        if input.is_empty() {
            return Err(syn::Error::new(
                open_span,
                format!("unclosed `#{construct}` block; expected `{{/{construct}}}`"),
            ));
        }

        if is_control_marker(input)? {
            return Ok(nodes);
        }

        nodes.push(parse_node(input)?);
    }
}

fn is_control_marker(input: ParseStream<'_>) -> syn::Result<bool> {
    if !input.peek(token::Brace) {
        return Ok(false);
    }

    let fork = input.fork();
    let content;
    let _brace = braced!(content in fork);

    Ok(content.peek(Token![:]) || content.peek(Token![/]))
}

fn parse_control_marker(input: ParseStream<'_>) -> syn::Result<ControlMarker> {
    let content;
    let brace = braced!(content in input);
    let span = brace.span.join();

    if content.peek(Token![:]) {
        content.parse::<Token![:]>()?;
        let keyword = Ident::parse_any(&content)?;

        return match keyword.to_string().as_str() {
            "else" => {
                if content.is_empty() {
                    Ok(ControlMarker {
                        kind: MarkerKind::Else,
                        span,
                    })
                } else {
                    content.parse::<Token![if]>()?;
                    let condition = content.parse::<Expr>()?;
                    ensure_empty(&content, "unexpected tokens after `else if` condition")?;
                    Ok(ControlMarker {
                        kind: MarkerKind::ElseIf(condition),
                        span,
                    })
                }
            }
            other => Err(syn::Error::new(
                keyword.span(),
                format!("expected `else` or `else if` after `:`, found `{other}`"),
            )),
        };
    }

    if content.peek(Token![/]) {
        content.parse::<Token![/]>()?;
        let keyword = Ident::parse_any(&content)?;
        ensure_empty(&content, "unexpected tokens after closing directive")?;

        return match keyword.to_string().as_str() {
            "if" => Ok(ControlMarker {
                kind: MarkerKind::CloseIf,
                span,
            }),
            "each" => Ok(ControlMarker {
                kind: MarkerKind::CloseEach,
                span,
            }),
            other => Err(syn::Error::new(
                keyword.span(),
                format!("unknown closing directive `{other}`"),
            )),
        };
    }

    Err(content.error("expected a control marker such as `{:else}` or `{/if}`"))
}

fn ensure_empty(input: ParseStream<'_>, message: &str) -> syn::Result<()> {
    if input.is_empty() {
        Ok(())
    } else {
        Err(input.error(message))
    }
}

fn starts_uppercase(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
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

struct ControlMarker {
    kind: MarkerKind,
    span: Span,
}

enum MarkerKind {
    Else,
    ElseIf(Expr),
    CloseIf,
    CloseEach,
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
    fn parses_static_boolean_and_dynamic_attributes() {
        let view = parse(r#"<input type="text" disabled class={name} tabindex={-1}>"#)
            .expect("view should parse");
        let Node::Element(input) = &view.nodes[0] else {
            panic!("expected an input element");
        };

        assert_eq!(input.attributes.len(), 4);
        assert!(matches!(input.attributes[0].value, AttributeValue::Text(_)));
        assert!(matches!(
            input.attributes[1].value,
            AttributeValue::Bool(true)
        ));
        assert!(matches!(
            input.attributes[2].value,
            AttributeValue::Dynamic(_)
        ));
        assert!(matches!(
            input.attributes[3].value,
            AttributeValue::Dynamic(_)
        ));
    }

    #[test]
    fn parses_components_props_and_children() {
        let view =
            parse(r#"<Layout><Greeting name={user.name} /></Layout>"#).expect("view should parse");
        let Node::Component(layout) = &view.nodes[0] else {
            panic!("expected a component");
        };

        assert_eq!(layout.name.value, "Layout");
        assert!(!layout.self_closing);
        assert_eq!(layout.children.len(), 1);

        let Node::Component(greeting) = &layout.children[0] else {
            panic!("expected a nested component");
        };
        assert_eq!(greeting.name.value, "Greeting");
        assert!(greeting.self_closing);
        assert_eq!(greeting.attributes.len(), 1);
        assert!(matches!(
            greeting.attributes[0].value,
            AttributeValue::Dynamic(_)
        ));
    }

    #[test]
    fn parses_paired_components_with_empty_children() {
        let view = parse("<Layout></Layout>").expect("view should parse");
        let Node::Component(layout) = &view.nodes[0] else {
            panic!("expected a component");
        };
        assert!(!layout.self_closing);
        assert!(layout.children.is_empty());
    }

    #[test]
    fn allows_children_prop_on_self_closing_components() {
        let view = parse("<Layout children={content} />").expect("view should parse");
        let Node::Component(layout) = &view.nodes[0] else {
            panic!("expected a component");
        };
        assert!(layout.self_closing);
        assert_eq!(layout.attributes.len(), 1);
        assert!(matches!(
            layout.attributes[0].value,
            AttributeValue::Dynamic(_)
        ));
    }

    #[test]
    fn parses_dynamic_expressions() {
        let view = parse("<p>{format!(\"Hello, {name}\")}</p>").expect("view should parse");
        let Node::Element(p) = &view.nodes[0] else {
            panic!("expected an element");
        };
        assert!(matches!(p.children[0], Node::Dynamic(_)));
    }

    #[test]
    fn parses_if_else_if_else() {
        let view = parse(
            r#"
            {#if first}
                "one"
            {:else if second}
                "two"
            {:else}
                "three"
            {/if}
            "#,
        )
        .expect("view should parse");

        let Node::If(conditional) = &view.nodes[0] else {
            panic!("expected an if block");
        };
        assert_eq!(conditional.branches.len(), 2);
        assert!(conditional.else_children.is_some());
    }

    #[test]
    fn parses_each_blocks() {
        let view = parse(r#"{#each users.iter() as user}<p>{user.name}</p>{/each}"#)
            .expect("view should parse");

        let Node::Each(each) = &view.nodes[0] else {
            panic!("expected an each block");
        };
        assert_eq!(each.children.len(), 1);
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
    fn parses_event_listeners_separately_from_attributes() {
        let view = parse(r#"<button class="primary" on:click={save}>"Save"</button>"#)
            .expect("view should parse");
        let Node::Element(button) = &view.nodes[0] else {
            panic!("expected a button");
        };

        assert_eq!(button.attributes.len(), 1);
        assert_eq!(button.events.len(), 1);
        assert_eq!(button.events[0].name.value, "click");
        assert!(matches!(
            button.attributes[0].value,
            AttributeValue::Text(_)
        ));
    }

    #[test]
    fn rejects_invalid_events_and_non_expression_handlers() {
        let invalid = parse(r#"<button on:click:foo={handler} />"#)
            .err()
            .expect("event name must be validated");
        assert!(
            invalid.to_string().contains("invalid HTML event name"),
            "{invalid}"
        );

        let non_expression = parse(r#"<button on:click="save" />"#)
            .err()
            .expect("event handlers must be expressions");
        assert!(non_expression
            .to_string()
            .contains("require a `{ handler }` expression"));
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
    fn rejects_unclosed_if_blocks() {
        let error = parse(r#"{#if ready}"text""#)
            .err()
            .expect("if block must be closed");
        assert!(error.to_string().contains("unclosed `#if`"));
    }

    #[test]
    fn rejects_mismatched_control_markers() {
        let error = parse(r#"{#if ready}"text"{/each}"#)
            .err()
            .expect("markers must match");
        assert!(error.to_string().contains("while closing an `#if`"));
    }

    #[test]
    fn rejects_unknown_directives() {
        let error = parse(r#"{#match value}{/match}"#)
            .err()
            .expect("unknown directive must fail");
        assert!(error.to_string().contains("unknown control directive"));
    }

    #[test]
    fn rejects_component_tags_with_separators() {
        let error = parse("<My-Widget />")
            .err()
            .expect("component tag must be one identifier");
        assert!(error.to_string().contains("single identifier"));
    }

    #[test]
    fn rejects_component_props_with_separators() {
        let error = parse(r#"<Card data-id="1" />"#)
            .err()
            .expect("component prop must be a Rust identifier");
        assert!(error.to_string().contains("valid Rust identifiers"));
    }

    #[test]
    fn rejects_duplicate_attributes() {
        let error = parse(r#"<div class="a" class="b"></div>"#)
            .err()
            .expect("duplicate attributes must be rejected");
        assert!(error.to_string().contains("duplicate attribute"));
    }

    #[test]
    fn rejects_children_prop_with_nested_content() {
        let error = parse(r#"<Layout children={first}><p>"second"</p></Layout>"#)
            .err()
            .expect("children cannot be passed twice");
        assert!(error
            .to_string()
            .contains("cannot be passed as a prop on a paired component"));
    }
}
