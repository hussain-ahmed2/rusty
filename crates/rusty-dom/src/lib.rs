//! Minimal DOM abstraction shared by the browser renderer and tests.
//!
//! [`Dom`] exposes only the operations the renderer needs. [`MockDom`] provides
//! an in-memory implementation for native tests. On WebAssembly, `WebDom` wraps
//! `web-sys` and translates browser events into [`rusty_core::Event`].

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::error::Error;
use std::fmt;

use rusty_core::{EventListener, EventName};

mod mock;

pub use mock::{MockDom, MockNode};

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
pub use web::{WebDom, WebNode};

/// An operation that failed while updating a DOM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomError {
    /// The browser environment is unavailable or a native API failed.
    Browser(String),
    /// The operation was not valid for the supplied node.
    InvalidOperation(String),
}

impl DomError {
    /// Creates a browser error from a message.
    #[must_use]
    pub fn browser(message: impl Into<String>) -> Self {
        Self::Browser(message.into())
    }

    /// Creates an invalid-operation error from a message.
    #[must_use]
    pub fn invalid_operation(message: impl Into<String>) -> Self {
        Self::InvalidOperation(message.into())
    }
}

impl fmt::Display for DomError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Browser(message) => write!(formatter, "browser DOM error: {message}"),
            Self::InvalidOperation(message) => {
                write!(formatter, "invalid DOM operation: {message}")
            }
        }
    }
}

impl Error for DomError {}

/// The kind of a DOM node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// A normal element.
    Element,
    /// A text node.
    Text,
    /// A comment node.
    Comment,
    /// An opaque trusted raw HTML node.
    RawHtml,
    /// Any other node kind, such as a document or fragment.
    Other,
}

/// A minimal DOM used by the renderer.
///
/// Node handles are cheap to clone and compare by identity. Implementations must
/// keep handles valid until the node is removed from its parent.
pub trait Dom {
    /// A node handle.
    type Node: Clone + PartialEq + Eq;

    /// Creates an element with the given tag.
    fn create_element(&mut self, tag: &str) -> Result<Self::Node, DomError>;

    /// Creates a text node.
    fn create_text(&mut self, text: &str) -> Result<Self::Node, DomError>;

    /// Creates an opaque node for trusted raw HTML.
    fn create_raw_html(&mut self, html: &str) -> Result<Self::Node, DomError>;

    /// Creates a comment node.
    fn create_comment(&mut self, text: &str) -> Result<Self::Node, DomError>;

    /// Returns the first child of a node.
    fn first_child(&self, parent: &Self::Node) -> Result<Option<Self::Node>, DomError>;

    /// Returns the next sibling of a node.
    fn next_sibling(&self, node: &Self::Node) -> Result<Option<Self::Node>, DomError>;

    /// Returns the kind of a node.
    fn node_kind(&self, node: &Self::Node) -> Result<NodeKind, DomError>;

    /// Returns the tag name for an element node.
    fn tag_name(&self, node: &Self::Node) -> Result<Option<String>, DomError>;

    /// Returns the content of a text node.
    fn text(&self, node: &Self::Node) -> Result<Option<String>, DomError>;

    /// Returns the content of a comment node.
    fn comment(&self, node: &Self::Node) -> Result<Option<String>, DomError>;

    /// Returns the trusted markup of a raw HTML node.
    fn raw_html(&self, node: &Self::Node) -> Result<Option<String>, DomError>;

    /// Returns attributes as name/value pairs, using `""` for boolean attributes.
    fn attributes(&self, node: &Self::Node) -> Result<Vec<(String, String)>, DomError>;

    /// Appends `child` to `parent`.
    fn append_child(&mut self, parent: &Self::Node, child: &Self::Node) -> Result<(), DomError>;

    /// Inserts `child` before `reference` under `parent`.
    fn insert_before(
        &mut self,
        parent: &Self::Node,
        child: &Self::Node,
        reference: &Self::Node,
    ) -> Result<(), DomError>;

    /// Removes `child` from `parent`.
    fn remove_child(&mut self, parent: &Self::Node, child: &Self::Node) -> Result<(), DomError>;

    /// Replaces `old_child` with `new_child` under `parent`.
    fn replace_child(
        &mut self,
        parent: &Self::Node,
        new_child: &Self::Node,
        old_child: &Self::Node,
    ) -> Result<(), DomError>;

    /// Sets a text attribute. Use an empty value for a true boolean attribute.
    fn set_attribute(&mut self, node: &Self::Node, name: &str, value: &str)
        -> Result<(), DomError>;

    /// Removes an attribute.
    fn remove_attribute(&mut self, node: &Self::Node, name: &str) -> Result<(), DomError>;

    /// Replaces the text of a text node.
    fn set_text(&mut self, node: &Self::Node, text: &str) -> Result<(), DomError>;

    /// Replaces the trusted markup of a raw HTML node.
    fn set_raw_html(&mut self, node: &Self::Node, html: &str) -> Result<(), DomError>;

    /// Adds or replaces a listener for the listener's event name.
    fn set_event_listener(
        &mut self,
        node: &Self::Node,
        listener: EventListener,
    ) -> Result<(), DomError>;

    /// Removes the listener for an event name.
    fn remove_event_listener(
        &mut self,
        node: &Self::Node,
        name: &EventName,
    ) -> Result<(), DomError>;
}
