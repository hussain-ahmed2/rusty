//! Syntax tree produced by the `view!` parser.
//!
//! The tree is independent of `proc_macro` expansion so that parsing and
//! diagnostics can be tested directly.

use proc_macro2::Span;
use syn::{Expr, Ident, LitStr, Pat};

/// A complete `view!` invocation.
pub(crate) struct View {
    /// Top-level nodes. More than one produces a fragment.
    pub(crate) nodes: Vec<Node>,
}

/// A node that can appear inside an element or at the top level.
pub(crate) enum Node {
    /// A nested HTML element.
    Element(HtmlElement),
    /// A component invocation.
    Component(ComponentElement),
    /// A string-literal text node.
    Text(LitStr),
    /// A `{ ... }` Rust expression converted through `IntoElement`.
    Dynamic(Expr),
    /// An `{#if}` / `{:else if}` / `{:else}` block.
    If(IfNode),
    /// An `{#each}` block.
    Each(Box<EachNode>),
}

/// An HTML element parsed from the macro input.
pub(crate) struct HtmlElement {
    /// The validated-at-expansion tag name.
    pub(crate) tag: Name,
    /// Attributes in declaration order.
    pub(crate) attributes: Vec<Attribute>,
    /// Client-side event listeners in declaration order.
    pub(crate) events: Vec<Event>,
    /// Child nodes in render order.
    pub(crate) children: Vec<Node>,
}

/// A component invocation parsed from the macro input.
pub(crate) struct ComponentElement {
    /// The component type name.
    pub(crate) name: Name,
    /// Props in declaration order.
    pub(crate) attributes: Vec<Attribute>,
    /// Child nodes passed as the `children` prop.
    pub(crate) children: Vec<Node>,
    /// Whether the tag was written as `<Name />` rather than `<Name>...</Name>`.
    pub(crate) self_closing: bool,
}

/// An attribute or component prop.
pub(crate) struct Attribute {
    /// The attribute name.
    pub(crate) name: Name,
    /// The attribute value.
    pub(crate) value: AttributeValue,
}

/// An event listener written as `on:name={handler}`.
pub(crate) struct Event {
    /// The validated event name, without the `on:` prefix.
    pub(crate) name: Name,
    /// The handler expression.
    pub(crate) handler: Expr,
}

/// The value of a parsed attribute.
pub(crate) enum AttributeValue {
    /// A static string literal.
    Text(LitStr),
    /// A boolean attribute.
    Bool(bool),
    /// A `{ ... }` Rust expression.
    Dynamic(Expr),
}

/// A conditional block.
pub(crate) struct IfNode {
    /// The `if` branch followed by any `else if` branches.
    pub(crate) branches: Vec<IfBranch>,
    /// The optional final `else` branch.
    pub(crate) else_children: Option<Vec<Node>>,
}

/// One condition and its body.
pub(crate) struct IfBranch {
    /// The Rust boolean expression.
    pub(crate) condition: Expr,
    /// The body rendered when the condition is true.
    pub(crate) children: Vec<Node>,
}

/// An iteration block.
pub(crate) struct EachNode {
    /// The iterable expression.
    pub(crate) iterable: Expr,
    /// The binding pattern for each item.
    pub(crate) pattern: Pat,
    /// The body rendered for each item.
    pub(crate) children: Vec<Node>,
}

/// A name with the span used for diagnostics.
pub(crate) struct Name {
    /// Normalized name (raw identifiers have their `r#` prefix removed).
    pub(crate) value: String,
    /// The original identifier, preserving rawness for generated props.
    pub(crate) ident: Ident,
    /// Whether the name contained `-` or `:` separators.
    pub(crate) has_separators: bool,
    /// Location in the caller's source.
    pub(crate) span: Span,
}
