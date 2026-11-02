//! Syntax tree produced by the `view!` parser.
//!
//! This tree is independent of `proc_macro` token types so it can be reasoned
//! about and tested without expanding a macro.

use proc_macro2::Span;
use syn::LitStr;

/// A complete `view!` invocation.
pub(crate) struct View {
    /// Top-level nodes. More than one produces a fragment.
    pub(crate) nodes: Vec<Node>,
}

/// A node that can appear inside an element or at the top level.
pub(crate) enum Node {
    /// A nested HTML element.
    Element(HtmlElement),
    /// A string-literal text node.
    Text(LitStr),
}

/// An HTML element parsed from the macro input.
pub(crate) struct HtmlElement {
    /// The validated-at-expansion tag name.
    pub(crate) tag: Name,
    /// Attributes in declaration order.
    pub(crate) attributes: Vec<Attribute>,
    /// Child nodes in render order.
    pub(crate) children: Vec<Node>,
}

/// An attribute parsed from an opening tag.
pub(crate) struct Attribute {
    /// The attribute name.
    pub(crate) name: Name,
    /// The attribute value.
    pub(crate) value: AttributeValue,
}

/// The value of a parsed attribute.
pub(crate) enum AttributeValue {
    /// A static string literal.
    Text(LitStr),
    /// A boolean attribute.
    Bool(bool),
}

/// A name with the span used for diagnostics.
pub(crate) struct Name {
    /// Normalized name (raw identifiers have their `r#` prefix removed).
    pub(crate) value: String,
    /// Location in the caller's source.
    pub(crate) span: Span,
}
