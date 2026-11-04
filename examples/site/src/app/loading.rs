//! Root loading boundary.

use rusty::prelude::*;

/// Renders the home page skeleton during navigation.
pub fn loading() -> Element {
    crate::chrome::body(view! {
        <main aria-busy="true" aria-label="Loading page">
            <div aria-hidden="true">
                <section class="hero">
                    <div class="skeleton skeleton-eyebrow"></div>
                    <div class="skeleton skeleton-h1"></div>
                    <div class="skeleton skeleton-h1"></div>
                    <div class="skeleton skeleton-h1 short"></div>
                    <div class="skeleton skeleton-lead"></div>
                    <div class="skeleton skeleton-lead"></div>
                    <div class="skeleton skeleton-lead short"></div>
                    <div class="skeleton-actions">
                        <div class="skeleton skeleton-button"></div>
                        <div class="skeleton skeleton-button"></div>
                    </div>
                </section>
                <section class="grid">
                    <div class="skeleton card-skeleton"></div>
                    <div class="skeleton card-skeleton"></div>
                    <div class="skeleton card-skeleton"></div>
                    <div class="skeleton card-skeleton"></div>
                </section>
                <section class="callout">
                    <div class="skeleton-stack">
                        <div class="skeleton skeleton-eyebrow"></div>
                        <div class="skeleton skeleton-h3"></div>
                        <div class="skeleton"></div>
                        <div class="skeleton short"></div>
                    </div>
                    <div class="skeleton skeleton-button"></div>
                </section>
            </div>
        </main>
    })
}
