//! A server-rendered counter that Rusty hydrates in the browser.
//!
//! Build for the browser with `wasm-pack build --target web` from this
//! directory, then serve the directory and open `index.html`.

#![forbid(unsafe_code)]

use rusty::prelude::*;

/// Renders the counter with state restored by the hydration session.
#[must_use]
pub fn counter(count: Signal<u32>) -> Element {
    let handler_count = count.clone();

    view! {
        <main>
            <h1>"Rusty counter"</h1>
            <button on:click={move |_| {
                handler_count.set(handler_count.get() + 1);
            }}>"Increment"</button>
            <p>{count}</p>
        </main>
    }
}

/// Restores state and hydrates the server-rendered counter.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    let session = HydrationSession::from_document().expect("failed to read hydration state");
    let count = session
        .signal("count", 0_u32)
        .expect("failed to restore the counter");
    let (root, _outcome) = rusty::client::hydrate_to_id("app", move || counter(count.clone()))
        .expect("failed to hydrate the counter");
    std::mem::forget(root);
}

#[cfg(test)]
mod tests {
    use rusty::prelude::*;

    use super::counter;

    #[test]
    fn server_markup_matches_the_hydration_example() {
        let session = HydrationSession::server();
        let count = session.signal("count", 5_u32).expect("signal");

        assert_eq!(
            render_hydrated(&counter(count), &session).expect("render"),
            "<!--rusty:0--><main><!--rusty:1--><h1><!--rusty:2-->Rusty counter</h1><!--rusty:3--><button><!--rusty:4-->Increment</button><!--rusty:5--><p><!--rusty:6-->5</p></main>"
        );
        assert_eq!(session.state_json().expect("state"), r#"{"count":5}"#);
    }
}
