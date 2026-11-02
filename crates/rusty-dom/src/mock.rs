//! In-memory DOM implementation used by native tests.

use std::cell::RefCell;
use std::rc::Rc;

use rusty_core::{is_void_element, AttributeValue, Event, EventListener, EventName};

use crate::{Dom, DomError};

/// A handle to a node in a [`MockDom`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MockNode(usize);

/// A deterministic in-memory DOM.
#[derive(Debug, Default)]
pub struct MockDom {
    nodes: Vec<Rc<RefCell<MockNodeData>>>,
}

#[derive(Debug)]
struct MockNodeData {
    parent: Option<MockNode>,
    kind: MockNodeKind,
}

#[derive(Debug)]
enum MockNodeKind {
    Element {
        tag: String,
        attributes: Vec<(String, AttributeValue)>,
        listeners: Vec<EventListener>,
        children: Vec<MockNode>,
    },
    Text(String),
    RawHtml(String),
}

impl MockDom {
    /// Creates an empty DOM.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the tag name for an element node.
    #[must_use]
    pub fn tag(&self, node: &MockNode) -> Option<String> {
        match &self.data(*node).kind {
            MockNodeKind::Element { tag, .. } => Some(tag.clone()),
            _ => None,
        }
    }

    /// Returns attributes for an element node.
    #[must_use]
    pub fn attributes(&self, node: &MockNode) -> Option<Vec<(String, AttributeValue)>> {
        match &self.data(*node).kind {
            MockNodeKind::Element { attributes, .. } => Some(attributes.clone()),
            _ => None,
        }
    }

    /// Returns child handles for an element node.
    #[must_use]
    pub fn children(&self, node: &MockNode) -> Option<Vec<MockNode>> {
        match &self.data(*node).kind {
            MockNodeKind::Element { children, .. } => Some(children.clone()),
            _ => None,
        }
    }

    /// Returns the text of a text node.
    #[must_use]
    pub fn text(&self, node: &MockNode) -> Option<String> {
        match &self.data(*node).kind {
            MockNodeKind::Text(text) => Some(text.clone()),
            _ => None,
        }
    }

    /// Returns the raw markup of a raw HTML node.
    #[must_use]
    pub fn raw_html(&self, node: &MockNode) -> Option<String> {
        match &self.data(*node).kind {
            MockNodeKind::RawHtml(html) => Some(html.clone()),
            _ => None,
        }
    }

    /// Returns the event names registered on an element.
    #[must_use]
    pub fn listener_names(&self, node: &MockNode) -> Vec<String> {
        match &self.data(*node).kind {
            MockNodeKind::Element { listeners, .. } => listeners
                .iter()
                .map(|listener| listener.name().as_str().to_owned())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Returns the parent of a node.
    #[must_use]
    pub fn parent(&self, node: &MockNode) -> Option<MockNode> {
        self.data(*node).parent
    }

    /// Dispatches an event to a node, bubbling to ancestors when requested.
    pub fn dispatch(&mut self, target: MockNode, event: Event) -> Result<(), DomError> {
        let mut current = Some(target);

        while let Some(node) = current {
            let (listeners, parent) = {
                let data = self.data(node);
                match &data.kind {
                    MockNodeKind::Element { listeners, .. } => (listeners.clone(), data.parent),
                    _ => (Vec::new(), data.parent),
                }
            };

            for listener in listeners {
                if listener.name() == event.name() {
                    listener.invoke(event.clone());
                }
            }

            if !event.bubbles() {
                break;
            }

            current = parent;
        }

        Ok(())
    }

    /// Serializes a node and its descendants to HTML for assertions.
    #[must_use]
    pub fn to_html(&self, node: &MockNode) -> String {
        let mut output = String::new();
        self.render_html(*node, &mut output);
        output
    }

    fn data(&self, node: MockNode) -> std::cell::Ref<'_, MockNodeData> {
        self.nodes[node.0].borrow()
    }

    fn data_mut(&self, node: MockNode) -> std::cell::RefMut<'_, MockNodeData> {
        self.nodes[node.0].borrow_mut()
    }

