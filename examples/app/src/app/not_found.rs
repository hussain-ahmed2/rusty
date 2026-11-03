//! Application not-found page.

use rusty::prelude::*;

/// Renders the not-found page.
pub fn not_found() -> Element {
    view! {
        <main>
            <h1>"Not found"</h1>
            <p><a href="/">"Back home"</a></p>
        </main>
    }
}
