//! Root loading boundary shown during client navigation.

use rusty::prelude::*;

/// Renders the application loading state.
pub fn loading() -> Element {
    view! { <main aria-busy="true"><p>"Loading…"</p></main> }
}
