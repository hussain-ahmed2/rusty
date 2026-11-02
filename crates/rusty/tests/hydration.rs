//! SSR hydration integration tests.

use rusty::client::MockDom;
use rusty::prelude::*;

#[test]
fn server_markup_and_state_round_trip_through_the_facade() {
    let server = HydrationSession::server();
    let count = server.signal("count", 3_u32).expect("server signal");
    let page = view! { <p>{count.get()}</p> };

    assert_eq!(
        render_hydrated(&page, &server).expect("hydrated render"),
        "<!--rusty:0--><p><!--rusty:1-->3</p>"
    );
    assert_eq!(server.state_json().expect("state"), r#"{"count":3}"#);
    assert!(server
        .state_script()
        .expect("state script")
        .contains(HYDRATION_STATE_ID));

    let client =
        HydrationSession::from_json(&server.state_json().expect("state")).expect("restore state");
    let restored = client.signal("count", 0_u32).expect("client signal");
    assert_eq!(restored.get(), 3);

    let mut dom = MockDom::new();
    let container = dom.create_element("div").expect("container");
    let root_marker = dom.create_comment("rusty:0").expect("root marker");
    let paragraph = dom.create_element("p").expect("paragraph");
    let text_marker = dom.create_comment("rusty:1").expect("text marker");
    let text = dom.create_text("3").expect("text");

    dom.append_child(&container, &root_marker)
        .expect("append root marker");
    dom.append_child(&container, &paragraph)
        .expect("append paragraph");
    dom.append_child(&paragraph, &text_marker)
        .expect("append text marker");
    dom.append_child(&paragraph, &text).expect("append text");

    let (root, outcome) = ReactiveRoot::hydrate(dom, container, {
        let restored = restored.clone();
        move || view! { <p>{restored.get()}</p> }
    })
    .expect("hydrate");

    assert_eq!(outcome, HydrationOutcome::Hydrated);
    restored.set(7);
    assert_eq!(
        root.with_dom(|dom| dom.to_html(&container)),
        "<div><p>7</p></div>"
    );
    assert_eq!(
        root.with_dom(|dom| dom.children(&paragraph).expect("children")),
        [text]
    );
}
