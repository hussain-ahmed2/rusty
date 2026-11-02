//! Event values and listeners used by client rendering.
//!
//! The core crate stays browser-independent. Browser bindings translate native
//! events into [`Event`] and invoke [`EventListener`] callbacks.

use std::fmt;
use std::rc::Rc;

use crate::EventName;

/// A browser-independent event delivered to a listener.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    name: EventName,
    target_value: Option<String>,
    key: Option<String>,
    checked: Option<bool>,
    bubbles: bool,
}

impl Event {
    /// Creates an event with the given name that bubbles by default.
    #[must_use]
    pub fn new(name: EventName) -> Self {
        Self {
            name,
            target_value: None,
            key: None,
            checked: None,
            bubbles: true,
        }
    }

    /// Sets the current value of the event target.
    #[must_use]
    pub fn with_target_value(mut self, value: impl Into<String>) -> Self {
        self.target_value = Some(value.into());
        self
    }

    /// Sets the key from a keyboard event.
    #[must_use]
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Sets the checked state of the event target.
    #[must_use]
    pub const fn with_checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// Sets whether the event bubbles to ancestor elements.
    #[must_use]
    pub const fn with_bubbles(mut self, bubbles: bool) -> Self {
        self.bubbles = bubbles;
        self
    }

    /// Returns the event name.
    #[must_use]
    pub const fn name(&self) -> &EventName {
        &self.name
    }

    /// Returns the event target's current value, when available.
    #[must_use]
    pub fn target_value(&self) -> Option<&str> {
        self.target_value.as_deref()
    }

    /// Returns the key for keyboard events, when available.
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    /// Returns the checked state of the event target, when available.
    #[must_use]
    pub const fn checked(&self) -> Option<bool> {
        self.checked
    }

    /// Returns whether the event bubbles to ancestor elements.
    #[must_use]
    pub const fn bubbles(&self) -> bool {
        self.bubbles
    }
}

/// A typed event handler attached to an element.
#[derive(Clone)]
pub struct EventListener {
    name: EventName,
    handler: Rc<dyn Fn(Event)>,
}

impl EventListener {
    /// Creates a listener for an event name.
    #[must_use]
    pub fn new<F>(name: EventName, handler: F) -> Self
    where
        F: Fn(Event) + 'static,
    {
        Self {
            name,
            handler: Rc::new(handler),
        }
    }

    /// Returns the event name this listener observes.
    #[must_use]
    pub const fn name(&self) -> &EventName {
        &self.name
    }

    /// Invokes the handler.
    pub fn invoke(&self, event: Event) {
        (self.handler)(event);
    }
}

impl fmt::Debug for EventListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventListener")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl PartialEq for EventListener {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && Rc::ptr_eq(&self.handler, &other.handler)
    }
}

impl Eq for EventListener {}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn events_expose_browser_fields() {
        let event = Event::new(EventName::new("input").expect("event"))
            .with_target_value("Rusty")
            .with_key("Enter")
            .with_checked(true)
            .with_bubbles(false);

        assert_eq!(event.name().as_str(), "input");
        assert_eq!(event.target_value(), Some("Rusty"));
        assert_eq!(event.key(), Some("Enter"));
        assert_eq!(event.checked(), Some(true));
        assert!(!event.bubbles());
    }

    #[test]
    fn listeners_invoke_handlers_and_compare_by_identity() {
        let calls = Rc::new(Cell::new(0_u32));
        let listener = EventListener::new(EventName::new("click").expect("event"), {
            let calls = Rc::clone(&calls);
            move |_| calls.set(calls.get() + 1)
        });
        let clone = listener.clone();
        let other = EventListener::new(EventName::new("click").expect("event"), {
            let calls = Rc::clone(&calls);
            move |_| calls.set(calls.get() + 1)
        });

        assert_eq!(listener, clone);
        assert_ne!(listener, other);
        listener.invoke(Event::new(EventName::new("click").expect("event")));
        assert_eq!(calls.get(), 1);
    }
}
