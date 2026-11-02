//! Virtual node representation used by the client renderer.
//!
//! [`VNode`] is an owned snapshot of an [`Element`] tree.
//! It preserves validated names, attributes, event listeners, and children so
//! the renderer can diff one snapshot against the next without consulting the
//! browser DOM. Fragments remain in the tree and are flattened by the renderer.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::error::Error;
use std::fmt;

use rusty_core::{is_void_element, Attribute, Element, EventListener, TagName};

mod hydration;

pub use hydration::HydrationId;

/// A client-renderable node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VNode {
    /// A normal HTML element.
    Element(VElement),
    /// A text node.
    Text(String),
    /// Opaque trusted markup.
    RawHtml(String),
    /// An ordered list of nodes without a wrapper element.
    Fragment(Vec<VNode>),
}

impl VNode {
    /// Creates a text node.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    /// Creates raw, unescaped markup.
    #[must_use]
    pub fn raw_html(html: impl Into<String>) -> Self {
        Self::RawHtml(html.into())
    }

    /// Creates a fragment.
    #[must_use]
    pub fn fragment(children: impl IntoIterator<Item = Self>) -> Self {
        Self::Fragment(children.into_iter().collect())
    }

    /// Returns `true` for fragments.
    #[must_use]
    pub const fn is_fragment(&self) -> bool {
        matches!(self, Self::Fragment(_))
    }
}

/// An HTML element in the virtual tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VElement {
    tag: TagName,
    attributes: Vec<Attribute>,
    listeners: Vec<EventListener>,
    children: Vec<VNode>,
}

impl VElement {
    /// Creates an element from validated parts.
    #[must_use]
    pub fn new(
        tag: TagName,
        attributes: Vec<Attribute>,
        listeners: Vec<EventListener>,
        children: Vec<VNode>,
    ) -> Self {
        Self {
            tag,
            attributes,
            listeners,
            children,
        }
    }

    /// Returns the validated tag name.
    #[must_use]
    pub const fn tag(&self) -> &TagName {
        &self.tag
    }

    /// Returns attributes in declaration order.
    #[must_use]
    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }

    /// Returns client-side event listeners in declaration order.
    #[must_use]
    pub fn listeners(&self) -> &[EventListener] {
        &self.listeners
    }

    /// Returns child nodes in render order.
    #[must_use]
    pub fn children(&self) -> &[VNode] {
        &self.children
    }
}

impl TryFrom<Element> for VNode {
    type Error = VNodeError;

    fn try_from(element: Element) -> Result<Self, Self::Error> {
        match element {
            Element::Html(html) => {
                if html.tag().is_void() && !html.children().is_empty() {
                    return Err(VNodeError::ChildrenInVoidElement {
                        tag: html.tag().to_string(),
                    });
                }

                let children = html
                    .children()
                    .iter()
                    .cloned()
                    .map(Self::try_from)
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(Self::Element(VElement::new(
                    html.tag().clone(),
                    html.attributes().to_vec(),
                    html.listeners().to_vec(),
                    children,
                )))
            }
            Element::Text(text) => Ok(Self::Text(text)),
            Element::RawHtml(html) => Ok(Self::RawHtml(html)),
            Element::Fragment(children) => children
                .into_iter()
                .map(Self::try_from)
                .collect::<Result<Vec<_>, _>>()
                .map(Self::Fragment),
        }
    }
}

impl TryFrom<&Element> for VNode {
    type Error = VNodeError;

    fn try_from(element: &Element) -> Result<Self, Self::Error> {
        Self::try_from(element.clone())
    }
}

/// Describes why an element tree cannot become a client tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VNodeError {
    /// A void element was given children.
    ChildrenInVoidElement {
        /// The offending tag name.
        tag: String,
    },
}

impl fmt::Display for VNodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ChildrenInVoidElement { tag } => {
                write!(formatter, "void element <{tag}> cannot have children")
            }
        }
    }
}

impl Error for VNodeError {}

/// Returns `true` when `tag` is a void element.
///
/// This is a convenience wrapper around the core HTML table for renderer code.
#[must_use]
pub fn is_void_tag(tag: &TagName) -> bool {
    is_void_element(tag.as_str())
}

#[cfg(test)]
mod tests {
    use rusty_core::{AttributeName, AttributeValue, EventName};

    use super::*;

    fn tag(name: &str) -> TagName {
        TagName::new(name).expect("valid tag")
    }

    fn attribute(name: &str, value: &str) -> Attribute {
        Attribute::new(
            AttributeName::new(name).expect("valid attribute"),
            AttributeValue::text(value),
        )
    }

    #[test]
    fn converts_elements_text_and_fragments() {
        let element = Element::html(
            tag("main"),
            vec![attribute("class", "app")],
            vec![Element::text("hello")],
        );

        let vnode = VNode::try_from(element).expect("conversion succeeds");
        let VNode::Element(main) = vnode else {
            panic!("expected an element");
        };
        assert_eq!(main.tag().as_str(), "main");
        assert_eq!(main.attributes()[0].name().as_str(), "class");
        assert_eq!(main.children(), &[VNode::text("hello")]);

        assert_eq!(
            VNode::try_from(Element::fragment([Element::text("a"), Element::text("b")]))
                .expect("conversion succeeds"),
            VNode::fragment([VNode::text("a"), VNode::text("b")])
        );
    }

    #[test]
    fn preserves_event_listeners() {
        let listener = EventListener::new(EventName::new("click").expect("valid event"), |_| {});
        let element = Element::html_with_events(
            tag("button"),
            Vec::new(),
            vec![Element::text("go")],
            vec![listener],
        );

        let VNode::Element(button) = VNode::try_from(element).expect("conversion succeeds") else {
            panic!("expected an element");
        };
        assert_eq!(button.listeners().len(), 1);
        assert_eq!(button.listeners()[0].name().as_str(), "click");
    }

    #[test]
    fn rejects_children_on_void_elements() {
        let element = Element::html(tag("br"), Vec::new(), vec![Element::text("invalid")]);
        assert_eq!(
            VNode::try_from(element),
            Err(VNodeError::ChildrenInVoidElement {
                tag: String::from("br")
            })
        );
    }

    #[test]
    fn recognizes_void_tags() {
        assert!(is_void_tag(&tag("img")));
        assert!(!is_void_tag(&tag("div")));
    }
}
