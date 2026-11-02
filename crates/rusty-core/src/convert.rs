//! Conversions used by dynamic `view!` expressions and attributes.
//!
//! These traits are deliberately explicit rather than blanket implementations
//! over `Display`. That keeps coherence predictable, avoids surprising
//! conversions, and leaves room for reactive values to implement the same traits
//! in a later milestone.

use std::borrow::Cow;

use crate::{AttributeValue, Children, Element, HtmlElement};

/// A value that can become an [`Element`].
///
/// Implemented for elements, text-like values, numbers, booleans, optional
/// values, and collections. Custom application types should generally be
/// converted explicitly with [`Element::text`] or by rendering a component.
pub trait IntoElement {
    /// Converts the value into an element.
    #[must_use]
    fn into_element(self) -> Element;
}

impl IntoElement for Element {
    fn into_element(self) -> Self {
        self
    }
}

impl IntoElement for &Element {
    fn into_element(self) -> Element {
        self.clone()
    }
}

impl IntoElement for HtmlElement {
    fn into_element(self) -> Element {
        Element::Html(self)
    }
}

impl IntoElement for Children {
    fn into_element(self) -> Element {
        Children::into_element(self)
    }
}

impl IntoElement for String {
    fn into_element(self) -> Element {
        Element::Text(self)
    }
}

impl IntoElement for &str {
    fn into_element(self) -> Element {
        Element::text(self)
    }
}

impl IntoElement for &String {
    fn into_element(self) -> Element {
        Element::text(self.as_str())
    }
}

impl IntoElement for Cow<'_, str> {
    fn into_element(self) -> Element {
        Element::Text(self.into_owned())
    }
}

impl IntoElement for () {
    fn into_element(self) -> Element {
        Element::Fragment(Vec::new())
    }
}

impl<T: IntoElement> IntoElement for Option<T> {
    fn into_element(self) -> Element {
        match self {
            Some(value) => value.into_element(),
            None => Element::Fragment(Vec::new()),
        }
    }
}

impl<T: IntoElement> IntoElement for Vec<T> {
    fn into_element(self) -> Element {
        Element::fragment(self.into_iter().map(IntoElement::into_element))
    }
}

impl<T: IntoElement, const N: usize> IntoElement for [T; N] {
    fn into_element(self) -> Element {
        Element::fragment(self.into_iter().map(IntoElement::into_element))
    }
}

macro_rules! impl_display_into_element {
    ($($type:ty),+ $(,)?) => {
        $(
            impl IntoElement for $type {
                fn into_element(self) -> Element {
                    Element::text(self.to_string())
                }
            }
        )+
    };
}

impl_display_into_element!(
    char, bool, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64
);

/// Converts an iterator of element-like values into a fragment.
///
/// This is used by iteration rendering and is useful when composing elements
/// programmatically.
#[must_use]
pub fn fragment_from_iter<I, T>(values: I) -> Element
where
    I: IntoIterator<Item = T>,
    T: IntoElement,
{
    Element::fragment(values.into_iter().map(IntoElement::into_element))
}

/// A value that can become an [`AttributeValue`].
///
/// Dynamic HTML attributes use this trait. `None` and `false` omit the attribute
/// from rendered output.
pub trait IntoAttributeValue {
    /// Converts the value into an attribute value.
    #[must_use]
    fn into_attribute_value(self) -> AttributeValue;
}

impl IntoAttributeValue for AttributeValue {
    fn into_attribute_value(self) -> Self {
        self
    }
}

impl IntoAttributeValue for String {
    fn into_attribute_value(self) -> AttributeValue {
        AttributeValue::Text(self)
    }
}

impl IntoAttributeValue for &str {
    fn into_attribute_value(self) -> AttributeValue {
        AttributeValue::text(self)
    }
}

impl IntoAttributeValue for &String {
    fn into_attribute_value(self) -> AttributeValue {
        AttributeValue::text(self.as_str())
    }
}

impl IntoAttributeValue for Cow<'_, str> {
    fn into_attribute_value(self) -> AttributeValue {
        AttributeValue::Text(self.into_owned())
    }
}

impl IntoAttributeValue for bool {
    fn into_attribute_value(self) -> AttributeValue {
        AttributeValue::Bool(self)
    }
}

impl<T: IntoAttributeValue> IntoAttributeValue for Option<T> {
    fn into_attribute_value(self) -> AttributeValue {
        match self {
            Some(value) => value.into_attribute_value(),
            None => AttributeValue::None,
        }
    }
}

macro_rules! impl_display_into_attribute_value {
    ($($type:ty),+ $(,)?) => {
        $(
            impl IntoAttributeValue for $type {
                fn into_attribute_value(self) -> AttributeValue {
                    AttributeValue::Text(self.to_string())
                }
            }
        )+
    };
}

impl_display_into_attribute_value!(
    char, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_elements_and_text() {
        assert_eq!(
            Element::text("explicit").into_element(),
            Element::text("explicit")
        );
        assert_eq!("borrowed".into_element(), Element::text("borrowed"));
        assert_eq!(String::from("owned").into_element(), Element::text("owned"));
        assert_eq!(42_u32.into_element(), Element::text("42"));
        assert_eq!(true.into_element(), Element::text("true"));
    }

    #[test]
    fn converts_optional_values() {
        assert_eq!(Some("value").into_element(), Element::text("value"));
        assert_eq!(None::<&str>.into_element(), Element::fragment([]));
    }

    #[test]
    fn converts_collections_to_fragments() {
        let values = vec!["a", "b"];
        assert_eq!(
            values.into_element(),
            Element::fragment([Element::text("a"), Element::text("b")])
        );
        assert_eq!(
            ["c", "d"].into_element(),
            Element::fragment([Element::text("c"), Element::text("d")])
        );
    }

    #[test]
    fn converts_children() {
        let children = Children::new(Element::text("child"));
        assert_eq!(children.into_element(), Element::text("child"));
    }

    #[test]
    fn converts_attribute_values() {
        assert_eq!(
            "value".into_attribute_value(),
            AttributeValue::text("value")
        );
        assert_eq!(true.into_attribute_value(), AttributeValue::Bool(true));
        assert_eq!(7_u8.into_attribute_value(), AttributeValue::text("7"));
        assert_eq!(None::<&str>.into_attribute_value(), AttributeValue::None);
        assert_eq!(
            Some("present").into_attribute_value(),
            AttributeValue::text("present")
        );
    }

    #[test]
    fn fragments_an_iterator() {
        assert_eq!(
            fragment_from_iter(["a", "b"]),
            Element::fragment([Element::text("a"), Element::text("b")])
        );
    }
}
