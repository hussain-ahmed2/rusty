//! A browser counter built with the Rusty client runtime.
//!
//! Build for the browser with `wasm-pack build --target web` from this
//! directory, then serve the directory and open `index.html`.

#![forbid(unsafe_code)]

use rusty::prelude::*;

/// Renders the counter application with externally owned state.
///
/// The signal must outlive the view function because the browser runtime
/// rerenders by calling the view again.
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

/// Starts the counter in the document body.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    let count = Signal::new(0_u32);
    let root = rusty::client::mount_to_body(move || counter(count.clone()))
        .expect("failed to mount the counter");
    std::mem::forget(root);
}
