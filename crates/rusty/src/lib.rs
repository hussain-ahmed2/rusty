//! Rusty — the Next.js experience, in Rust.
//!
//! Rusty is a Rust-native full-stack framework. The current milestones provide an
//! owned element model, the JSX-like [`view!`] macro, typed components, safe
//! server-side HTML rendering, fine-grained reactive primitives, a browser
//! runtime, SSR hydration, and file-based routing.
//!
//! # Example
//!
//! ```
//! use rusty::prelude::*;
//!
//! let page = view! {
//!     <main>
//!         <h1>"Hello from Rusty"</h1>
//!     </main>
//! };
//!
//! assert_eq!(
//!     render_to_string(&page).expect("rendering should succeed"),
//!     "<main><h1>Hello from Rusty</h1></main>"
//! );
//! ```
//!
//! Typed components are functions transformed into props structs:
//!
//! ```
//! use rusty::prelude::*;
//!
//! #[component]
//! fn Greeting(name: String) -> Element {
//!     view! { <p>{format!("Hello, {name}!")}</p> }
//! }
//!
//! let page = view! { <Greeting name="Rusty" /> };
//!
//! assert_eq!(
//!     render_to_string(&page).expect("rendering should succeed"),
//!     "<p>Hello, Rusty!</p>"
//! );
//! ```
//!
//! Signals, memos, and effects provide fine-grained reactive state:
//!
//! ```
//! use rusty::prelude::*;
//!
//! let (owner, (count, doubled)) = create_root(|| {
//!     let count = Signal::new(1_u32);
//!     let doubled = Memo::new({
//!         let count = count.clone();
//!         move || count.get() * 2
//!     });
//!     (count, doubled)
//! });
//!
//! assert_eq!(doubled.get(), 2);
//! count.set(3);
//! assert_eq!(doubled.get(), 6);
//! owner.dispose();
//! ```
//!
//! Reactive values can be rendered directly; they display their current value:
//!
//! ```
//! use rusty::prelude::*;
//!
//! let count = Signal::new(3_u32);
//! let page = view! { <p class={count.clone()}>{count}</p> };
//!
//! assert_eq!(
//!     render_to_string(&page).expect("rendering should succeed"),
//!     "<p class=\"3\">3</p>"
//! );
//! ```
//!
//! Hydration renders deterministic node markers and serializes state with stable
//! signal keys:
//!
//! ```
//! use rusty::prelude::*;
//!
//! let session = HydrationSession::server();
//! let count = session.signal("count", 1_u32).expect("signal");
//! let page = view! { <p>{count.get()}</p> };
//!
//! assert_eq!(
//!     render_hydrated(&page, &session).expect("hydration render"),
//!     "<!--rusty:0--><p><!--rusty:1-->1</p>"
//! );
//! assert_eq!(session.state_json().expect("state"), r#"{"count":1}"#);
//! ```
//!
//! The [`client`] module mounts the same trees into a DOM and patches them when
//! reactive state changes:
//!
//! ```
//! use rusty::client::MockDom;
//! use rusty::prelude::*;
//!
//! let count = Signal::new(0_u32);
//! let mut dom = MockDom::new();
//! let container = dom.create_element("div").expect("container");
//! let root = ReactiveRoot::mount(dom, container.clone(), {
//!     let count = count.clone();
//!     move || view! { <p>{count.get()}</p> }
//! })
//! .expect("mount");
//!
//! count.set(2);
//! root.with_dom(|dom| {
//!     assert_eq!(dom.to_html(&container), "<div><p>2</p></div>");
//! });
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

// Macro expansions resolve through the public facade (`::rusty::__private`).
// This alias makes those paths valid inside this crate as well as in downstream
// applications, without requiring users to depend on `rusty-core` directly.
// The lint cannot see macro-expanded uses of the alias.
#[allow(unused_extern_crates)]
extern crate self as rusty;

