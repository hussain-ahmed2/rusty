//! Server-side HTML rendering for Rusty.
//!
//! Rendering is deterministic and escapes all text and attribute values by
//! default. Only [`rusty_core::Element::raw_html`] bypasses escaping, and it is
//! intentionally explicit.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::error::Error;
use std::fmt;

use rusty_core::{AttributeValue, Element, HtmlElement};
use rusty_hydrate::{HydrationError, HydrationSession};
use rusty_vdom::HydrationId;

/// An error encountered while serializing an element tree.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderError {
    /// A void element was given children.
    ///
    /// Void elements such as `br` and `img` cannot have content in HTML. The
    /// renderer rejects this structure rather than emitting invalid markup.
    VoidElementWithChildren {
        /// The offending tag name.
        tag: String,
    },
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VoidElementWithChildren { tag } => {
                write!(formatter, "void element <{tag}> cannot have children")
            }
        }
    }
}

impl Error for RenderError {}

/// An error from rendering hydration markup.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HydrationRenderError {
    /// The element tree could not be serialized.
    Render(RenderError),
    /// Hydration state was unavailable for server rendering.
    Protocol(HydrationError),
}

impl fmt::Display for HydrationRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Render(error) => error.fmt(formatter),
            Self::Protocol(error) => error.fmt(formatter),
        }
    }
}

impl Error for HydrationRenderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Render(error) => Some(error),
            Self::Protocol(error) => Some(error),
        }
    }
}

impl From<RenderError> for HydrationRenderError {
    fn from(error: RenderError) -> Self {
        Self::Render(error)
    }
}

impl From<HydrationError> for HydrationRenderError {
    fn from(error: HydrationError) -> Self {
        Self::Protocol(error)
    }
}

/// Renders an element tree to an HTML string.
///
/// Text inside elements such as `<script>` and `<style>` is escaped like any
/// other text. Use [`rusty_core::Element::raw_html`] with trusted content when
/// raw text-element content is required.
///
/// # Errors
///
/// Returns [`RenderError`] when the tree cannot be represented as valid HTML,
/// such as children on a void element.
pub fn render_to_string(element: &Element) -> Result<String, RenderError> {
    let mut output = String::new();
    render_element(element, &mut output)?;
    Ok(output)
}

fn render_element(element: &Element, output: &mut String) -> Result<(), RenderError> {
    match element {
        Element::Html(html) => render_html(html, output),
        Element::Text(text) => {
            escape_text_into(text, output);
            Ok(())
        }
        Element::Fragment(children) => {
            for child in children {
                render_element(child, output)?;
            }
            Ok(())
        }
        Element::RawHtml(html) => {
            output.push_str(html);
            Ok(())
        }
    }
}

fn render_html(element: &HtmlElement, output: &mut String) -> Result<(), RenderError> {
    validate_html(element)?;
    write_open_tag(element, output);

    if !element.tag().is_void() {
        for child in element.children() {
            render_element(child, output)?;
        }

        write_close_tag(element, output);
    }

    Ok(())
}

/// Renders hydration markup with deterministic node markers.
///
/// The output must be placed inside the container that will be hydrated. Call
/// [`HydrationSession::state_script`] separately and place the state script
/// outside that container.
///
/// # Errors
///
/// Returns [`HydrationRenderError`] when the tree is invalid or the session is
/// not a server session.
pub fn render_hydrated(
    element: &Element,
    session: &HydrationSession,
) -> Result<String, HydrationRenderError> {
    if !session.is_server() {
        return Err(HydrationError::NotServer.into());
    }

    let mut output = String::new();
    let mut id = HydrationId::new(0);
    render_hydration_element(element, &mut id, &mut output)?;
    Ok(output)
}

fn render_hydration_element(
    element: &Element,
    id: &mut HydrationId,
    output: &mut String,
) -> Result<(), HydrationRenderError> {
    match element {
        Element::Html(html) => {
            write_marker(id, output);
            validate_html(html)?;
            write_open_tag(html, output);

            if !html.tag().is_void() {
                for child in html.children() {
                    render_hydration_element(child, id, output)?;
                }

                write_close_tag(html, output);
            }
        }
        Element::Text(text) => {
            write_marker(id, output);
            escape_text_into(text, output);
        }
        Element::Fragment(children) => {
            for child in children {
                render_hydration_element(child, id, output)?;
            }
        }
        Element::RawHtml(html) => {
            write_marker(id, output);
            output.push_str("<rusty-raw-html style=\"display: contents\">");
            output.push_str(html);
            output.push_str("</rusty-raw-html>");
        }
    }

    Ok(())
}

fn write_marker(id: &mut HydrationId, output: &mut String) {
    output.push_str(&id.marker());
    *id = HydrationId::new(id.value() + 1);
}

fn validate_html(element: &HtmlElement) -> Result<(), RenderError> {
    if element.tag().is_void() && !element.children().is_empty() {
        return Err(RenderError::VoidElementWithChildren {
            tag: element.tag().as_str().to_owned(),
        });
    }

    Ok(())
}

fn write_open_tag(element: &HtmlElement, output: &mut String) {
    output.push('<');
    output.push_str(element.tag().as_str());

    for attribute in element.attributes() {
        match attribute.value() {
            AttributeValue::Text(value) => {
                output.push(' ');
                output.push_str(attribute.name().as_str());
                output.push_str("=\"");
                escape_attribute_into(value, output);
                output.push('"');
            }
            AttributeValue::Bool(true) => {
                output.push(' ');
                output.push_str(attribute.name().as_str());
            }
            AttributeValue::Bool(false) | AttributeValue::None => {}
        }
    }

    output.push('>');
}