    fn create_node(&mut self, kind: MockNodeKind) -> MockNode {
        let node = MockNode(self.nodes.len());
        self.nodes
            .push(Rc::new(RefCell::new(MockNodeData { parent: None, kind })));
        node
    }

    fn element_children_mut(
        &self,
        parent: MockNode,
        operation: &str,
    ) -> Result<std::cell::RefMut<'_, MockNodeData>, DomError> {
        let data = self.data_mut(parent);

        if !matches!(data.kind, MockNodeKind::Element { .. }) {
            return Err(DomError::invalid_operation(format!(
                "{operation} requires an element parent"
            )));
        }

        Ok(data)
    }

    fn detach(&self, child: MockNode) -> Result<(), DomError> {
        let parent = self.data(child).parent;

        if let Some(parent) = parent {
            let mut parent_data = self.element_children_mut(parent, "detach")?;
            let MockNodeKind::Element { children, .. } = &mut parent_data.kind else {
                unreachable!("element_children_mut validated the node kind");
            };

            if let Some(index) = children.iter().position(|existing| *existing == child) {
                children.remove(index);
            }
        }

        self.data_mut(child).parent = None;
        Ok(())
    }

    fn render_html(&self, node: MockNode, output: &mut String) {
        let data = self.data(node);

        match &data.kind {
            MockNodeKind::Element {
                tag,
                attributes,
                children,
                ..
            } => {
                output.push('<');
                output.push_str(tag);

                for (name, value) in attributes {
                    match value {
                        AttributeValue::Text(value) => {
                            output.push(' ');
                            output.push_str(name);
                            output.push_str("=\"");
                            escape_attribute_into(value, output);
                            output.push('"');
                        }
                        AttributeValue::Bool(true) => {
                            output.push(' ');
                            output.push_str(name);
                        }
                        AttributeValue::Bool(false) | AttributeValue::None => {}
                    }
                }

                output.push('>');

                if !is_void_element(tag) {
                    for child in children {
                        self.render_html(*child, output);
                    }

                    output.push_str("</");
                    output.push_str(tag);
                    output.push('>');
                }
            }
            MockNodeKind::Text(text) => escape_text_into(text, output),
            MockNodeKind::RawHtml(html) => output.push_str(html),
        }
    }
}

impl Dom for MockDom {
    type Node = MockNode;

    fn create_element(&mut self, tag: &str) -> Result<Self::Node, DomError> {
        Ok(self.create_node(MockNodeKind::Element {
            tag: tag.to_owned(),
            attributes: Vec::new(),
            listeners: Vec::new(),
            children: Vec::new(),
        }))
    }

    fn create_text(&mut self, text: &str) -> Result<Self::Node, DomError> {
        Ok(self.create_node(MockNodeKind::Text(text.to_owned())))
    }

    fn create_raw_html(&mut self, html: &str) -> Result<Self::Node, DomError> {
        Ok(self.create_node(MockNodeKind::RawHtml(html.to_owned())))
    }

    fn append_child(&mut self, parent: &Self::Node, child: &Self::Node) -> Result<(), DomError> {
        self.detach(*child)?;
        let mut data = self.element_children_mut(*parent, "append_child")?;
        let MockNodeKind::Element { children, .. } = &mut data.kind else {
            unreachable!("element_children_mut validated the node kind");
        };
        children.push(*child);
        drop(data);
        self.data_mut(*child).parent = Some(*parent);
        Ok(())
    }

    fn insert_before(
        &mut self,
        parent: &Self::Node,
        child: &Self::Node,
        reference: &Self::Node,
    ) -> Result<(), DomError> {
        self.detach(*child)?;
        let mut data = self.element_children_mut(*parent, "insert_before")?;
        let MockNodeKind::Element { children, .. } = &mut data.kind else {
            unreachable!("element_children_mut validated the node kind");
        };
        let index = children
            .iter()
            .position(|existing| existing == reference)
            .ok_or_else(|| {
                DomError::invalid_operation("insert_before reference is not a child of the parent")
            })?;
        children.insert(index, *child);
        drop(data);
        self.data_mut(*child).parent = Some(*parent);
        Ok(())
    }