pub use rusty_core::{
    fragment_from_iter, is_void_element, Attribute, AttributeName, AttributeValue, Children,
    Component, Element, Event, EventListener, EventName, HtmlElement, IntoAttributeValue,
    IntoElement, InvalidName, NameKind, TagName, VOID_ELEMENTS,
};
pub use rusty_dom::{Dom, DomError, MockDom};
pub use rusty_hydrate::{HydrationError, HydrationSession, HYDRATION_STATE_ID};
pub use rusty_macros::{app, component, server, view};
pub use rusty_reactivity::{
    batch, create_effect, create_root, expect_context, memo, on_cleanup, provide_context, signal,
    untrack, use_context, Effect, Memo, Owner, Signal,
};
pub use rusty_renderer::{
    HydrationMismatch, HydrationOutcome, ReactiveRoot, RenderError as ClientRenderError, Renderer,
};
pub use rusty_router::{
    ApiRoute, ApiRouteMatch, RouteEntry, RouteManifest, RouteMatch, RouteParams, RoutePattern,
    RouteSegment, RouterError,
};
pub use rusty_rpc::{
    invoke, invoke_with, set_transport, with_transport, RpcClient, RpcError, ServerFunction,
    Transport, TransportGuard,
};
#[cfg(not(target_arch = "wasm32"))]
pub use rusty_rpc::{RpcRegistry, ServerInvoker};
#[cfg(not(target_arch = "wasm32"))]
pub use rusty_server::{
    authenticate, csrf, require_auth, AuthIdentity, Authenticator, BearerAuthenticator,
    CsrfMiddleware, CsrfToken, Extensions, Handler, Headers, HttpRouter, Method, Middleware, Next,
    Request, RequestContext, Response, ServerError, StatusCode, CSRF_COOKIE, CSRF_HEADER,
};
pub use rusty_ssr::{
    render_hydrated, render_to_chunks, render_to_string, HydrationRenderError, RenderError,
};
pub use rusty_vdom::{VNode, VNodeError};

#[cfg(target_arch = "wasm32")]
pub use rusty_dom::WebDom;

/// Internal glue used by code generated by Rusty macros.
///
/// This module is not part of Rusty's public API. Its contents may change in any
/// release without notice.
#[doc(hidden)]
pub mod __private {
    pub use rusty_core::{
        Attribute, AttributeName, AttributeValue, Children, Component, Element, Event,
        EventListener, EventName, IntoAttributeValue, IntoElement, TagName,
    };
    pub use rusty_rpc::__private::*;

    /// Constructs an HTML element from a macro-generated literal tag name.
    ///
    /// The macro validates the name at expansion time. This defensive check keeps
    /// manually crafted expansions from bypassing the tag-name invariant.
    pub fn element(
        tag: &'static str,
        attributes: Vec<Attribute>,
        children: Vec<Element>,
    ) -> Element {
        let tag = TagName::new(tag).expect("view! expansion produced an invalid tag name");
        Element::html(tag, attributes, children)
    }

    /// Constructs an attribute from a macro-generated literal name.
    ///
    /// The macro validates the name at expansion time. This defensive check keeps
    /// manually crafted expansions from bypassing the attribute-name invariant.
    pub fn attribute(name: &'static str, value: AttributeValue) -> Attribute {
        let name =
            AttributeName::new(name).expect("view! expansion produced an invalid attribute name");
        Attribute::new(name, value)
    }

    /// Constructs an HTML element with macro-generated client-side listeners.
    pub fn element_with_events(
        tag: &'static str,
        attributes: Vec<Attribute>,
        children: Vec<Element>,
        listeners: Vec<EventListener>,
    ) -> Element {
        let tag = TagName::new(tag).expect("view! expansion produced an invalid tag name");
        Element::html_with_events(tag, attributes, children, listeners)
    }

    /// Converts a dynamic attribute expression through `IntoAttributeValue`.
    pub fn dynamic_attribute(name: &'static str, value: impl IntoAttributeValue) -> Attribute {
        attribute(name, value.into_attribute_value())
    }

    /// Constructs a client-side event listener from a macro-generated event name.
    ///
    /// The macro validates the event name at expansion time.
    pub fn event_listener(name: &'static str, handler: impl Fn(Event) + 'static) -> EventListener {
        let name = EventName::new(name).expect("view! expansion produced an invalid event name");
        EventListener::new(name, handler)
    }

    /// Wraps rendered child nodes as a typed children value.
    pub fn children(element: Element) -> Children {
        Children::new(element)
    }

    /// Renders each item of an iterable into a fragment.
    pub fn each<I, T, F>(iterable: I, render: F) -> Element
    where
        I: IntoIterator<Item = T>,
        F: FnMut(T) -> Element,
    {
        Element::fragment(iterable.into_iter().map(render))
    }

    /// Renders a component through the `Component` trait.
    pub fn render_component<C: Component>(component: C) -> Element {
        component.render()
    }
}

/// Transport-neutral HTTP runtime.
#[cfg(not(target_arch = "wasm32"))]
pub mod server {
    pub use rusty_server::*;
}

/// File-based application runtime used by `#[rusty::app]`.
#[cfg(not(target_arch = "wasm32"))]
pub mod app;

/// Typed server-function runtime.
pub mod rpc {
    pub use rusty_rpc::*;
}

/// Browser-runtime exports.
///
/// The module is available on every target so tests can use [`MockDom`]. The
/// browser-specific `WebDom` and `mount_to_body` helpers exist only on
/// WebAssembly targets.
pub mod client {
    pub use rusty_core::Element;
    pub use rusty_dom::{Dom, DomError, MockDom};
    pub use rusty_hydrate::{HydrationError, HydrationSession, HYDRATION_STATE_ID};
    pub use rusty_renderer::{HydrationMismatch, HydrationOutcome, ReactiveRoot, Renderer};
    pub use rusty_vdom::{VNode, VNodeError};

