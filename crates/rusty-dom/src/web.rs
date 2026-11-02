//! Browser DOM implementation backed by `web-sys`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use rusty_core::{Event, EventListener, EventName};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{
    Document, Element, Event as WebEvent, HtmlInputElement, HtmlTextAreaElement, KeyboardEvent,
    Node, Text,
};

use crate::{Dom, DomError};

type ListenerClosure = Closure<dyn FnMut(WebEvent)>;

/// A browser DOM node handle.
#[derive(Clone)]
pub struct WebNode {
    inner: Rc<WebNodeInner>,
}

struct WebNodeInner {
    node: Node,
    kind: WebNodeKind,
    listeners: RefCell<HashMap<String, ListenerClosure>>,
}

enum WebNodeKind {
    Element(Element),
    Text(Text),
    RawHtml(Element),
}

impl WebNode {
    fn new(node: Node, kind: WebNodeKind) -> Self {
        Self {
            inner: Rc::new(WebNodeInner {
                node,
                kind,
                listeners: RefCell::new(HashMap::new()),
            }),
        }
    }

    fn element(&self) -> Result<&Element, DomError> {
        match &self.inner.kind {
            WebNodeKind::Element(element) | WebNodeKind::RawHtml(element) => Ok(element),
            WebNodeKind::Text(_) => Err(DomError::invalid_operation(
                "operation requires an element node",
            )),
        }
    }
}

impl fmt::Debug for WebNode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("WebNode").finish_non_exhaustive()
    }
}

impl PartialEq for WebNode {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for WebNode {}

/// A DOM backed by the browser document.
pub struct WebDom {
    document: Document,
}

impl WebDom {
    /// Connects to the current browser document.
    ///
    /// # Errors
    ///
    /// Returns [`DomError`] when no browser window or document is available.
    pub fn new() -> Result<Self, DomError> {
        let window =
            web_sys::window().ok_or_else(|| DomError::browser("browser window is unavailable"))?;
        let document = window
            .document()
            .ok_or_else(|| DomError::browser("browser document is unavailable"))?;
        Ok(Self { document })
    }

    /// Returns the document body as a mount point.
    ///
    /// # Errors
    ///
    /// Returns [`DomError`] when the document has no body element.
    pub fn body(&self) -> Result<WebNode, DomError> {
        let body = self
            .document
            .body()
            .ok_or_else(|| DomError::browser("document body is unavailable"))?;
        let element: Element = body.unchecked_into();
        let node: Node = element.clone().unchecked_into();
        Ok(WebNode::new(node, WebNodeKind::Element(element)))
    }

    fn create_element_node(&self, tag: &str) -> Result<(Element, Node), DomError> {
        let element = self.document.create_element(tag).map_err(js_error)?;
        let node: Node = element.clone().unchecked_into();
        Ok((element, node))
    }
}

impl Dom for WebDom {
    type Node = WebNode;

    fn create_element(&mut self, tag: &str) -> Result<Self::Node, DomError> {
        let (element, node) = self.create_element_node(tag)?;
        Ok(WebNode::new(node, WebNodeKind::Element(element)))
    }

    fn create_text(&mut self, text: &str) -> Result<Self::Node, DomError> {
        let text = self.document.create_text_node(text);
        let node: Node = text.clone().unchecked_into();
        Ok(WebNode::new(node, WebNodeKind::Text(text)))
    }

    fn create_raw_html(&mut self, html: &str) -> Result<Self::Node, DomError> {
        let (element, node) = self.create_element_node("rusty-raw-html")?;
        element
            .set_attribute("style", "display: contents")
            .map_err(js_error)?;
        element.set_inner_html(html);
        Ok(WebNode::new(node, WebNodeKind::RawHtml(element)))
    }

    fn append_child(&mut self, parent: &Self::Node, child: &Self::Node) -> Result<(), DomError> {
        parent
            .inner
            .node
            .append_child(&child.inner.node)
            .map_err(js_error)?;
        Ok(())
    }

