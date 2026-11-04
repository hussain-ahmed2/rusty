//! Documentation index loading boundary.

use rusty::prelude::*;

/// Renders the documentation index skeleton during navigation.
pub fn loading() -> Element {
    let cards = Element::fragment(
        (0..crate::content::DOCS.len())
            .map(|_| view! { <div class="skeleton card-skeleton"></div> })
            .collect::<Vec<_>>(),
    );
    crate::chrome::body(view! {
        <main class="page docs-page" aria-busy="true" aria-label="Loading page">
            {crate::chrome::docs_sidebar_skeleton()}
            <article class="prose" aria-hidden="true">
                <div class="skeleton skeleton-eyebrow"></div>
                <div class="skeleton skeleton-h1"></div>
                <div class="skeleton skeleton-h1 short"></div>
                <div class="skeleton skeleton-lead"></div>
                <div class="skeleton skeleton-lead short"></div>
                <div class="grid">{cards}</div>
            </article>
        </main>
    })
}
