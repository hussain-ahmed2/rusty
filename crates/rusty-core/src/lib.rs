//! Core types shared by every Rusty crate.
//!
//! This crate deliberately has no rendering or runtime dependencies. It defines
//! the validated names and the owned element tree that the rest of the framework
//! builds on.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod children;
mod component;
mod convert;
mod element;
mod name;

pub use children::Children;
pub use component::Component;
pub use convert::{fragment_from_iter, IntoAttributeValue, IntoElement};
pub use element::{Attribute, AttributeValue, Element, HtmlElement};
pub use name::{AttributeName, InvalidName, NameKind, TagName};

/// HTML elements that must not have children or an explicit closing tag.
///
/// The list follows the HTML Living Standard's void-element set.
pub const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Returns `true` when `name` is a void HTML element.
///
/// This is used by both the macro (for early diagnostics) and the server
/// renderer (as a structural safety check).
#[must_use]
pub fn is_void_element(name: &str) -> bool {
    VOID_ELEMENTS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_void_elements() {
        assert!(is_void_element("br"));
        assert!(is_void_element("img"));
        assert!(!is_void_element("main"));
    }
}
