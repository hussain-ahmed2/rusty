//! Client-side rendering and node-level DOM patching.
//!
//! [`Renderer`] mounts a [`VNode`] tree into any [`Dom`] implementation and
//! patches the DOM in place on later renders. [`ReactiveRoot`] combines the
//! renderer with a Rusty effect: the view function reruns when signals it reads
//! change, and only changed DOM nodes are updated.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::cell::RefCell;
use std::error::Error;
use std::fmt;
use std::rc::Rc;

use rusty_core::Element;
use rusty_dom::{Dom, DomError};
use rusty_reactivity::{create_effect, create_root, Owner};
use rusty_vdom::{VElement, VNode, VNodeError};

/// An error from mounting or patching a DOM tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// The element tree was structurally invalid.
    Vdom(VNodeError),
    /// A DOM operation failed.
    Dom(DomError),
    /// A renderer was reused with a different container node.
    ContainerChanged,
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vdom(error) => error.fmt(formatter),
            Self::Dom(error) => error.fmt(formatter),
            Self::ContainerChanged => {
                formatter.write_str("renderer cannot switch to a different container")
            }
        }
    }
}

impl Error for RenderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Vdom(error) => Some(error),
            Self::Dom(error) => Some(error),
            Self::ContainerChanged => None,
        }
    }
}

impl From<VNodeError> for RenderError {
    fn from(error: VNodeError) -> Self {
        Self::Vdom(error)
    }
}

impl From<DomError> for RenderError {
    fn from(error: DomError) -> Self {
        Self::Dom(error)
    }
}

/// A renderer that mounts and patches an element tree.
pub struct Renderer<D: Dom> {
    dom: D,
    container: Option<D::Node>,
    roots: Vec<MountedNode<D>>,
}

impl<D: Dom> Renderer<D> {
    /// Creates a renderer around a DOM.
    #[must_use]
    pub fn new(dom: D) -> Self {
        Self {
            dom,
            container: None,
            roots: Vec::new(),
        }
    }

    /// Returns the underlying DOM.
    #[must_use]
    pub fn dom(&self) -> &D {
        &self.dom
    }

    /// Returns the underlying DOM mutably.
    pub fn dom_mut(&mut self) -> &mut D {
        &mut self.dom
    }

    /// Mounts or patches an element tree inside `container`.
    ///
    /// The first call mounts the tree. Later calls must use the same container
    /// and patch the existing DOM nodes in place.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] when the tree is invalid, a DOM operation fails,
    /// or the container changes between calls.
    pub fn render(&mut self, container: &D::Node, element: Element) -> Result<(), RenderError> {
        let node = VNode::try_from(element)?;
        self.render_vnode(container, node)
    }

    /// Mounts or patches a prepared virtual tree inside `container`.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] when a DOM operation fails or the container
    /// changes between calls.
    pub fn render_vnode(&mut self, container: &D::Node, node: VNode) -> Result<(), RenderError> {
        if let Some(existing) = &self.container {
            if existing != container {
                return Err(RenderError::ContainerChanged);
            }
        } else {
            self.container = Some(container.clone());
        }

        let next = flatten(node);
        patch_children(&mut self.dom, container, &mut self.roots, &next)?;
        Ok(())
    }

    /// Removes all mounted roots from the DOM.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] when a DOM removal fails.
    pub fn unmount(&mut self) -> Result<(), RenderError> {
        if let Some(container) = self.container.take() {
            for mounted in self.roots.drain(..).rev() {
                self.dom.remove_child(&container, &mounted.node)?;
            }
        }

        Ok(())
    }
}

impl<D: Dom> Drop for Renderer<D> {
    fn drop(&mut self) {
        let _ = self.unmount();
    }
}

struct MountedNode<D: Dom> {
    node: D::Node,
    vnode: VNode,
    children: Vec<Self>,
}

fn flatten(node: VNode) -> Vec<VNode> {
    let mut output = Vec::new();
    flatten_into(node, &mut output);
    output
}

fn flatten_into(node: VNode, output: &mut Vec<VNode>) {
    match node {
        VNode::Fragment(children) => {
            for child in children {
                flatten_into(child, output);
            }
        }
        node => output.push(node),
    }
}

