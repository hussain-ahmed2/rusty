//! Documentation index.

use rusty::prelude::*;

/// Renders the documentation overview.
pub fn page(_context: &mut RequestContext) -> Result<Element, ServerError> {
    let cards = Element::fragment(
        crate::content::DOCS
            .iter()
            .map(|doc| {
                let href = crate::content::url(&format!("docs/{}/", doc.slug));
                let title = doc.title;
                let summary = doc.summary;
                view! {
                    <a class="card link-card" href={href}>
                        <h2>{title}</h2>
                        <p>{summary}</p>
                    </a>
                }
            })
            .collect::<Vec<_>>(),
    );

    Ok(view! {
        <header class="page-heading">
            <p class="eyebrow">"Documentation"</p>
            <h1>"Learn Rusty from the route tree up."</h1>
            <p class="lead">"Start with a scaffold, then add pages, layouts, dynamic segments, loading boundaries, and a static export."</p>
        </header>
        <div class="grid">{cards}</div>
    })
}
