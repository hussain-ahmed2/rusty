//! Client-runtime integration tests.

use std::cell::Cell;
use std::rc::Rc;

use rusty::client::MockDom;
use rusty::prelude::*;

#[test]
fn view_events_render_only_on_the_client() {
    let page = view! {
        <button on:click={|_| {}}>"Save"</button>
    };

    assert_eq!(
        render_to_string(&page).expect("server render"),
        "<button>Save</button>"
    );
}

#[test]
fn view_events_are_dispatched_by_the_dom() {
    let clicks = Rc::new(Cell::new(0_u32));
    let page = view! {
        <button on:click={{
            let clicks = Rc::clone(&clicks);
            move |_| clicks.set(clicks.get() + 1)
        }}>"Save"</button>
    };

    let mut dom = MockDom::new();
    let container = dom.create_element("div").expect("container");
    let mut renderer = Renderer::new(dom);
    renderer.render(&container, page).expect("render");

    let button = renderer.dom().children(&container).expect("children")[0];
    assert_eq!(renderer.dom().listener_names(&button), ["click"]);

    renderer
        .dom_mut()
        .dispatch(button, Event::new(EventName::new("click").expect("event")))
        .expect("dispatch");

    assert_eq!(clicks.get(), 1);
}

#[test]
fn signals_render_and_patch_through_the_facade() {
    let count = Signal::new(0_u32);
    let mut dom = MockDom::new();
    let container = dom.create_element("div").expect("container");

    let root = ReactiveRoot::mount(dom, container, {
        let count = count.clone();
        move || view! { <p class="count">{count.get()}</p> }
    })
    .expect("mount");

    assert_eq!(
        root.with_dom(|dom| dom.to_html(&container)),
        "<div><p class=\"count\">0</p></div>"
    );
    let first = root.with_dom(|dom| dom.children(&container).expect("children")[0]);

    count.set(3);

    assert_eq!(
        root.with_dom(|dom| dom.to_html(&container)),
        "<div><p class=\"count\">3</p></div>"
    );
    assert_eq!(
        root.with_dom(|dom| dom.children(&container).expect("children")[0]),
        first
    );
}