fn flatten_slice(nodes: &[VNode]) -> Vec<VNode> {
    let mut output = Vec::new();
    for node in nodes {
        flatten_into(node.clone(), &mut output);
    }
    output
}

fn patch_children<D: Dom>(
    dom: &mut D,
    parent: &D::Node,
    existing: &mut Vec<MountedNode<D>>,
    next: &[VNode],
) -> Result<(), RenderError> {
    for (index, node) in next.iter().enumerate() {
        if let Some(mounted) = existing.get_mut(index) {
            if same_kind(&mounted.vnode, node) {
                patch_node(dom, mounted, node)?;
            } else {
                let replacement = mount_node(dom, node)?;
                dom.replace_child(parent, &replacement.node, &mounted.node)?;
                *mounted = replacement;
            }
        } else {
            let mounted = mount_node(dom, node)?;
            dom.append_child(parent, &mounted.node)?;
            existing.push(mounted);
        }
    }

    while existing.len() > next.len() {
        let removed = existing
            .pop()
            .expect("the loop condition guarantees a mounted node");
        dom.remove_child(parent, &removed.node)?;
    }

    Ok(())
}

fn same_kind(existing: &VNode, next: &VNode) -> bool {
    match (existing, next) {
        (VNode::Element(existing), VNode::Element(next)) => existing.tag() == next.tag(),
        (VNode::Text(_), VNode::Text(_)) => true,
        (VNode::RawHtml(_), VNode::RawHtml(_)) => true,
        _ => false,
    }
}

fn mount_node<D: Dom>(dom: &mut D, node: &VNode) -> Result<MountedNode<D>, RenderError> {
    match node {
        VNode::Element(element) => {
            validate_element(element)?;
            let handle = dom.create_element(element.tag().as_str())?;
            apply_attributes(dom, &handle, element.attributes())?;
            apply_listeners(dom, &handle, element.listeners())?;

            let mut children = Vec::new();
            for child in flatten_slice(element.children()) {
                let mounted = mount_node(dom, &child)?;
                dom.append_child(&handle, &mounted.node)?;
                children.push(mounted);
            }

            Ok(MountedNode {
                node: handle,
                vnode: node.clone(),
                children,
            })
        }
        VNode::Text(text) => Ok(MountedNode {
            node: dom.create_text(text)?,
            vnode: node.clone(),
            children: Vec::new(),
        }),
        VNode::RawHtml(html) => Ok(MountedNode {
            node: dom.create_raw_html(html)?,
            vnode: node.clone(),
            children: Vec::new(),
        }),
        VNode::Fragment(_) => {
            unreachable!("fragments are flattened before mounting")
        }
    }
}

fn patch_node<D: Dom>(
    dom: &mut D,
    mounted: &mut MountedNode<D>,
    next: &VNode,
) -> Result<(), RenderError> {
    let MountedNode {
        node,
        vnode,
        children,
    } = mounted;

    match next {
        VNode::Element(next_element) => {
            validate_element(next_element)?;
            let VNode::Element(previous_element) = vnode else {
                unreachable!("patch_node is only called for matching node kinds");
            };

            update_attributes(
                dom,
                node,
                previous_element.attributes(),
                next_element.attributes(),
            )?;
            update_listeners(
                dom,
                node,
                previous_element.listeners(),
                next_element.listeners(),
            )?;
            patch_children(dom, node, children, &flatten_slice(next_element.children()))?;
        }
        VNode::Text(next_text) => {
            if let VNode::Text(previous_text) = vnode {
                if previous_text != next_text {
                    dom.set_text(node, next_text)?;
                }
            }
        }
        VNode::RawHtml(next_html) => {
            if let VNode::RawHtml(previous_html) = vnode {
                if previous_html != next_html {
                    dom.set_raw_html(node, next_html)?;
                }
            }
        }
        VNode::Fragment(_) => {
            unreachable!("fragments are flattened before patching")
        }
    }

    *vnode = next.clone();
    Ok(())
}

fn validate_element(element: &VElement) -> Result<(), RenderError> {
    if element.tag().is_void() && !element.children().is_empty() {
        return Err(VNodeError::ChildrenInVoidElement {
            tag: element.tag().to_string(),
        }
        .into());
    }

    Ok(())
}

fn apply_attributes<D: Dom>(
    dom: &mut D,
    node: &D::Node,
    attributes: &[rusty_core::Attribute],
) -> Result<(), RenderError> {
    for attribute in attributes {
        apply_attribute(dom, node, attribute)?;
    }

    Ok(())
}