fn write_close_tag(element: &HtmlElement, output: &mut String) {
    output.push_str("</");
    output.push_str(element.tag().as_str());
    output.push('>');
}

fn escape_text_into(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(character),
        }
    }
}

fn escape_attribute_into(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#x27;"),
            _ => output.push(character),
        }
    }
}

#[cfg(test)]
mod tests {
    use rusty_core::{Attribute, AttributeName, AttributeValue, Element, TagName};

    use super::*;

    fn tag(name: &str) -> TagName {
        TagName::new(name).expect("test tag should be valid")
    }

    fn attribute(name: &str, value: &str) -> Attribute {
        Attribute::new(
            AttributeName::new(name).expect("test attribute should be valid"),
            AttributeValue::text(value),
        )
    }

    fn element(tag_name: &str, attributes: Vec<Attribute>, children: Vec<Element>) -> Element {
        Element::html(tag(tag_name), attributes, children)
    }

    #[test]
    fn renders_the_acceptance_example() {
        let page = element(
            "main",
            vec![],
            vec![
                element("h1", vec![], vec![Element::text("Hello from Rusty")]),
                element(
                    "p",
                    vec![],
                    vec![Element::text(
                        "A Next.js-inspired framework built with Rust.",
                    )],
                ),
            ],
        );

        assert_eq!(
            render_to_string(&page).expect("rendering should succeed"),
            "<main><h1>Hello from Rusty</h1><p>A Next.js-inspired framework built with Rust.</p></main>"
        );
    }

    #[test]
    fn renders_fragments_without_a_wrapper() {
        let page = Element::fragment([Element::text("a"), Element::text("b")]);
        assert_eq!(
            render_to_string(&page).expect("rendering should succeed"),
            "ab"
        );
    }

    #[test]
    fn escapes_text_nodes() {
        let page = element("p", vec![], vec![Element::text("5 < 6 & 7 > 3")]);

        assert_eq!(
            render_to_string(&page).expect("rendering should succeed"),
            "<p>5 &lt; 6 &amp; 7 &gt; 3</p>"
        );
    }

    #[test]
    fn escapes_attribute_values() {
        let page = element(
            "p",
            vec![attribute("title", "\"quoted\" & <tag> 'single'")],
            vec![],
        );

        assert_eq!(
            render_to_string(&page).expect("rendering should succeed"),
            "<p title=\"&quot;quoted&quot; &amp; &lt;tag&gt; &#x27;single&#x27;\"></p>"
        );
    }

    #[test]
    fn renders_boolean_attributes() {
        let page = element(
            "input",
            vec![
                Attribute::new(
                    AttributeName::new("disabled").expect("valid attribute"),
                    AttributeValue::boolean(true),
                ),
                Attribute::new(
                    AttributeName::new("checked").expect("valid attribute"),
                    AttributeValue::boolean(false),
                ),
            ],
            vec![],
        );

        assert_eq!(
            render_to_string(&page).expect("rendering should succeed"),
            "<input disabled>"
        );
    }

    #[test]
    fn renders_void_elements_without_end_tags() {
        let page = element("br", vec![], vec![]);
        assert_eq!(
            render_to_string(&page).expect("rendering should succeed"),
            "<br>"
        );
    }

    #[test]
    fn emits_raw_html_only_when_explicitly_requested() {
        let page = element(
            "div",
            vec![],
            vec![Element::raw_html("<strong>trusted</strong>")],
        );

        assert_eq!(
            render_to_string(&page).expect("rendering should succeed"),
            "<div><strong>trusted</strong></div>"
        );
    }

    #[test]
    fn renders_hydration_markers_and_raw_html_wrappers() {
        let page = element(
            "main",
            vec![],
            vec![
                element("h1", vec![], vec![Element::text("Title")]),
                Element::raw_html("<b>raw</b>"),
            ],
        );
        let session = HydrationSession::server();

        assert_eq!(
            render_hydrated(&page, &session).expect("hydrate render"),
            "<!--rusty:0--><main><!--rusty:1--><h1><!--rusty:2-->Title</h1><!--rusty:3--><rusty-raw-html style=\"display: contents\"><b>raw</b></rusty-raw-html></main>"
        );
    }

    #[test]
    fn hydrates_fragments_by_flattening_markers() {
        let page = Element::fragment([
            Element::text("a"),
            element("div", vec![], vec![]),
            Element::text("b"),
        ]);
        let session = HydrationSession::server();

        assert_eq!(
            render_hydrated(&page, &session).expect("hydrate render"),
            "<!--rusty:0-->a<!--rusty:1--><div></div><!--rusty:2-->b"
        );
    }

    #[test]
    fn hydration_rendering_requires_a_server_session() {
        let page = Element::text("hello");
        assert_eq!(
            render_hydrated(&page, &HydrationSession::client()),
            Err(HydrationRenderError::Protocol(HydrationError::NotServer))
        );
    }

    #[test]
    fn rejects_children_on_void_elements() {
        let page = element("br", vec![], vec![Element::text("nope")]);
        assert_eq!(
            render_to_string(&page),
            Err(RenderError::VoidElementWithChildren {
                tag: String::from("br"),
            })
        );
    }

    #[test]
    fn renders_custom_elements_and_namespaced_attributes() {
        let page = element(
            "my-widget",
            vec![attribute("aria-label", "Widget")],
            vec![Element::text("content")],
        );

        assert_eq!(
            render_to_string(&page).expect("rendering should succeed"),
            "<my-widget aria-label=\"Widget\">content</my-widget>"
        );
    }
}
