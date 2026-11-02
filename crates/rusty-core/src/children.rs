//! Typed children passed to parent components.

use crate::Element;

/// The children supplied between an opening and closing component tag.
///
/// A `children` prop should use this type:
///
/// ```
/// use rusty_core::{Children, Element};
///
/// fn wrap(children: Children) -> Element {
///     Element::html(
///         rusty_core::TagName::new("main").expect("valid tag"),
///         Vec::new(),
///         vec![children.into_element()],
///     )
/// }
/// ```
///
/// The contents are stored as a single [`Element`], normally a fragment. This
/// keeps multiple child nodes ordered without introducing a wrapper element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Children(Element);

impl Children {
    /// Wraps an element tree as children.
    #[must_use]
    pub fn new(element: Element) -> Self {
        Self(element)
    }

    /// Creates an empty children value.
    #[must_use]
    pub fn empty() -> Self {
        Self(Element::fragment(std::iter::empty()))
    }

    /// Consumes the value and returns the underlying element tree.
    #[must_use]
    pub fn into_element(self) -> Element {
        self.0
    }

    /// Borrows the underlying element tree.
    #[must_use]
    pub const fn as_element(&self) -> &Element {
        &self.0
    }

    /// Returns `true` when the children contain no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(&self.0, Element::Fragment(children) if children.is_empty())
    }
}

impl Default for Children {
    fn default() -> Self {
        Self::empty()
    }
}

impl From<Element> for Children {
    fn from(element: Element) -> Self {
        Self::new(element)
    }
}

impl From<Children> for Element {
    fn from(children: Children) -> Self {
        children.into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_children_are_empty() {
        let children = Children::default();
        assert!(children.is_empty());
        assert_eq!(children.into_element(), Element::fragment([]));
    }

    #[test]
    fn wraps_and_unwraps_elements() {
        let children = Children::from(Element::text("hello"));
        assert!(!children.is_empty());
        assert_eq!(children.into_element(), Element::text("hello"));
    }
}
