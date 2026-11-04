//! Documentation detail loading boundary.

use rusty::prelude::*;

/// Renders a documentation article skeleton during navigation.
pub fn loading() -> Element {
    crate::chrome::body(view! {
        <main class="page docs-page" aria-busy="true" aria-label="Loading page">
            {crate::chrome::docs_sidebar_skeleton()}
            <article class="prose" aria-hidden="true">
                <div class="skeleton skeleton-eyebrow"></div>
                <div class="skeleton skeleton-h1"></div>
                <div class="skeleton skeleton-lead"></div>
                <div class="skeleton skeleton-lead short"></div>
                <div class="skeleton skeleton-h3"></div>
                <div class="skeleton"></div>
                <div class="skeleton short"></div>
                <div class="skeleton skeleton-h3"></div>
                <div class="skeleton"></div>
                <div class="skeleton"></div>
                <div class="skeleton short"></div>
                <div class="skeleton skeleton-h3"></div>
                <div class="skeleton"></div>
                <div class="skeleton short"></div>
                <nav class="docs-pager skeleton-pager" aria-hidden="true">
                    <div class="skeleton pager-skeleton"></div>
                    <div class="skeleton pager-skeleton"></div>
                </nav>
                <div class="skeleton skeleton-rule"></div>
                <div class="skeleton skeleton-link"></div>
            </article>
        </main>
    })
}
