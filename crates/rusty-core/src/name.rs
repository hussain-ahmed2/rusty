//! Validated HTML element and attribute names.
//!
//! Tag and attribute names cannot be escaped safely in HTML. Instead of trusting
//! raw strings at render time, Rusty validates names once at construction and
//! stores them in newtypes. The renderer can therefore emit names verbatim.

use std::error::Error;
use std::fmt;

/// Identifies which kind of name failed validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NameKind {
    /// An HTML element tag name, such as `main` or `my-widget`.
    Tag,
    /// An HTML attribute name, such as `class`, `data-id`, or `xlink:href`.
    Attribute,
}

impl NameKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Tag => "tag",
            Self::Attribute => "attribute",
        }
    }
}

/// Describes why a tag or attribute name was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidName {
    kind: NameKind,
    value: String,
    reason: &'static str,
}

impl InvalidName {
    fn new(kind: NameKind, value: String, reason: &'static str) -> Self {
        Self {
            kind,
            value,
            reason,
        }
    }

    /// Returns the kind of name that failed validation.
    #[must_use]
    pub const fn kind(&self) -> NameKind {
        self.kind
    }

    /// Returns the rejected value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Returns a human-readable explanation of the validation failure.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        self.reason
    }
}

impl fmt::Display for InvalidName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid HTML {} name {:?}: {}",
            self.kind.label(),
            self.value,
            self.reason
        )
    }
}

impl Error for InvalidName {}

/// A validated HTML element tag name.
///
/// Construction rejects empty names and characters that could break out of the
/// tag position. Use [`TagName::new`] or a `TryFrom` implementation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TagName(String);

impl TagName {
    /// Validates and constructs a tag name.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidName`] when the name is empty, does not start with an
    /// ASCII letter, or contains a character outside ASCII letters, digits, and
    /// `-`.
    pub fn new(name: impl Into<String>) -> Result<Self, InvalidName> {
        let name = name.into();
        match validate_tag_name(&name) {
            Ok(()) => Ok(Self(name)),
            Err(reason) => Err(InvalidName::new(NameKind::Tag, name, reason)),
        }
    }

    /// Returns the name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns `true` for HTML void elements such as `br` and `img`.
    #[must_use]
    pub fn is_void(&self) -> bool {
        crate::is_void_element(self.as_str())
    }
}

impl fmt::Display for TagName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for TagName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<&str> for TagName {
    type Error = InvalidName;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for TagName {
    type Error = InvalidName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// A validated HTML attribute name.
///
/// Attribute names allow letters, digits, `-`, `_`, `.`, and `:` so that common
/// forms such as `data-testid`, `aria-label`, and `xlink:href` are supported.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttributeName(String);

impl AttributeName {
    /// Validates and constructs an attribute name.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidName`] when the name is empty, does not start with an
    /// ASCII letter, `_`, or `:`, or contains an unsupported character.
    pub fn new(name: impl Into<String>) -> Result<Self, InvalidName> {
        let name = name.into();
        match validate_attribute_name(&name) {
            Ok(()) => Ok(Self(name)),
            Err(reason) => Err(InvalidName::new(NameKind::Attribute, name, reason)),
        }
    }

    /// Returns the name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AttributeName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for AttributeName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<&str> for AttributeName {
    type Error = InvalidName;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for AttributeName {
    type Error = InvalidName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

fn validate_tag_name(name: &str) -> Result<(), &'static str> {
    let Some(first) = name.chars().next() else {
        return Err("the name must not be empty");
    };

    if !first.is_ascii_alphabetic() {
        return Err("the name must start with an ASCII letter");
    }

    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err("the name may contain only ASCII letters, digits, and '-'");
    }

    Ok(())
}

fn validate_attribute_name(name: &str) -> Result<(), &'static str> {
    let Some(first) = name.chars().next() else {
        return Err("the name must not be empty");
    };

    if !(first.is_ascii_alphabetic() || first == '_' || first == ':') {
        return Err("the name must start with an ASCII letter, '_', or ':'");
    }

    if !name.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':')
    }) {
        return Err("the name may contain only ASCII letters, digits, '-', '_', '.', and ':'");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_standard_and_custom_tags() {
        for name in ["main", "h1", "my-widget", "x-foo"] {
            assert!(TagName::new(name).is_ok(), "{name} should be valid");
        }
    }

    #[test]
    fn rejects_unsafe_tag_names() {
        for name in ["", "1main", "main>", "main onclick=x", "main/", "ma in"] {
            assert!(TagName::new(name).is_err(), "{name:?} should be invalid");
        }
    }

    #[test]
    fn tag_error_reports_kind_and_value() {
        let error = TagName::new("bad>tag").expect_err("name must be rejected");
        assert_eq!(error.kind(), NameKind::Tag);
        assert_eq!(error.value(), "bad>tag");
        assert!(error.to_string().contains("invalid HTML tag name"));
    }

    #[test]
    fn accepts_common_attribute_names() {
        for name in [
            "class",
            "id",
            "data-testid",
            "aria-label",
            "xlink:href",
            "_private",
        ] {
            assert!(
                AttributeName::new(name).is_ok(),
                "{name} should be a valid attribute"
            );
        }
    }

    #[test]
    fn rejects_unsafe_attribute_names() {
        for name in ["", "1class", "class=", "on click", "class\"", "class>"] {
            assert!(
                AttributeName::new(name).is_err(),
                "{name:?} should be invalid"
            );
        }
    }

    #[test]
    fn try_from_matches_new() {
        let tag = TagName::try_from("main").expect("valid tag");
        assert_eq!(tag.as_str(), "main");
        assert!(AttributeName::try_from(String::from("class")).is_ok());
    }
}
