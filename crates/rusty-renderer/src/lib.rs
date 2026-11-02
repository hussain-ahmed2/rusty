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
use rusty_dom::{Dom, DomError, NodeKind};
use rusty_reactivity::{create_effect, create_root, Owner};
use rusty_vdom::{HydrationId, VElement, VNode, VNodeError};

/// An error from mounting or patching a DOM tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// The element tree was structurally invalid.
    Vdom(VNodeError),
    /// A DOM operation failed.
    Dom(DomError),
    /// A renderer was reused with a different container node.
    ContainerChanged,
    /// Hydration was attempted on a renderer that already mounted content.
    AlreadyMounted,
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vdom(error) => error.fmt(formatter),
            Self::Dom(error) => error.fmt(formatter),
            Self::ContainerChanged => {
                formatter.write_str("renderer cannot switch to a different container")
            }
            Self::AlreadyMounted => {
                formatter.write_str("renderer already mounted content; create a new renderer")
            }
        }
    }
}

impl Error for RenderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Vdom(error) => Some(error),
            Self::Dom(error) => Some(error),
            Self::ContainerChanged | Self::AlreadyMounted => None,
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

/// A mismatch found while adopting server-rendered DOM nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HydrationMismatch {
    id: u64,
    expected: String,
    found: String,
}

impl HydrationMismatch {
    /// Returns the deterministic node identity where hydration diverged.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Returns what the client tree expected.
    #[must_use]
    pub fn expected(&self) -> &str {
        &self.expected
    }

    /// Returns what the server DOM contained.
    #[must_use]
    pub fn found(&self) -> &str {
        &self.found
    }
}

impl fmt::Display for HydrationMismatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "hydration mismatch at node {}: expected {}, found {}",
            self.id, self.expected, self.found
        )
    }
}

impl Error for HydrationMismatch {}