fn apply_attribute<D: Dom>(
    dom: &mut D,
    node: &D::Node,
    attribute: &rusty_core::Attribute,
) -> Result<(), RenderError> {
    let name = attribute.name().as_str();

    match attribute.value() {
        rusty_core::AttributeValue::Text(value) => {
            dom.set_attribute(node, name, value)?;
        }
        rusty_core::AttributeValue::Bool(true) => {
            dom.set_attribute(node, name, "")?;
        }
        rusty_core::AttributeValue::Bool(false) | rusty_core::AttributeValue::None => {
            dom.remove_attribute(node, name)?;
        }
    }

    Ok(())
}

fn update_attributes<D: Dom>(
    dom: &mut D,
    node: &D::Node,
    previous: &[rusty_core::Attribute],
    next: &[rusty_core::Attribute],
) -> Result<(), RenderError> {
    for old in previous {
        if !next.iter().any(|attribute| attribute.name() == old.name()) {
            dom.remove_attribute(node, old.name().as_str())?;
        }
    }

    for attribute in next {
        let old_value = previous
            .iter()
            .find(|old| old.name() == attribute.name())
            .map(rusty_core::Attribute::value);

        if old_value != Some(attribute.value()) {
            apply_attribute(dom, node, attribute)?;
        }
    }

    Ok(())
}

fn apply_listeners<D: Dom>(
    dom: &mut D,
    node: &D::Node,
    listeners: &[rusty_core::EventListener],
) -> Result<(), RenderError> {
    for listener in listeners {
        dom.set_event_listener(node, listener.clone())?;
    }

    Ok(())
}

fn update_listeners<D: Dom>(
    dom: &mut D,
    node: &D::Node,
    previous: &[rusty_core::EventListener],
    next: &[rusty_core::EventListener],
) -> Result<(), RenderError> {
    for old in previous {
        if !next.iter().any(|listener| listener.name() == old.name()) {
            dom.remove_event_listener(node, old.name())?;
        }
    }

    for listener in next {
        if previous.iter().any(|old| old == listener) {
            continue;
        }

        dom.set_event_listener(node, listener.clone())?;
    }

    Ok(())
}

/// A mounted reactive application.
///
/// The root owns the render effect and disposes it when dropped. Dropping the
/// root also unmounts the rendered DOM nodes.
pub struct ReactiveRoot<D: Dom> {
    _owner: Owner,
    renderer: Rc<RefCell<Renderer<D>>>,
}

