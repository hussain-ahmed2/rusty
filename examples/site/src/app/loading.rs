//! Root loading boundary.

use rusty::prelude::*;

/// Renders a lightweight page skeleton during navigation.
pub fn loading() -> Element {
    view! {
        <main class="page" aria-busy="true">
            <div class="skeleton skeleton-title"></div>
            <div class="skeleton"></div>
            <div class="skeleton short"></div>
            <div class="grid">
                <div class="skeleton card-skeleton"></div>
                <div class="skeleton card-skeleton"></div>
                <div class="skeleton card-skeleton"></div>
            </div>
        </main>
    }
}
