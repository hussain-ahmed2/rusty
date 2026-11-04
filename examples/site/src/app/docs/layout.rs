//! Documentation layout with a section sidebar.

use rusty::prelude::*;

/// Wraps documentation pages in the sidebar layout.
pub fn layout(children: Children) -> Element {
    let links = Element::fragment(
        crate::content::DOCS
            .iter()
            .map(|doc| {
                let href = crate::content::url(&format!("docs/{}", doc.slug));
                let title = doc.title;
                view! { <li><a href={href}>{title}</a></li> }
            })
            .collect::<Vec<_>>(),
    );
    let overview = crate::content::url("docs");

    view! {
        <main class="page docs-page">
            <aside class="docs-sidebar" aria-label="Documentation">
                <p class="eyebrow">"Documentation"</p>
                <div class="docs-search">
                    <input id="docs-search" type="search" placeholder="Search docs" aria-label="Search documentation" autocomplete="off">
                </div>
                <p id="docs-search-empty" class="docs-search-empty" aria-live="polite" hidden>"No matching pages."</p>
                <ul>
                    <li><a href={overview}>"Overview"</a></li>
                    {links}
                </ul>
            </aside>
            <article class="prose">{children}</article>
        </main>
    }
}
