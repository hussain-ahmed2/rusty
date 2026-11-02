//! Rendering integration for reactive values.

use rusty_core::{AttributeValue, Element, IntoAttributeValue, IntoElement};

use crate::{Memo, Signal};

impl<T> IntoElement for Signal<T>
where
    T: IntoElement + Clone + 'static,
{
    fn into_element(self) -> Element {
        self.get().into_element()
    }
}

impl<T> IntoElement for &Signal<T>
where
    T: IntoElement + Clone + 'static,
{
    fn into_element(self) -> Element {
        self.get().into_element()
    }
}

impl<T> IntoElement for Memo<T>
where
    T: IntoElement + Clone + 'static,
{
    fn into_element(self) -> Element {
        self.get().into_element()
    }
}

impl<T> IntoElement for &Memo<T>
where
    T: IntoElement + Clone + 'static,
{
    fn into_element(self) -> Element {
        self.get().into_element()
    }
}

impl<T> IntoAttributeValue for Signal<T>
where
    T: IntoAttributeValue + Clone + 'static,
{
    fn into_attribute_value(self) -> AttributeValue {
        self.get().into_attribute_value()
    }
}

impl<T> IntoAttributeValue for &Signal<T>
where
    T: IntoAttributeValue + Clone + 'static,
{
    fn into_attribute_value(self) -> AttributeValue {
        self.get().into_attribute_value()
    }
}

impl<T> IntoAttributeValue for Memo<T>
where
    T: IntoAttributeValue + Clone + 'static,
{
    fn into_attribute_value(self) -> AttributeValue {
        self.get().into_attribute_value()
    }
}

impl<T> IntoAttributeValue for &Memo<T>
where
    T: IntoAttributeValue + Clone + 'static,
{
    fn into_attribute_value(self) -> AttributeValue {
        self.get().into_attribute_value()
    }
}
