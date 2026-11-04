//! Dynamic documentation page.

use rusty::prelude::*;

/// Renders a documentation page selected by its slug.
pub fn page(context: &mut RequestContext) -> Result<Element, ServerError> {
    let docs = crate::content::url("docs");
    let Some(doc) = context
        .param("slug")
        .and_then(crate::content::doc)
    else {
        return Ok(view! {
            <header class="page-heading">
                <p class="eyebrow">"Documentation"</p>
                <h1>"Page not found"</h1>
                <p class="lead">"This documentation route does not exist."</p>
                <p><a class="text-link" href={docs}>"Back to the documentation index"</a></p>
            </header>
        });
    };

    let position = crate::content::DOCS
        .iter()
        .position(|candidate| candidate.slug == doc.slug);
    let mut pager = Vec::new();
    if let Some(previous) = position
        .and_then(|index| index.checked_sub(1))
        .and_then(|index| crate::content::DOCS.get(index))
    {
        let href = crate::content::url(&format!("docs/{}", previous.slug));
        let title = previous.title;
        pager.push(view! {
            <a class="pager-link previous" href={href}>
                <span>"Previous"</span>
                <strong>{title}</strong>
            </a>
        });
    }
    if let Some(next) = position.and_then(|index| crate::content::DOCS.get(index + 1)) {
        let href = crate::content::url(&format!("docs/{}", next.slug));
        let title = next.title;
        pager.push(view! {
            <a class="pager-link next" href={href}>
                <span>"Next"</span>
                <strong>{title}</strong>
            </a>
        });
    }

    let title = doc.title;
    let summary = doc.summary;
    let sections = Element::fragment(
        doc.sections
            .iter()
            .map(|section| {
                let title = section.title;
                let body = crate::content::rich(section.body);
                view! { <section><h2>{title}</h2><p>{body}</p></section> }
            })
            .collect::<Vec<_>>(),
    );

    Ok(view! {
        <header class="page-heading">
            <p class="eyebrow">"Documentation"</p>
            <h1>{title}</h1>
            <p class="lead">{summary}</p>
        </header>
        {sections}
        <nav class="docs-pager" aria-label="Documentation pages">{Element::fragment(pager)}</nav>
        <hr>
        <p><a class="text-link" href={docs}>"All documentation"</a></p>
    })
}