    fn remove_child(&mut self, parent: &Self::Node, child: &Self::Node) -> Result<(), DomError> {
        let mut data = self.element_children_mut(*parent, "remove_child")?;
        let MockNodeKind::Element { children, .. } = &mut data.kind else {
            unreachable!("element_children_mut validated the node kind");
        };
        let index = children
            .iter()
            .position(|existing| existing == child)
            .ok_or_else(|| {
                DomError::invalid_operation("remove_child target is not a child of the parent")
            })?;
        children.remove(index);
        drop(data);
        self.data_mut(*child).parent = None;
        Ok(())
    }

    fn replace_child(
        &mut self,
        parent: &Self::Node,
        new_child: &Self::Node,
        old_child: &Self::Node,
    ) -> Result<(), DomError> {
        self.detach(*new_child)?;
        let mut data = self.element_children_mut(*parent, "replace_child")?;
        let MockNodeKind::Element { children, .. } = &mut data.kind else {
            unreachable!("element_children_mut validated the node kind");
        };
        let index = children
            .iter()
            .position(|existing| existing == old_child)
            .ok_or_else(|| {
                DomError::invalid_operation("replace_child target is not a child of the parent")
            })?;
        children[index] = *new_child;
        drop(data);
        self.data_mut(*old_child).parent = None;
        self.data_mut(*new_child).parent = Some(*parent);
        Ok(())
    }

    fn set_attribute(
        &mut self,
        node: &Self::Node,
        name: &str,
        value: &str,
    ) -> Result<(), DomError> {
        let mut data = self.data_mut(*node);
        let MockNodeKind::Element { attributes, .. } = &mut data.kind else {
            return Err(DomError::invalid_operation(
                "set_attribute requires an element",
            ));
        };

        if let Some((_, existing)) = attributes.iter_mut().find(|(existing, _)| existing == name) {
            *existing = AttributeValue::text(value);
        } else {
            attributes.push((name.to_owned(), AttributeValue::text(value)));
        }

        Ok(())
    }

    fn remove_attribute(&mut self, node: &Self::Node, name: &str) -> Result<(), DomError> {
        let mut data = self.data_mut(*node);
        let MockNodeKind::Element { attributes, .. } = &mut data.kind else {
            return Err(DomError::invalid_operation(
                "remove_attribute requires an element",
            ));
        };
        attributes.retain(|(existing, _)| existing != name);
        Ok(())
    }

    fn set_text(&mut self, node: &Self::Node, text: &str) -> Result<(), DomError> {
        let mut data = self.data_mut(*node);
        let MockNodeKind::Text(existing) = &mut data.kind else {
            return Err(DomError::invalid_operation("set_text requires a text node"));
        };
        *existing = text.to_owned();
        Ok(())
    }

    fn set_raw_html(&mut self, node: &Self::Node, html: &str) -> Result<(), DomError> {
        let mut data = self.data_mut(*node);
        let MockNodeKind::RawHtml(existing) = &mut data.kind else {
            return Err(DomError::invalid_operation(
                "set_raw_html requires a raw HTML node",
            ));
        };
        *existing = html.to_owned();
        Ok(())
    }

    fn set_event_listener(
        &mut self,
        node: &Self::Node,
        listener: EventListener,
    ) -> Result<(), DomError> {
        let mut data = self.data_mut(*node);
        let MockNodeKind::Element { listeners, .. } = &mut data.kind else {
            return Err(DomError::invalid_operation(
                "set_event_listener requires an element",
            ));
        };

        if let Some(existing) = listeners
            .iter_mut()
            .find(|existing| existing.name() == listener.name())
        {
            *existing = listener;
        } else {
            listeners.push(listener);
        }

        Ok(())
    }

    fn remove_event_listener(
        &mut self,
        node: &Self::Node,
        name: &EventName,
    ) -> Result<(), DomError> {
        let mut data = self.data_mut(*node);
        let MockNodeKind::Element { listeners, .. } = &mut data.kind else {
            return Err(DomError::invalid_operation(
                "remove_event_listener requires an element",
            ));
        };
        listeners.retain(|existing| existing.name() != name);
        Ok(())
    }
}

fn escape_text_into(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(character),
        }
    }
}

fn escape_attribute_into(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#x27;"),
            _ => output.push(character),
        }
    }
}