    fn insert_before(
        &mut self,
        parent: &Self::Node,
        child: &Self::Node,
        reference: &Self::Node,
    ) -> Result<(), DomError> {
        parent
            .inner
            .node
            .insert_before(&child.inner.node, Some(&reference.inner.node))
            .map_err(js_error)?;
        Ok(())
    }

    fn remove_child(&mut self, parent: &Self::Node, child: &Self::Node) -> Result<(), DomError> {
        parent
            .inner
            .node
            .remove_child(&child.inner.node)
            .map_err(js_error)?;
        Ok(())
    }

    fn replace_child(
        &mut self,
        parent: &Self::Node,
        new_child: &Self::Node,
        old_child: &Self::Node,
    ) -> Result<(), DomError> {
        parent
            .inner
            .node
            .replace_child(&new_child.inner.node, &old_child.inner.node)
            .map_err(js_error)?;
        Ok(())
    }

    fn set_attribute(
        &mut self,
        node: &Self::Node,
        name: &str,
        value: &str,
    ) -> Result<(), DomError> {
        node.element()?.set_attribute(name, value).map_err(js_error)
    }

    fn remove_attribute(&mut self, node: &Self::Node, name: &str) -> Result<(), DomError> {
        node.element()?.remove_attribute(name).map_err(js_error)
    }

    fn set_text(&mut self, node: &Self::Node, text: &str) -> Result<(), DomError> {
        let WebNodeKind::Text(existing) = &node.inner.kind else {
            return Err(DomError::invalid_operation("set_text requires a text node"));
        };
        existing.set_data(text);
        Ok(())
    }

    fn set_raw_html(&mut self, node: &Self::Node, html: &str) -> Result<(), DomError> {
        let WebNodeKind::RawHtml(element) = &node.inner.kind else {
            return Err(DomError::invalid_operation(
                "set_raw_html requires a raw HTML node",
            ));
        };
        element.set_inner_html(html);
        Ok(())
    }

    fn set_event_listener(
        &mut self,
        node: &Self::Node,
        listener: EventListener,
    ) -> Result<(), DomError> {
        let element = node.element()?;
        let name = listener.name().as_str().to_owned();

        if let Some(old) = node.inner.listeners.borrow_mut().remove(&name) {
            let callback: &js_sys::Function = old.as_ref().unchecked_ref();
            element
                .remove_event_listener_with_callback(&name, callback)
                .map_err(js_error)?;
        }

        let closure = Closure::<dyn FnMut(WebEvent)>::new(move |event: WebEvent| {
            listener.invoke(convert_event(&event));
        });

        let callback: &js_sys::Function = closure.as_ref().unchecked_ref();
        element
            .add_event_listener_with_callback(&name, callback)
            .map_err(js_error)?;
        node.inner.listeners.borrow_mut().insert(name, closure);
        Ok(())
    }

    fn remove_event_listener(
        &mut self,
        node: &Self::Node,
        name: &EventName,
    ) -> Result<(), DomError> {
        let element = node.element()?;

        if let Some(old) = node.inner.listeners.borrow_mut().remove(name.as_str()) {
            let callback: &js_sys::Function = old.as_ref().unchecked_ref();
            element
                .remove_event_listener_with_callback(name.as_str(), callback)
                .map_err(js_error)?;
        }

        Ok(())
    }
}

fn convert_event(event: &WebEvent) -> Event {
    let name = EventName::new(event.type_())
        .unwrap_or_else(|_| EventName::new("unknown").expect("the fallback event name is valid"));
    let mut converted = Event::new(name).with_bubbles(event.bubbles());

    if let Some(target) = event.target() {
        if let Some(input) = target.dyn_ref::<HtmlInputElement>() {
            converted = converted
                .with_target_value(input.value())
                .with_checked(input.checked());
        } else if let Some(textarea) = target.dyn_ref::<HtmlTextAreaElement>() {
            converted = converted.with_target_value(textarea.value());
        }
    }

    if let Some(keyboard) = event.dyn_ref::<KeyboardEvent>() {
        converted = converted.with_key(keyboard.key());
    }

    converted
}

fn js_error(value: wasm_bindgen::JsValue) -> DomError {
    DomError::browser(format!("{value:?}"))
}