impl<D: Dom + 'static> ReactiveRoot<D> {
    /// Mounts a view function and rerenders it when its signals change.
    ///
    /// The view runs inside an effect and is called again after each change.
    /// State that must survive rerenders, such as signals, should be created
    /// before mounting and captured by the closure; signals created inside the
    /// view start from their initial value on every run.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] when the initial render fails. Later render
    /// failures panic because they occur inside the reactive effect.
    pub fn mount<F>(dom: D, container: D::Node, mut view: F) -> Result<Self, RenderError>
    where
        F: FnMut() -> Element + 'static,
    {
        let renderer = Rc::new(RefCell::new(Renderer::new(dom)));
        let initial = Rc::new(std::cell::Cell::new(true));
        let error = Rc::new(RefCell::new(None));

        let (owner, ()) = create_root(|| {
            let effect_renderer = Rc::clone(&renderer);
            let effect_container = container.clone();
            let effect_initial = Rc::clone(&initial);
            let effect_error = Rc::clone(&error);

            let _ = create_effect(move || {
                let is_initial = effect_initial.replace(false);
                let element = view();
                let result = effect_renderer
                    .borrow_mut()
                    .render(&effect_container, element);

                match result {
                    Ok(()) => {}
                    Err(error) if is_initial => {
                        *effect_error.borrow_mut() = Some(error);
                    }
                    Err(error) => panic!("Rusty client render failed: {error}"),
                }
            });
        });

        if let Some(error) = error.borrow_mut().take() {
            return Err(error);
        }

        Ok(Self {
            _owner: owner,
            renderer,
        })
    }

    /// Runs `f` with immutable access to the live renderer.
    ///
    /// # Panics
    ///
    /// Panics if called while the renderer is already mutably borrowed by an
    /// in-progress render.
    pub fn with_renderer<R>(&self, f: impl FnOnce(&Renderer<D>) -> R) -> R {
        f(&self.renderer.borrow())
    }

    /// Runs `f` with immutable access to the underlying DOM.
    ///
    /// # Panics
    ///
    /// Panics if called while the renderer is already mutably borrowed by an
    /// in-progress render.
    pub fn with_dom<R>(&self, f: impl FnOnce(&D) -> R) -> R {
        f(self.renderer.borrow().dom())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use rusty_core::{
        Attribute, AttributeName, AttributeValue, Event, EventListener, EventName, TagName,
    };
    use rusty_dom::MockDom;

    use super::*;

    fn tag(name: &str) -> TagName {
        TagName::new(name).expect("valid tag")
    }

    fn attribute(name: &str, value: &str) -> Attribute {
        Attribute::new(
            AttributeName::new(name).expect("valid attribute"),
            AttributeValue::text(value),
        )
    }

    fn paragraph(text: &str) -> Element {
        Element::html(tag("p"), Vec::new(), vec![Element::text(text)])
    }

    #[test]
    fn mounts_and_patches_text_and_attributes_in_place() {
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let mut renderer = Renderer::new(dom);

        renderer
            .render(&container, paragraph("zero"))
            .expect("initial render");
        assert_eq!(renderer.dom().to_html(&container), "<div><p>zero</p></div>");

        let paragraph_node = renderer.dom().children(&container).expect("children")[0];
        let text_node = renderer.dom().children(&paragraph_node).expect("children")[0];

        let updated = Element::html(
            tag("p"),
            vec![attribute("class", "active")],
            vec![Element::text("one")],
        );
        renderer.render(&container, updated).expect("patch");

        assert_eq!(
            renderer.dom().to_html(&container),
            "<div><p class=\"active\">one</p></div>"
        );
        assert_eq!(
            renderer.dom().children(&container).expect("children")[0],
            paragraph_node
        );
        assert_eq!(
            renderer.dom().children(&paragraph_node).expect("children")[0],
            text_node
        );
        assert_eq!(renderer.dom().text(&text_node).as_deref(), Some("one"));
    }

    #[test]
    fn replaces_nodes_when_the_tag_changes() {
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let mut renderer = Renderer::new(dom);

        renderer
            .render(&container, paragraph("first"))
            .expect("initial render");
        let old = renderer.dom().children(&container).expect("children")[0];

        renderer
            .render(
                &container,
                Element::html(tag("span"), Vec::new(), vec![Element::text("second")]),
            )
            .expect("patch");

        let children = renderer.dom().children(&container).expect("children");
        assert_eq!(children.len(), 1);
        assert_ne!(children[0], old);
        assert_eq!(renderer.dom().tag(&children[0]).as_deref(), Some("span"));
    }

    #[test]
    fn appends_and_removes_children() {
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let mut renderer = Renderer::new(dom);

        renderer
            .render(
                &container,
                Element::html(tag("ul"), Vec::new(), vec![paragraph("a")]),
            )
            .expect("initial render");

        renderer
            .render(
                &container,
                Element::html(
                    tag("ul"),
                    Vec::new(),
                    vec![paragraph("a"), paragraph("b"), paragraph("c")],
                ),
            )
            .expect("append");
        assert_eq!(
            renderer.dom().to_html(&container),
            "<div><ul><p>a</p><p>b</p><p>c</p></ul></div>"
        );

        renderer
            .render(
                &container,
                Element::html(tag("ul"), Vec::new(), vec![paragraph("b")]),
            )
            .expect("remove");
        assert_eq!(
            renderer.dom().to_html(&container),
            "<div><ul><p>b</p></ul></div>"
        );
    }

    #[test]
    fn flattens_fragments_at_the_root_and_in_children() {
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let mut renderer = Renderer::new(dom);

        renderer
            .render(
                &container,
                Element::fragment([
                    Element::text("a"),
                    Element::html(
                        tag("p"),
                        Vec::new(),
                        vec![Element::fragment([Element::text("b"), Element::text("c")])],
                    ),
                ]),
            )
            .expect("render");

        assert_eq!(renderer.dom().to_html(&container), "<div>a<p>bc</p></div>");
    }

    #[test]
    fn dispatches_events_and_replaces_listeners() {
        let clicks = std::rc::Rc::new(Cell::new(0_u32));
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let mut renderer = Renderer::new(dom);

        let listener = EventListener::new(EventName::new("click").expect("event"), {
            let clicks = std::rc::Rc::clone(&clicks);
            move |_| clicks.set(clicks.get() + 1)
        });
        renderer
            .render(
                &container,
                Element::html_with_events(
                    tag("button"),
                    Vec::new(),
                    vec![Element::text("go")],
                    vec![listener],
                ),
            )
            .expect("render");

        let button = renderer.dom().children(&container).expect("children")[0];
        assert_eq!(renderer.dom().listener_names(&button), ["click"]);

        renderer
            .dom_mut()
            .dispatch(button, Event::new(EventName::new("click").expect("event")))
            .expect("dispatch");
        assert_eq!(clicks.get(), 1);

        renderer
            .render(
                &container,
                Element::html(tag("button"), Vec::new(), vec![Element::text("go")]),
            )
            .expect("remove listener");
        assert!(renderer.dom().listener_names(&button).is_empty());

        renderer
            .dom_mut()
            .dispatch(button, Event::new(EventName::new("click").expect("event")))
            .expect("dispatch");
        assert_eq!(clicks.get(), 1);
    }

    #[test]
    fn updates_raw_html_in_place() {
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let mut renderer = Renderer::new(dom);

        renderer
            .render(&container, Element::raw_html("<b>old</b>"))
            .expect("render");
        let raw = renderer.dom().children(&container).expect("children")[0];

        renderer
            .render(&container, Element::raw_html("<b>new</b>"))
            .expect("patch");

        assert_eq!(
            renderer.dom().children(&container).expect("children")[0],
            raw
        );
        assert_eq!(renderer.dom().raw_html(&raw).as_deref(), Some("<b>new</b>"));
    }

    #[test]
    fn reactive_root_rerenders_only_changed_nodes() {
        use rusty_reactivity::Signal;

        let count = Signal::new(0_u32);
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");

        let root = ReactiveRoot::mount(dom, container, {
            let count = count.clone();
            move || paragraph(&count.get().to_string())
        })
        .expect("mount");

        let first = root.with_dom(|dom| dom.children(&container).expect("children")[0]);
        assert_eq!(
            root.with_dom(|dom| dom.to_html(&container)),
            "<div><p>0</p></div>"
        );

        count.set(5);
        assert_eq!(
            root.with_dom(|dom| dom.to_html(&container)),
            "<div><p>5</p></div>"
        );
        assert_eq!(
            root.with_dom(|dom| dom.children(&container).expect("children")[0]),
            first
        );
    }

    #[test]
    fn unmount_removes_nodes_from_the_container() {
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let mut renderer = Renderer::new(dom);

        renderer
            .render(&container, paragraph("bye"))
            .expect("render");
        let node = renderer.dom().children(&container).expect("children")[0];
        renderer.unmount().expect("unmount");

        assert_eq!(renderer.dom().to_html(&container), "<div></div>");
        assert_eq!(renderer.dom().parent(&node), None);
    }

    #[test]
    fn rejects_manually_built_vnodes_with_void_children() {
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let mut renderer = Renderer::new(dom);
        let invalid = VNode::Element(VElement::new(
            tag("br"),
            Vec::new(),
            Vec::new(),
            vec![VNode::text("bad")],
        ));

        assert!(matches!(
            renderer.render_vnode(&container, invalid),
            Err(RenderError::Vdom(VNodeError::ChildrenInVoidElement { .. }))
        ));
    }

    #[test]
    fn rejects_invalid_trees_and_container_changes() {
        let mut dom = MockDom::new();
        let first = dom.create_element("div").expect("container");
        let second = dom.create_element("div").expect("container");
        let mut renderer = Renderer::new(dom);

        let invalid = Element::html(tag("br"), Vec::new(), vec![Element::text("bad")]);
        assert!(matches!(
            renderer.render(&first, invalid),
            Err(RenderError::Vdom(_))
        ));

        renderer.render(&first, paragraph("ok")).expect("render");
        assert_eq!(
            renderer.render(&second, paragraph("other")),
            Err(RenderError::ContainerChanged)
        );
    }
}
