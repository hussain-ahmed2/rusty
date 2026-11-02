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

use crate::{Dom, DomError, NodeKind};

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
    Comment,
    RawHtml(Element),
    Other,
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

    fn from_node(node: Node) -> Self {
        let kind = match node.node_type() {
            Node::ELEMENT_NODE => node
                .dyn_ref::<Element>()
                .map_or(WebNodeKind::Other, |element| {
                    if element.tag_name().eq_ignore_ascii_case("rusty-raw-html") {
                        WebNodeKind::RawHtml(element.clone())
                    } else {
                        WebNodeKind::Element(element.clone())
                    }
                }),
            Node::TEXT_NODE => node
                .dyn_ref::<Text>()
                .map_or(WebNodeKind::Other, |text| WebNodeKind::Text(text.clone())),
            Node::COMMENT_NODE => WebNodeKind::Comment,
            _ => WebNodeKind::Other,
        };

        Self::new(node, kind)
    }

    fn element(&self) -> Result<&Element, DomError> {
        match &self.inner.kind {
            WebNodeKind::Element(element) | WebNodeKind::RawHtml(element) => Ok(element),
            WebNodeKind::Text(_) | WebNodeKind::Comment | WebNodeKind::Other => Err(
                DomError::invalid_operation("operation requires an element node"),
            ),
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

    /// Returns an element by its `id` attribute.
    ///
    /// # Errors
    ///
    /// Returns [`DomError`] when the browser document is unavailable.
    pub fn element_by_id(&self, id: &str) -> Result<Option<WebNode>, DomError> {
        Ok(self.document.get_element_by_id(id).map(|element| {
            let node: Node = element.unchecked_into();
            WebNode::from_node(node)
        }))
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

    fn create_comment(&mut self, text: &str) -> Result<Self::Node, DomError> {
        let comment = self.document.create_comment(text);
        let node: Node = comment.unchecked_into();
        Ok(WebNode::new(node, WebNodeKind::Comment))
    }

    fn first_child(&self, parent: &Self::Node) -> Result<Option<Self::Node>, DomError> {
        Ok(parent.inner.node.first_child().map(WebNode::from_node))
    }

    fn next_sibling(&self, node: &Self::Node) -> Result<Option<Self::Node>, DomError> {
        Ok(node.inner.node.next_sibling().map(WebNode::from_node))
    }

    fn node_kind(&self, node: &Self::Node) -> Result<NodeKind, DomError> {
        Ok(match node.inner.kind {
            WebNodeKind::Element(_) => NodeKind::Element,
            WebNodeKind::Text(_) => NodeKind::Text,
            WebNodeKind::Comment => NodeKind::Comment,
            WebNodeKind::RawHtml(_) => NodeKind::RawHtml,
            WebNodeKind::Other => NodeKind::Other,
        })
    }

    fn tag_name(&self, node: &Self::Node) -> Result<Option<String>, DomError> {
        Ok(match &node.inner.kind {
            WebNodeKind::Element(element) | WebNodeKind::RawHtml(element) => {
                Some(element.tag_name().to_ascii_lowercase())
            }
            WebNodeKind::Text(_) | WebNodeKind::Comment | WebNodeKind::Other => None,
        })
    }

    fn text(&self, node: &Self::Node) -> Result<Option<String>, DomError> {
        Ok(match &node.inner.kind {
            WebNodeKind::Text(_) => node.inner.node.node_value(),
            WebNodeKind::Element(_)
            | WebNodeKind::Comment
            | WebNodeKind::RawHtml(_)
            | WebNodeKind::Other => None,
        })
    }

    fn comment(&self, node: &Self::Node) -> Result<Option<String>, DomError> {
        Ok(match &node.inner.kind {
            WebNodeKind::Comment => node.inner.node.node_value(),
            WebNodeKind::Element(_)
            | WebNodeKind::Text(_)
            | WebNodeKind::RawHtml(_)
            | WebNodeKind::Other => None,
        })
    }

    fn raw_html(&self, node: &Self::Node) -> Result<Option<String>, DomError> {
        Ok(match &node.inner.kind {
            WebNodeKind::RawHtml(element) => Some(element.inner_html()),
            WebNodeKind::Element(_)
            | WebNodeKind::Text(_)
            | WebNodeKind::Comment
            | WebNodeKind::Other => None,
        })
    }

    fn attributes(&self, node: &Self::Node) -> Result<Vec<(String, String)>, DomError> {
        let element = node.element()?;
        let mut attributes = Vec::new();

        for name in element.get_attribute_names().iter() {
            let Some(name) = name.as_string() else {
                continue;
            };

            if let Some(value) = element.get_attribute(&name) {
                attributes.push((name, value));
            }
        }

        Ok(attributes)
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
