//! Integration tests for reactive values in facade views.

use rusty::prelude::*;

fn render(element: &Element) -> String {
    render_to_string(element).expect("rendering should succeed")
}

#[test]
fn renders_signals_and_memos_in_views() {
    let count = Signal::new(3_u32);
    let doubled = Memo::new({
        let count = count.clone();
        move || count.get() * 2
    });

    let page = view! {
        <main class={count.clone()}>
            <p>{count}</p>
            <p>{doubled}</p>
        </main>
    };

    assert_eq!(render(&page), "<main class=\"3\"><p>3</p><p>6</p></main>");
}
