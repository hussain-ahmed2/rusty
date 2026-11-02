//! Deterministic node markers shared by server rendering and hydration.
//!
//! Hydration output places a comment marker before every DOM node. Markers are
//! assigned in pre-order after fragments are flattened, so the server and client
//! traverse the same sequence for the same tree.

use std::fmt;

const MARKER_PREFIX: &str = "rusty:";

/// A deterministic identity assigned to a hydrated DOM node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HydrationId(u64);

impl HydrationId {
    /// Creates an identity from its traversal index.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the traversal index.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Returns the marker comment content, such as `rusty:4`.
    #[must_use]
    pub fn content(self) -> String {
        format!("{MARKER_PREFIX}{}", self.0)
    }

    /// Returns the full marker comment, such as `<!--rusty:4-->`.
    #[must_use]
    pub fn marker(self) -> String {
        format!("<!--{}-->", self.content())
    }

    /// Parses marker comment content.
    ///
    /// Returns `None` when the comment is not a Rusty hydration marker.
    #[must_use]
    pub fn parse(comment: &str) -> Option<Self> {
        comment
            .strip_prefix(MARKER_PREFIX)
            .and_then(|value| value.parse().ok())
            .map(Self)
    }
}

impl fmt::Display for HydrationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_parses_markers() {
        let id = HydrationId::new(42);
        assert_eq!(id.content(), "rusty:42");
        assert_eq!(id.marker(), "<!--rusty:42-->");
        assert_eq!(HydrationId::parse("rusty:42"), Some(id));
        assert_eq!(HydrationId::parse("other:42"), None);
        assert_eq!(HydrationId::parse("rusty:not-a-number"), None);
    }
}
