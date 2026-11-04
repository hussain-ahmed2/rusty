//! About loading boundary.

use rusty::prelude::*;

/// Renders the about page skeleton during navigation.
pub fn loading() -> Element {
    crate::chrome::body(view! {
        <main class="page narrow" aria-busy="true" aria-label="Loading page">
            <div aria-hidden="true">
                <div class="skeleton skeleton-eyebrow"></div>
                <div class="skeleton skeleton-h1"></div>
                <div class="skeleton skeleton-h1"></div>
                <div class="skeleton skeleton-h1 short"></div>
                <div class="skeleton skeleton-lead"></div>
                <div class="skeleton skeleton-lead"></div>
                <div class="skeleton skeleton-lead short"></div>
                <div class="skeleton skeleton-h3"></div>
                <div class="skeleton"></div>
                <div class="skeleton"></div>
                <div class="skeleton"></div>
                <div class="skeleton"></div>
                <div class="skeleton short"></div>
                <div class="skeleton skeleton-h3"></div>
                <div class="skeleton"></div>
                <div class="skeleton"></div>
                <div class="skeleton short"></div>
                <div class="skeleton short"></div>
            </div>
        </main>
    })
}