/// The result of a hydration attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HydrationOutcome {
    /// Server-rendered nodes were adopted successfully.
    Hydrated,
    /// Server-rendered nodes did not match; the container was rendered fresh.
    Recovered(HydrationMismatch),
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

    /// Hydrates server-rendered markup inside `container`.
    ///
    /// On a mismatch, the existing container content is removed and the tree is
    /// mounted fresh. The returned outcome reports whether recovery occurred.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] when the tree is invalid or a DOM operation fails.
    pub fn hydrate(
        &mut self,
        container: &D::Node,
        element: Element,
    ) -> Result<HydrationOutcome, RenderError> {
        let node = VNode::try_from(element)?;
        self.hydrate_vnode(container, node)
    }

    /// Hydrates a prepared virtual tree inside `container`.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] when a DOM operation fails, the tree is invalid,
    /// or the renderer already mounted content.
    pub fn hydrate_vnode(
        &mut self,
        container: &D::Node,
        node: VNode,
    ) -> Result<HydrationOutcome, RenderError> {
        if self.container.is_some() {
            return Err(RenderError::AlreadyMounted);
        }

        let next = flatten(node);
        let mut id = HydrationId::new(0);

        match hydrate_children(&mut self.dom, container, &next, &mut id) {
            Ok(roots) => {
                self.container = Some(container.clone());
                self.roots = roots;
                Ok(HydrationOutcome::Hydrated)
            }
            Err(HydrationFailure::Mismatch(mismatch)) => {
                clear_children(&mut self.dom, container)?;
                let mut roots = Vec::new();
                patch_children(&mut self.dom, container, &mut roots, &next)?;
                self.container = Some(container.clone());
                self.roots = roots;
                Ok(HydrationOutcome::Recovered(mismatch))
            }
            Err(HydrationFailure::Render(error)) => Err(error),
            Err(HydrationFailure::Dom(error)) => Err(error.into()),
        }
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

enum HydrationFailure {
    Render(RenderError),
    Dom(DomError),
    Mismatch(HydrationMismatch),
}

impl From<RenderError> for HydrationFailure {
    fn from(error: RenderError) -> Self {
        Self::Render(error)
    }
}

impl From<DomError> for HydrationFailure {
    fn from(error: DomError) -> Self {
        Self::Dom(error)
    }
}

fn hydrate_children<D: Dom>(
    dom: &mut D,
    parent: &D::Node,
    next: &[VNode],
    id: &mut HydrationId,
) -> Result<Vec<MountedNode<D>>, HydrationFailure> {
    let mut mounted = Vec::new();
    let mut cursor = dom.first_child(parent)?;

    for node in next {
        let node_id = *id;
        let marker = cursor.take().ok_or_else(|| {
            mismatch(
                node_id,
                "a hydration marker comment",
                "the end of the container",
            )
        })?;
        let marker_kind = dom.node_kind(&marker)?;
        if marker_kind != NodeKind::Comment {
            return Err(mismatch(
                node_id,
                "a hydration marker comment",
                &describe_node(dom, &marker)?,
            ));
        }

        let comment = dom.comment(&marker)?.unwrap_or_default();
        if HydrationId::parse(&comment) != Some(node_id) {
            return Err(mismatch(
                node_id,
                &format!("marker {:?}", node_id.content()),
                &format!("comment {comment:?}"),
            ));
        }

        let candidate = dom.next_sibling(&marker)?;
        dom.remove_child(parent, &marker)?;
        *id = HydrationId::new(node_id.value() + 1);

        let (mounted_node, next_cursor) = match node {
            VNode::Element(element) => {
                let candidate = candidate.ok_or_else(|| {
                    mismatch(
                        node_id,
                        &format!("element <{}>", element.tag().as_str()),
                        "the end of the container",
                    )
                })?;
                let next_cursor = dom.next_sibling(&candidate)?;
                let children = hydrate_element(dom, &candidate, element, id, node_id)?;
                (
                    MountedNode {
                        node: candidate,
                        vnode: node.clone(),
                        children,
                    },
                    next_cursor,
                )
            }
            VNode::Text(text) => hydrate_text(dom, parent, candidate, text, node_id)?,
            VNode::RawHtml(html) => {
                let candidate = candidate.ok_or_else(|| {
                    mismatch(node_id, "a raw HTML node", "the end of the container")
                })?;
                if dom.node_kind(&candidate)? != NodeKind::RawHtml {
                    return Err(mismatch(
                        node_id,
                        "a raw HTML node",
                        &describe_node(dom, &candidate)?,
                    ));
                }
                if dom.raw_html(&candidate)?.as_deref() != Some(html.as_str()) {
                    return Err(mismatch(
                        node_id,
                        &format!("raw HTML {html:?}"),
                        &format!(
                            "raw HTML {:?}",
                            dom.raw_html(&candidate)?.unwrap_or_default()
                        ),
                    ));
                }
                let next_cursor = dom.next_sibling(&candidate)?;
                (
                    MountedNode {
                        node: candidate,
                        vnode: node.clone(),
                        children: Vec::new(),
                    },
                    next_cursor,
                )
            }
            VNode::Fragment(_) => {
                unreachable!("fragments are flattened before hydration")
            }
        };

        mounted.push(mounted_node);
        cursor = next_cursor;
    }

    if let Some(extra) = cursor {
        return Err(mismatch(
            *id,
            "the end of the container",
            &describe_node(dom, &extra)?,
        ));
    }

    Ok(mounted)
}

fn hydrate_element<D: Dom>(
    dom: &mut D,
    node: &D::Node,
    element: &VElement,
    id: &mut HydrationId,
    node_id: HydrationId,
) -> Result<Vec<MountedNode<D>>, HydrationFailure> {
    validate_element(element)?;

    if dom.node_kind(node)? != NodeKind::Element {
        return Err(mismatch(
            node_id,
            &format!("element <{}>", element.tag().as_str()),
            &describe_node(dom, node)?,
        ));
    }

    let actual_tag = dom.tag_name(node)?.unwrap_or_default();
    if actual_tag != element.tag().as_str().to_ascii_lowercase() {
        return Err(mismatch(
            node_id,
            &format!("<{}>", element.tag().as_str().to_ascii_lowercase()),
            &format!("<{actual_tag}>"),
        ));
    }

    let mut expected_attributes = element
        .attributes()
        .iter()
        .filter_map(|attribute| match attribute.value() {
            rusty_core::AttributeValue::Text(value) => Some((
                attribute.name().as_str().to_ascii_lowercase(),
                value.clone(),
            )),
            rusty_core::AttributeValue::Bool(true) => Some((
                attribute.name().as_str().to_ascii_lowercase(),
                String::new(),
            )),
            rusty_core::AttributeValue::Bool(false) | rusty_core::AttributeValue::None => None,
        })
        .collect::<Vec<_>>();
    let mut actual_attributes = dom
        .attributes(node)?
        .into_iter()
        .map(|(name, value)| (name.to_ascii_lowercase(), value))
        .collect::<Vec<_>>();
    expected_attributes.sort();
    actual_attributes.sort();

    if actual_attributes != expected_attributes {
        return Err(mismatch(
            node_id,
            &format!("attributes {expected_attributes:?}"),
            &format!("attributes {actual_attributes:?}"),
        ));
    }

    for listener in element.listeners() {
        dom.set_event_listener(node, listener.clone())?;
    }

    hydrate_children(dom, node, &flatten_slice(element.children()), id)
}

fn hydrate_text<D: Dom>(
    dom: &mut D,
    parent: &D::Node,
    candidate: Option<D::Node>,
    text: &str,
    node_id: HydrationId,
) -> Result<(MountedNode<D>, Option<D::Node>), HydrationFailure> {
    if text.is_empty() {
        if let Some(candidate) = candidate {
            if dom.node_kind(&candidate)? == NodeKind::Text
                && dom.text(&candidate)?.as_deref() == Some("")
            {
                let next_cursor = dom.next_sibling(&candidate)?;
                return Ok((
                    MountedNode {
                        node: candidate,
                        vnode: VNode::text(""),
                        children: Vec::new(),
                    },
                    next_cursor,
                ));
            }

            let node = dom.create_text("")?;
            dom.insert_before(parent, &node, &candidate)?;
            return Ok((
                MountedNode {
                    node,
                    vnode: VNode::text(""),
                    children: Vec::new(),
                },
                Some(candidate),
            ));
        }

        let node = dom.create_text("")?;
        dom.append_child(parent, &node)?;
        return Ok((
            MountedNode {
                node,
                vnode: VNode::text(""),
                children: Vec::new(),
            },
            None,
        ));
    }

    let candidate = candidate.ok_or_else(|| {
        mismatch(
            node_id,
            &format!("text {text:?}"),
            "the end of the container",
        )
    })?;
    if dom.node_kind(&candidate)? != NodeKind::Text
        || dom.text(&candidate)?.as_deref() != Some(text)
    {
        return Err(mismatch(
            node_id,
            &format!("text {text:?}"),
            &describe_node(dom, &candidate)?,
        ));
    }

    let next_cursor = dom.next_sibling(&candidate)?;
    Ok((
        MountedNode {
            node: candidate,
            vnode: VNode::text(text),
            children: Vec::new(),
        },
        next_cursor,
    ))
}

fn clear_children<D: Dom>(dom: &mut D, parent: &D::Node) -> Result<(), DomError> {
    while let Some(child) = dom.first_child(parent)? {
        dom.remove_child(parent, &child)?;
    }

    Ok(())
}

fn describe_node<D: Dom>(dom: &D, node: &D::Node) -> Result<String, DomError> {
    Ok(match dom.node_kind(node)? {
        NodeKind::Element => format!("element <{}>", dom.tag_name(node)?.unwrap_or_default()),
        NodeKind::Text => format!("text {:?}", dom.text(node)?.unwrap_or_default()),
        NodeKind::Comment => format!("comment {:?}", dom.comment(node)?.unwrap_or_default()),
        NodeKind::RawHtml => String::from("raw HTML node"),
        NodeKind::Other => String::from("an unsupported node"),
    })
}

fn mismatch(id: HydrationId, expected: &str, found: &str) -> HydrationFailure {
    HydrationFailure::Mismatch(HydrationMismatch {
        id: id.value(),
        expected: expected.to_owned(),
        found: found.to_owned(),
    })
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

    /// Hydrates a view function and rerenders it when its signals change.
    ///
    /// The existing server markup is adopted instead of remounted. If markup
    /// cannot be adopted, the container is cleared, the tree is mounted fresh,
    /// and the returned outcome reports the mismatch. State that must survive
    /// the first effect run, such as restored signals, should be created before
    /// hydration and captured by the view closure.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] when the initial hydration or render fails.
    /// Later render failures panic because they occur inside the effect.
    pub fn hydrate<F>(
        dom: D,
        container: D::Node,
        mut view: F,
    ) -> Result<(Self, HydrationOutcome), RenderError>
    where
        F: FnMut() -> Element + 'static,
    {
        let renderer = Rc::new(RefCell::new(Renderer::new(dom)));
        let outcome = renderer.borrow_mut().hydrate(&container, view())?;
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

        Ok((
            Self {
                _owner: owner,
                renderer,
            },
            outcome,
        ))
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

    fn append_marker(dom: &mut MockDom, parent: &rusty_dom::MockNode, id: u64) {
        let marker = dom
            .create_comment(&HydrationId::new(id).content())
            .expect("marker");
        dom.append_child(parent, &marker).expect("append marker");
    }

    #[test]
    fn hydrates_server_markup_in_place() {
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let main = dom.create_element("main").expect("main");
        let heading = dom.create_element("h1").expect("h1");
        let text = dom.create_text("Title").expect("text");

        append_marker(&mut dom, &container, 0);
        dom.append_child(&container, &main).expect("append main");
        append_marker(&mut dom, &main, 1);
        dom.append_child(&main, &heading).expect("append heading");
        append_marker(&mut dom, &heading, 2);
        dom.append_child(&heading, &text).expect("append text");

        let mut renderer = Renderer::new(dom);
        let page = Element::html(
            tag("main"),
            Vec::new(),
            vec![Element::html(
                tag("h1"),
                Vec::new(),
                vec![Element::text("Title")],
            )],
        );

        assert_eq!(
            renderer.hydrate(&container, page),
            Ok(HydrationOutcome::Hydrated)
        );
        assert_eq!(
            renderer.dom().to_html(&container),
            "<div><main><h1>Title</h1></main></div>"
        );
        assert_eq!(
            renderer.dom().children(&container).expect("children"),
            [main]
        );
        assert_eq!(
            renderer.dom().children(&heading).expect("heading children"),
            [text]
        );
    }

    #[test]
    fn hydrates_fragments_and_empty_text() {
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let paragraph_node = dom.create_element("p").expect("p");

        append_marker(&mut dom, &container, 0);
        append_marker(&mut dom, &container, 1);
        dom.append_child(&container, &paragraph_node)
            .expect("append p");

        let mut renderer = Renderer::new(dom);
        let page = Element::fragment([
            Element::text(""),
            Element::html(tag("p"), Vec::new(), vec![]),
        ]);

        assert_eq!(
            renderer.hydrate(&container, page),
            Ok(HydrationOutcome::Hydrated)
        );

        let children = renderer.dom().children(&container).expect("children");
        assert_eq!(children.len(), 2);
        assert_eq!(renderer.dom().text(&children[0]).as_deref(), Some(""));
        assert_eq!(children[1], paragraph_node);
    }

    #[test]
    fn hydrates_raw_html_with_its_wrapper_node() {
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let raw = dom.create_raw_html("<b>raw</b>").expect("raw");

        append_marker(&mut dom, &container, 0);
        dom.append_child(&container, &raw).expect("append raw");

        let mut renderer = Renderer::new(dom);
        assert_eq!(
            renderer.hydrate(&container, Element::raw_html("<b>raw</b>")),
            Ok(HydrationOutcome::Hydrated)
        );
        assert_eq!(
            renderer.dom().children(&container).expect("children"),
            [raw]
        );
        assert_eq!(renderer.dom().to_html(&container), "<div><b>raw</b></div>");
    }

    #[test]
    fn recovers_from_text_mismatches_and_reports_the_node() {
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let paragraph_node = dom.create_element("p").expect("p");
        let server_text = dom.create_text("server").expect("text");

        append_marker(&mut dom, &container, 0);
        dom.append_child(&container, &paragraph_node)
            .expect("append p");
        append_marker(&mut dom, &paragraph_node, 1);
        dom.append_child(&paragraph_node, &server_text)
            .expect("append text");

        let mut renderer = Renderer::new(dom);
        let outcome = renderer
            .hydrate(&container, paragraph("client"))
            .expect("hydrate");

        let HydrationOutcome::Recovered(mismatch) = outcome else {
            panic!("expected recovery");
        };
        assert_eq!(mismatch.id(), 1);
        assert_eq!(mismatch.expected(), "text \"client\"");
        assert_eq!(mismatch.found(), "text \"server\"");
        assert_eq!(
            renderer.dom().to_html(&container),
            "<div><p>client</p></div>"
        );
    }

    #[test]
    fn recovers_from_extra_node_mismatches() {
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let paragraph_node = dom.create_element("p").expect("p");
        let text = dom.create_text("a").expect("text");
        let extra = dom.create_text("extra").expect("extra");

        append_marker(&mut dom, &container, 0);
        dom.append_child(&container, &paragraph_node)
            .expect("append p");
        append_marker(&mut dom, &paragraph_node, 1);
        dom.append_child(&paragraph_node, &text)
            .expect("append text");
        dom.append_child(&container, &extra).expect("append extra");

        let mut renderer = Renderer::new(dom);
        let outcome = renderer
            .hydrate(&container, paragraph("a"))
            .expect("hydrate");
        let HydrationOutcome::Recovered(mismatch) = outcome else {
            panic!("expected recovery");
        };
        assert_eq!(mismatch.id(), 2);
        assert_eq!(renderer.dom().to_html(&container), "<div><p>a</p></div>");
    }

    #[test]
    fn recovers_from_attribute_mismatches() {
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let paragraph_node = dom.create_element("p").expect("p");

        append_marker(&mut dom, &container, 0);
        dom.append_child(&container, &paragraph_node)
            .expect("append p");
        append_marker(&mut dom, &paragraph_node, 1);
        let text = dom.create_text("value").expect("text");
        dom.append_child(&paragraph_node, &text)
            .expect("append text");
        dom.set_attribute(&paragraph_node, "class", "server")
            .expect("attribute");

        let mut renderer = Renderer::new(dom);
        let outcome = renderer
            .hydrate(
                &container,
                Element::html(
                    tag("p"),
                    vec![attribute("class", "client")],
                    vec![Element::text("value")],
                ),
            )
            .expect("hydrate");
        let HydrationOutcome::Recovered(mismatch) = outcome else {
            panic!("expected recovery");
        };
        assert_eq!(mismatch.id(), 0);
        assert_eq!(
            renderer.dom().to_html(&container),
            "<div><p class=\"client\">value</p></div>"
        );
    }

    #[test]
    fn reactive_root_hydrates_and_patches_adopted_nodes() {
        use rusty_reactivity::Signal;

        let count = Signal::new(0_u32);
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let paragraph_node = dom.create_element("p").expect("p");
        let text = dom.create_text("0").expect("text");

        append_marker(&mut dom, &container, 0);
        dom.append_child(&container, &paragraph_node)
            .expect("append p");
        append_marker(&mut dom, &paragraph_node, 1);
        dom.append_child(&paragraph_node, &text)
            .expect("append text");

        let (root, outcome) = ReactiveRoot::hydrate(dom, container, {
            let count = count.clone();
            move || paragraph(&count.get().to_string())
        })
        .expect("hydrate");

        assert_eq!(outcome, HydrationOutcome::Hydrated);
        let first = root.with_dom(|dom| {
            dom.children(&container)
                .expect("children")
                .first()
                .copied()
                .expect("paragraph")
        });
        assert_eq!(first, paragraph_node);

        count.set(5);
        assert_eq!(
            root.with_dom(|dom| dom.to_html(&container)),
            "<div><p>5</p></div>"
        );
        assert_eq!(
            root.with_dom(|dom| dom.children(&container).expect("children")[0]),
            paragraph_node
        );
        assert_eq!(root.with_dom(|dom| dom.text(&text)), Some("5".to_owned()));
    }

    #[test]
    fn rejects_hydration_after_mounting() {
        let mut dom = MockDom::new();
        let container = dom.create_element("div").expect("container");
        let mut renderer = Renderer::new(dom);

        renderer
            .render(&container, paragraph("mounted"))
            .expect("render");
        assert_eq!(
            renderer.hydrate(&container, paragraph("mounted")),
            Err(RenderError::AlreadyMounted)
        );
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