    pub use crate::ClientRenderError;

    #[cfg(target_arch = "wasm32")]
    pub use rusty_dom::WebDom;

    /// Mounts a view function into the document body.
    ///
    /// The returned root must be kept alive. Dropping it disposes the render
    /// effect and removes the mounted nodes. State that must survive rerenders
    /// should be captured by the view closure, not created inside it.
    ///
    /// # Errors
    ///
    /// Returns [`ClientRenderError`] when the browser document is unavailable or
    /// the initial render fails.
    #[cfg(target_arch = "wasm32")]
    pub fn mount_to_body<F>(view: F) -> Result<ReactiveRoot<WebDom>, ClientRenderError>
    where
        F: FnMut() -> Element + 'static,
    {
        let dom = WebDom::new().map_err(ClientRenderError::Dom)?;
        let body = dom.body().map_err(ClientRenderError::Dom)?;
        ReactiveRoot::mount(dom, body, view)
    }

    /// Hydrates a view function into the element with the given `id`.
    ///
    /// Server-rendered markers must be inside that element. The state script
    /// and bootstrap script can live elsewhere in the document.
    ///
    /// # Errors
    ///
    /// Returns [`ClientRenderError`] when the document or element is missing,
    /// hydration fails, or the initial render fails.
    #[cfg(target_arch = "wasm32")]
    pub fn hydrate_to_id<F>(
        id: &str,
        view: F,
    ) -> Result<(ReactiveRoot<WebDom>, HydrationOutcome), ClientRenderError>
    where
        F: FnMut() -> Element + 'static,
    {
        let dom = WebDom::new().map_err(ClientRenderError::Dom)?;
        let container = dom
            .element_by_id(id)
            .map_err(ClientRenderError::Dom)?
            .ok_or_else(|| {
                ClientRenderError::Dom(DomError::browser(format!(
                    "hydration container `#{id}` was not found"
                )))
            })?;
        ReactiveRoot::hydrate(dom, container, view)
    }

    /// Hydrates a view function into the document body.
    ///
    /// The body must contain Rusty's server-rendered markers. Read persisted
    /// state with [`HydrationSession::from_document`] before building the view,
    /// then capture the restored signals in the view closure.
    ///
    /// The returned root must be kept alive. The outcome reports whether the
    /// server markup was adopted or rendered fresh after a mismatch.
    ///
    /// # Errors
    ///
    /// Returns [`ClientRenderError`] when the browser document is unavailable,
    /// hydration fails, or the initial render fails.
    #[cfg(target_arch = "wasm32")]
    pub fn hydrate_to_body<F>(
        view: F,
    ) -> Result<(ReactiveRoot<WebDom>, HydrationOutcome), ClientRenderError>
    where
        F: FnMut() -> Element + 'static,
    {
        let dom = WebDom::new().map_err(ClientRenderError::Dom)?;
        let body = dom.body().map_err(ClientRenderError::Dom)?;
        ReactiveRoot::hydrate(dom, body, view)
    }
}

/// Common imports for Rusty applications.
///
/// ```
/// use rusty::prelude::*;
/// ```
pub mod prelude {
    pub use crate::{
        app, batch, component, create_effect, create_root, expect_context, memo, on_cleanup,
        provide_context, render_hydrated, render_to_chunks, render_to_string, server, signal,
        untrack, use_context, view, ApiRoute, ApiRouteMatch, Attribute, AttributeName,
        AttributeValue, Children, ClientRenderError, Component, Dom, DomError, Effect, Element,
        Event, EventListener, EventName, HtmlElement, HydrationError, HydrationMismatch,
        HydrationOutcome, HydrationRenderError, HydrationSession, IntoAttributeValue, IntoElement,
        Memo, MockDom, Owner, ReactiveRoot, RenderError, Renderer, RouteEntry, RouteManifest,
        RouteMatch, RouteParams, RoutePattern, RouteSegment, RouterError, RpcClient, RpcError,
        ServerFunction, Signal, TagName, Transport, VNode, VNodeError, HYDRATION_STATE_ID,
    };
    #[cfg(not(target_arch = "wasm32"))]
    pub use crate::{
        authenticate, csrf, require_auth, AuthIdentity, Authenticator, BearerAuthenticator,
        CsrfMiddleware, CsrfToken, Extensions, Handler, Headers, HttpRouter, Method, Middleware,
        Next, Request, RequestContext, Response, RpcRegistry, ServerError, ServerInvoker,
        StatusCode, CSRF_COOKIE, CSRF_HEADER,
    };
}
