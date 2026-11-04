//! About page.

use rusty::prelude::*;

/// Renders the project overview.
pub fn page(_context: &mut RequestContext) -> Result<Element, ServerError> {
    let docs = crate::content::url("docs");
    Ok(view! {
        <main class="page narrow prose">
            <p class="eyebrow">"About"</p>
            <h1>"A Rust framework with Next.js ergonomics."</h1>
            <p class="lead">"Rusty explores what a full-stack Rust framework feels like when routing, rendering, reactivity, and tooling share one coherent model."</p>
            <h2>"What is here today"</h2>
            <ul>
                <li>"Typed view! components and safe HTML escaping."</li>
                <li>"File-based pages, nested layouts, loading boundaries, and error boundaries."</li>
                <li>"Blocking HTTP transport, streaming SSR, and async page/API handlers."</li>
                <li>"Fine-grained signals, hydration, and same-origin client navigation."</li>
                <li>"CLI scaffolding, applications manifests, public assets, and deployment bundles."</li>
            </ul>
            <h2>"What comes next"</h2>
            <p>"Static generation is currently demonstrated by this example rather than a first-class CLI command. Incremental builds, platform adapters, richer data loading, and a persistent client shell are the next layer."</p>
            <p><a class="text-link" href={docs}>"Read the documentation"</a></p>
        </main>
    })
}
