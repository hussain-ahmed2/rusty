//! Shared site chrome used by layouts and loading boundaries.

use rusty::prelude::*;

/// Wraps page content in the site header and footer.
pub(crate) fn body(children: Element) -> Element {
    let home = crate::content::url("");
    let docs = crate::content::url("docs/");
    let about = crate::content::url("about/");

    Element::fragment([
        view! { <a class="skip-link" href="#main">"Skip to content"</a> },
        view! {
            <header class="site-header">
                <a class="brand" href={home}>
                    <span class="brand-mark">"R"</span>
                    <span>"Rusty"</span>
                </a>
                <nav aria-label="Primary">
                    <a href={docs}>"Docs"</a>
                    <a href={about}>"About"</a>
                    <a href="https://github.com/hussain-ahmed2/rusty" target="_blank" rel="external">"GitHub"</a>
                </nav>
            </header>
        },
        view! { <div id="main" class="site-main">{children}</div> },
        view! {
            <footer class="site-footer">
                <p>"Built with Rusty. Static export deployed by GitHub Actions."</p>
                <p>"MIT OR Apache-2.0"</p>
            </footer>
        },
    ])
}

/// Renders the documentation sidebar placeholder.
pub(crate) fn docs_sidebar_skeleton() -> Element {
    view! {
        <aside class="docs-sidebar" aria-hidden="true">
            <div class="skeleton skeleton-eyebrow"></div>
            <div class="skeleton-list">
                <div class="skeleton skeleton-nav"></div>
                <div class="skeleton skeleton-nav"></div>
                <div class="skeleton skeleton-nav"></div>
                <div class="skeleton skeleton-nav"></div>
                <div class="skeleton skeleton-nav"></div>
                <div class="skeleton skeleton-nav"></div>
            </div>
        </aside>
    }
}
