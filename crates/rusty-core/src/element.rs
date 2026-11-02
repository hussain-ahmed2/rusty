//! Owned element tree used by server rendering and, later, hydration.
//!
//! The tree is immutable and fully materialized. Dynamic and reactive behavior
//! will be layered on top in later milestones; keeping the foundation owned
//! avoids shared mutable state and reference cycles.

use crate::{AttributeName, TagName};

/// A node in a Rusty render tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Element {
    /// A normal HTML element with validated names, attributes, and children.
    Html(HtmlElement),
    /// Plain text. Escaped by the renderer.
    Text(String),
    /// An ordered list of roots rendered without a wrapper element.
    Fragment(Vec<Element>),
    /// Trusted markup emitted verbatim.
    ///
    /// This bypasses escaping. Never construct it from untrusted input.
    RawHtml(String),
}

impl Element {
    /// Creates a text node.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    /// Creates a fragment from an iterator of child nodes.
    #[must_use]
    pub fn fragment(children: impl IntoIterator<Item = Self>) -> Self {
        Self::Fragment(children.into_iter().collect())
    }

    /// Creates raw, unescaped HTML.
    ///
    /// # Safety boundary
    ///
    /// This method is safe Rust, but the value is emitted without escaping. Only
    /// pass trusted, already-sanitized markup.
    #[must_use]
    pub fn raw_html(html: impl Into<String>) -> Self {
        Self::RawHtml(html.into())
    }

    /// Creates an HTML element node.
    #[must_use]
    pub fn html(tag: TagName, attributes: Vec<Attribute>, children: Vec<Self>) -> Self {
        Self::Html(HtmlElement::new(tag, attributes, children))
    }
}

/// An HTML element with a validated tag name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlElement {
    tag: TagName,
    attributes: Vec<Attribute>,
    children: Vec<Element>,
}

impl HtmlElement {
    /// Creates an element from validated parts.
    ///
    /// The renderer rejects children on void elements at serialization time.
    #[must_use]
    pub fn new(tag: TagName, attributes: Vec<Attribute>, children: Vec<Element>) -> Self {
        Self {
            tag,
            attributes,
            children,
        }
    }

    /// Returns the validated tag name.
    #[must_use]
    pub const fn tag(&self) -> &TagName {
        &self.tag
    }

    /// Returns the element's attributes in declaration order.
    #[must_use]
    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }

    /// Returns the element's children in render order.
    #[must_use]
    pub fn children(&self) -> &[Element] {
        &self.children
    }
}

/// A single HTML attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    name: AttributeName,
    value: AttributeValue,
}

impl Attribute {
    /// Creates an attribute from a validated name and value.
    #[must_use]
    pub const fn new(name: AttributeName, value: AttributeValue) -> Self {
        Self { name, value }
    }

    /// Returns the validated attribute name.
    #[must_use]
    pub const fn name(&self) -> &AttributeName {
        &self.name
    }

    /// Returns the attribute value.
    #[must_use]
    pub const fn value(&self) -> &AttributeValue {
        &self.value
    }
}

/// The value of an HTML attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeValue {
    /// A text value. Escaped by the renderer.
    Text(String),
    /// A boolean attribute. `false` is omitted from output.
    Bool(bool),
}

impl AttributeValue {
    /// Creates a text attribute value.
    #[must_use]
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    /// Creates a boolean attribute value.
    #[must_use]
    pub const fn boolean(value: bool) -> Self {
        Self::Bool(value)
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn builds_an_element_tree() {
        let element = Element::html(
            tag("main"),
            vec![],
            vec![Element::html(
                tag("h1"),
                vec![attribute("class", "title")],
                vec![Element::text("Hello")],
            )],
        );

        let Element::Html(main) = &element else {
            panic!("expected an HTML element");
        };
        assert_eq!(main.tag().as_str(), "main");
        assert_eq!(main.children().len(), 1);
    }

    #[test]
    fn fragments_preserve_order() {
        let fragment = Element::fragment([Element::text("a"), Element::text("b")]);
        let Element::Fragment(children) = fragment else {
            panic!("expected a fragment");
        };
        assert_eq!(children, vec![Element::text("a"), Element::text("b")]);
    }

    #[test]
    fn boolean_attribute_values_are_typed() {
        let attribute = Attribute::new(
            AttributeName::new("disabled").expect("valid attribute"),
            AttributeValue::boolean(true),
        );
        assert_eq!(attribute.value(), &AttributeValue::Bool(true));
    }
}
